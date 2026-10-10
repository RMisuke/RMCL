//! Version completeness check & repair downloads
//! (HMCL `checkGameCompletionAsync` equivalent).
//!
//! A version is "complete" when every file the resolved launch manifest needs
//! is present on disk: client JAR, asset index + objects, platform libraries
//! and natives. [`check_completion`] scans the disk and reports what is
//! missing; [`build_completion_downloads`] turns the missing set into
//! resumable [`DownloadItem`]s for a repair/complete flow.

use rmccc_download::DownloadItem;
use rmccc_error::{Result, RmcccError};
use std::path::{Path, PathBuf};

use super::assets::AssetIndexContent;
use super::file_layout;
use super::libraries;
use super::resolved::ResolvedVersion;

/// Kind of a missing file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MissingKind {
    ClientJar,
    Library,
    Native,
    AssetIndex,
    Asset,
}

/// A missing file entry reported by [`check_completion`].
#[derive(Debug, Clone)]
pub struct MissingItem {
    pub kind: MissingKind,
    /// Human-readable description.
    pub description: String,
    /// The absolute path that is missing.
    pub path: PathBuf,
}

/// Scan an installed version against its resolved manifest and report missing files.
pub fn check_completion(game_dir: &Path, resolved: &ResolvedVersion) -> Vec<MissingItem> {
    let mut missing = Vec::new();
    let launch = &resolved.launch;

    // 1. Client JAR.
    let jar_id = launch.jar.as_deref().unwrap_or(&launch.id);
    let jar_path = file_layout::instance_jar(game_dir, jar_id);
    if !jar_path.exists() {
        missing.push(MissingItem {
            kind: MissingKind::ClientJar,
            description: format!("Client JAR for '{jar_id}'"),
            path: jar_path,
        });
    }

    // 2. Asset index.
    let asset_id = &launch.asset_index.id;
    if !asset_id.is_empty() {
        let idx = file_layout::asset_index_file(game_dir, asset_id);
        if !idx.exists() {
            missing.push(MissingItem {
                kind: MissingKind::AssetIndex,
                description: format!("Asset index '{asset_id}'"),
                path: idx,
            });
        } else if let Ok(content) = std::fs::read_to_string(&idx) {
            if let Ok(index) = serde_json::from_str::<AssetIndexContent>(&content) {
                let count = index.objects.len();
                let existing = index
                    .objects
                    .values()
                    .filter(|o| file_layout::asset_object(game_dir, &o.hash).exists())
                    .count();
                if existing < count {
                    missing.push(MissingItem {
                        kind: MissingKind::Asset,
                        description: format!("Assets {existing}/{count} present"),
                        path: idx,
                    });
                }
            }
        }
    }

    // 3. Libraries (platform-filtered) + natives.
    for lib in launch.libraries.iter().filter(|l| libraries::library_applies(l)) {
        let lib_path = lib
            .downloads
            .as_ref()
            .and_then(|d| d.artifact.as_ref())
            .and_then(|a| a.path.as_ref())
            .map(|p| file_layout::libraries_dir(game_dir).join(p))
            .unwrap_or_else(|| file_layout::library_file(game_dir, &lib.name));
        if !lib_path.exists() {
            missing.push(MissingItem {
                kind: MissingKind::Library,
                description: format!("Library {}", lib.name),
                path: lib_path,
            });
        }

        if let Some(dl) = &lib.downloads {
            if let Some(classifiers) = &dl.classifiers {
                for (classifier, entry) in classifiers {
                    if libraries::is_current_platform_classifier(classifier) {
                        let native_path = entry
                            .path
                            .as_ref()
                            .map(|p| file_layout::libraries_dir(game_dir).join(p))
                            .unwrap_or_else(|| {
                                file_layout::library_file(
                                    game_dir,
                                    &format!("{}:{}", lib.name, classifier),
                                )
                            });
                        if !native_path.exists() {
                            missing.push(MissingItem {
                                kind: MissingKind::Native,
                                description: format!("Native library {}", classifier),
                                path: native_path,
                            });
                        }
                    }
                }
            }
        }
    }

    missing
}

/// Build download items for every missing file of an installed version.
///
/// Asset objects are resolved from the local index when present (offline-safe);
/// a missing index triggers a network fetch of the index itself.
pub async fn build_completion_downloads(
    client: &reqwest::Client,
    game_dir: &Path,
    resolved: &ResolvedVersion,
) -> Result<Vec<DownloadItem>> {
    let launch = &resolved.launch;
    let mut items = Vec::new();

    // Client JAR.
    let jar_id = launch.jar.as_deref().unwrap_or(&launch.id);
    let jar_path = file_layout::instance_jar(game_dir, jar_id);
    if !jar_path.exists() {
        if let Some(client_entry) = launch.downloads.get("client") {
            items.push(DownloadItem {
                url: client_entry.url.clone(),
                dest: jar_path,
                expected_sha1: Some(client_entry.sha1.clone()),
                size: Some(client_entry.size),
                use_cache: true,
                candidates: Vec::new(),
            });
        }
    }

    // Asset index + objects.
    if !launch.asset_index.id.is_empty() {
        let idx_path = file_layout::asset_index_file(game_dir, &launch.asset_index.id);
        let index: Option<AssetIndexContent> = if idx_path.exists() {
            std::fs::read_to_string(&idx_path)
                .ok()
                .and_then(|s| serde_json::from_str(&s).ok())
        } else {
            None
        };

        match index {
            Some(content) => {
                for obj in content.objects.values() {
                    let dest = file_layout::asset_object(game_dir, &obj.hash);
                    if dest.exists() {
                        continue;
                    }
                    let prefix = &obj.hash[..2];
                    items.push(DownloadItem {
                        url: format!(
                            "https://resources.download.minecraft.net/{prefix}/{}",
                            obj.hash
                        ),
                        dest,
                        expected_sha1: Some(obj.hash.clone()),
                        size: Some(obj.size),
                        use_cache: true,
                        candidates: Vec::new(),
                    });
                }
            }
            None => {
                // Fetch the asset index from the network.
                let resp = client
                    .get(&launch.asset_index.url)
                    .send()
                    .await
                    .map_err(|e| RmcccError::Network(e.to_string()))?;
                if !resp.status().is_success() {
                    return Err(RmcccError::HttpStatus {
                        status: resp.status().as_u16(),
                        url: launch.asset_index.url.clone(),
                    });
                }
                let body = resp.bytes().await.map_err(|e| RmcccError::Network(e.to_string()))?;
                let content: AssetIndexContent =
                    serde_json::from_slice(&body).map_err(RmcccError::Json)?;
                std::fs::create_dir_all(idx_path.parent().unwrap_or(game_dir))?;
                std::fs::write(&idx_path, &body)?;
                for obj in content.objects.values() {
                    let dest = file_layout::asset_object(game_dir, &obj.hash);
                    if dest.exists() {
                        continue;
                    }
                    let prefix = &obj.hash[..2];
                    items.push(DownloadItem {
                        url: format!(
                            "https://resources.download.minecraft.net/{prefix}/{}",
                            obj.hash
                        ),
                        dest,
                        expected_sha1: Some(obj.hash.clone()),
                        size: Some(obj.size),
                        use_cache: true,
                        candidates: Vec::new(),
                    });
                }
            }
        }
    }

    // Libraries + natives.
    for lib in launch.libraries.iter().filter(|l| libraries::library_applies(l)) {
        let lib_path = lib
            .downloads
            .as_ref()
            .and_then(|d| d.artifact.as_ref())
            .and_then(|a| a.path.as_ref())
            .map(|p| file_layout::libraries_dir(game_dir).join(p))
            .unwrap_or_else(|| file_layout::library_file(game_dir, &lib.name));
        if !lib_path.exists() {
            if let Some(artifact) = lib.downloads.as_ref().and_then(|d| d.artifact.as_ref()) {
                items.push(DownloadItem {
                    url: artifact.url.clone(),
                    dest: lib_path,
                    expected_sha1: Some(artifact.sha1.clone()),
                    size: Some(artifact.size),
                    use_cache: true,
                    candidates: Vec::new(),
                });
            } else {
                let urls = libraries::maven_urls_from_name(&lib.name);
                let primary = urls.first().cloned().unwrap_or_default();
                let candidates: Vec<String> = urls.into_iter().skip(1).collect();
                items.push(DownloadItem {
                    url: primary,
                    dest: lib_path,
                    expected_sha1: None,
                    size: None,
                    use_cache: true,
                    candidates,
                });
            }
        }

        if let Some(dl) = &lib.downloads {
            if let Some(classifiers) = &dl.classifiers {
                for (classifier, entry) in classifiers {
                    if !libraries::is_current_platform_classifier(classifier) {
                        continue;
                    }
                    let native_path = entry
                        .path
                        .as_ref()
                        .map(|p| file_layout::libraries_dir(game_dir).join(p))
                        .unwrap_or_else(|| {
                            file_layout::library_file(game_dir, &format!("{}:{}", lib.name, classifier))
                        });
                    if !native_path.exists() {
                        items.push(DownloadItem {
                            url: entry.url.clone(),
                            dest: native_path,
                            expected_sha1: Some(entry.sha1.clone()),
                            size: Some(entry.size),
                            use_cache: true,
                            candidates: Vec::new(),
                        });
                    }
                }
            }
        }
    }

    Ok(items)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::GameVersion;

    #[test]
    fn test_check_completion_reports_missing_jar() {
        let game_dir = Path::new("/tmp/rmccc-completion/game");
        let mut launch = GameVersion::default();
        launch.id = "1.20.4".to_string();
        launch.downloads.insert(
            "client".to_string(),
            crate::types::DownloadEntry {
                url: "https://example.com/client.jar".into(),
                sha1: "abc".into(),
                size: 100,
                path: None,
            },
        );
        let resolved = ResolvedVersion {
            unresolved: GameVersion::default(),
            launch: launch.clone(),
            standalone: GameVersion::default(),
        };
        let missing = check_completion(game_dir, &resolved);
        assert!(missing.iter().any(|m| m.kind == MissingKind::ClientJar));
    }
}
