//! Server modpack (`server-manifest.json`).
//!
//! Server packs describe the game/loaders via `addons` and the file list via a
//! `fileApi` base URL. HMCL treats a missing `fileApi` as "overrides only".

use super::{finish, non_empty, normalize_loader_version};
use crate::archive::{safe_relative_path, Archive};
use crate::model::{LoaderKind, Modpack, ModpackKind, PackFile, PackFileSource, PackLoader};
use rmccc_error::{Result, RmcccError};
use serde::Deserialize;
use serde_json::Value;

const MANIFEST: &str = "server-manifest.json";

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ServerManifest {
    #[serde(default)]
    name: String,
    #[serde(default)]
    author: Option<String>,
    #[serde(default)]
    version: Option<String>,
    #[serde(default)]
    description: String,
    #[serde(default)]
    file_api: Option<String>,
    #[serde(default)]
    files: Vec<ServerFile>,
    #[serde(default)]
    addons: Vec<Addon>,
}

#[derive(Debug, Deserialize)]
struct Addon {
    #[serde(default)]
    id: String,
    #[serde(default)]
    version: String,
}

#[derive(Debug, Deserialize)]
struct ServerFile {
    #[serde(default)]
    path: String,
    #[serde(default)]
    hash: Option<String>,
    #[serde(rename = "downloadURL", default)]
    download_url: Option<String>,
}

/// Detect and parse a server pack, if present.
pub(crate) fn detect(archive: &mut Archive) -> Result<Option<Modpack>> {
    let Some(text) = archive.read_text_opt(MANIFEST) else {
        return Ok(None);
    };
    let value: Value = match serde_json::from_str(&text) {
        Ok(value) => value,
        Err(_) => return Ok(None),
    };
    let manifest: ServerManifest = serde_json::from_value(value.clone())
        .map_err(|e| RmcccError::Validation(format!("invalid server-manifest.json: {e}")))?;

    let game_version = manifest
        .addons
        .iter()
        .find(|addon| matches!(addon.id.as_str(), "game" | "net.minecraft"))
        .map(|addon| addon.version.trim().to_string())
        .filter(|version| !version.is_empty())
        .ok_or_else(|| {
            RmcccError::Validation(
                "server-manifest.json is missing the Minecraft addon".to_string(),
            )
        })?;

    let mut warnings = Vec::new();
    let mut loaders = Vec::new();
    for addon in &manifest.addons {
        if addon.id.is_empty() || matches!(addon.id.as_str(), "game" | "net.minecraft") {
            continue;
        }
        let Some(kind) = LoaderKind::from_manifest_id(&addon.id) else {
            warnings.push(format!("unsupported server addon: {}", addon.id));
            continue;
        };
        let version = normalize_loader_version(kind, &addon.version, &game_version);
        if version.is_empty() {
            warnings.push(format!("server addon {} has no version", addon.id));
            continue;
        }
        loaders.push(PackLoader::new(kind, version, false));
    }

    let file_api = manifest
        .file_api
        .as_deref()
        .map(str::trim)
        .filter(|api| !api.is_empty())
        .map(|api| api.trim_end_matches('/').to_string());

    let mut files = Vec::with_capacity(manifest.files.len());
    for file in &manifest.files {
        let Some(path) = safe_relative_path(&file.path) else {
            warnings.push(format!("skipped unsafe server file path: {}", file.path));
            continue;
        };
        if path.is_empty() {
            continue;
        }
        let mut urls = Vec::new();
        if let Some(url) = non_empty(file.download_url.clone()) {
            urls.push(url);
        }
        if let Some(api) = &file_api {
            urls.push(format!("{api}/overrides/{path}"));
        }
        files.push(PackFile {
            path: Some(path.clone()),
            urls,
            sha1: file.hash.clone().filter(|hash| !hash.is_empty()),
            size: None,
            required: true,
            origin: path,
            source: PackFileSource::FileApi {
                file_api: file_api.clone().unwrap_or_default(),
                hash: file.hash.clone(),
            },
        });
    }

    let metadata = vec![crate::model::MetadataFile {
        path: MANIFEST.to_string(),
        content: text,
    }];

    let pack = Modpack {
        kind: ModpackKind::Server,
        name: manifest.name,
        version: non_empty(manifest.version),
        author: non_empty(manifest.author),
        description: manifest.description,
        game_version: Some(game_version),
        loaders,
        files,
        override_dirs: vec!["overrides".to_string()],
        settings: Default::default(),
        extra_libraries: Vec::new(),
        metadata_files: metadata,
        icon: None,
        raw_manifest: value,
        archive: std::path::PathBuf::new(),
        manual_root: None,
        warnings,
    };
    Ok(Some(finish(pack, archive)?))
}
