//! Native library decompression for Minecraft.
//!
//! Minecraft's native libraries (LWJGL, etc.) are shipped as JAR files containing
//! platform-specific `.dll` / `.so` / `.dylib` files. These must be extracted to a
//! directory before the game can load them.
//!
//! Following HMCL's [`decompressNatives()`] pattern:
//! - Skips files that already exist with matching size (idempotent via `replaceExistentFile=false`)
//! - Filters out `.sha1`, `.git`, and other non-native files
//! - Supports `useNativeGLFW` / `useNativeOpenAL` skip flags

use rmccc_error::Result;
use std::io::Read;
use std::path::{Path, PathBuf};

/// Determine if a library is a native (platform-specific) library.
///
/// Native libraries have classifiers in their Maven coordinates like
/// `natives-windows`, `natives-macos`, `natives-linux`, or just contain
/// the OS name as a classifier.
pub fn is_native(lib: &rmccc_version::Library) -> bool {
    let name = &lib.name;
    let classifiers = [
        "natives-windows",
        "natives-macos",
        "natives-osx",
        "natives-linux",
        "natives-unix",
    ];
    classifiers.iter().any(|c| name.contains(c))
}

/// Extract native library files from all native JARs into the natives directory.
///
/// # Arguments
/// * `libraries` - All libraries from the resolved game version
/// * `libs_dir` - The libraries directory (e.g., `.minecraft/libraries`)
/// * `natives_dir` - The target directory for extracted native files
/// * `skip_glfw` - If true, skip GLFW-related native files
/// * `skip_openal` - If true, skip OpenAL-related native files
///
/// # Returns
/// Number of files extracted (or verified as existing).
pub fn decompress_natives(
    libraries: &[rmccc_version::Library],
    libs_dir: &Path,
    natives_dir: &Path,
    skip_glfw: bool,
    skip_openal: bool,
) -> Result<u32> {
    std::fs::create_dir_all(natives_dir)?;

    let mut count = 0u32;

    for lib in libraries.iter().filter(|l| is_native(l)) {
        let jar_path = resolve_library_path(&lib.name, libs_dir);
        if !jar_path.exists() {
            tracing::warn!(
                "Native library JAR not found: {}",
                jar_path.display()
            );
            continue;
        }

        let file = match std::fs::File::open(&jar_path) {
            Ok(f) => f,
            Err(e) => {
                tracing::warn!(
                    "Cannot open native library JAR {}: {e}",
                    jar_path.display()
                );
                continue;
            }
        };

        let mut archive = match zip::ZipArchive::new(file) {
            Ok(a) => a,
            Err(e) => {
                tracing::warn!(
                    "Cannot read native library JAR {}: {e}",
                    jar_path.display()
                );
                continue;
            }
        };

        for i in 0..archive.len() {
            let mut entry = match archive.by_index(i) {
                Ok(e) => e,
                Err(_) => continue,
            };

            let entry_name = entry.name().to_string();

            // Skip directories, symlinks, non-native files
            if entry.is_dir() || entry.is_symlink() {
                continue;
            }

            // Skip non-native file extensions
            let ext = match Path::new(&entry_name).extension() {
                Some(e) => e.to_str().unwrap_or(""),
                None => continue,
            };
            if !matches!(ext, "dll" | "so" | "dylib" | "jnilib") {
                continue;
            }

            // Skip .sha1 / .git files
            if entry_name.ends_with(".sha1") || entry_name.ends_with(".git") {
                continue;
            }

            // Optional: skip GLFW/OpenAL
            let lower = entry_name.to_lowercase();
            if skip_glfw && lower.contains("glfw") {
                continue;
            }
            if skip_openal && lower.contains("openal") {
                continue;
            }

            let dest_path = natives_dir.join(&entry_name);

            // Skip if file already exists with matching size (idempotent)
            if dest_path.exists() {
                if let Ok(meta) = std::fs::metadata(&dest_path) {
                    if meta.len() == entry.size() {
                        continue;
                    }
                }
            }

            // Ensure parent directory exists
            if let Some(parent) = dest_path.parent() {
                std::fs::create_dir_all(parent)?;
            }

            // Extract
            let mut data = Vec::with_capacity(entry.size() as usize);
            entry.read_to_end(&mut data)?;
            std::fs::write(&dest_path, &data)?;

            tracing::debug!("Extracted native: {}", entry_name);
            count += 1;
        }
    }

    tracing::info!(
        count = count,
        natives_dir = %natives_dir.display(),
        "Native library extraction completed"
    );

    Ok(count)
}

/// Resolve a Maven library name to its JAR file path.
///
/// Format: `group:artifact:version` or `group:artifact:version:classifier`
/// Example: `org.lwjgl:lwjgl:3.3.1:natives-macos`
fn resolve_library_path(name: &str, libs_dir: &Path) -> PathBuf {
    let parts: Vec<&str> = name.split(':').collect();
    if parts.len() < 3 {
        return libs_dir.join(format!("{}.jar", name.replace(":", "-")));
    }

    let group = parts[0].replace('.', "/");
    let artifact = parts[1];
    let version = parts[2];
    let classifier = parts.get(3);

    let jar_name = match classifier {
        Some(cls) => format!("{}-{}-{}.jar", artifact, version, cls),
        None => format!("{}-{}.jar", artifact, version),
    };

    libs_dir.join(&group).join(artifact).join(version).join(&jar_name)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rmccc_version::Library;

    #[test]
    fn test_is_native_detection() {
        let native_lib = Library {
            name: "org.lwjgl:lwjgl:3.3.1:natives-macos".into(),
            downloads: None,
            rules: None,
        };
        assert!(is_native(&native_lib));

        let normal_lib = Library {
            name: "com.google.guava:guava:31.1-jre".into(),
            downloads: None,
            rules: None,
        };
        assert!(!is_native(&normal_lib));
    }

    #[test]
    fn test_resolve_library_path_normal() {
        let path = resolve_library_path(
            "org.lwjgl:lwjgl:3.3.1",
            Path::new("/libs"),
        );
        assert_eq!(
            path,
            Path::new("/libs/org/lwjgl/lwjgl/3.3.1/lwjgl-3.3.1.jar")
        );
    }

    #[test]
    fn test_resolve_library_path_with_classifier() {
        let path = resolve_library_path(
            "org.lwjgl:lwjgl:3.3.1:natives-macos",
            Path::new("/libs"),
        );
        assert_eq!(
            path,
            Path::new("/libs/org/lwjgl/lwjgl/3.3.1/lwjgl-3.3.1-natives-macos.jar")
        );
    }

    #[test]
    fn test_is_native_windows() {
        let lib = Library {
            name: "org.lwjgl:lwjgl:3.3.1:natives-windows".into(),
            downloads: None,
            rules: None,
        };
        assert!(is_native(&lib));
    }

    #[test]
    fn test_is_native_linux() {
        let lib = Library {
            name: "org.lwjgl:lwjgl:3.3.1:natives-linux".into(),
            downloads: None,
            rules: None,
        };
        assert!(is_native(&lib));
    }

    #[test]
    fn test_is_native_unix() {
        let lib = Library {
            name: "org.lwjgl:lwjgl:3.3.1:natives-unix".into(),
            downloads: None,
            rules: None,
        };
        assert!(is_native(&lib));
    }

    #[test]
    fn test_resolve_library_path_minimal_parts() {
        // Less than 3 parts: fallback to simple filename
        let path = resolve_library_path("solo:artifact", Path::new("/libs"));
        assert_eq!(path, Path::new("/libs/solo-artifact.jar"));
    }

    #[test]
    fn test_resolve_library_path_long_group() {
        let path = resolve_library_path(
            "net.minecraftforge:forge:1.20.4-49.0.0",
            Path::new("/minecraft/libraries"),
        );
        assert_eq!(
            path,
            Path::new("/minecraft/libraries/net/minecraftforge/forge/1.20.4-49.0.0/forge-1.20.4-49.0.0.jar")
        );
    }

    #[test]
    fn test_is_native_detection_not_native() {
        let tests = vec![
            ("com.google.guava:guava:31.1-jre", false),
            ("net.java.dev.jna:jna:5.13.0", false),
            ("org.apache.commons:commons-lang3:3.12.0", false),
            ("org.lwjgl:lwjgl:3.3.1:natives-macos", true),
        ];
        for (name, expected) in tests {
            let lib = Library {
                name: name.to_string(),
                downloads: None,
                rules: None,
            };
            assert_eq!(is_native(&lib), expected, "Failed for: {name}");
        }
    }
}
