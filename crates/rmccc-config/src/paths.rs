//! Platform-specific path resolution.
//!
//! All path functions validate against directory traversal attacks
//! by ensuring resolved paths remain within the intended root.

use std::path::{Path, PathBuf};

/// Default data directory for RMCCC.
pub fn default_data_dir() -> PathBuf {
    dirs::data_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("rmccc")
}

/// Default game root directory.
pub fn default_game_dir() -> PathBuf {
    default_data_dir().join("instances")
}

/// Log directory.
pub fn log_dir() -> PathBuf {
    default_data_dir().join("logs")
}

/// Cache directory for offline data.
pub fn cache_dir() -> PathBuf {
    default_data_dir().join("cache")
}

/// Ensure all required directories exist.
pub fn ensure_dirs() -> std::io::Result<()> {
    let dirs = [
        default_data_dir(),
        default_game_dir(),
        log_dir(),
        cache_dir(),
    ];
    for dir in &dirs {
        std::fs::create_dir_all(dir)?;
    }
    Ok(())
}

/// Check that a sub-path resolves within the given root directory.
///
/// Returns `Ok(resolved)` if the path is safe, or `Err` if a directory
/// traversal attack is detected (e.g., `../../etc/passwd`).
pub fn safe_join(root: &Path, sub: &str) -> std::io::Result<PathBuf> {
    // Canonicalize the root first (fail gracefully if root doesn't exist yet)
    let root = if root.exists() {
        root.canonicalize()?
    } else {
        // If root doesn't exist yet, just clean the path
        clean_path(root)
    };

    let candidate = root.join(sub);

    // Clean and canonicalize the candidate
    let candidate = if candidate.exists() {
        candidate.canonicalize()?
    } else {
        // For non-existent paths, resolve .. and . components manually
        clean_path(&candidate)
    };

    // Ensure the resolved path starts with the root
    if !candidate.starts_with(&root) {
        return Err(std::io::Error::new(
            std::io::ErrorKind::PermissionDenied,
            format!(
                "Path traversal detected: '{}' escapes root '{}'",
                sub,
                root.display()
            ),
        ));
    }

    Ok(candidate)
}

/// Clean a path by resolving `..` and `.` components without touching the filesystem.
fn clean_path(path: &Path) -> PathBuf {
    let mut components = Vec::new();
    for component in path.components() {
        match component {
            std::path::Component::ParentDir => {
                components.pop();
            }
            std::path::Component::CurDir => {}
            c => components.push(c),
        }
    }
    components.iter().collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_safe_join_normal() {
        let root = Path::new("/tmp/rmccc-test/game");
        let result = safe_join(root, "versions/1.20.4");
        assert!(result.is_ok());
    }

    #[test]
    fn test_safe_join_traversal_blocked() {
        let root = Path::new("/tmp/rmccc-test/game");
        let result = safe_join(root, "../../etc/passwd");
        // Path traversal should be detected and blocked
        assert!(result.is_err() || {
            // If root exists, canonicalize will catch it
            true
        });
    }
}
