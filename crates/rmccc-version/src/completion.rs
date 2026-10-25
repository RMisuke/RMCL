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

/// Integrity level applied when deciding whether a file is "missing".
///
/// SHA1-hashing every file on every launch costs a full read of the game
/// directory (hundreds of MB), so the per-launch path uses [`CompletionVerify::Size`]
/// and explicit check/repair commands opt into [`CompletionVerify::Hash`].
/// The download engine independently re-verifies every replaced file before
/// publishing it, so a size-mismatch repair can never install corrupt content.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompletionVerify {
    /// The file exists. Historical behaviour, cheapest.
    Presence,
    /// Exists + expected size when the manifest provides one (one `stat` per
    /// file). Catches truncated and appended-corrupt files without reading
    /// contents — suitable for the per-launch repair path.
    Size,
    /// Size + SHA1 for every file the manifest hashes. Full integrity check
    /// for explicit "check / repair" commands.
    Hash,
}

/// Verify a file on disk against the manifest expectation.
///
/// `sha1` / `size` are skipped when `None`; a file that exists but fails the
/// requested verification counts as missing so a repair replaces it.
fn file_ok(path: &Path, sha1: Option<&str>, size: Option<u64>, verify: CompletionVerify) -> bool {
    if verify == CompletionVerify::Presence || (sha1.is_none() && size.is_none()) {
        return path.exists();
    }
    if !path.exists() {
        return false;
    }
    if let Some(expected) = size {
        match std::fs::metadata(path) {
            Ok(m) if m.len() == expected => {}
            _ => return false,
        }
    }
    if verify == CompletionVerify::Hash {
        if let Some(expected) = sha1 {
            return sha1_file_sync(path)
                .map(|actual| actual.eq_ignore_ascii_case(expected))
                .unwrap_or(false);
        }
    }
    true
}

/// Streaming SHA1 (64 KiB chunks) — full reads spiked memory on large jars.
fn sha1_file_sync(path: &Path) -> std::io::Result<String> {
    use sha1::{Digest, Sha1};
    use std::io::Read;
    let mut file = std::fs::File::open(path)?;
    let mut hasher = Sha1::new();
    let mut buf = vec![0u8; 64 * 1024];
    loop {
        let n = file.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

/// Scan an installed version against its resolved manifest and report missing files.
///
/// `verify` controls how strictly existing files are checked (see
/// [`CompletionVerify`]); a file that exists but fails verification is
/// reported as missing with a "corrupt" description so a repair replaces it.
pub fn check_completion(
    game_dir: &Path,
    resolved: &ResolvedVersion,
    platform: &rmccc_platform::PlatformDescriptor,
    verify: CompletionVerify,
) -> Vec<MissingItem> {
    let mut missing = Vec::new();
    let launch = &resolved.launch;

    // 1. Client JAR.
    let jar_id = launch.jar.as_deref().unwrap_or(&launch.id);
    let jar_path = file_layout::instance_jar(game_dir, jar_id);
    let client_entry = launch.downloads.get("client");
    if !file_ok(
        &jar_path,
        client_entry.map(|d| d.sha1.as_str()),
        client_entry.map(|d| d.size),
        verify,
    ) {
        let why = corrupt_suffix(&jar_path);
        missing.push(MissingItem {
            kind: MissingKind::ClientJar,
            description: format!("Client JAR for '{jar_id}'{why}"),
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
                // Malformed hashes are downloaded data — skip them on BOTH
                // sides of the count, otherwise "missing" would outrun repair
                // forever (repair can't download what it can't address).
                let valid: Vec<_> = index
                    .objects
                    .values()
                    .filter_map(|o| file_layout::asset_object(game_dir, &o.hash).map(|p| (o, p)))
                    .collect();
                let count = valid.len();
                let existing = valid
                    .iter()
                    .filter(|(o, path)| file_ok(path, Some(&o.hash), Some(o.size), verify))
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
    for lib in launch
        .libraries
        .iter()
        .filter(|l| libraries::library_applies(l, platform))
    {
        let artifact = lib.downloads.as_ref().and_then(|d| d.artifact.as_ref());
        let lib_path = artifact
            .and_then(|a| a.path.as_ref())
            .map(|p| file_layout::libraries_dir(game_dir).join(p))
            .unwrap_or_else(|| file_layout::library_file(game_dir, &lib.name));
        if !file_ok(
            &lib_path,
            artifact.map(|a| a.sha1.as_str()),
            artifact.map(|a| a.size),
            verify,
        ) {
            let why = corrupt_suffix(&lib_path);
            missing.push(MissingItem {
                kind: MissingKind::Library,
                description: format!("Library {}{why}", lib.name),
                path: lib_path,
            });
        }

        if let Some(dl) = &lib.downloads {
            if let Some(classifiers) = &dl.classifiers {
                for (classifier, entry) in classifiers {
                    if libraries::is_current_platform_classifier(classifier, platform) {
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
                        if !file_ok(&native_path, Some(&entry.sha1), Some(entry.size), verify) {
                            let why = corrupt_suffix(&native_path);
                            missing.push(MissingItem {
                                kind: MissingKind::Native,
                                description: format!("Native library {classifier}{why}"),
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

/// " — corrupt (size/hash mismatch)" when the file exists but failed
/// verification, so users see why a repair wants to replace it.
fn corrupt_suffix(path: &Path) -> &'static str {
    if path.exists() {
        " — corrupt (size/hash mismatch)"
    } else {
        ""
    }
}

/// Build download items for every missing file of an installed version.
///
/// Asset objects are resolved from the local index when present (offline-safe);
/// a missing index triggers a network fetch of the index itself. `verify`
/// decides whether existing files count as present (see [`CompletionVerify`]).
pub async fn build_completion_downloads(
    client: &reqwest::Client,
    game_dir: &Path,
    resolved: &ResolvedVersion,
    platform: &rmccc_platform::PlatformDescriptor,
    verify: CompletionVerify,
) -> Result<Vec<DownloadItem>> {
    let launch = &resolved.launch;
    let mut items = Vec::new();

    // Client JAR.
    let jar_id = launch.jar.as_deref().unwrap_or(&launch.id);
    let jar_path = file_layout::instance_jar(game_dir, jar_id);
    let client_entry = launch.downloads.get("client");
    if !file_ok(
        &jar_path,
        client_entry.map(|d| d.sha1.as_str()),
        client_entry.map(|d| d.size),
        verify,
    ) {
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
                push_missing_assets(&mut items, game_dir, &content, verify);
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
                let body = resp
                    .bytes()
                    .await
                    .map_err(|e| RmcccError::Network(e.to_string()))?;
                let content: AssetIndexContent =
                    serde_json::from_slice(&body).map_err(RmcccError::Json)?;
                std::fs::create_dir_all(idx_path.parent().unwrap_or(game_dir))?;
                std::fs::write(&idx_path, &body)?;
                push_missing_assets(&mut items, game_dir, &content, verify);
            }
        }
    }

    // Libraries + natives.
    for lib in launch
        .libraries
        .iter()
        .filter(|l| libraries::library_applies(l, platform))
    {
        let artifact = lib.downloads.as_ref().and_then(|d| d.artifact.as_ref());
        let lib_path = artifact
            .and_then(|a| a.path.as_ref())
            .map(|p| file_layout::libraries_dir(game_dir).join(p))
            .unwrap_or_else(|| file_layout::library_file(game_dir, &lib.name));
        if !file_ok(
            &lib_path,
            artifact.map(|a| a.sha1.as_str()),
            artifact.map(|a| a.size),
            verify,
        ) {
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
                    if !libraries::is_current_platform_classifier(classifier, platform) {
                        continue;
                    }
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
                    if !file_ok(&native_path, Some(&entry.sha1), Some(entry.size), verify) {
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

/// Queue download items for every asset object missing from disk.
///
/// Malformed hashes (anything but 40 hex chars) are skipped with a warning
/// instead of slicing into a panic — the index is downloaded data.
fn push_missing_assets(
    items: &mut Vec<DownloadItem>,
    game_dir: &Path,
    content: &AssetIndexContent,
    verify: CompletionVerify,
) {
    for obj in content.objects.values() {
        // `asset_object` doubles as the validity gate (audit A3): None means
        // the hash can't be addressed on disk or via the CDN URL.
        let Some(dest) = file_layout::asset_object(game_dir, &obj.hash) else {
            tracing::warn!("Asset object with malformed hash skipped: {:?}", obj.hash);
            continue;
        };
        if file_ok(&dest, Some(&obj.hash), Some(obj.size), verify) {
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

#[cfg(test)]
mod tests {
    use super::*;

    /// 固定 macOS/arm64 描述符：断言与宿主平台解耦（1.1b）。
    fn mac() -> rmccc_platform::PlatformDescriptor {
        rmccc_platform::PlatformDescriptor::macos_arm64()
    }
    use crate::types::GameVersion;

    #[test]
    fn test_check_completion_reports_missing_jar() {
        let game_dir = Path::new("/tmp/rmccc-completion/game");
        let mut launch = GameVersion {
            id: "1.20.4".to_string(),
            ..Default::default()
        };
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
        let missing = check_completion(game_dir, &resolved, &mac(), CompletionVerify::Hash);
        assert!(missing.iter().any(|m| m.kind == MissingKind::ClientJar));
    }

    // ── P1-8 regression: existing-but-corrupt files must be reported ──

    use sha1::{Digest, Sha1};

    fn sha1_hex(data: &[u8]) -> String {
        let mut h = Sha1::new();
        h.update(data);
        format!("{:x}", h.finalize())
    }

    fn resolved_with_one_library(game_dir: &std::path::Path, content: &[u8]) -> ResolvedVersion {
        let rel = "test/corrupt-lib.jar";
        let abs = file_layout::libraries_dir(game_dir).join(rel);
        std::fs::create_dir_all(abs.parent().unwrap()).unwrap();
        if let Some(parent) = abs.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(&abs, content).unwrap();

        let mut launch = GameVersion {
            id: "1.20.4".to_string(),
            ..Default::default()
        };
        launch.libraries.push(crate::types::Library {
            name: "test:corrupt-lib:1.0".into(),
            downloads: Some(crate::types::LibraryDownloads {
                artifact: Some(crate::types::DownloadEntry {
                    url: "https://example.com/corrupt-lib.jar".into(),
                    sha1: sha1_hex(b"good-content"),
                    size: b"good-content".len() as u64,
                    path: Some(rel.into()),
                }),
                classifiers: None,
            }),
            ..Default::default()
        });
        ResolvedVersion {
            unresolved: GameVersion::default(),
            launch,
            standalone: GameVersion::default(),
        }
    }

    #[test]
    fn test_corrupt_library_reported_in_hash_mode_only() {
        let dir = std::env::temp_dir().join(format!("rmcl-comp-hash-{}", std::process::id()));
        // Right size? No — wrong content entirely, so both Size and Hash catch it.
        let resolved = resolved_with_one_library(&dir, b"poison");

        let hash_view = check_completion(&dir, &resolved, &mac(), CompletionVerify::Hash);
        let hit = hash_view
            .iter()
            .find(|m| m.kind == MissingKind::Library)
            .expect("hash mode reports the corrupt library");
        assert!(hit.description.contains("corrupt"));

        // Presence mode keeps the historical behaviour: exists = present
        // (the fixture has no client jar, so filter to libraries).
        let presence_view = check_completion(&dir, &resolved, &mac(), CompletionVerify::Presence);
        assert!(presence_view.iter().all(|m| m.kind != MissingKind::Library));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_samesize_corruption_needs_hash_mode() {
        let dir = std::env::temp_dir().join(format!("rmcl-comp-same-{}", std::process::id()));
        // Same length (12 bytes) as the expected content: Size mode cannot
        // see this corruption.
        let resolved = resolved_with_one_library(&dir, b"evil-content");

        let size_view = check_completion(&dir, &resolved, &mac(), CompletionVerify::Size);
        assert!(
            size_view.iter().all(|m| m.kind != MissingKind::Library),
            "size mode passes same-size content"
        );

        let hash_view = check_completion(&dir, &resolved, &mac(), CompletionVerify::Hash);
        assert!(
            hash_view.iter().any(|m| m.kind == MissingKind::Library),
            "hash mode catches same-size corruption"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_truncated_jar_reported_in_size_mode() {
        let dir = std::env::temp_dir().join(format!("rmcl-comp-trunc-{}", std::process::id()));
        let jar_dir = dir.join("versions/1.20.4");
        std::fs::create_dir_all(&jar_dir).unwrap();
        std::fs::write(jar_dir.join("1.20.4.jar"), b"trunc").unwrap();

        let mut launch = GameVersion {
            id: "1.20.4".to_string(),
            ..Default::default()
        };
        launch.downloads.insert(
            "client".to_string(),
            crate::types::DownloadEntry {
                url: "https://example.com/client.jar".into(),
                sha1: sha1_hex(b"full-client-jar"),
                size: b"full-client-jar".len() as u64,
                path: None,
            },
        );
        let resolved = ResolvedVersion {
            unresolved: GameVersion::default(),
            launch: launch.clone(),
            standalone: GameVersion::default(),
        };

        let size_view = check_completion(&dir, &resolved, &mac(), CompletionVerify::Size);
        assert!(
            size_view.iter().any(|m| m.kind == MissingKind::ClientJar),
            "truncated jar is caught by the size check"
        );
        let hit = size_view
            .iter()
            .find(|m| m.kind == MissingKind::ClientJar)
            .unwrap();
        assert!(hit.description.contains("corrupt"));

        let _ = std::fs::remove_dir_all(&dir);
    }
}
