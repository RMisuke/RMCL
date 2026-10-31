//! HMCL-compatible Minecraft modpack support.
//!
//! This crate owns **format detection and parsing**, **archive extraction**,
//! **CurseForge metadata resolution**, and the **`modpack.cfg` contract**.
//! Orchestration (installing the game, loaders, settings, and downloads) lives
//! in `rmccc-core::modpack`, mirroring the split between HMCL's
//! `ModpackProvider` and its install `Task`s.

pub mod archive;
pub mod curse_api;
pub mod extract;
pub mod formats;
pub mod model;

pub use archive::{safe_join, safe_relative_path, Archive};
pub use curse_api::{CurseApi, CurseFileInfo};
pub use extract::{extract_overrides, extract_subdirectory, file_sha1, verify_files};
pub use formats::{is_modpack_extension, parse_archive, read_modpack};
pub use model::{
    LoaderKind, MemorySettings, MetadataFile, Modpack, ModpackConfiguration, ModpackInfo,
    ModpackKind, OverrideEntry, PackFile, PackFileSource, PackIcon, PackLoader, PackSettings,
    WindowSettings,
};

use crate::formats::curse::folder_for_class;
use rmccc_download::provider::DownloadProvider;
use rmccc_error::Result;
use std::collections::{HashMap, HashSet};
use std::path::Path;

/// Icon filename used inside an instance (`versions/{id}/icon.png`).
pub const INSTANCE_ICON: &str = "icon.png";

/// Inspect an archive without touching the network or disk.
pub fn inspect_modpack(path: &Path) -> Result<ModpackInfo> {
    let pack = read_modpack(path)?;
    Ok(ModpackInfo::from(&pack))
}

/// Resolve CurseForge file names/URLs and inject mirror candidates.
///
/// Returns non-fatal warnings; download failures are reported by the core
/// installer (HMCL treats a partially completed Curse pack as tolerable and
/// still keeps the instance).
pub async fn resolve_files(
    pack: &mut Modpack,
    client: &reqwest::Client,
    provider: &dyn DownloadProvider,
    curse_api_key: Option<String>,
) -> Vec<String> {
    let mut warnings = Vec::new();
    if pack
        .files
        .iter()
        .any(|file| matches!(file.source, PackFileSource::Curse { .. }))
    {
        let api = CurseApi::new(client.clone(), curse_api_key);
        resolve_curse_files(pack, &api, &mut warnings).await;
    }

    for file in &mut pack.files {
        if file.urls.is_empty() {
            continue;
        }
        let mut candidates = Vec::new();
        for url in &file.urls {
            for candidate in provider.inject_url_with_candidates(url) {
                if !candidates.contains(&candidate) {
                    candidates.push(candidate);
                }
            }
        }
        file.urls = candidates;
    }
    warnings
}

async fn resolve_curse_files(pack: &mut Modpack, api: &CurseApi, warnings: &mut Vec<String>) {
    // 1. Resolve class ids once per project (resourcepack/shader folder).
    let project_ids: Vec<u64> = {
        let mut seen = HashSet::new();
        pack.files
            .iter()
            .filter_map(|file| match &file.source {
                PackFileSource::Curse { project_id, .. } => {
                    seen.insert(*project_id).then_some(*project_id)
                }
                _ => None,
            })
            .collect()
    };
    let class_ids = fetch_class_ids(api, &project_ids).await;

    // 2. Resolve metadata for files that do not carry a filename.
    let missing: Vec<(usize, u64, u64)> = pack
        .files
        .iter()
        .enumerate()
        .filter_map(|(index, file)| match &file.source {
            PackFileSource::Curse {
                project_id,
                file_id,
                file_name,
            } if file_name.is_none() => Some((index, *project_id, *file_id)),
            _ => None,
        })
        .collect();
    let infos = fetch_file_infos(api, &missing).await;

    for (index, result) in &infos {
        if let Err(error) = result {
            warnings.push(format!(
                "CurseForge file metadata unavailable for {}: {error}",
                pack.files[*index].origin
            ));
        }
    }

    // 3. Apply metadata to paths/URLs.
    for (index, file) in pack.files.iter_mut().enumerate() {
        let PackFileSource::Curse {
            project_id,
            file_id,
            file_name,
        } = &mut file.source
        else {
            continue;
        };
        let info = infos
            .iter()
            .find(|(entry_index, _)| *entry_index == index)
            .and_then(|(_, result)| result.as_ref().ok());
        let name = info
            .map(|info| info.file_name.clone())
            .or_else(|| file_name.clone());
        if let Some(name) = &name {
            *file_name = Some(name.clone());
            let folder = folder_for_class(class_ids.get(project_id).copied().flatten());
            file.path = Some(format!("{folder}/{name}"));
        }
        if let Some(info) = info {
            if !info.urls.is_empty() {
                // Keep the manifest URL (if any) first; the API URL is the
                // canonical fallback when the manifest only had an ID.
                for url in info.urls.iter().rev() {
                    if !file.urls.contains(url) {
                        file.urls.insert(0, url.clone());
                    }
                }
            }
            if file.sha1.is_none() {
                file.sha1 = info.sha1.clone();
            }
            if file.size.is_none() {
                file.size = info.size;
            }
        }
        let _ = file_id;
    }

    // Keep `manifest.json` / `modpack.cfg` in sync with resolved names, the
    // same way HMCL's CurseCompletionTask rewrites the manifest.
    if let Some(files) = pack
        .raw_manifest
        .get_mut("files")
        .and_then(serde_json::Value::as_array_mut)
    {
        for (index, file) in pack.files.iter().enumerate() {
            let Some(entry) = files.get_mut(index) else {
                continue;
            };
            if let Some(name) = file
                .path
                .as_deref()
                .and_then(|path| path.rsplit('/').next())
            {
                entry["fileName"] = serde_json::Value::String(name.to_string());
            }
            if let Some(url) = file.urls.first() {
                entry["url"] = serde_json::Value::String(url.clone());
            }
        }
    }
    for metadata in &mut pack.metadata_files {
        if metadata.path == "manifest.json" {
            if let Ok(content) = serde_json::to_string_pretty(&pack.raw_manifest) {
                metadata.content = content;
            }
        }
    }
}

async fn fetch_class_ids(api: &CurseApi, project_ids: &[u64]) -> HashMap<u64, Option<u32>> {
    let mut out = HashMap::new();
    if project_ids.is_empty() {
        return out;
    }
    let semaphore = std::sync::Arc::new(tokio::sync::Semaphore::new(8));
    let mut set = tokio::task::JoinSet::new();
    for project_id in project_ids {
        let api = api.clone();
        let semaphore = semaphore.clone();
        let project_id = *project_id;
        set.spawn(async move {
            let _permit = semaphore.acquire_owned().await.ok();
            let class_id = api.class_id(project_id).await.ok().flatten();
            (project_id, class_id)
        });
    }
    while let Some(joined) = set.join_next().await {
        if let Ok((project_id, class_id)) = joined {
            out.insert(project_id, class_id);
        }
    }
    out
}

#[derive(Debug, Clone)]
struct ResolvedInfo {
    file_name: String,
    urls: Vec<String>,
    sha1: Option<String>,
    size: Option<u64>,
}

async fn fetch_file_infos(
    api: &CurseApi,
    missing: &[(usize, u64, u64)],
) -> Vec<(usize, Result<ResolvedInfo>)> {
    let mut out = Vec::new();
    if missing.is_empty() {
        return out;
    }
    let semaphore = std::sync::Arc::new(tokio::sync::Semaphore::new(8));
    let mut set = tokio::task::JoinSet::new();
    for (index, project_id, file_id) in missing {
        let api = api.clone();
        let semaphore = semaphore.clone();
        let index = *index;
        let project_id = *project_id;
        let file_id = *file_id;
        set.spawn(async move {
            let _permit = semaphore.acquire_owned().await.ok();
            let result = api
                .file(project_id, file_id)
                .await
                .map(|info| ResolvedInfo {
                    file_name: info.file_name,
                    urls: info.download_url.into_iter().collect(),
                    sha1: info.sha1,
                    size: info.size,
                });
            (index, result)
        });
    }
    while let Some(joined) = set.join_next().await {
        if let Ok(result) = joined {
            out.push(result);
        }
    }
    out.sort_by_key(|(index, _)| *index);
    out
}

/// Write the provider manifest(s) and `modpack.cfg` into the instance root.
pub fn write_modpack_metadata(
    run_dir: &Path,
    pack: &Modpack,
    overrides: Vec<OverrideEntry>,
) -> Result<()> {
    for metadata in &pack.metadata_files {
        let Some(path) = archive::safe_relative_path(&metadata.path) else {
            continue;
        };
        if path.is_empty() {
            continue;
        }
        let Some(target) = archive::safe_join(run_dir, &path).ok() else {
            continue;
        };
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(target, metadata.content.as_bytes())?;
    }
    let config = ModpackConfiguration::new(pack, overrides);
    let content = serde_json::to_string_pretty(&config)?;
    std::fs::write(run_dir.join("modpack.cfg"), content)?;
    Ok(())
}

/// Read an existing `modpack.cfg` (used by future update support).
pub fn read_modpack_configuration(instance_root: &Path) -> Option<ModpackConfiguration> {
    let text = std::fs::read_to_string(instance_root.join("modpack.cfg")).ok()?;
    serde_json::from_str(&text).ok()
}

/// Default instance id/name derived from the pack name.
///
/// The id is also a directory name: strip separators and trailing dots/spaces,
/// then truncate on a UTF-8 boundary. Returns a name safe to pass to
/// `InstanceService`.
pub fn suggest_instance_id(raw: &str, fallback: &str) -> String {
    let mut cleaned: String = raw
        .trim()
        .chars()
        .map(|ch| match ch {
            '/' | '\\' | ':' | '?' | '*' | '"' | '<' | '>' | '|' => '-',
            ch if ch.is_control() => '-',
            ch => ch,
        })
        .collect();
    cleaned = cleaned.trim().trim_end_matches(['.', ' ']).to_string();
    if cleaned.is_empty() {
        cleaned = fallback.trim().to_string();
    }
    if cleaned.is_empty() {
        cleaned = "Modpack".to_string();
    }
    // 128 bytes max (instance id contract).
    if cleaned.len() > 128 {
        let mut end = 128;
        while !cleaned.is_char_boundary(end) {
            end -= 1;
        }
        cleaned.truncate(end);
        cleaned = cleaned.trim_end_matches(['.', ' ']).to_string();
    }
    const RESERVED: &[&str] = &[
        "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8",
        "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
    ];
    let stem = cleaned
        .split('.')
        .next()
        .unwrap_or(&cleaned)
        .to_ascii_uppercase();
    if RESERVED.contains(&stem.as_str()) || cleaned == "." || cleaned == ".." {
        cleaned.push_str("-pack");
    }
    cleaned
}

#[cfg(test)]
mod tests;
