//! `.minecraft` directory layout helpers.
//!
//! Centralizes every path used by install / launch / repository so the on-disk
//! layout matches HMCL's:
//!
//! ```text
//! {game_dir}/
//! ├── versions/{id}/{id}.json + {id}.jar + natives-{platform}/
//! ├── libraries/{group}/{artifact}/{version}/{artifact}-{version}[-{classifier}].jar
//! ├── assets/
//! │   ├── indexes/{assetIndex}.json
//! │   ├── objects/{hash[0:2]}/{hash}
//! │   ├── virtual/legacy/
//! │   └── log_configs/
//! ├── mods/ resourcepacks/ saves/
//! └── launcher_profiles.json
//! ```

use rmccc_platform::PlatformDescriptor;
use std::path::{Path, PathBuf};

/// Platform classifier used by Minecraft native libraries (e.g. `natives-osx`).
///
/// 经注入的描述符取值（`osx` / `windows` / `linux`），无裸 `cfg!`。
pub fn platform_classifier(platform: &PlatformDescriptor) -> &'static str {
    platform.os.rule_name()
}

/// Native directory suffix — HMCL uses `natives-{platform}`.
pub fn native_dir_name(platform: &PlatformDescriptor) -> String {
    format!("natives-{}", platform_classifier(platform))
}

/// `{game_dir}/versions/{id}` — the instance root (HMCL `getInstanceRoot`).
pub fn instance_root(game_dir: &Path, id: &str) -> PathBuf {
    game_dir.join("versions").join(id)
}

/// `{game_dir}/versions/{id}/{id}.json` — the instance manifest.
pub fn instance_json(game_dir: &Path, id: &str) -> PathBuf {
    instance_root(game_dir, id).join(format!("{id}.json"))
}

/// `{game_dir}/versions/{id}/{id}.jar` — the client JAR.
pub fn instance_jar(game_dir: &Path, id: &str) -> PathBuf {
    instance_root(game_dir, id).join(format!("{id}.jar"))
}

/// `{game_dir}/versions` — version directory root.
pub fn versions_dir(game_dir: &Path) -> PathBuf {
    game_dir.join("versions")
}

/// `{game_dir}/libraries` — shared library repository.
pub fn libraries_dir(game_dir: &Path) -> PathBuf {
    game_dir.join("libraries")
}

/// `{game_dir}/libraries/{group}/{artifact}/{version}/{artifact}-{version}[-{classifier}].jar`
pub fn library_file(game_dir: &Path, name: &str) -> PathBuf {
    let parts: Vec<&str> = name.split(':').collect();
    if parts.len() < 3 {
        return libraries_dir(game_dir).join(name.replace(':', "/"));
    }
    let group = parts[0].replace('.', "/");
    let artifact = parts[1];
    let version = parts[2];
    let classifier = parts.get(3);
    let jar_name = match classifier {
        Some(cls) => format!("{artifact}-{version}-{cls}.jar"),
        None => format!("{artifact}-{version}.jar"),
    };
    libraries_dir(game_dir)
        .join(&group)
        .join(artifact)
        .join(version)
        .join(jar_name)
}

/// `{game_dir}/versions/{id}/natives-{platform}` — HMCL native dir.
pub fn native_dir(game_dir: &Path, id: &str, platform: &PlatformDescriptor) -> PathBuf {
    instance_root(game_dir, id).join(native_dir_name(platform))
}

/// Legacy `{game_dir}/versions/{id}/natives` (pre-HMCL naming).
pub fn native_dir_legacy(game_dir: &Path, id: &str) -> PathBuf {
    instance_root(game_dir, id).join("natives")
}

/// Resolve the natives directory, preferring the HMCL `natives-{platform}`
/// layout and falling back to the legacy `natives` folder for migration.
pub fn resolve_native_dir(game_dir: &Path, id: &str, platform: &PlatformDescriptor) -> PathBuf {
    let preferred = native_dir(game_dir, id, platform);
    if preferred.exists() {
        preferred
    } else {
        let legacy = native_dir_legacy(game_dir, id);
        if legacy.exists() {
            legacy
        } else {
            preferred
        }
    }
}

/// `{game_dir}/assets`.
pub fn assets_dir(game_dir: &Path) -> PathBuf {
    game_dir.join("assets")
}

/// `{game_dir}/assets/indexes/{asset_id}.json`.
pub fn asset_index_file(game_dir: &Path, asset_id: &str) -> PathBuf {
    assets_dir(game_dir)
        .join("indexes")
        .join(format!("{asset_id}.json"))
}

/// `{game_dir}/assets/objects/{hash[0:2]}/{hash}`.
pub fn asset_object(game_dir: &Path, hash: &str) -> PathBuf {
    assets_dir(game_dir)
        .join("objects")
        .join(&hash[..2])
        .join(hash)
}

/// `{game_dir}/assets/virtual/legacy` — pre-1.6 resource compatibility.
pub fn asset_legacy_dir(game_dir: &Path) -> PathBuf {
    assets_dir(game_dir).join("virtual").join("legacy")
}

/// `{game_dir}/assets/log_configs/{file_name}` — logging objects.
pub fn log_config_file(game_dir: &Path, file_name: &str) -> PathBuf {
    assets_dir(game_dir).join("log_configs").join(file_name)
}

/// `{game_dir}/mods`.
pub fn mods_dir(game_dir: &Path) -> PathBuf {
    game_dir.join("mods")
}

/// `{game_dir}/resourcepacks`.
pub fn resourcepacks_dir(game_dir: &Path) -> PathBuf {
    game_dir.join("resourcepacks")
}

/// `{game_dir}/saves`.
pub fn saves_dir(game_dir: &Path) -> PathBuf {
    game_dir.join("saves")
}

/// `{game_dir}/launcher_profiles.json` — Mojang / HMCL compatible marker.
pub fn launcher_profiles_path(game_dir: &Path) -> PathBuf {
    game_dir.join("launcher_profiles.json")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn gd() -> &'static Path {
        Path::new("/tmp/rmccc-layout/game")
    }

    #[test]
    fn test_instance_layout() {
        let root = instance_root(gd(), "1.20.4");
        assert_eq!(root, Path::new("/tmp/rmccc-layout/game/versions/1.20.4"));
        assert_eq!(
            instance_json(gd(), "1.20.4"),
            Path::new("/tmp/rmccc-layout/game/versions/1.20.4/1.20.4.json")
        );
        assert_eq!(
            instance_jar(gd(), "1.20.4"),
            Path::new("/tmp/rmccc-layout/game/versions/1.20.4/1.20.4.jar")
        );
    }

    #[test]
    fn test_library_file() {
        assert_eq!(
            library_file(gd(), "com.mojang:logging:1.0.0"),
            Path::new(
                "/tmp/rmccc-layout/game/libraries/com/mojang/logging/1.0.0/logging-1.0.0.jar"
            )
        );
        assert_eq!(
            library_file(gd(), "org.lwjgl:lwjgl:3.3.1:natives-macos"),
            Path::new(
                "/tmp/rmccc-layout/game/libraries/org/lwjgl/lwjgl/3.3.1/lwjgl-3.3.1-natives-macos.jar"
            )
        );
    }

    #[test]
    fn test_native_dir_naming() {
        let mac = PlatformDescriptor::macos_arm64();
        let dir = native_dir(gd(), "1.20.4", &mac);
        assert!(dir.ends_with("natives-osx"));
        // 换平台描述符 → 目录名随之变化（不再依赖宿主平台）。
        let win = PlatformDescriptor::new(
            rmccc_platform::PlatformOs::Windows,
            rmccc_platform::PlatformArch::X86_64,
        );
        assert!(native_dir(gd(), "1.20.4", &win).ends_with("natives-windows"));
        assert_eq!(
            native_dir_legacy(gd(), "1.20.4"),
            Path::new("/tmp/rmccc-layout/game/versions/1.20.4/natives")
        );
    }

    #[test]
    fn test_assets_layout() {
        assert_eq!(
            asset_index_file(gd(), "17"),
            Path::new("/tmp/rmccc-layout/game/assets/indexes/17.json")
        );
        assert_eq!(
            asset_object(gd(), "a1b2c3"),
            Path::new("/tmp/rmccc-layout/game/assets/objects/a1/a1b2c3")
        );
        assert_eq!(
            asset_legacy_dir(gd()),
            Path::new("/tmp/rmccc-layout/game/assets/virtual/legacy")
        );
        assert_eq!(
            launcher_profiles_path(gd()),
            Path::new("/tmp/rmccc-layout/game/launcher_profiles.json")
        );
    }
}
