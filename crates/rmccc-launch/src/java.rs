//! Cross-platform Java detection.

use rmccc_error::{Result, RmcccError};
use std::path::PathBuf;
use std::process::Command;

/// Detected Java installation.
#[derive(Debug, Clone)]
pub struct JavaInstallation {
    pub path: PathBuf,
    pub major_version: u32,
    pub vendor: String,
    pub is_64bit: bool,
}

/// Detect available Java installations on the system.
/// Returns installations sorted by version (highest first).
pub fn detect_java_installations() -> Vec<JavaInstallation> {
    let mut found: Vec<JavaInstallation> = Vec::new();

    // Platform-specific detection
    detect_platform_specific(&mut found);

    // Scan common paths as fallback
    detect_common_paths(&mut found);

    // Sort by version descending
    found.sort_by_key(|a| std::cmp::Reverse(a.major_version));
    found.dedup_by(|a, b| a.path == b.path);

    found
}

/// Find the best Java installation for the given required version.
pub fn find_java(required_version: u32) -> Result<JavaInstallation> {
    let installations = detect_java_installations();
    if installations.is_empty() {
        return Err(RmcccError::JavaNotFound);
    }

    // First: prefer exact match
    if let Some(java) = installations
        .iter()
        .find(|j| j.major_version == required_version)
    {
        return Ok(java.clone());
    }

    // Second: prefer closest compatible version (not highest)
    // This avoids incompatibilities like Java 25's strict module-path limits
    // when the game version only needs Java 17.
    if let Some(java) = installations
        .iter()
        .filter(|j| j.major_version >= required_version)
        .min_by_key(|j| j.major_version)
    {
        tracing::warn!(
            required = required_version,
            found = java.major_version,
            "Using compatible Java version (not exact match, closest selected)"
        );
        return Ok(java.clone());
    }

    Err(RmcccError::JavaVersionMismatch {
        found: installations[0].major_version,
        required: required_version,
    })
}

#[cfg(target_os = "macos")]
fn detect_platform_specific(found: &mut Vec<JavaInstallation>) {
    // /usr/libexec/java_home -V
    if let Ok(output) = Command::new("/usr/libexec/java_home").arg("-V").output() {
        // java_home writes to stderr
        let stderr = String::from_utf8_lossy(&output.stderr);
        for line in stderr.lines() {
            let trimmed = line.trim();
            // Match lines like: 21.0.3 (arm64) "Amazon.com Inc." ...
            if let Some(java) = parse_java_home_line(trimmed) {
                found.push(java);
            }
        }
    }

    // Also check JAVA_HOME
    if let Ok(home) = std::env::var("JAVA_HOME") {
        let path = PathBuf::from(&home).join("bin").join("java");
        if let Some(java) = scan_java_binary(&path) {
            found.push(java);
        }
    }
}

#[cfg(target_os = "macos")]
fn parse_java_home_line(line: &str) -> Option<JavaInstallation> {
    // Format: "VERSION (ARCH) \"VENDOR\" - \"VM_NAME\" HOME_PATH"
    // HOME_PATH always starts with "/" and is the last whitespace-delimited token.
    let trimmed = line.trim();
    let version_part = trimmed.split_whitespace().next()?;
    let major: u32 = version_part.split('.').next()?.parse().ok()?;

    // The home path is always the last token and starts with "/"
    let home = trimmed.split_whitespace().next_back()?;
    if !home.starts_with('/') {
        return None;
    }

    let java_bin = PathBuf::from(home).join("bin").join("java");
    scan_java_binary(&java_bin).map(|mut j| {
        j.major_version = major;
        j
    })
}

#[cfg(target_os = "windows")]
fn detect_platform_specific(found: &mut Vec<JavaInstallation>) {
    // Check JAVA_HOME
    if let Ok(home) = std::env::var("JAVA_HOME") {
        let path = PathBuf::from(&home).join("bin").join("java.exe");
        if let Some(java) = scan_java_binary(&path) {
            found.push(java);
        }
    }

    // Check PATH for java.exe
    if let Ok(paths) = std::env::var("PATH") {
        for dir in paths.split(';') {
            let path = PathBuf::from(dir).join("java.exe");
            if let Some(java) = scan_java_binary(&path) {
                found.push(java);
            }
        }
    }

    // Scan common Windows Java install paths
    let common_dirs = [
        "C:\\Program Files\\Java",
        "C:\\Program Files (x86)\\Java",
        "C:\\Program Files\\Eclipse Adoptium",
        "C:\\Program Files\\Amazon Corretto",
    ];
    for dir in &common_dirs {
        let base = std::path::Path::new(dir);
        if let Ok(entries) = std::fs::read_dir(base) {
            for entry in entries.flatten() {
                let java_bin = entry.path().join("bin").join("java.exe");
                if let Some(java) = scan_java_binary(&java_bin) {
                    found.push(java);
                }
            }
        }
    }
}

#[cfg(target_os = "linux")]
fn detect_platform_specific(found: &mut Vec<JavaInstallation>) {
    // Check JAVA_HOME
    if let Ok(home) = std::env::var("JAVA_HOME") {
        let path = PathBuf::from(&home).join("bin").join("java");
        if let Some(java) = scan_java_binary(&path) {
            found.push(java);
        }
    }

    // Scan /usr/lib/jvm (common Linux Java install location)
    let jvm_dir = std::path::Path::new("/usr/lib/jvm");
    if let Ok(entries) = std::fs::read_dir(jvm_dir) {
        for entry in entries.flatten() {
            let java_bin = entry.path().join("bin").join("java");
            if let Some(java) = scan_java_binary(&java_bin) {
                found.push(java);
            }
        }
    }

    // Check alternatives symlink
    let alternatives_path = std::path::Path::new("/etc/alternatives/java");
    if let Some(java) = scan_java_binary(alternatives_path) {
        found.push(java);
    }
}

/// Scan common paths for java binary.
fn detect_common_paths(found: &mut Vec<JavaInstallation>) {
    let paths = vec![
        "/usr/bin/java",
        "/usr/local/bin/java",
        "/opt/homebrew/opt/openjdk/bin/java",
    ];

    for path_str in paths {
        let path = PathBuf::from(path_str);
        if let Some(java) = scan_java_binary(&path) {
            found.push(java);
        }
    }
}

/// Try to get version info from a java binary.
fn scan_java_binary(path: &std::path::Path) -> Option<JavaInstallation> {
    if !path.exists() {
        return None;
    }

    let output = Command::new(path)
        .arg("-XshowSettings:properties")
        .arg("-version")
        .output()
        .ok()?;

    let stdout = String::from_utf8_lossy(&output.stderr);
    // Also check stdout (version info can appear in either)
    let combined = format!("{}\n{}", stdout, String::from_utf8_lossy(&output.stdout));

    // Parse java.version = X.Y.Z
    let major = combined
        .lines()
        .find(|l| l.contains("java.version"))
        .and_then(|l| l.split('=').nth(1))
        .and_then(|v| v.trim().split('.').next())
        .and_then(|v| v.parse::<u32>().ok());

    let Some(major) = major else {
        tracing::warn!("Failed to parse Java version from: {}", path.display());
        return None;
    };

    // Parse java.vendor
    let vendor = combined
        .lines()
        .find(|l| l.contains("java.vendor"))
        .and_then(|l| l.split('=').nth(1))
        .map(|v| v.trim().to_string())
        .unwrap_or_default();

    // Check 64-bit
    let is_64bit = combined
        .lines()
        .find(|l| l.contains("sun.arch.data.model"))
        .and_then(|l| l.split('=').nth(1))
        .map(|v| v.trim() == "64")
        .unwrap_or(true);

    Some(JavaInstallation {
        path: path.to_path_buf(),
        major_version: major,
        vendor,
        is_64bit,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_find_java_not_found() {
        // On CI machines without Java, this will return JavaNotFound
        // On dev machines with Java, this will return Ok
        let result = find_java(8);
        // Just verify it doesn't crash
        let _ = result;
    }
}
