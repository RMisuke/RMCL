//! Public game-directory formats shared with HMCL, PCL and the official launcher.
//!
//! RMCCC deliberately does **not** invent a private game-directory layout: a
//! game directory is a standard `.minecraft` root (`versions/`, `libraries/`,
//! `assets/`, `mods/`, `saves/`, …), so the same folder can be opened by any
//! launcher. What is launcher-specific is only the *registry* of directories and
//! the per-instance overlay:
//!
//! - RMCCC: `{data_dir}/config/game-directories.json` + `.rmccc/instance.json`;
//! - HMCL: `config/game-directories.json` + `.hmcl/config/instance-game-settings.json`;
//! - PCL: `PCL/Setup.ini` (`LaunchFolders`) + `versions/{id}/PCL/Setup.ini`.
//!
//! This module reads the two public/other-launcher registries so users do not
//! have to re-add their folders by hand, and inspects a folder to tell the UI
//! whether it is a standard Minecraft directory.

use serde::Serialize;
use std::collections::{BTreeMap, HashSet};
use std::path::{Path, PathBuf};
use ts_rs::TS;

/// Where a candidate game directory was discovered.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export_to = "types.ts")]
pub enum DirectorySource {
    /// RMCCC's own managed root (`{data_dir}/instances`).
    Rmccc,
    /// Official launcher / standard `.minecraft` location.
    Mojang,
    /// HMCL's `game-directories.json` (or legacy `hmcl.json`).
    Hmcl,
    /// PCL's `PCL/Setup.ini` `LaunchFolders`.
    Pcl,
    /// A folder the user picked manually that is not registered yet.
    Manual,
}

impl DirectorySource {
    /// Stable label for the UI (also used in JSON).
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Rmccc => "rmccc",
            Self::Mojang => "mojang",
            Self::Hmcl => "hmcl",
            Self::Pcl => "pcl",
            Self::Manual => "manual",
        }
    }
}

/// Standard layout markers of a game directory.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, TS)]
#[ts(export_to = "types.ts")]
pub struct DirectoryLayout {
    /// `versions/` exists.
    pub has_versions: bool,
    /// `libraries/` exists.
    pub has_libraries: bool,
    /// `assets/` exists.
    pub has_assets: bool,
    /// `launcher_profiles.json` exists (official-launcher/Forge compatible).
    pub has_launcher_profiles: bool,
    /// At least one `versions/{id}/.hmcl` metadata directory exists.
    pub has_hmcl_metadata: bool,
    /// At least one `versions/{id}/PCL` metadata directory exists.
    pub has_pcl_metadata: bool,
    /// `versions/` exists and the folder can be opened by another launcher.
    pub standard: bool,
}

impl DirectoryLayout {
    /// Human-readable compatibility summary (`standard` / `empty` / `unknown`).
    pub fn summary(&self) -> &'static str {
        if self.standard {
            "standard"
        } else if !self.has_versions && !self.has_libraries && !self.has_assets {
            "empty"
        } else {
            "unknown"
        }
    }
}

/// A discovered (not necessarily registered) game directory.
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export_to = "types.ts")]
pub struct DirectoryCandidate {
    pub path: PathBuf,
    pub name: String,
    /// All launchers/config files that point at this path.
    pub sources: Vec<DirectorySource>,
    /// Already present in RMCCC's registry (path match).
    pub registered: bool,
    /// HMCL/PCL currently have this directory selected.
    pub preferred: bool,
    pub layout: DirectoryLayout,
    /// Number of standard `versions/{id}/{id}.json` instances.
    pub instance_count: usize,
}

impl DirectoryCandidate {
    /// Primary source for display/badge purposes.
    pub fn primary_source(&self) -> DirectorySource {
        self.sources
            .first()
            .copied()
            .unwrap_or(DirectorySource::Manual)
    }
}

/// Inspect one folder for standard Minecraft layout markers.
///
/// Cheap on purpose: only a handful of `metadata` calls plus a directory entry
/// scan of `versions/`; it never parses version JSON.
pub fn detect_layout(path: &Path) -> DirectoryLayout {
    let versions = path.join("versions");
    let has_versions = versions.is_dir();
    let mut layout = DirectoryLayout {
        has_versions,
        has_libraries: path.join("libraries").is_dir(),
        has_assets: path.join("assets").is_dir(),
        has_launcher_profiles: path.join("launcher_profiles.json").is_file(),
        has_hmcl_metadata: false,
        has_pcl_metadata: false,
        standard: has_versions,
    };

    if has_versions {
        if let Ok(entries) = std::fs::read_dir(&versions) {
            for entry in entries.flatten() {
                if !entry.path().is_dir() {
                    continue;
                }
                let root = entry.path();
                if root.join(".hmcl").is_dir() {
                    layout.has_hmcl_metadata = true;
                }
                if root.join("PCL").is_dir() {
                    layout.has_pcl_metadata = true;
                }
                if layout.has_hmcl_metadata && layout.has_pcl_metadata {
                    break;
                }
            }
        }
    }

    layout
}

/// Count standard version directories (`versions/{id}/{id}.json`).
pub fn count_instances(path: &Path) -> usize {
    let versions = path.join("versions");
    let Ok(entries) = std::fs::read_dir(versions) else {
        return 0;
    };
    entries
        .flatten()
        .filter(|entry| entry.path().is_dir())
        .filter(|entry| {
            let name = entry.file_name();
            let Some(id) = name.to_str() else {
                return false;
            };
            entry.path().join(format!("{id}.json")).is_file()
        })
        .count()
}

/// Scan the well-known locations plus HMCL/PCL registries.
///
/// `registered` must contain canonical paths already present in RMCCC's
/// registry; those candidates are still returned (with `registered = true`) so
/// the UI can explain why they cannot be imported again.
pub fn scan_candidates(registered: &[PathBuf]) -> Vec<DirectoryCandidate> {
    let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
    let cwd = std::env::current_dir().unwrap_or_else(|_| home.clone());
    scan_candidates_in(&home, &cwd, registered)
}

/// Testable scanning core with explicit home/cwd.
pub fn scan_candidates_in(
    home: &Path,
    cwd: &Path,
    registered: &[PathBuf],
) -> Vec<DirectoryCandidate> {
    let registered_paths: Vec<PathBuf> = registered.to_vec();
    let registered: HashSet<PathBuf> = registered_paths.iter().map(|p| canonical(p)).collect();
    let mut found: BTreeMap<PathBuf, DirectoryCandidate> = BTreeMap::new();

    // Official launcher / standard `.minecraft` locations.
    for path in mojang_locations(home, cwd) {
        if path.is_dir() {
            add_candidate(&mut found, path, DirectorySource::Mojang, &registered);
        }
    }

    // HMCL registries (new JSON stores + legacy hmcl.json).
    for (path, name, preferred) in hmcl_directories(home, cwd) {
        if path.is_dir() {
            add_candidate_with(
                &mut found,
                path,
                DirectorySource::Hmcl,
                &registered,
                name,
                preferred,
            );
        }
    }

    // PCL `PCL/Setup.ini`.
    for (path, name, preferred) in pcl_directories(home, cwd) {
        if path.is_dir() {
            add_candidate_with(
                &mut found,
                path,
                DirectorySource::Pcl,
                &registered,
                name,
                preferred,
            );
        }
    }

    // Registered roots are listed too (greyed out by the UI) so the scan result
    // is a complete picture instead of "only new folders". Added last so a root
    // that also belongs to HMCL/PCL keeps its provenance badge.
    for path in &registered_paths {
        if path.is_dir() {
            add_candidate(
                &mut found,
                path.clone(),
                DirectorySource::Rmccc,
                &registered,
            );
        }
    }

    found.into_values().collect()
}

fn add_candidate(
    found: &mut BTreeMap<PathBuf, DirectoryCandidate>,
    path: PathBuf,
    source: DirectorySource,
    registered: &HashSet<PathBuf>,
) {
    let name = folder_name(&path);
    add_candidate_with(found, path, source, registered, name, false);
}

fn add_candidate_with(
    found: &mut BTreeMap<PathBuf, DirectoryCandidate>,
    path: PathBuf,
    source: DirectorySource,
    registered: &HashSet<PathBuf>,
    name: String,
    preferred: bool,
) {
    let canonical = canonical(&path);
    let entry = found
        .entry(canonical.clone())
        .or_insert_with(|| DirectoryCandidate {
            path: canonical.clone(),
            name,
            sources: Vec::new(),
            registered: registered.contains(&canonical),
            preferred: false,
            layout: detect_layout(&canonical),
            instance_count: count_instances(&canonical),
        });
    if !entry.sources.contains(&source) {
        entry.sources.push(source);
    }
    entry.preferred |= preferred;
}

fn folder_name(path: &Path) -> String {
    path.file_name()
        .and_then(|name| name.to_str())
        .filter(|name| !name.is_empty())
        .unwrap_or("Minecraft")
        .to_string()
}

fn canonical(path: &Path) -> PathBuf {
    std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

// ── Official launcher locations ─────────────────────────────────────

fn mojang_locations(home: &Path, cwd: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    // Windows: %APPDATA%\.minecraft; `dirs::data_dir` is %APPDATA% there.
    if let Some(data) = dirs::data_dir() {
        out.push(data.join(".minecraft"));
    }
    // macOS official launcher.
    out.push(home.join("Library/Application Support/minecraft"));
    // Linux / portable.
    out.push(home.join(".minecraft"));
    out.push(cwd.join(".minecraft"));
    out
}

// ── HMCL ────────────────────────────────────────────────────────────

/// HMCL's config homes (new JSON stores + legacy `hmcl.json`).
fn hmcl_homes(home: &Path, cwd: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    for key in ["HMCL_USER_HOME", "HMCL_LOCAL_HOME"] {
        if let Some(value) = std::env::var_os(key) {
            if !value.is_empty() {
                out.push(PathBuf::from(value));
            }
        }
    }
    if cfg!(target_os = "macos") {
        out.push(home.join("Library/Application Support/hmcl"));
    }
    out.push(home.join(".hmcl"));
    out.push(home.join(".local/share/hmcl"));
    out.push(cwd.join(".hmcl"));
    out
}

/// Read HMCL directory registries.
///
/// Returns `(path, name, preferred)` where `preferred` marks HMCL's currently
/// selected directory (from `config/launcher-settings.json`).
fn hmcl_directories(home: &Path, cwd: &Path) -> Vec<(PathBuf, String, bool)> {
    let mut out: Vec<(PathBuf, String, bool)> = Vec::new();
    for hmcl_home in hmcl_homes(home, cwd) {
        let selected = hmcl_selected_directory(&hmcl_home);
        for relative in [
            "config/game-directories.json",
            "config/user-game-directories.json",
        ] {
            out.extend(read_hmcl_store(
                &hmcl_home.join(relative),
                selected.as_deref(),
            ));
        }
        // HMCL < 3.6 wrote a single `hmcl.json` with `gameDir`.
        let legacy = hmcl_home.join("hmcl.json");
        if let Ok(text) = std::fs::read_to_string(&legacy) {
            if let Ok(value) = serde_json::from_str::<serde_json::Value>(&text) {
                if let Some(path) = value.get("gameDir").and_then(|v| v.as_str()) {
                    if !path.trim().is_empty() {
                        out.push((PathBuf::from(path), folder_name(Path::new(path)), true));
                    }
                }
            }
        }
    }
    out
}

fn hmcl_selected_directory(hmcl_home: &Path) -> Option<String> {
    let text = std::fs::read_to_string(hmcl_home.join("config/launcher-settings.json")).ok()?;
    let value: serde_json::Value = serde_json::from_str(&text).ok()?;
    value
        .get("selectedGameDirectory")
        .and_then(|v| v.as_str())
        .map(str::to_string)
}

fn read_hmcl_store(path: &Path, selected: Option<&str>) -> Vec<(PathBuf, String, bool)> {
    let Ok(text) = std::fs::read_to_string(path) else {
        return Vec::new();
    };
    let Ok(value) = serde_json::from_str::<serde_json::Value>(&text) else {
        tracing::warn!(path = %path.display(), "Ignoring unreadable HMCL directory store");
        return Vec::new();
    };

    let Some(list) = value.get("directories").and_then(|v| v.as_array()) else {
        return Vec::new();
    };

    list.iter()
        .filter_map(|entry| {
            let raw_path = entry.get("path")?.as_str()?.trim();
            if raw_path.is_empty() {
                return None;
            }
            let path = PathBuf::from(raw_path);
            let id = entry.get("id").and_then(|v| v.as_str());
            let preferred = match (id, selected) {
                (Some(id), Some(selected)) => id == selected,
                _ => false,
            };
            let name = entry
                .get("name")
                .and_then(localized_text)
                .unwrap_or_else(|| folder_name(&path));
            Some((path, name, preferred))
        })
        .collect()
}

/// HMCL `LocalizedText` serializes either as a plain string or a locale map.
fn localized_text(value: &serde_json::Value) -> Option<String> {
    if let Some(text) = value.as_str() {
        return Some(text.to_string());
    }
    let map = value.as_object()?;
    for key in ["default", "zh_CN", "zh", "en"] {
        if let Some(text) = map.get(key).and_then(|v| v.as_str()) {
            return Some(text.to_string());
        }
    }
    map.values().find_map(|v| v.as_str()).map(str::to_string)
}

// ── PCL ─────────────────────────────────────────────────────────────

/// Common PCL roots (the executable folder that contains `PCL/Setup.ini`).
fn pcl_bases(home: &Path, cwd: &Path) -> Vec<PathBuf> {
    let mut out = vec![
        cwd.to_path_buf(),
        cwd.parent().unwrap_or(cwd).to_path_buf(),
        home.to_path_buf(),
        home.join("Desktop"),
        home.join("Downloads"),
        home.join("Documents"),
    ];
    out.push(PathBuf::from("/Applications"));
    out
}

fn pcl_directories(home: &Path, cwd: &Path) -> Vec<(PathBuf, String, bool)> {
    let mut out = Vec::new();
    for base in pcl_bases(home, cwd) {
        let ini = base.join("PCL/Setup.ini");
        if !ini.is_file() {
            continue;
        }
        let settings = read_pcl_ini(&ini);
        let selected = settings
            .get("LaunchFolderSelect")
            .map(|value| pcl_expand_path(value, &base));
        let Some(folders) = settings.get("LaunchFolders") else {
            continue;
        };
        for entry in folders.split('|') {
            let entry = entry.trim();
            if entry.is_empty() {
                continue;
            }
            let Some((name, raw_path)) = entry.split_once('>') else {
                continue;
            };
            let raw_path = pcl_expand_path(raw_path, &base);
            let preferred = selected
                .as_ref()
                .is_some_and(|selected| paths_equal(selected, &raw_path));
            out.push((
                raw_path,
                if name.trim().is_empty() {
                    "PCL".to_string()
                } else {
                    name.trim().to_string()
                },
                preferred,
            ));
        }
    }
    out
}

/// PCL stores selected folders relative to the PCL root as `$...`.
fn pcl_expand_path(raw: &str, base: &Path) -> PathBuf {
    let raw = raw.trim();
    let normalized = raw.trim_end_matches(['\\', '/']);
    if let Some(relative) = normalized.strip_prefix('$') {
        let relative = relative.trim_start_matches(['\\', '/']);
        return base.join(relative);
    }
    PathBuf::from(normalized)
}

/// PCL's INI dialect: one `Key:Value` pair per line, no sections.
fn read_pcl_ini(path: &Path) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    let Ok(text) = std::fs::read_to_string(path) else {
        return out;
    };
    for line in text.lines() {
        let line = line.trim_start_matches('\u{feff}').trim_end();
        if line.is_empty() || line.starts_with(';') || line.starts_with('#') {
            continue;
        }
        let Some((key, value)) = line.split_once(':') else {
            continue;
        };
        if !key.is_empty() {
            out.insert(key.to_string(), value.to_string());
        }
    }
    out
}

fn paths_equal(a: &Path, b: &Path) -> bool {
    canonical(a) == canonical(b)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "rmccc-compat-{tag}-{}-{}",
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

    #[test]
    fn detects_standard_layout_and_counts_instances() {
        let root = temp("layout");
        let version = root.join("versions/1.20.4");
        std::fs::create_dir_all(&version).unwrap();
        std::fs::write(version.join("1.20.4.json"), "{}").unwrap();
        std::fs::create_dir_all(root.join("libraries")).unwrap();
        std::fs::create_dir_all(root.join("assets")).unwrap();

        let layout = detect_layout(&root);
        assert!(layout.has_versions);
        assert!(layout.has_libraries);
        assert!(layout.has_assets);
        assert!(layout.standard);
        assert_eq!(layout.summary(), "standard");
        assert_eq!(count_instances(&root), 1);

        // PCL/HMCL metadata is detected per instance.
        std::fs::create_dir_all(version.join(".hmcl")).unwrap();
        std::fs::create_dir_all(version.join("PCL")).unwrap();
        let layout = detect_layout(&root);
        assert!(layout.has_hmcl_metadata);
        assert!(layout.has_pcl_metadata);

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn reads_hmcl_directories_and_selected_flag() {
        let home = temp("hmcl-home");
        let cwd = temp("hmcl-cwd");
        let game = temp("hmcl-game");
        std::fs::create_dir_all(game.join("versions")).unwrap();
        let hmcl_home = home.join("Library/Application Support/hmcl");
        std::fs::create_dir_all(hmcl_home.join("config")).unwrap();
        std::fs::write(
            hmcl_home.join("config/game-directories.json"),
            format!(
                r#"{{"directories":[{{"id":"abc","path":"{}","name":{{"default":"HMCL 目录","zh_CN":"HMCL 中文"}}}}]}}"#,
                game.display()
            ),
        )
        .unwrap();
        std::fs::write(
            hmcl_home.join("config/launcher-settings.json"),
            r#"{"selectedGameDirectory":"abc"}"#,
        )
        .unwrap();

        let dirs: Vec<_> = hmcl_directories(&home, &cwd)
            .into_iter()
            .filter(|(path, _, _)| canonical(path) == canonical(&game))
            .collect();
        assert_eq!(dirs.len(), 1);
        assert_eq!(dirs[0].1, "HMCL 目录");
        assert!(dirs[0].2, "selected HMCL directory should be preferred");

        let _ = std::fs::remove_dir_all(&home);
        let _ = std::fs::remove_dir_all(&cwd);
        let _ = std::fs::remove_dir_all(&game);
    }

    #[test]
    fn reads_pcl_launch_folders_and_relative_selection() {
        let home = temp("pcl-home");
        let cwd = temp("pcl-cwd");
        let game = temp("pcl-game");
        std::fs::create_dir_all(game.join("versions")).unwrap();
        let pcl = cwd.join("PCL");
        std::fs::create_dir_all(&pcl).unwrap();
        std::fs::write(
            pcl.join("Setup.ini"),
            format!(
                "LaunchFolders:我的世界>{}|第二目录>{}\nLaunchFolderSelect:{}\n",
                game.display(),
                game.display(),
                game.display()
            ),
        )
        .unwrap();

        let dirs: Vec<_> = pcl_directories(&home, &cwd)
            .into_iter()
            .filter(|(path, _, _)| canonical(path) == canonical(&game))
            .collect();
        assert_eq!(dirs.len(), 2);
        assert_eq!(dirs[0].1, "我的世界");
        assert!(dirs[0].2, "PCL selected folder should be preferred");

        let _ = std::fs::remove_dir_all(&home);
        let _ = std::fs::remove_dir_all(&cwd);
        let _ = std::fs::remove_dir_all(&game);
    }

    #[test]
    fn scan_marks_registered_directories_and_dedups() {
        let home = temp("scan-home");
        let cwd = temp("scan-cwd");
        let game = temp("scan-game");
        std::fs::create_dir_all(game.join("versions/1.20.4")).unwrap();
        std::fs::write(game.join("versions/1.20.4/1.20.4.json"), "{}").unwrap();
        std::fs::create_dir_all(home.join(".minecraft")).unwrap();
        // A PCL install points at the same folder as the official location.
        std::fs::create_dir_all(cwd.join("PCL")).unwrap();
        std::fs::write(
            cwd.join("PCL/Setup.ini"),
            format!("LaunchFolders:共享>{}\n", game.display()),
        )
        .unwrap();

        let registered = vec![canonical(&home.join(".minecraft"))];
        let found = scan_candidates_in(&home, &cwd, &registered);
        let official = found
            .iter()
            .find(|c| c.path == canonical(&home.join(".minecraft")))
            .unwrap();
        assert!(official.registered);
        assert_eq!(official.primary_source(), DirectorySource::Mojang);

        let shared = found.iter().find(|c| c.path == canonical(&game)).unwrap();
        assert!(!shared.registered);
        assert_eq!(shared.primary_source(), DirectorySource::Pcl);
        assert_eq!(shared.instance_count, 1);

        let _ = std::fs::remove_dir_all(&home);
        let _ = std::fs::remove_dir_all(&cwd);
        let _ = std::fs::remove_dir_all(&game);
    }
}

/// A registered directory with the cheap facts the UI needs (count + layout).
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export_to = "types.ts")]
pub struct GameDirectoryOverview {
    pub directory: crate::directory::GameDirectory,
    pub layout: DirectoryLayout,
    /// Standard version-backed instances (`versions/{id}/{id}.json`).
    pub instance_count: usize,
    pub is_selected: bool,
}
