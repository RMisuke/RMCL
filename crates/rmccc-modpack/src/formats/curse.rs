//! CurseForge modpack (`manifest.json` + `overrides/`).

use super::{file_name_from_url, finish, html_to_text, non_empty, parse_curse_loader};
use crate::archive::{safe_relative_path, Archive};
use crate::model::{Modpack, ModpackKind, PackFile, PackFileSource};
use rmccc_error::{Result, RmcccError};
use serde::Deserialize;
use serde_json::Value;

/// CurseForge manifest entry point.
pub(crate) const MANIFEST: &str = "manifest.json";

#[derive(Debug, Deserialize)]
struct CurseManifest {
    #[serde(rename = "manifestType", default)]
    manifest_type: String,
    #[serde(rename = "manifestVersion", default)]
    manifest_version: i64,
    #[serde(default)]
    name: String,
    #[serde(default)]
    version: Option<String>,
    #[serde(default)]
    author: Option<String>,
    #[serde(default)]
    overrides: Option<String>,
    minecraft: CurseMinecraft,
    #[serde(default)]
    files: Option<Vec<CurseFile>>,
}

#[derive(Debug, Deserialize)]
struct CurseMinecraft {
    #[serde(default)]
    version: String,
    #[serde(rename = "modLoaders", default)]
    mod_loaders: Option<Vec<CurseModLoader>>,
}

#[derive(Debug, Deserialize)]
struct CurseModLoader {
    #[serde(default)]
    id: String,
    #[serde(default)]
    primary: bool,
}

#[derive(Debug, Deserialize)]
struct CurseFile {
    #[serde(rename = "projectID")]
    project_id: u64,
    #[serde(rename = "fileID")]
    file_id: u64,
    #[serde(rename = "fileName", default)]
    file_name: Option<String>,
    #[serde(default)]
    url: Option<String>,
    #[serde(default = "default_required")]
    required: bool,
}

fn default_required() -> bool {
    true
}

/// Detect and parse a CurseForge manifest, if present.
pub(crate) fn detect(archive: &mut Archive) -> Result<Option<Modpack>> {
    let Some(text) = archive.read_text_opt(MANIFEST) else {
        return Ok(None);
    };
    let value: Value = match serde_json::from_str(&text) {
        Ok(value) => value,
        Err(_) => return Ok(None),
    };
    // MCBBS manifests also use `manifestType: minecraftModpack`; the caller
    // tries MCBBS first, so a `minecraft` object is the Curse discriminator.
    if value.get("minecraft").is_none() {
        return Ok(None);
    }
    let manifest: CurseManifest = match serde_json::from_value(value.clone()) {
        Ok(manifest) => manifest,
        Err(e) => {
            return Err(RmcccError::Validation(format!(
                "invalid CurseForge manifest.json: {e}"
            )))
        }
    };
    if !manifest.manifest_type.is_empty()
        && !manifest
            .manifest_type
            .eq_ignore_ascii_case("minecraftModpack")
    {
        return Ok(None);
    }
    let _ = (
        manifest.manifest_version,
        non_empty(manifest.version.clone()),
    );

    let game_version = manifest.minecraft.version.trim().to_string();
    if game_version.is_empty() {
        return Err(RmcccError::Validation(
            "CurseForge manifest is missing minecraft.version".to_string(),
        ));
    }

    let mut warnings = Vec::new();
    let mut loaders = Vec::new();
    for raw in manifest
        .minecraft
        .mod_loaders
        .as_deref()
        .unwrap_or_default()
    {
        if raw.id.trim().is_empty() {
            continue;
        }
        match parse_curse_loader(&raw.id, raw.primary, &game_version) {
            Some(loader) => loaders.push(loader),
            None => warnings.push(format!("unsupported CurseForge loader: {}", raw.id)),
        }
    }

    let manifest_files = manifest.files.unwrap_or_default();
    let mut files = Vec::with_capacity(manifest_files.len());
    for file in manifest_files {
        if file.project_id == 0 || file.file_id == 0 {
            warnings.push("CurseForge manifest contains a file with missing IDs".to_string());
            continue;
        }
        let url = non_empty(file.url);
        let file_name =
            non_empty(file.file_name).or_else(|| url.as_deref().and_then(file_name_from_url));
        let inferred_path = file_name.as_ref().map(|name| format!("mods/{name}"));
        let fallback_url = file_name
            .as_ref()
            .map_or_else(Vec::new, |name| vec![edge_url(file.file_id, name)]);
        let urls = if let Some(url) = url {
            let mut urls = vec![url];
            urls.extend(fallback_url);
            urls
        } else {
            fallback_url
        };
        files.push(PackFile {
            path: inferred_path,
            urls,
            sha1: None,
            size: None,
            required: file.required,
            origin: format!("curse:{}/{}", file.project_id, file.file_id),
            source: PackFileSource::Curse {
                project_id: file.project_id,
                file_id: file.file_id,
                file_name,
            },
        });
    }

    let overrides = manifest
        .overrides
        .as_deref()
        .and_then(safe_relative_path)
        .unwrap_or_default();

    let description = archive
        .read_text_opt("modlist.html")
        .map(|html| html_to_text(&html))
        .filter(|text| !text.is_empty())
        .unwrap_or_else(|| "No description".to_string());

    let metadata = vec![crate::model::MetadataFile {
        path: MANIFEST.to_string(),
        content: text,
    }];

    let pack = Modpack {
        kind: ModpackKind::Curse,
        name: manifest.name,
        version: non_empty(manifest.version),
        author: non_empty(manifest.author),
        description,
        game_version: Some(game_version),
        loaders,
        files,
        override_dirs: if overrides.is_empty() {
            Vec::new()
        } else {
            vec![overrides]
        },
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

/// CurseForge edge CDN URL for a file id + filename.
pub(crate) fn edge_url(file_id: u64, file_name: &str) -> String {
    format!(
        "https://edge.forgecdn.net/files/{}/{}/{}",
        file_id / 1000,
        file_id % 1000,
        file_name
    )
}

/// Classify a CurseForge `classId` into a run-directory folder.
pub(crate) fn folder_for_class(class_id: Option<u32>) -> &'static str {
    match class_id {
        Some(12) | Some(6945) => "resourcepacks",
        Some(6552) => "shaderpacks",
        _ => "mods",
    }
}
