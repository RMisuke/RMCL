//! HMCL-aligned mod-loader detection for installed versions.
//!
//! HMCL writes the exact loader identity into the version JSON's `patches`
//! array (`{"id":"forge","version":"47.4.20",...}`), which is also what its
//! `LibraryAnalyzer` consumes. Versions produced by other launchers (or old
//! Forge installers) may only carry the loader as a library coordinate, and
//! NeoForge standalone manifests can expose the version through
//! `--fml.neoForgeVersion`; both fallbacks mirror HMCL's behaviour.

use crate::types::{GameVersion, VersionArgument};

/// Detected mod loader. `kind` uses the backend ids (`forge`, `neo_forge`,
/// `opti_fine`, …) so the frontend's `loaderLabel()` can render brand casing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoaderIdentity {
    pub kind: String,
    /// Loader version; empty when only the loader kind could be determined.
    pub version: String,
}

/// Canonical backend id for a patch id / library / main-class hint.
fn canonical_id(id: &str) -> Option<&'static str> {
    match id.to_ascii_lowercase().as_str() {
        "forge" => Some("forge"),
        "neoforge" | "neo_forge" => Some("neo_forge"),
        "fabric" => Some("fabric"),
        "quilt" => Some("quilt"),
        "liteloader" | "lite_loader" => Some("lite_loader"),
        "optifine" | "opti_fine" => Some("opti_fine"),
        "cleanroom" => Some("cleanroom"),
        "legacyfabric" | "legacy_fabric" => Some("legacy_fabric"),
        _ => None,
    }
}

/// Detect the loader of a standalone (resolved-patches) version manifest.
pub fn detect_loader(version: &GameVersion) -> Option<LoaderIdentity> {
    // 1. HMCL patch metadata — exact version, highest confidence.
    for patch in &version.patches {
        let Some(kind) = canonical_id(&patch.id) else {
            continue;
        };
        let version_text = if patch.version.trim().is_empty() {
            fml_arg_version(version, kind).unwrap_or_default()
        } else {
            patch.version.clone()
        };
        return Some(LoaderIdentity {
            kind: kind.to_string(),
            version: version_text,
        });
    }

    // 2. `--fml.neoForgeVersion` / `--fml.forgeVersion` arguments (NeoForge
    //    standalone manifests; HMCL's NeoForge branch scans the same flags).
    if let Some(v) = fml_arg_version(version, "neo_forge") {
        return Some(LoaderIdentity {
            kind: "neo_forge".into(),
            version: v,
        });
    }
    if let Some(v) = fml_arg_version(version, "forge") {
        return Some(LoaderIdentity {
            kind: "forge".into(),
            version: v,
        });
    }

    // 3. Library coordinates (HMCL LibraryAnalyzer patterns).
    for library in &version.libraries {
        if let Some(found) = library_loader(&library.name) {
            return Some(found);
        }
    }

    // 4. Main class prefix — kind only, no version available.
    canonical_main_class(&version.main_class).map(|kind| LoaderIdentity {
        kind: kind.to_string(),
        version: String::new(),
    })
}

/// Base Minecraft version carried by the HMCL `game` patch (`1.20.1`), used
/// when the version has no `inheritsFrom` parent to walk.
pub fn base_version(version: &GameVersion) -> Option<String> {
    version
        .patches
        .iter()
        .find(|p| p.id == "game" && !p.version.trim().is_empty())
        .map(|p| p.version.clone())
}

fn library_loader(name: &str) -> Option<LoaderIdentity> {
    let mut parts = name.split(':');
    let group = parts.next()?.to_ascii_lowercase();
    let artifact = parts.next()?.to_ascii_lowercase();
    let raw_version = parts.next().unwrap_or("");

    let (kind, version) = if group.contains("minecraftforge") && artifact == "forge" {
        ("forge", strip_mc_prefix(raw_version))
    } else if group.contains("neoforged") && (artifact == "neoforge" || artifact == "fmlcore") {
        ("neo_forge", strip_mc_prefix(raw_version))
    } else if group.contains("fabricmc") && artifact == "fabric-loader" {
        ("fabric", raw_version.to_string())
    } else if group.contains("quiltmc") && artifact == "quilt-loader" {
        ("quilt", raw_version.to_string())
    } else if group.contains("cleanroommc") && artifact == "cleanroom" {
        ("cleanroom", raw_version.to_string())
    } else if group.contains("mumfrey") && artifact == "liteloader" {
        ("lite_loader", raw_version.to_string())
    } else if group.contains("optifine") {
        ("opti_fine", raw_version.to_string())
    } else {
        return None;
    };

    Some(LoaderIdentity {
        kind: kind.to_string(),
        version,
    })
}

fn canonical_main_class(main_class: &str) -> Option<&'static str> {
    if main_class.starts_with("net.fabricmc") {
        Some("fabric")
    } else if main_class.starts_with("org.quiltmc") {
        Some("quilt")
    } else if main_class.starts_with("net.neoforged") {
        Some("neo_forge")
    } else if main_class.starts_with("net.minecraftforge") || main_class.starts_with("cpw.mods") {
        Some("forge")
    } else if main_class.starts_with("top.outlands") {
        Some("cleanroom")
    } else {
        None
    }
}

/// Forge library versions look like `1.12.2-14.23.5.2864`; NeoForge may use
/// `1.20.1-47.1.106`. Strip the Minecraft-version prefix when present.
fn strip_mc_prefix(version: &str) -> String {
    let Some((prefix, rest)) = version.split_once('-') else {
        return version.to_string();
    };
    let looks_like_mc = prefix.starts_with(|c: char| c.is_ascii_digit())
        && prefix.contains('.')
        && prefix.chars().all(|c| c.is_ascii_digit() || c == '.');
    if looks_like_mc {
        rest.to_string()
    } else {
        version.to_string()
    }
}

fn fml_arg_version(version: &GameVersion, kind: &str) -> Option<String> {
    let flags: &[&str] = match kind {
        "neo_forge" => &["--fml.neoForgeVersion"],
        "forge" => &["--fml.forgeVersion"],
        _ => return None,
    };
    let args = version.arguments.as_ref()?.game.as_ref()?;
    let mut iter = args.iter();
    while let Some(arg) = iter.next() {
        let is_flag = argument_values(arg)
            .iter()
            .any(|value| flags.contains(value));
        if is_flag {
            return iter
                .next()
                .and_then(|next| argument_values(next).first().map(|v| (*v).to_string()));
        }
    }
    None
}

fn argument_values(arg: &VersionArgument) -> Vec<&str> {
    match arg {
        VersionArgument::Simple(value) => vec![value.as_str()],
        VersionArgument::Ruled { value, .. } => value.as_vec(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::manifest::parse_version_json;

    fn parse(json: &str) -> GameVersion {
        parse_version_json("test-version", json).expect("parse version json")
    }

    #[test]
    fn hmcl_patches_win_with_exact_versions() {
        let forge = parse(
            r#"{
              "id": "1.12.2-Forge",
              "mainClass": "net.minecraft.launchwrapper.Launch",
              "patches": [
                {"id": "game", "version": "1.12.2"},
                {"id": "forge", "version": "14.23.5.2864"}
              ],
              "libraries": [{"name": "net.minecraftforge:forge:1.12.2-14.23.5.2864"}]
            }"#,
        );
        assert_eq!(
            detect_loader(&forge),
            Some(LoaderIdentity {
                kind: "forge".into(),
                version: "14.23.5.2864".into()
            })
        );
        assert_eq!(base_version(&forge).as_deref(), Some("1.12.2"));

        let fabric = parse(
            r#"{
              "id": "1.21.11-Fabric",
              "mainClass": "net.fabricmc.loader.impl.launch.knot.KnotClient",
              "patches": [
                {"id": "game", "version": "1.21.11"},
                {"id": "fabric", "version": "0.19.2"}
              ]
            }"#,
        );
        assert_eq!(
            detect_loader(&fabric),
            Some(LoaderIdentity {
                kind: "fabric".into(),
                version: "0.19.2".into()
            })
        );
        assert_eq!(base_version(&fabric).as_deref(), Some("1.21.11"));
    }

    #[test]
    fn library_and_argument_fallbacks() {
        let forge = parse(
            r#"{
              "id": "legacy",
              "mainClass": "cpw.mods.bootstraplauncher.BootstrapLauncher",
              "libraries": [{"name": "net.minecraftforge:forge:1.20.1-47.4.5"}]
            }"#,
        );
        assert_eq!(
            detect_loader(&forge),
            Some(LoaderIdentity {
                kind: "forge".into(),
                version: "47.4.5".into()
            })
        );

        let neoforge = parse(
            r#"{
              "id": "neoforge",
              "mainClass": "cpw.mods.bootstraplauncher.BootstrapLauncher",
              "arguments": {"game": ["--fml.neoForgeVersion", "47.1.106"]},
              "libraries": [{"name": "net.neoforged.fancymodloader:loader:47.2.2"}]
            }"#,
        );
        assert_eq!(
            detect_loader(&neoforge),
            Some(LoaderIdentity {
                kind: "neo_forge".into(),
                version: "47.1.106".into()
            })
        );
    }

    #[test]
    fn vanilla_has_no_loader() {
        let vanilla = parse(
            r#"{"id": "1.20.1", "mainClass": "net.minecraft.client.main.Main", "libraries": []}"#,
        );
        assert_eq!(detect_loader(&vanilla), None);
        assert_eq!(base_version(&vanilla), None);
    }
}
