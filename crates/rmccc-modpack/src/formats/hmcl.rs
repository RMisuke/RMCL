//! HMCL's own modpack export (`modpack.json` + `minecraft/pack.json`).

use super::{finish, non_empty, normalize_loader_version};
use crate::archive::Archive;
use crate::model::{LoaderKind, Modpack, ModpackKind, PackLoader};
use rmccc_error::{Result, RmcccError};
use serde::Deserialize;
use serde_json::Value;

const MODPACK_JSON: &str = "modpack.json";
const PACK_JSON: &str = "minecraft/pack.json";

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct HmclManifest {
    #[serde(default)]
    name: String,
    #[serde(default)]
    author: Option<String>,
    #[serde(default)]
    version: Option<String>,
    #[serde(default)]
    game_version: Option<String>,
    #[serde(default)]
    description: String,
}

/// Detect and parse an HMCL export, if present.
pub(crate) fn detect(archive: &mut Archive) -> Result<Option<Modpack>> {
    let Some(manifest_text) = archive.read_text_opt(MODPACK_JSON) else {
        return Ok(None);
    };
    let Some(pack_text) = archive.read_text_opt(PACK_JSON) else {
        return Ok(None);
    };
    let manifest_value: Value = match serde_json::from_str(&manifest_text) {
        Ok(value) => value,
        Err(_) => return Ok(None),
    };
    let manifest: HmclManifest = serde_json::from_value(manifest_value.clone())
        .map_err(|e| RmcccError::Validation(format!("invalid HMCL modpack.json: {e}")))?;

    let pack_value: Value = serde_json::from_str(&pack_text)
        .map_err(|e| RmcccError::Validation(format!("invalid minecraft/pack.json: {e}")))?;

    // Prefer the explicit gameVersion field; HMCL itself falls back to the
    // exported jar id (or the pack version as a last resort).
    let mut game_version = non_empty(manifest.game_version.clone());
    if game_version.is_none() {
        game_version = pack_value
            .get("jar")
            .and_then(|jar| jar.get("id"))
            .and_then(Value::as_str)
            .map(str::to_string);
    }
    if game_version.is_none() {
        game_version = non_empty(manifest.version.clone());
    }
    let game_version = game_version.ok_or_else(|| {
        RmcccError::Validation("HMCL pack does not declare a Minecraft version".to_string())
    })?;

    let mut warnings = Vec::new();
    let loaders = detect_loaders(&pack_value, &game_version, &mut warnings);

    let metadata = vec![crate::model::MetadataFile {
        path: MODPACK_JSON.to_string(),
        content: manifest_text,
    }];

    let mut icon = None;
    for name in ["icon.png", "minecraft/icon.png"] {
        if let Some(entry) = archive.entry(name) {
            if !entry.is_dir {
                if let Ok(bytes) = archive.read_index(entry.index, 8 * 1024 * 1024) {
                    icon = Some(crate::model::PackIcon {
                        ext: "png".to_string(),
                        bytes,
                    });
                    break;
                }
            }
        }
    }

    let pack = Modpack {
        kind: ModpackKind::Hmcl,
        name: manifest.name,
        version: non_empty(manifest.version),
        author: non_empty(manifest.author),
        description: manifest.description,
        game_version: Some(game_version),
        loaders,
        files: Vec::new(),
        override_dirs: vec!["minecraft".to_string()],
        settings: Default::default(),
        extra_libraries: Vec::new(),
        metadata_files: metadata,
        icon,
        raw_manifest: manifest_value,
        archive: std::path::PathBuf::new(),
        manual_root: None,
        warnings,
    };
    Ok(Some(finish(pack, archive)?))
}

/// Detect loaders from the exported pack manifest (HMCL `LibraryAnalyzer`).
fn detect_loaders(pack: &Value, game_version: &str, warnings: &mut Vec<String>) -> Vec<PackLoader> {
    let mut found: Vec<PackLoader> = Vec::new();
    let mut add = |kind: LoaderKind, raw: &str| {
        let version = normalize_loader_version(kind, raw, game_version);
        if !version.is_empty()
            && !found
                .iter()
                .any(|loader| loader.kind == kind && loader.version == version)
        {
            found.push(PackLoader::new(kind, version, false));
        }
    };

    // Explicit patches are the most reliable source.
    if let Some(patches) = pack.get("patches").and_then(Value::as_array) {
        for patch in patches {
            let Some(id) = patch.get("id").and_then(Value::as_str) else {
                continue;
            };
            let Some(kind) = LoaderKind::from_manifest_id(id) else {
                continue;
            };
            let version = patch
                .get("version")
                .and_then(Value::as_str)
                .unwrap_or_default();
            add(kind, version);
        }
    }

    // Fall back to library coordinates (externally produced packs).
    if let Some(libraries) = pack.get("libraries").and_then(Value::as_array) {
        for library in libraries {
            let Some(name) = library.get("name").and_then(Value::as_str) else {
                continue;
            };
            let parts: Vec<&str> = name.split(':').collect();
            if parts.len() < 3 {
                continue;
            }
            let (group, artifact, version) = (parts[0], parts[1], parts[2]);
            let kind = if group == "net.neoforged.fancymodloader"
                && (artifact == "core" || artifact == "loader")
            {
                Some(LoaderKind::NeoForge)
            } else if group == "net.minecraftforge"
                && (artifact == "forge" || artifact == "fmlloader")
            {
                Some(LoaderKind::Forge)
            } else if group == "net.fabricmc" && artifact == "fabric-loader" {
                Some(LoaderKind::Fabric)
            } else if group == "org.quiltmc" && artifact == "quilt-loader" {
                Some(LoaderKind::Quilt)
            } else if group == "com.mumfrey" && artifact == "liteloader" {
                Some(LoaderKind::LiteLoader)
            } else if group.starts_with("net.optifine") || artifact == "optifine" {
                Some(LoaderKind::OptiFine)
            } else {
                None
            };
            if let Some(kind) = kind {
                let version = if matches!(kind, LoaderKind::Forge | LoaderKind::NeoForge) {
                    version
                        .split_once('-')
                        .map(|(_, rest)| rest)
                        .unwrap_or(version)
                } else {
                    version
                };
                add(kind, version);
            }
        }
    }

    // NeoForge/Forge expose their real version through launch arguments.
    if let Some(args) = pack
        .get("arguments")
        .and_then(|args| args.get("game"))
        .and_then(Value::as_array)
    {
        for window in args.windows(2) {
            let flag = window[0].as_str().unwrap_or_default();
            let value = window[1].as_str().unwrap_or_default();
            match flag {
                "--fml.neoForgeVersion" => add(LoaderKind::NeoForge, value),
                "--fml.forgeVersion" => add(LoaderKind::Forge, value),
                _ => {}
            }
        }
    }

    if found.len() > 1 {
        warnings.push(
            "This HMCL pack declares several loaders; only the primary one is installed"
                .to_string(),
        );
    }
    found
}
