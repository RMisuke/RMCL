//! Storage accounting for the storage-management settings page.
//!
//! Everything here is read-only except [`clean_caches`], which only removes the
//! launcher's *regenerable* cache (`{data_dir}/cache`). Sizes are computed with
//! a plain recursive walk that never follows symlinks and tolerates permission
//! errors, so a single unreadable folder cannot break the page.

use crate::directory::GameDirectory;
use crate::settings::InstanceSettings;
use rmccc_version::file_layout;
use serde::Serialize;
use std::path::{Path, PathBuf};
use ts_rs::TS;

/// Category of a launcher-owned storage location.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export_to = "types.ts")]
pub enum StorageKind {
    Config,
    Cache,
    Logs,
    Plugins,
    PluginData,
    PluginState,
    State,
    Agent,
    Dashboard,
    Private,
    LegacyProfiles,
    WebviewCache,
    File,
    Other,
}

/// One launcher-owned directory or file.
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export_to = "types.ts")]
pub struct StorageEntry {
    pub id: String,
    pub label: String,
    pub path: PathBuf,
    pub bytes: u64,
    pub kind: StorageKind,
    /// Included in the one-click cache cleanup.
    pub cleanable: bool,
    pub exists: bool,
}

/// One instance inside a game directory.
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export_to = "types.ts")]
pub struct InstanceStorage {
    /// `{directory_id}/{instance_id}`.
    pub key: String,
    pub directory_id: String,
    pub id: String,
    pub display_name: String,
    pub path: PathBuf,
    pub bytes: u64,
}

/// One registered game directory with per-instance breakdown.
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export_to = "types.ts")]
pub struct GameDirectoryStorage {
    pub directory_id: String,
    pub name: String,
    pub path: PathBuf,
    pub bytes: u64,
    /// `libraries/`, `assets/`, launchers, and anything not owned by one instance.
    pub shared_bytes: u64,
    pub instances: Vec<InstanceStorage>,
}

/// Full storage overview for the settings page.
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export_to = "types.ts")]
pub struct StorageOverview {
    pub data_dir: PathBuf,
    /// Sum of [`Self::entries`].
    pub launcher_bytes: u64,
    /// Sum of [`Self::game_directories`].
    pub game_bytes: u64,
    pub total_bytes: u64,
    /// Bytes removed by [`clean_caches`].
    pub cleanable_bytes: u64,
    pub entries: Vec<StorageEntry>,
    pub game_directories: Vec<GameDirectoryStorage>,
}

/// Result of one-click cache cleanup.
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export_to = "types.ts")]
pub struct CacheCleanReport {
    pub freed_bytes: u64,
    pub removed: Vec<String>,
    pub failed: Vec<String>,
}

/// Recursive directory size; symlinks are counted but not followed.
pub fn dir_size(path: &Path) -> u64 {
    let Ok(metadata) = std::fs::symlink_metadata(path) else {
        return 0;
    };
    if metadata.is_file() {
        return metadata.len();
    }
    if !metadata.is_dir() {
        return 0;
    }
    let Ok(entries) = std::fs::read_dir(path) else {
        return 0;
    };
    entries.flatten().map(|entry| dir_size(&entry.path())).sum()
}

fn label_for(name: &str) -> (&'static str, StorageKind, bool) {
    match name {
        "config" => ("Launcher settings", StorageKind::Config, false),
        "cache" => ("Download & icon cache", StorageKind::Cache, true),
        "logs" => ("Runtime logs", StorageKind::Logs, false),
        "plugins" => ("Installed plugins", StorageKind::Plugins, false),
        "plugin-data" => ("Plugin data", StorageKind::PluginData, false),
        "plugin-state" => ("Plugin state", StorageKind::PluginState, false),
        "state" => ("Runtime state", StorageKind::State, false),
        "agent" => ("MCP / agent audit", StorageKind::Agent, false),
        "dashboard" => ("Dashboard data", StorageKind::Dashboard, false),
        "private" => ("Private data", StorageKind::Private, false),
        "profiles" => ("Legacy profile files", StorageKind::LegacyProfiles, false),
        "accounts.json" => ("Accounts", StorageKind::File, false),
        "user-settings.json" => ("User settings", StorageKind::File, false),
        ".key" => ("Local encryption key", StorageKind::File, false),
        ".rmcli_history" => ("CLI history", StorageKind::File, false),
        "mcp-token" => ("MCP token", StorageKind::File, false),
        "mcp-endpoint" => ("MCP endpoint", StorageKind::File, false),
        _ => ("Other", StorageKind::Other, false),
    }
}

fn is_inside(path: &Path, root: &Path) -> bool {
    let canon = |p: &Path| std::fs::canonicalize(p).unwrap_or_else(|_| p.to_path_buf());
    canon(path).starts_with(canon(root))
}

/// Build the full overview. Game directories are listed separately and are not
/// double-counted as launcher entries.
pub fn overview(data_dir: &Path, directories: &[GameDirectory]) -> StorageOverview {
    let mut entries = Vec::new();

    if let Ok(read) = std::fs::read_dir(data_dir) {
        for entry in read.flatten() {
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().to_string();
            // The default managed game directory lives under the data dir; it
            // is accounted for in `game_directories` instead.
            if directories
                .iter()
                .any(|dir| is_inside(&path, &dir.path) || is_inside(&dir.path, &path))
            {
                continue;
            }
            let (label, kind, cleanable) = label_for(&name);
            let metadata = entry.metadata().ok();
            entries.push(StorageEntry {
                id: name.clone(),
                label: label.to_string(),
                bytes: dir_size(&path),
                path,
                kind,
                cleanable,
                exists: metadata.is_some(),
            });
        }
    }

    // macOS WebView cache is outside the data dir but owned by the launcher.
    if let Some(cache_root) = dirs::cache_dir() {
        let webview = cache_root.join("rmccc");
        if webview.is_dir() {
            entries.push(StorageEntry {
                id: "webview-cache".to_string(),
                label: "WebView cache".to_string(),
                bytes: dir_size(&webview),
                path: webview,
                kind: StorageKind::WebviewCache,
                cleanable: false,
                exists: true,
            });
        }
    }

    entries.sort_by(|a, b| {
        b.bytes
            .cmp(&a.bytes)
            .then_with(|| a.label.to_lowercase().cmp(&b.label.to_lowercase()))
    });

    let game_directories: Vec<GameDirectoryStorage> =
        directories.iter().map(game_directory_storage).collect();

    let launcher_bytes = entries.iter().map(|entry| entry.bytes).sum();
    let game_bytes = game_directories.iter().map(|dir| dir.bytes).sum();
    let cleanable_bytes = entries
        .iter()
        .filter(|entry| entry.cleanable)
        .map(|entry| entry.bytes)
        .sum();

    StorageOverview {
        data_dir: data_dir.to_path_buf(),
        launcher_bytes,
        game_bytes,
        total_bytes: launcher_bytes.saturating_add(game_bytes),
        cleanable_bytes,
        entries,
        game_directories,
    }
}

fn game_directory_storage(directory: &GameDirectory) -> GameDirectoryStorage {
    let total = dir_size(&directory.path);
    let mut instances = Vec::new();

    let versions = file_layout::versions_dir(&directory.path);
    if let Ok(read) = std::fs::read_dir(&versions) {
        for entry in read.flatten() {
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }
            let Some(id) = entry.file_name().to_str().map(str::to_string) else {
                continue;
            };
            if !file_layout::instance_json(&directory.path, &id).is_file() {
                continue;
            }
            let settings = InstanceSettings::load(&path).settings;
            let display_name = settings
                .display_name
                .filter(|name| !name.trim().is_empty())
                .unwrap_or_else(|| id.clone());
            instances.push(InstanceStorage {
                key: format!("{}/{}", directory.id, id),
                directory_id: directory.id.to_string(),
                path: path.clone(),
                bytes: dir_size(&path),
                display_name,
                id,
            });
        }
    }

    instances.sort_by(|a, b| {
        b.bytes.cmp(&a.bytes).then_with(|| {
            a.display_name
                .to_lowercase()
                .cmp(&b.display_name.to_lowercase())
        })
    });
    let instance_total: u64 = instances.iter().map(|instance| instance.bytes).sum();

    GameDirectoryStorage {
        directory_id: directory.id.to_string(),
        name: directory.name.clone(),
        path: directory.path.clone(),
        bytes: total,
        shared_bytes: total.saturating_sub(instance_total),
        instances,
    }
}

/// Instance sizes for one directory (used by the instance cards).
pub fn instances_in(directory: &GameDirectory) -> Vec<InstanceStorage> {
    game_directory_storage(directory).instances
}

/// Remove the launcher's regenerable cache. Never touches saves, mods, configs,
/// plugins, or game files.
pub fn clean_caches(data_dir: &Path) -> CacheCleanReport {
    let cache = data_dir.join("cache");
    let freed = dir_size(&cache);
    let mut report = CacheCleanReport {
        freed_bytes: 0,
        removed: Vec::new(),
        failed: Vec::new(),
    };
    if !cache.exists() {
        return report;
    }
    if let Err(error) = std::fs::remove_dir_all(&cache) {
        report.failed.push(format!("{}: {error}", cache.display()));
        return report;
    }
    report.freed_bytes = freed;
    report.removed.push(cache.display().to_string());
    report
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "rmccc-storage-{tag}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn write(path: &Path, bytes: usize) {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(path, vec![0u8; bytes]).unwrap();
    }

    #[test]
    fn dir_size_sums_files_and_skips_missing() {
        let root = temp("size");
        write(&root.join("a.bin"), 100);
        write(&root.join("nested/b.bin"), 250);
        assert_eq!(dir_size(&root), 350);
        assert_eq!(dir_size(&root.join("missing")), 0);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn overview_splits_instances_and_shared_bytes() {
        let data = temp("overview-data");
        let game = temp("overview-game");
        // Launcher-owned cache.
        write(&data.join("cache/objects/x"), 2048);
        write(&data.join("config/launcher-settings.json"), 32);
        // Game directory: one instance plus shared assets.
        let version = game.join("versions/1.20.4");
        std::fs::create_dir_all(&version).unwrap();
        std::fs::write(version.join("1.20.4.json"), "{}").unwrap();
        write(&version.join("1.20.4.jar"), 4096);
        write(&game.join("assets/objects/z"), 1024);
        write(&game.join("libraries/lib.jar"), 512);

        let directory = GameDirectory {
            id: crate::id::GameDirectoryId::default_id(),
            name: "Default".into(),
            path: game.clone(),
            kind: crate::directory::GameDirectoryKind::Managed,
            created_at: chrono::Utc::now(),
        };
        let overview = overview(&data, &[directory]);
        assert_eq!(overview.cleanable_bytes, 2048);
        let entry_bytes = |id: &str| {
            overview
                .entries
                .iter()
                .find(|entry| entry.id == id)
                .map(|entry| entry.bytes)
                .unwrap_or(0)
        };
        assert_eq!(entry_bytes("cache"), 2048);
        assert_eq!(entry_bytes("config"), 32);
        assert_eq!(overview.game_directories.len(), 1);
        let dir = &overview.game_directories[0];
        assert_eq!(dir.instances.len(), 1);
        assert_eq!(dir.instances[0].bytes, 4096 + 2); // json "{}" = 2 bytes
        assert_eq!(dir.bytes, 4096 + 2 + 1536);
        assert_eq!(dir.shared_bytes, 1536);

        let _ = std::fs::remove_dir_all(&data);
        let _ = std::fs::remove_dir_all(&game);
    }

    #[test]
    fn clean_caches_removes_only_cache() {
        let data = temp("clean");
        write(&data.join("cache/objects/x"), 4096);
        write(&data.join("config/keep.json"), 8);
        write(&data.join("instances/versions/1.20.4/1.20.4.json"), 2);
        let report = clean_caches(&data);
        assert_eq!(report.freed_bytes, 4096);
        assert!(!data.join("cache").exists());
        assert!(data.join("config/keep.json").exists());
        assert!(data.join("instances/versions/1.20.4/1.20.4.json").exists());
        let _ = std::fs::remove_dir_all(&data);
    }
}
