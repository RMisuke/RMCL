//! Modrinth modpack (`modrinth.index.json` + `client-overrides/` + `overrides/`).

use super::{finish, non_empty};
use crate::archive::{safe_relative_path, Archive};
use crate::model::{LoaderKind, Modpack, ModpackKind, PackFile, PackFileSource, PackLoader};
use rmccc_error::{Result, RmcccError};
use serde::Deserialize;
use serde_json::Value;

/// Modrinth index entry point.
pub(crate) const MANIFEST: &str = "modrinth.index.json";

#[derive(Debug, Deserialize)]
struct ModrinthManifest {
    #[serde(default)]
    game: String,
    #[serde(rename = "formatVersion", default)]
    format_version: i64,
    #[serde(rename = "versionId", default)]
    version_id: String,
    #[serde(default)]
    name: String,
    #[serde(default)]
    summary: Option<String>,
    #[serde(default)]
    files: Vec<ModrinthFile>,
    #[serde(default)]
    dependencies: std::collections::HashMap<String, String>,
}

#[derive(Debug, Deserialize)]
struct ModrinthFile {
    #[serde(default)]
    path: String,
    #[serde(default)]
    hashes: std::collections::HashMap<String, String>,
    #[serde(default)]
    env: Option<std::collections::HashMap<String, String>>,
    #[serde(default)]
    downloads: Vec<String>,
    #[serde(rename = "fileSize", default)]
    file_size: Option<u64>,
}

/// Detect and parse a Modrinth pack, if present.
pub(crate) fn detect(archive: &mut Archive) -> Result<Option<Modpack>> {
    let Some(text) = archive.read_text_opt(MANIFEST) else {
        return Ok(None);
    };
    let value: Value = match serde_json::from_str(&text) {
        Ok(value) => value,
        Err(_) => return Ok(None),
    };
    let manifest: ModrinthManifest = match serde_json::from_value(value.clone()) {
        Ok(manifest) => manifest,
        Err(e) => {
            return Err(RmcccError::Validation(format!(
                "invalid modrinth.index.json: {e}"
            )))
        }
    };
    let _ = (manifest.game, manifest.format_version);
    let game_version = manifest
        .dependencies
        .get("minecraft")
        .cloned()
        .ok_or_else(|| {
            RmcccError::Validation("Modrinth pack is missing dependencies.minecraft".to_string())
        })?;

    let mut loaders = Vec::new();
    for (key, value) in &manifest.dependencies {
        let kind = match key.as_str() {
            "minecraft" => continue,
            "forge" => LoaderKind::Forge,
            "neoforge" | "neo-forge" => LoaderKind::NeoForge,
            "fabric-loader" => LoaderKind::Fabric,
            "quilt-loader" => LoaderKind::Quilt,
            other => {
                // HMCL aborts on unknown loaders; be tolerant and warn instead
                // so a future dependency key does not brick an otherwise
                // installable pack.
                tracing::warn!(dependency = %other, "Ignoring unsupported Modrinth dependency");
                continue;
            }
        };
        if !value.trim().is_empty() {
            loaders.push(PackLoader::new(kind, value.trim(), false));
        }
    }

    let mut warnings = Vec::new();
    let mut files = Vec::with_capacity(manifest.files.len());
    for file in manifest.files {
        if file
            .env
            .as_ref()
            .and_then(|env| env.get("client"))
            .is_some_and(|client| client.eq_ignore_ascii_case("unsupported"))
        {
            continue;
        }
        let Some(path) = safe_relative_path(&file.path) else {
            warnings.push(format!("skipped unsafe Modrinth path: {}", file.path));
            continue;
        };
        if path.is_empty() {
            continue;
        }
        let urls: Vec<String> = file
            .downloads
            .into_iter()
            .filter(|url| url.starts_with("https://") || url.starts_with("http://"))
            .collect();
        let sha1 = file
            .hashes
            .get("sha1")
            .cloned()
            .filter(|hash| !hash.is_empty());
        files.push(PackFile {
            path: Some(path.clone()),
            urls,
            sha1,
            size: file.file_size,
            required: true,
            origin: path,
            source: PackFileSource::Http,
        });
    }

    let mut override_dirs = Vec::new();
    for dir in ["client-overrides", "overrides"] {
        if archive
            .entries()
            .iter()
            .any(|entry| entry.name == dir || entry.name.starts_with(&format!("{dir}/")))
        {
            override_dirs.push(dir.to_string());
        }
    }

    let metadata = vec![crate::model::MetadataFile {
        path: MANIFEST.to_string(),
        content: text,
    }];

    let pack = Modpack {
        kind: ModpackKind::Modrinth,
        name: manifest.name,
        version: non_empty(Some(manifest.version_id)),
        author: None,
        description: manifest.summary.unwrap_or_default(),
        game_version: Some(game_version),
        loaders,
        files,
        override_dirs,
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
