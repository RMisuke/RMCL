//! MCBBS modpack (`mcbbs.packmeta` or MCBBS-flavoured `manifest.json`).

use super::{file_name_from_url, finish, non_empty, normalize_loader_version};
use crate::archive::{safe_relative_path, Archive};
use crate::formats::curse::edge_url;
use crate::model::{
    LoaderKind, MemorySettings, Modpack, ModpackKind, PackFile, PackFileSource, PackLoader,
    PackSettings,
};
use rmccc_error::{Result, RmcccError};
use serde::Deserialize;
use serde_json::Value;

const PACKMETA: &str = "mcbbs.packmeta";
const MANIFEST: &str = "manifest.json";

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct McbbsManifest {
    #[serde(default)]
    manifest_type: String,
    #[serde(default)]
    manifest_version: i64,
    #[serde(default)]
    name: String,
    #[serde(default)]
    version: Option<String>,
    #[serde(default)]
    author: Option<String>,
    #[serde(default)]
    description: String,
    #[serde(default)]
    file_api: Option<String>,
    #[serde(default)]
    addons: Vec<Addon>,
    #[serde(default)]
    libraries: Vec<rmccc_version::Library>,
    #[serde(default)]
    files: Vec<McbbsFile>,
    #[serde(default)]
    settings: McbbsSettings,
    #[serde(default)]
    launch_info: Option<LaunchInfo>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct McbbsSettings {
    #[serde(default)]
    install_mods: bool,
    #[serde(default, rename = "install_resourcepack")]
    _install_resourcepack: bool,
}

#[derive(Debug, Deserialize)]
struct Addon {
    #[serde(default)]
    id: String,
    #[serde(default)]
    version: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LaunchInfo {
    #[serde(default)]
    min_memory: u32,
    #[serde(default)]
    _support_java: Vec<u32>,
    #[serde(default)]
    launch_argument: Vec<String>,
    #[serde(default)]
    java_argument: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
enum McbbsFile {
    #[serde(rename_all = "camelCase")]
    Addon {
        #[serde(default, rename = "force")]
        _force: bool,
        path: String,
        #[serde(default)]
        hash: Option<String>,
    },
    #[serde(rename_all = "camelCase")]
    Curse {
        #[serde(default, rename = "force")]
        _force: bool,
        #[serde(rename = "projectID")]
        project_id: u64,
        #[serde(rename = "fileID")]
        file_id: u64,
        #[serde(default)]
        file_name: Option<String>,
        #[serde(default)]
        url: Option<String>,
    },
}

/// Detect and parse an MCBBS pack, if present.
pub(crate) fn detect(archive: &mut Archive) -> Result<Option<Modpack>> {
    let (entry, text) = if let Some(text) = archive.read_text_opt(PACKMETA) {
        (PACKMETA, text)
    } else if let Some(text) = archive.read_text_opt(MANIFEST) {
        let value: Value = match serde_json::from_str(&text) {
            Ok(value) => value,
            Err(_) => return Ok(None),
        };
        // Curse manifests have `minecraft`; MCBBS manifests have `addons`.
        if value.get("addons").is_none() {
            return Ok(None);
        }
        (MANIFEST, text)
    } else {
        return Ok(None);
    };

    let value: Value = match serde_json::from_str(&text) {
        Ok(value) => value,
        Err(e) => {
            return Err(RmcccError::Validation(format!(
                "invalid MCBBS manifest: {e}"
            )))
        }
    };
    let manifest: McbbsManifest = serde_json::from_value(value.clone())
        .map_err(|e| RmcccError::Validation(format!("invalid MCBBS manifest ({entry}): {e}")))?;
    if !manifest.manifest_type.is_empty()
        && !manifest
            .manifest_type
            .eq_ignore_ascii_case("minecraftModpack")
    {
        return Ok(None);
    }
    let _ = (manifest.manifest_version, manifest.settings.install_mods);

    let game_version = manifest
        .addons
        .iter()
        .find(|addon| matches!(addon.id.as_str(), "game" | "net.minecraft"))
        .map(|addon| addon.version.trim().to_string())
        .filter(|version| !version.is_empty())
        .ok_or_else(|| {
            RmcccError::Validation("MCBBS manifest is missing the Minecraft addon".to_string())
        })?;

    let mut warnings = Vec::new();
    let mut loaders = Vec::new();
    for addon in &manifest.addons {
        if addon.id.is_empty() || matches!(addon.id.as_str(), "game" | "net.minecraft") {
            continue;
        }
        let Some(kind) = LoaderKind::from_manifest_id(&addon.id) else {
            continue;
        };
        let version = normalize_loader_version(kind, &addon.version, &game_version);
        if version.is_empty() {
            warnings.push(format!("MCBBS addon {} has no version", addon.id));
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
        match file {
            McbbsFile::Addon { path, hash, .. } => {
                let Some(path) = safe_relative_path(path) else {
                    warnings.push(format!("skipped unsafe MCBBS path: {path}"));
                    continue;
                };
                if let Some(api) = &file_api {
                    files.push(PackFile {
                        path: Some(path.clone()),
                        urls: vec![format!("{api}/overrides/{path}")],
                        sha1: hash.clone().filter(|hash| !hash.is_empty()),
                        size: None,
                        required: false,
                        origin: format!("mcbbs-fileapi:{path}"),
                        source: PackFileSource::FileApi {
                            file_api: api.clone(),
                            hash: hash.clone(),
                        },
                    });
                }
                // Otherwise the file is expected inside `overrides/` and is
                // covered by the override extraction + hash scan.
            }
            McbbsFile::Curse {
                project_id,
                file_id,
                file_name,
                url,
                ..
            } => {
                if *project_id == 0 || *file_id == 0 {
                    warnings
                        .push("MCBBS manifest contains a curse file with missing IDs".to_string());
                    continue;
                }
                let mut urls = Vec::new();
                if let Some(url) = non_empty(url.clone()) {
                    urls.push(url);
                }
                let file_name = non_empty(file_name.clone())
                    .or_else(|| urls.first().and_then(|url| file_name_from_url(url)));
                let inferred = file_name.as_ref().map(|name| format!("mods/{name}"));
                if let Some(name) = &file_name {
                    urls.push(edge_url(*file_id, name));
                }
                files.push(PackFile {
                    path: inferred,
                    urls,
                    sha1: None,
                    size: None,
                    required: true,
                    origin: format!("mcbbs-curse:{project_id}/{file_id}"),
                    source: PackFileSource::Curse {
                        project_id: *project_id,
                        file_id: *file_id,
                        file_name,
                    },
                });
            }
        }
    }

    let mut settings = PackSettings::default();
    if let Some(info) = &manifest.launch_info {
        if info.min_memory > 0 {
            settings.memory = Some(MemorySettings {
                min_mb: None,
                max_mb: Some(info.min_memory),
                perm_gen_mb: None,
                require_at_least: true,
            });
        }
        if !info.java_argument.is_empty() {
            settings.jvm_args = Some(info.java_argument.clone());
        }
        settings.game_args = info.launch_argument.clone();
    }

    let metadata = vec![crate::model::MetadataFile {
        path: if entry == PACKMETA {
            PACKMETA.to_string()
        } else {
            MANIFEST.to_string()
        },
        content: text,
    }];

    let pack = Modpack {
        kind: ModpackKind::Mcbbs,
        name: manifest.name,
        version: non_empty(manifest.version),
        author: non_empty(manifest.author),
        description: manifest.description,
        game_version: Some(game_version),
        loaders,
        files,
        override_dirs: vec!["overrides".to_string()],
        settings,
        extra_libraries: manifest.libraries,
        metadata_files: metadata,
        icon: None,
        raw_manifest: value,
        archive: std::path::PathBuf::new(),
        manual_root: None,
        warnings,
    };
    Ok(Some(finish(pack, archive)?))
}
