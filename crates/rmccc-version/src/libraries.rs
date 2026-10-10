//! Library dependency resolution.
//!
//! Filters library rules per-platform and resolves artifact URLs.

use rmccc_download::DownloadItem;
use std::path::{Path, PathBuf};

use super::types::Library;

/// Determine if a library should be included on the current platform.
///
/// Mojang rules use a last-match-wins strategy: iterate all rules,
/// track the last applicable action. Default is "allow" if no rules match.
pub fn library_applies(lib: &Library) -> bool {
    let rules = match &lib.rules {
        Some(r) => r,
        None => return true,
    };

    let mut allowed = false;
    let mut any_matched = false;

    for rule in rules {
        let applies = match &rule.os {
            Some(os) => {
                let os_name = os.name.as_deref().unwrap_or("");
                match os_name {
                    "osx" | "macos" => cfg!(target_os = "macos"),
                    "windows" => cfg!(target_os = "windows"),
                    "linux" => cfg!(target_os = "linux"),
                    _ => false,
                }
            }
            None => true,
        };

        if applies {
            allowed = rule.action == "allow";
            any_matched = true;
        }
    }

    if any_matched {
        allowed
    } else {
        true // no matching rules → default allow
    }
}

/// Resolve library download items for the current platform.
///
/// This generates items for both the main artifact AND classifier JARs (natives).
/// Native libraries with OS-specific classifiers like `natives-macos` are only
/// downloaded for the current platform.
pub fn resolve_library_items(libraries: &[Library], libs_dir: &Path) -> Vec<DownloadItem> {
    let mut items = Vec::new();

    for lib in libraries.iter().filter(|lib| library_applies(lib)) {
        // 1. Main artifact (the standard JAR)
        if let Some(item) = make_library_item(lib, libs_dir) {
            items.push(item);
        }

        // 2. Classifier JARs (natives) — only for the current platform
        if let Some(downloads) = &lib.downloads {
            if let Some(classifiers) = &downloads.classifiers {
                for (classifier, entry) in classifiers {
                    if is_current_platform_classifier(classifier) {
                        let full_name = format!("{}:{}", lib.name, classifier);
                        let dest = entry
                            .path
                            .as_ref()
                            .map(|p| libs_dir.join(p))
                            .unwrap_or_else(|| make_library_path(&full_name, libs_dir));

                        items.push(DownloadItem {
                            url: entry.url.clone(),
                            dest,
                            expected_sha1: Some(entry.sha1.clone()),
                            size: Some(entry.size),
                            use_cache: true,
                            candidates: Vec::new(),
                        });
                    }
                }
            }
        }
    }

    items
}

/// Return the Minecraft-specific architecture name for the current platform.
///
/// Minecraft uses different arch naming than Rust:
/// - `arm64` on macOS ARM (vs Rust's `aarch64`)
/// - `x86` on 32-bit x86 (same as Rust)
/// - `x86_64` on 64-bit x86 (same as Rust)
fn minecraft_arch() -> &'static str {
    #[cfg(target_arch = "aarch64")]
    {
        "arm64"
    }
    #[cfg(target_arch = "x86_64")]
    {
        "x86_64"
    }
    #[cfg(target_arch = "x86")]
    {
        "x86"
    }
    #[cfg(not(any(target_arch = "aarch64", target_arch = "x86_64", target_arch = "x86")))]
    {
        "unknown"
    }
}

/// Check if a classifier matches the current platform.
pub(crate) fn is_current_platform_classifier(classifier: &str) -> bool {
    // On each platform, match both the generic classifier (e.g. "natives-macos")
    // and the arch-specific classifier (e.g. "natives-macos-arm64").
    // Generic is always downloaded; arch-specific only if it matches current CPU.
    #[cfg(target_os = "macos")]
    {
        let arch_specific = format!("natives-macos-{}", minecraft_arch());
        classifier == "natives-macos" || classifier == "natives-osx" || classifier == arch_specific
    }
    #[cfg(target_os = "windows")]
    {
        let arch_specific = format!("natives-windows-{}", minecraft_arch());
        classifier == "natives-windows" || classifier == arch_specific
    }
    #[cfg(target_os = "linux")]
    {
        let arch_specific = format!("natives-linux-{}", minecraft_arch());
        classifier == "natives-linux" || classifier == "natives-unix" || classifier == arch_specific
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
    {
        false
    }
}

fn make_library_item(lib: &Library, libs_dir: &Path) -> Option<DownloadItem> {
    let path = make_library_path(&lib.name, libs_dir);

    // If library has explicit download info, use it
    if let Some(artifact) = lib.downloads.as_ref().and_then(|d| d.artifact.as_ref()) {
        let dest = artifact
            .path
            .as_ref()
            .map(|p| libs_dir.join(p))
            .unwrap_or_else(|| path.clone());

        return Some(DownloadItem {
            url: artifact.url.clone(),
            dest,
            expected_sha1: Some(artifact.sha1.clone()),
            size: Some(artifact.size),
            use_cache: true,
            candidates: Vec::new(),
        });
    }

    // Fall back: construct Maven URL from library name (for Fabric/Quilt/etc.)
    let urls = maven_urls_from_name(&lib.name);
    let primary = urls.first().cloned().unwrap_or_default();
    let candidates: Vec<String> = urls.into_iter().skip(1).collect();
    Some(DownloadItem {
        url: primary,
        dest: path,
        expected_sha1: None,
        size: None,
        use_cache: true,
        candidates,
    })
}

/// Construct Maven repository download URLs from a library name.
///
/// Returns a list of candidate URLs ordered by preference.
/// The first URL is the primary; subsequent URLs are fallbacks.
///
/// Known repositories:
/// - `net.fabricmc:*` → `https://maven.fabricmc.net/`
/// - `org.quiltmc:*` → `https://maven.quiltmc.org/repository/release/`
/// - Everything else → `https://repo1.maven.org/maven2/` (Maven Central)
///   with `https://libraries.minecraft.net/` as secondary fallback
pub(crate) fn maven_urls_from_name(name: &str) -> Vec<String> {
    let parts: Vec<&str> = name.split(':').collect();
    if parts.len() < 3 {
        return vec![String::new()];
    }
    let group = parts[0];
    let artifact = parts[1];
    let version = parts[2];
    let group_path = group.replace('.', "/");
    let jar_path = format!("{group_path}/{artifact}/{version}/{artifact}-{version}.jar");

    if group.starts_with("net.fabricmc") {
        vec![format!("https://maven.fabricmc.net/{jar_path}")]
    } else if group.starts_with("org.quiltmc") {
        vec![format!(
            "https://maven.quiltmc.org/repository/release/{jar_path}"
        )]
    } else {
        // Primary: Maven Central (universal), Fallback: Mojang's library repo
        vec![
            format!("https://repo1.maven.org/maven2/{jar_path}"),
            format!("https://libraries.minecraft.net/{jar_path}"),
        ]
    }
}

/// Build the standard library path from its Maven-style name.
///
/// Example:
///   `com.mojang:logging:1.0.0` → `libraries/com/mojang/logging/1.0.0/logging-1.0.0.jar`
///   `org.lwjgl:lwjgl:3.3.1:natives-macos` → `libraries/org/lwjgl/lwjgl/3.3.1/lwjgl-3.3.1-natives-macos.jar`
pub fn make_library_path(name: &str, libs_dir: &Path) -> PathBuf {
    let parts: Vec<&str> = name.split(':').collect();
    if parts.len() < 3 {
        return libs_dir.join(name.replace(':', "/"));
    }

    let group = parts[0].replace('.', "/");
    let artifact_id = parts[1];
    let version = parts[2];
    let classifier = parts.get(3);

    let jar_name = match classifier {
        Some(cls) => format!("{}-{}-{}.jar", artifact_id, version, cls),
        None => format!("{}-{}.jar", artifact_id, version),
    };

    libs_dir
        .join(&group)
        .join(artifact_id)
        .join(version)
        .join(&jar_name)
}

/// Collect all library paths for the classpath.
pub fn collect_classpath(libraries: &[Library], libs_dir: &Path) -> Vec<PathBuf> {
    libraries
        .iter()
        .filter(|lib| library_applies(lib))
        .map(|lib| {
            // If library has explicit download info, use artifact path if provided
            if let Some(artifact_path) = lib
                .downloads
                .as_ref()
                .and_then(|d| d.artifact.as_ref())
                .and_then(|a| a.path.as_ref())
            {
                return libs_dir.join(artifact_path);
            }
            // Fall back to Maven-style path from the library name.
            // Still include the path — Java will fail fast with a clear ClassNotFoundException
            // if the JAR is missing, which is better than a silent skip.
            make_library_path(&lib.name, libs_dir)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{LibraryRule, OsRule};

    #[test]
    fn test_make_library_path() {
        let dir = Path::new("/game/libraries");
        let path = make_library_path("com.mojang:logging:1.0.0", dir);
        assert_eq!(path, dir.join("com/mojang/logging/1.0.0/logging-1.0.0.jar"));
    }

    #[test]
    fn test_make_library_path_with_classifier() {
        let dir = Path::new("/libs");
        let path = make_library_path("org.lwjgl:lwjgl:3.3.1:natives-macos", dir);
        assert_eq!(
            path,
            dir.join("org/lwjgl/lwjgl/3.3.1/lwjgl-3.3.1-natives-macos.jar")
        );
    }

    #[test]
    fn test_library_applies_no_rules() {
        let lib = Library {
            name: "test:lib:1.0".into(),
            ..Default::default()
        };
        assert!(library_applies(&lib));
    }

    #[test]
    fn test_library_applies_osx_only() {
        let rules = vec![LibraryRule {
            action: "allow".into(),
            os: Some(OsRule {
                name: Some("osx".into()),
                arch: None,
                version: None,
            }),
        }];
        let lib = Library {
            name: "test:natives:1.0".into(),
            rules: Some(rules),
            ..Default::default()
        };
        #[cfg(target_os = "macos")]
        assert!(library_applies(&lib));
        #[cfg(not(target_os = "macos"))]
        assert!(!library_applies(&lib));
    }

    #[test]
    fn test_library_applies_allow_with_disallow() {
        let rules = vec![
            LibraryRule {
                action: "allow".into(),
                os: Some(OsRule {
                    name: Some("osx".into()),
                    arch: None,
                    version: None,
                }),
            },
            LibraryRule {
                action: "disallow".into(),
                os: None,
            },
        ];
        let lib = Library {
            name: "test:natives-all-disallowed:1.0".into(),
            rules: Some(rules),
            ..Default::default()
        };
        assert!(!library_applies(&lib));
    }

    #[test]
    fn test_is_current_platform_classifier() {
        #[cfg(target_os = "macos")]
        {
            assert!(super::is_current_platform_classifier("natives-macos"));
            assert!(super::is_current_platform_classifier("natives-osx"));
            assert!(!super::is_current_platform_classifier("natives-windows"));
            assert!(!super::is_current_platform_classifier("natives-linux"));
        }
        #[cfg(target_os = "windows")]
        {
            assert!(super::is_current_platform_classifier("natives-windows"));
            assert!(!super::is_current_platform_classifier("natives-macos"));
        }
        #[cfg(target_os = "linux")]
        {
            assert!(super::is_current_platform_classifier("natives-linux"));
            assert!(super::is_current_platform_classifier("natives-unix"));
            assert!(!super::is_current_platform_classifier("natives-macos"));
        }
    }
}
