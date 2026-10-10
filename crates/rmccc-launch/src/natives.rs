//! Native library decompression for Minecraft.
//!
//! Minecraft's native libraries (LWJGL, etc.) are shipped as JAR files containing
//! platform-specific `.dll` / `.so` / `.dylib` files. These must be extracted to a
//! directory before the game can load them.
//!
//! Following HMCL's [`decompressNatives()`] pattern:
//! - Cleans the destination directory first (`cleanDirectoryQuietly`)
//! - Skips files that already exist with matching size (idempotent via `replaceExistentFile=false`)
//! - Applies each library's `extract.exclude` rules (`ExtractRules.shouldExtract`)
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

/// Remove the contents of a directory without removing the directory itself
/// (HMCL `FileUtils.cleanDirectoryQuietly`). Failures are logged, not fatal.
pub fn clean_directory_quietly(dir: &Path) {
    if !dir.exists() {
        return;
    }
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            let result = if path.is_dir() {
                std::fs::remove_dir_all(&path)
            } else {
                std::fs::remove_file(&path)
            };
            if let Err(e) = result {
                tracing::warn!("Failed to clean {}: {e}", path.display());
            }
        }
    }
}

/// Whether an entry should be extracted from a native JAR, honoring the
/// library's `extract.exclude` list (HMCL `ExtractRules.shouldExtract`).
///
/// An entry is skipped when its relative path starts with (or equals) any
/// exclude rule.
fn should_extract(relative_path: &str, lib: &rmccc_version::Library) -> bool {
    let Some(excludes) = lib.extract.as_ref().map(|e| &e.exclude) else {
        return true;
    };
    for exclude in excludes {
        // HMCL `ExtractRules.shouldExtract`: skip when the entry starts with
        // (or equals) an exclude prefix, e.g. "META-INF/" skips "META-INF/versions/...".
        if relative_path.starts_with(exclude) {
            tracing::debug!(
                entry = relative_path,
                exclude = exclude,
                "Entry excluded by ExtractRules"
            );
            return false;
        }
    }
    true
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
    // HMCL cleanDirectoryQuietly — re-extract from a clean slate.
    clean_directory_quietly(natives_dir);

    let mut count = 0u32;

    for lib in libraries.iter().filter(|l| is_native(l)) {
        let jar_path = resolve_library_path(&lib.name, libs_dir);
        if !jar_path.exists() {
            tracing::warn!("Native library JAR not found: {}", jar_path.display());
            continue;
        }

        let file = match std::fs::File::open(&jar_path) {
            Ok(f) => f,
            Err(e) => {
                tracing::warn!("Cannot open native library JAR {}: {e}", jar_path.display());
                continue;
            }
        };

        let mut archive = match zip::ZipArchive::new(file) {
            Ok(a) => a,
            Err(e) => {
                tracing::warn!("Cannot read native library JAR {}: {e}", jar_path.display());
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

            // HMCL ExtractRules: honor the library's exclude list.
            if !should_extract(&entry_name, lib) {
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
///
/// Delegates to [`rmccc_version::libraries::make_library_path`] so the path
/// logic is shared with the download/classpath code (L14).
fn resolve_library_path(name: &str, libs_dir: &Path) -> PathBuf {
    rmccc_version::libraries::make_library_path(name, libs_dir)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rmccc_version::Library;

    #[test]
    fn test_is_native_detection() {
        let native_lib = Library {
            name: "org.lwjgl:lwjgl:3.3.1:natives-macos".into(),
            ..Default::default()
        };
        assert!(is_native(&native_lib));

        let normal_lib = Library {
            name: "com.google.guava:guava:31.1-jre".into(),
            ..Default::default()
        };
        assert!(!is_native(&normal_lib));
    }

    #[test]
    fn test_resolve_library_path_normal() {
        let path = resolve_library_path("org.lwjgl:lwjgl:3.3.1", Path::new("/libs"));
        assert_eq!(
            path,
            Path::new("/libs/org/lwjgl/lwjgl/3.3.1/lwjgl-3.3.1.jar")
        );
    }

    #[test]
    fn test_resolve_library_path_with_classifier() {
        let path = resolve_library_path("org.lwjgl:lwjgl:3.3.1:natives-macos", Path::new("/libs"));
        assert_eq!(
            path,
            Path::new("/libs/org/lwjgl/lwjgl/3.3.1/lwjgl-3.3.1-natives-macos.jar")
        );
    }

    #[test]
    fn test_is_native_windows() {
        let lib = Library {
            name: "org.lwjgl:lwjgl:3.3.1:natives-windows".into(),
            ..Default::default()
        };
        assert!(is_native(&lib));
    }

    #[test]
    fn test_is_native_linux() {
        let lib = Library {
            name: "org.lwjgl:lwjgl:3.3.1:natives-linux".into(),
            ..Default::default()
        };
        assert!(is_native(&lib));
    }

    #[test]
    fn test_is_native_unix() {
        let lib = Library {
            name: "org.lwjgl:lwjgl:3.3.1:natives-unix".into(),
            ..Default::default()
        };
        assert!(is_native(&lib));
    }

    #[test]
    fn test_resolve_library_path_minimal_parts() {
        // Less than 3 parts: shared `make_library_path` joins with a slash.
        let path = resolve_library_path("solo:artifact", Path::new("/libs"));
        assert_eq!(path, Path::new("/libs/solo/artifact"));
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
                ..Default::default()
            };
            assert_eq!(is_native(&lib), expected, "Failed for: {name}");
        }
    }

    #[test]
    fn test_should_extract_excludes() {
        // No extract rules → everything is extracted.
        let lib = Library {
            name: "org.lwjgl:lwjgl:3.3.1:natives-macos".into(),
            ..Default::default()
        };
        assert!(should_extract("META-INF/native/libglfw.dylib", &lib));

        // With exclude rules → matching entries are skipped.
        let lib = Library {
            name: "org.lwjgl:lwjgl:3.3.1:natives-macos".into(),
            extract: Some(rmccc_version::ExtractRules {
                exclude: vec![
                    "META-INF/".to_string(),
                    "org/lwjgl/glfw/GLFW.class".to_string(),
                ],
            }),
            ..Default::default()
        };
        assert!(!should_extract(
            "META-INF/versions/9/module-info.class",
            &lib
        ));
        assert!(!should_extract("org/lwjgl/glfw/GLFW.class", &lib));
        assert!(should_extract("libglfw.dylib", &lib));
    }

    #[test]
    fn test_clean_directory_quietly() {
        let dir = std::env::temp_dir().join(format!("rmcl-native-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(dir.join("sub")).unwrap();
        std::fs::write(dir.join("a.so"), b"x").unwrap();
        std::fs::write(dir.join("sub/b.dylib"), b"y").unwrap();
        clean_directory_quietly(&dir);
        assert!(!dir.join("a.so").exists());
        assert!(!dir.join("sub").exists());
        assert!(dir.exists());
        let _ = std::fs::remove_dir(&dir);
    }
}
