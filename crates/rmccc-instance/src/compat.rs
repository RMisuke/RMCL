//! Public game-directory formats shared with HMCL, PCL and the official launcher.
//!
//! RMCCC deliberately does **not** invent a private game-directory layout: a
//! game directory is a standard `.minecraft` root (`versions/`, `libraries/`,
//! `assets/`, `mods/`, `saves/`, …), so the same folder can be opened by any
//! launcher. What is launcher-specific is only the *registry* of directories and
//! the per-instance overlay:
//!
//! - RMCCC: `{data_dir}/config/game-directories.json` + `.rmccc/instance.json`;
//! - HMCL 3.16+: `<hmcl-home>/config/game-directories.json` and
//!   `user-game-directories.json`; `launcher-settings.json` holds the selected
//!   directory ID. Pre-3.16 `hmcl.json` / `.hmcl.json` are legacy inputs
//!   (`configurations` map + `last`).
//! - PCL: `<exe>/PCL/Setup.ini` holds `LaunchFolderSelect`; the custom folder
//!   list (`LaunchFolders`) lives in `HKCU\Software\PCL` (or `PCLDebug`), with
//!   `Name>Path|…` entries. `PCL.ini` inside each game folder is a per-folder
//!   marker, not a folder registry.
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
    /// PCL's folder registry (`HKCU\Software\PCL`) or `PCL/Setup.ini`.
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

/// HMCL config homes: workspace-local (`.hmcl`) and per-user locations.
///
/// HMCL 3.16+ keeps the directory registry in `<home>/config/`; the legacy
/// `hmcl.json` / `.hmcl.json` files are read only as migration inputs.
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
    // Windows HMCL stores its user home under %APPDATA%\.hmcl.
    #[cfg(windows)]
    if let Some(data) = dirs::data_dir() {
        out.push(data.join(".hmcl"));
    }
    out.push(home.join(".hmcl"));
    out.push(home.join(".local/share/hmcl"));
    out.push(cwd.join(".hmcl"));
    out
}

fn hmcl_directories(home: &Path, cwd: &Path) -> Vec<(PathBuf, String, bool)> {
    let mut out = Vec::new();
    for hmcl_home in hmcl_homes(home, cwd) {
        for hit in hmcl_hits_in_home(&hmcl_home, cwd) {
            push_hmcl_tuple(&mut out, hit);
        }
    }
    // `locateLegacyConfig()` also checks the working directory itself.
    for filename in ["hmcl.json", ".hmcl.json"] {
        for hit in hmcl_legacy_hits(&cwd.join(filename), cwd) {
            push_hmcl_tuple(&mut out, hit);
        }
    }
    out
}

fn push_hmcl_tuple(out: &mut Vec<(PathBuf, String, bool)>, hit: LauncherConfigHit) {
    let Some(path) = hit.game_dir else {
        return;
    };
    let name = hit.name.unwrap_or_else(|| folder_name(path.as_path()));
    out.push((path, name, hit.selected));
}

fn hmcl_selected_directory(hmcl_home: &Path) -> Option<String> {
    let text = read_text(&hmcl_home.join("config/launcher-settings.json"))?;
    let value: serde_json::Value = serde_json::from_str(&text).ok()?;
    value
        .get("selectedGameDirectory")
        .and_then(|v| v.as_str())
        .map(str::to_string)
}

/// Read both HMCL 3.16+ stores plus the legacy `hmcl.json` in one HMCL home.
fn hmcl_hits_in_home(hmcl_home: &Path, resolve_base: &Path) -> Vec<LauncherConfigHit> {
    let selected = hmcl_selected_directory(hmcl_home);
    let mut out = Vec::new();
    for relative in [
        "config/game-directories.json",
        "config/user-game-directories.json",
    ] {
        out.extend(read_hmcl_store(
            &hmcl_home.join(relative),
            resolve_base,
            selected.as_deref(),
        ));
    }
    if out.is_empty() {
        out = hmcl_legacy_hits(&hmcl_home.join("hmcl.json"), resolve_base);
    }
    out
}

fn read_hmcl_store(
    path: &Path,
    resolve_base: &Path,
    selected: Option<&str>,
) -> Vec<LauncherConfigHit> {
    let Some(text) = read_text(path) else {
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
            let raw_path = entry.get("path")?.as_str()?;
            let game_dir = resolve_launcher_path(raw_path, resolve_base)?;
            let id = entry.get("id").and_then(|v| v.as_str());
            let name = entry
                .get("name")
                .and_then(localized_text)
                .filter(|name| !name.trim().is_empty());
            Some(LauncherConfigHit {
                launcher: DirectorySource::Hmcl,
                config_path: path.to_path_buf(),
                name,
                instance_count: count_instances(&game_dir),
                game_dir: Some(game_dir),
                selected: matches!((id, selected), (Some(id), Some(selected)) if id == selected),
            })
        })
        .collect()
}

/// Pre-3.16 `hmcl.json` / `.hmcl.json`:
/// `configurations` is a map keyed by profile name; `last` selects one.
fn hmcl_legacy_hits(path: &Path, resolve_base: &Path) -> Vec<LauncherConfigHit> {
    if !path.is_file() {
        return Vec::new();
    }
    let Some(text) = read_text(path) else {
        return Vec::new();
    };
    let Ok(value) = serde_json::from_str::<serde_json::Value>(&text) else {
        return Vec::new();
    };
    let selected = value.get("last").and_then(|v| v.as_str());
    let mut out = Vec::new();

    if let Some(configurations) = value.get("configurations").and_then(|v| v.as_object()) {
        for (profile_name, profile) in configurations {
            let Some(profile) = profile.as_object() else {
                continue;
            };
            let raw_path = profile
                .get("gameDir")
                .or_else(|| profile.get("path"))
                .and_then(|v| v.as_str());
            let Some(game_dir) = raw_path.and_then(|raw| resolve_launcher_path(raw, resolve_base))
            else {
                continue;
            };
            let name = if matches!(profile_name.as_str(), "Default" | "Home") {
                None
            } else {
                Some(profile_name.clone())
            };
            out.push(LauncherConfigHit {
                launcher: DirectorySource::Hmcl,
                config_path: path.to_path_buf(),
                name,
                instance_count: count_instances(&game_dir),
                game_dir: Some(game_dir),
                selected: selected == Some(profile_name.as_str()),
            });
        }
    }

    // Very old configs stored a single `gameDir` at the top level.
    if out.is_empty() {
        if let Some(game_dir) = value
            .get("gameDir")
            .and_then(|v| v.as_str())
            .and_then(|raw| resolve_launcher_path(raw, resolve_base))
        {
            out.push(LauncherConfigHit {
                launcher: DirectorySource::Hmcl,
                config_path: path.to_path_buf(),
                name: None,
                instance_count: count_instances(&game_dir),
                game_dir: Some(game_dir),
                selected: false,
            });
        }
    }

    out
}

/// HMCL home(s) that belong to a portable install rooted at `base`.
fn hmcl_configs_for_base(base: &Path) -> Vec<LauncherConfigHit> {
    let mut out = hmcl_hits_in_home(&base.join(".hmcl"), base);
    if out.is_empty() {
        for filename in ["hmcl.json", ".hmcl.json"] {
            let legacy = base.join(filename);
            out = hmcl_legacy_hits(&legacy, base);
            if !out.is_empty() {
                break;
            }
        }
    }
    dedupe_game_dirs(out)
}

/// HMCL's `LocalizedText` serializes either as a plain string or a locale map.
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

fn resolve_launcher_path(raw: &str, base: &Path) -> Option<PathBuf> {
    let raw = raw.trim();
    if raw.is_empty() {
        return None;
    }
    let path = PathBuf::from(raw);
    Some(if is_absolute_like(raw) {
        path
    } else {
        base.join(path)
    })
}

/// `Path::is_absolute` does not recognize `C:\…` when inspecting a Windows
/// config on another platform.
fn is_absolute_like(raw: &str) -> bool {
    raw.starts_with(['/', '\\']) || raw.as_bytes().get(1) == Some(&b':')
}

fn path_key(path: &Path) -> String {
    let mut key = path.to_string_lossy().replace('\\', "/");
    while key.ends_with('/') {
        key.pop();
    }
    if cfg!(windows) {
        key.to_lowercase()
    } else {
        key
    }
}

/// Merge entries that point at the same folder, keeping the first config path.
fn dedupe_game_dirs(hits: Vec<LauncherConfigHit>) -> Vec<LauncherConfigHit> {
    let mut out: Vec<LauncherConfigHit> = Vec::new();
    for hit in hits {
        let Some(path) = hit.game_dir.as_deref() else {
            out.push(hit);
            continue;
        };
        let key = path_key(path);
        if let Some(existing) = out.iter_mut().find(|entry| {
            entry
                .game_dir
                .as_deref()
                .is_some_and(|p| path_key(p) == key)
        }) {
            existing.selected |= hit.selected;
            if existing.name.is_none() {
                existing.name = hit.name;
            }
        } else {
            out.push(hit);
        }
    }
    out
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

fn pcl_setup_path(base: &Path) -> PathBuf {
    base.join("PCL").join("Setup.ini")
}

/// PCL's custom folder list lives in `HKCU\Software\PCL` (official builds) or
/// `HKCU\Software\PCLDebug` (the open-source build). `Setup.ini` only carries
/// `LaunchFolderSelect`; older/patched builds may still have `LaunchFolders`.
#[cfg(windows)]
fn pcl_registry_launch_folders() -> Vec<String> {
    use winreg::enums::HKEY_CURRENT_USER;
    use winreg::RegKey;

    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let mut out = Vec::new();
    for path in ["Software\\PCL", "Software\\PCLDebug"] {
        let Ok(key) = hkcu.open_subkey(path) else {
            continue;
        };
        let Ok(value) = key.get_value::<String, _>("LaunchFolders") else {
            continue;
        };
        if !value.trim().is_empty() {
            out.push(value);
        }
    }
    out
}

#[cfg(not(windows))]
fn pcl_registry_launch_folders() -> Vec<String> {
    Vec::new()
}

#[derive(Debug, Clone)]
struct PclEntry {
    path: PathBuf,
    name: Option<String>,
    selected: bool,
}

fn pcl_directories(home: &Path, cwd: &Path) -> Vec<(PathBuf, String, bool)> {
    let registry = pcl_registry_launch_folders();
    let mut entries = Vec::new();
    for list in &registry {
        for (path, name) in pcl_parse_folder_list(list) {
            merge_pcl_entry(&mut entries, path, name, false);
        }
    }

    let mut selected_paths = Vec::new();
    let mut has_setup = false;
    for base in pcl_bases(home, cwd) {
        let ini = pcl_setup_path(&base);
        if !ini.is_file() {
            continue;
        }
        has_setup = true;
        let settings = read_pcl_ini(&ini);
        if let Some(selected) = settings.get("LaunchFolderSelect") {
            let path = pcl_expand_path(selected, &base);
            if !path.as_os_str().is_empty() {
                selected_paths.push(path);
            }
        }
        if let Some(folders) = settings.get("LaunchFolders") {
            for (path, name) in pcl_parse_folder_list(folders) {
                merge_pcl_entry(&mut entries, path, name, false);
            }
        }
    }
    // PCL also lists the exe folder and its immediate child worlds, but only
    // when there is an actual PCL installation to import from.
    if has_setup || registry.iter().any(|value| !value.trim().is_empty()) {
        for (path, name) in pcl_default_folders(cwd) {
            merge_pcl_entry(&mut entries, path, name, false);
        }
    }

    for path in &selected_paths {
        merge_pcl_entry(&mut entries, path.clone(), None, true);
    }
    if entries.is_empty() {
        return Vec::new();
    }
    for entry in &mut entries {
        entry.selected = selected_paths
            .iter()
            .any(|selected| pcl_paths_equal(selected, &entry.path));
    }
    if entries.iter().all(|entry| !entry.selected) {
        if let Some(first) = entries.first_mut() {
            first.selected = true;
        }
    }
    entries
        .into_iter()
        .map(|entry| {
            let name = entry.name.unwrap_or_else(|| "PCL".to_string());
            (entry.path, name, entry.selected)
        })
        .collect()
}

/// PCL `LaunchFolders` entries: `Name>Path|Name>Path`. PCL requires a trailing
/// separator; tolerate configs written without one.
fn pcl_parse_folder_list(list: &str) -> Vec<(PathBuf, Option<String>)> {
    list.split('|')
        .filter_map(|entry| {
            let entry = entry.trim();
            if entry.is_empty() {
                return None;
            }
            let (name, raw_path) = entry.split_once('>')?;
            let raw_path = raw_path.trim();
            if raw_path.is_empty() {
                return None;
            }
            let name = name.trim();
            Some((
                PathBuf::from(raw_path.trim_end_matches(['\\', '/'])),
                (!name.is_empty()).then(|| name.to_string()),
            ))
        })
        .collect()
}

fn merge_pcl_entry(
    entries: &mut Vec<PclEntry>,
    path: PathBuf,
    name: Option<String>,
    selected: bool,
) {
    let key = path_key(&path);
    if let Some(existing) = entries
        .iter_mut()
        .find(|entry| path_key(&entry.path) == key)
    {
        if existing.name.is_none() {
            existing.name = name;
        }
        existing.selected |= selected;
    } else {
        entries.push(PclEntry {
            path,
            name,
            selected,
        });
    }
}

/// PCL's own default discovery: the exe folder, its immediate children with
/// `versions/` (or named `.minecraft`), and `%APPDATA%\.minecraft` on Windows.
fn pcl_default_folders(base: &Path) -> Vec<(PathBuf, Option<String>)> {
    let mut out = Vec::new();
    if base.join("versions").is_dir() {
        out.push((base.to_path_buf(), None));
    }
    if let Ok(entries) = std::fs::read_dir(base) {
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }
            let is_world = path.join("versions").is_dir()
                || entry
                    .file_name()
                    .to_string_lossy()
                    .eq_ignore_ascii_case(".minecraft");
            if is_world {
                out.push((path, None));
            }
        }
    }
    #[cfg(windows)]
    if let Some(appdata) = dirs::data_dir() {
        let official = appdata.join(".minecraft");
        if official.join("versions").is_dir() {
            out.push((official, None));
        }
    }
    out
}

/// PCL's selected folder uses `$` for the exe directory (`$.minecraft\`).
fn pcl_expand_path(raw: &str, base: &Path) -> PathBuf {
    let expanded = raw.trim().replace('$', &base.to_string_lossy());
    PathBuf::from(expanded.trim_end_matches(['\\', '/']))
}

fn pcl_paths_equal(a: &Path, b: &Path) -> bool {
    path_key(a) == path_key(b)
}

/// PCL's INI dialect: one `Key:Value` pair per line, no sections.
fn read_pcl_ini(path: &Path) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    let Some(text) = read_text(path) else {
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

/// PCL rewrites INI files in their detected encoding: UTF-8 (with or without
/// BOM), UTF-16, or the system ANSI code page. Decode what we can.
fn read_text(path: &Path) -> Option<String> {
    let bytes = std::fs::read(path).ok()?;
    Some(decode_text(&bytes))
}

fn decode_text(bytes: &[u8]) -> String {
    if let Some(rest) = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]) {
        return String::from_utf8_lossy(rest).into_owned();
    }
    if let Some(rest) = bytes.strip_prefix(&[0xFF, 0xFE]) {
        return decode_utf16(rest, true);
    }
    if let Some(rest) = bytes.strip_prefix(&[0xFE, 0xFF]) {
        return decode_utf16(rest, false);
    }
    match std::str::from_utf8(bytes) {
        Ok(text) => text.to_string(),
        Err(_) => encoding_rs::GB18030.decode(bytes).0.into_owned(),
    }
}

fn decode_utf16(bytes: &[u8], little_endian: bool) -> String {
    let units: Vec<u16> = bytes
        .as_chunks::<2>()
        .0
        .iter()
        .map(|chunk| {
            if little_endian {
                u16::from_le_bytes([chunk[0], chunk[1]])
            } else {
                u16::from_be_bytes([chunk[0], chunk[1]])
            }
        })
        .collect();
    String::from_utf16_lossy(&units)
}

/// One HMCL/PCL configuration found beside the launcher or inside `.minecraft`.
#[derive(Debug, Clone)]
pub struct LauncherConfigHit {
    pub launcher: DirectorySource,
    pub config_path: PathBuf,
    /// Custom name from the other launcher's config, if any.
    pub name: Option<String>,
    pub game_dir: Option<PathBuf>,
    pub instance_count: usize,
    /// The other launcher currently has this directory selected.
    pub selected: bool,
}

/// Detect HMCL/PCL configs in `root` and `root/.minecraft`.
///
/// Used by the first-run wizard, which only cares about the portable layout
/// (the standard layout keeps its data under the per-user directory).
pub fn detect_launcher_configs(root: &Path, minecraft: &Path) -> Vec<LauncherConfigHit> {
    let mut hmcl = Vec::new();
    for base in [root, minecraft] {
        hmcl.extend(hmcl_configs_for_base(base));
    }
    let mut out = dedupe_game_dirs(hmcl);
    out.extend(pcl_configs_for_base(root, minecraft));
    out
}

/// PCL configs scoped to a portable install: the exe folder (normal) or
/// `.minecraft` itself (PCL unpacked inside it). Never descends into
/// `versions/{id}/PCL`.
fn pcl_configs_for_base(root: &Path, minecraft: &Path) -> Vec<LauncherConfigHit> {
    let root_setup = pcl_setup_path(root);
    let mc_setup = pcl_setup_path(minecraft);
    let (base, setup) = if root_setup.is_file() {
        (root.to_path_buf(), root_setup)
    } else if mc_setup.is_file() {
        (minecraft.to_path_buf(), mc_setup)
    } else {
        (root.to_path_buf(), root_setup)
    };

    let registry = pcl_registry_launch_folders();
    let settings = read_pcl_ini(&setup);
    let defaults = pcl_default_folders(&base);
    let has_setup = setup.is_file();
    let has_registry = registry.iter().any(|value| !value.trim().is_empty());
    let has_pcl_ini = minecraft.join("PCL.ini").is_file()
        || defaults
            .iter()
            .any(|(path, _)| path.join("PCL.ini").is_file());
    if !has_setup && !has_registry && !has_pcl_ini {
        return Vec::new();
    }

    let mut entries = Vec::new();
    for list in &registry {
        for (path, name) in pcl_parse_folder_list(list) {
            merge_pcl_entry(&mut entries, path, name, false);
        }
    }
    if let Some(folders) = settings.get("LaunchFolders") {
        for (path, name) in pcl_parse_folder_list(folders) {
            merge_pcl_entry(&mut entries, path, name, false);
        }
    }
    for (path, name) in defaults {
        merge_pcl_entry(&mut entries, path, name, false);
    }
    if minecraft.join("PCL.ini").is_file() {
        merge_pcl_entry(&mut entries, minecraft.to_path_buf(), None, false);
    }

    let selected = settings
        .get("LaunchFolderSelect")
        .map(|value| pcl_expand_path(value, &base))
        .filter(|path| !path.as_os_str().is_empty());
    if let Some(path) = &selected {
        merge_pcl_entry(&mut entries, path.clone(), None, true);
    }
    for entry in &mut entries {
        entry.selected |= selected
            .as_ref()
            .is_some_and(|path| pcl_paths_equal(path, &entry.path));
    }
    if entries.iter().all(|entry| !entry.selected) {
        if let Some(first) = entries.first_mut() {
            first.selected = true;
        }
    }

    let config_path = if has_setup {
        setup
    } else if minecraft.join("PCL.ini").is_file() {
        minecraft.join("PCL.ini")
    } else {
        setup
    };

    entries
        .into_iter()
        .map(|entry| LauncherConfigHit {
            launcher: DirectorySource::Pcl,
            config_path: config_path.clone(),
            name: entry.name,
            instance_count: count_instances(&entry.path),
            game_dir: Some(entry.path),
            selected: entry.selected,
        })
        .collect()
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
        // `.hmcl` is scanned on every platform. `Library/Application Support/hmcl`
        // is macOS-only in `hmcl_homes()`, so using it here would make this test
        // fail on Windows/Linux despite the implementation being correct.
        let hmcl_home = home.join(".hmcl");
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
        let second = temp("pcl-game-2");
        std::fs::create_dir_all(game.join("versions")).unwrap();
        let pcl = cwd.join("PCL");
        std::fs::create_dir_all(&pcl).unwrap();
        std::fs::write(
            pcl.join("Setup.ini"),
            format!(
                "LaunchFolders:我的世界>{}|第二目录>{}\nLaunchFolderSelect:{}\n",
                game.display(),
                second.display(),
                game.display()
            ),
        )
        .unwrap();

        let dirs: Vec<_> = pcl_directories(&home, &cwd)
            .into_iter()
            .filter(|(path, _, _)| {
                canonical(path) == canonical(&game) || canonical(path) == canonical(&second)
            })
            .collect();
        assert_eq!(dirs.len(), 2);
        assert_eq!(dirs[0].1, "我的世界");
        assert!(dirs[0].2, "PCL selected folder should be preferred");
        assert!(!dirs[1].2);

        let _ = std::fs::remove_dir_all(&home);
        let _ = std::fs::remove_dir_all(&cwd);
        let _ = std::fs::remove_dir_all(&game);
        let _ = std::fs::remove_dir_all(&second);
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

    #[test]
    fn onboarding_detects_hmcl_and_pcl_configs_beside_the_app() {
        let root = temp("onboarding-root");
        let minecraft = root.join(".minecraft");
        // Per-version metadata is not a directory registry: the importer must
        // not recurse into `versions/*` for it.
        std::fs::create_dir_all(minecraft.join("versions/1.20.4/.hmcl")).unwrap();
        std::fs::create_dir_all(minecraft.join("versions/1.20.4/PCL")).unwrap();
        std::fs::write(minecraft.join("versions/1.20.4/1.20.4.json"), "{}").unwrap();
        std::fs::create_dir_all(root.join(".hmcl/config")).unwrap();
        std::fs::write(
            root.join(".hmcl/config/game-directories.json"),
            format!(
                "{{\"directories\":[{{\"id\":\"a\",\"name\":\"Main\",\"path\":\"{}\"}}]}}",
                minecraft.display()
            ),
        )
        .unwrap();
        std::fs::create_dir_all(root.join("PCL")).unwrap();
        std::fs::write(
            root.join("PCL/Setup.ini"),
            format!("LaunchFolders:PCL 目录>{}\\\n", minecraft.display()),
        )
        .unwrap();

        let hits = detect_launcher_configs(&root, &minecraft);
        assert_eq!(
            hits.iter()
                .filter(|h| h.launcher == DirectorySource::Hmcl)
                .count(),
            1,
            "only the registry produces an HMCL card"
        );
        assert!(hits
            .iter()
            .any(|h| h.launcher == DirectorySource::Pcl && h.game_dir.is_some()));

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn hmcl_new_store_resolves_relative_paths_and_selection() {
        let root = temp("hmcl-new-store");
        let minecraft = root.join(".minecraft");
        std::fs::create_dir_all(minecraft.join("versions/1.20.4")).unwrap();
        std::fs::write(minecraft.join("versions/1.20.4/1.20.4.json"), "{}").unwrap();
        std::fs::create_dir_all(root.join(".hmcl/config")).unwrap();
        std::fs::write(
            root.join(".hmcl/config/game-directories.json"),
            r#"{"directories":[{"id":"game-directory:1","path":".minecraft"}]}"#,
        )
        .unwrap();
        std::fs::write(
            root.join(".hmcl/config/launcher-settings.json"),
            r#"{"selectedGameDirectory":"game-directory:1"}"#,
        )
        .unwrap();

        let hits = detect_launcher_configs(&root, &minecraft);
        let hmcl: Vec<_> = hits
            .iter()
            .filter(|hit| hit.launcher == DirectorySource::Hmcl)
            .collect();
        assert_eq!(hmcl.len(), 1);
        assert_eq!(hmcl[0].game_dir.as_deref(), Some(minecraft.as_path()));
        assert!(hmcl[0].selected);
        assert_eq!(hmcl[0].instance_count, 1);

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn hmcl_legacy_configurations_are_imported() {
        let root = temp("hmcl-legacy");
        let minecraft = root.join(".minecraft");
        std::fs::create_dir_all(minecraft.join("versions")).unwrap();
        std::fs::create_dir_all(root.join("instances/dev")).unwrap();
        std::fs::write(
            root.join(".hmcl.json"),
            r#"{
              "configurations": {
                "Default": { "gameDir": ".minecraft" },
                "Dev": { "gameDir": "instances/dev" }
              },
              "last": "Dev"
            }"#,
        )
        .unwrap();

        let hits = detect_launcher_configs(&root, &minecraft);
        let dev = hits
            .iter()
            .find(|hit| hit.name.as_deref() == Some("Dev"))
            .expect("custom profile");
        assert_eq!(
            dev.game_dir.as_deref(),
            Some(root.join("instances/dev").as_path())
        );
        assert!(dev.selected);
        let default = hits
            .iter()
            .find(|hit| hit.game_dir.as_deref() == Some(minecraft.as_path()))
            .expect("built-in profile");
        assert!(default.name.is_none());

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn pcl_dollar_selection_expands_to_the_pcl_base() {
        let root = temp("pcl-dollar");
        let minecraft = root.join(".minecraft");
        std::fs::create_dir_all(minecraft.join("versions/1.20.4")).unwrap();
        std::fs::create_dir_all(root.join("PCL")).unwrap();
        std::fs::write(
            root.join("PCL/Setup.ini"),
            "LaunchFolderSelect:$\\.minecraft\\\n",
        )
        .unwrap();

        let hits = detect_launcher_configs(&root, &minecraft);
        let pcl = hits
            .iter()
            .find(|hit| hit.launcher == DirectorySource::Pcl)
            .expect("PCL card");
        assert_eq!(pcl.game_dir.as_deref(), Some(minecraft.as_path()));
        assert!(pcl.selected);

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn pcl_minecraft_root_marker_is_enough_evidence() {
        let root = temp("pcl-marker");
        let minecraft = root.join(".minecraft");
        std::fs::create_dir_all(&minecraft).unwrap();
        std::fs::write(minecraft.join("PCL.ini"), "Version:1.20.4\n").unwrap();

        let hits = detect_launcher_configs(&root, &minecraft);
        let pcl = hits
            .iter()
            .find(|hit| hit.launcher == DirectorySource::Pcl)
            .expect("PCL card from PCL.ini");
        assert_eq!(pcl.game_dir.as_deref(), Some(minecraft.as_path()));
        assert_eq!(pcl.config_path, minecraft.join("PCL.ini"));

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn onboarding_ignores_version_subdirectory_metadata() {
        let root = temp("onboarding-no-recursion");
        let minecraft = root.join(".minecraft");
        std::fs::create_dir_all(minecraft.join("versions/1.20.4/.hmcl")).unwrap();
        std::fs::create_dir_all(minecraft.join("versions/1.20.4/PCL")).unwrap();

        assert!(detect_launcher_configs(&root, &minecraft).is_empty());

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn onboarding_skips_missing_configs() {
        let root = temp("onboarding-empty");
        let hits = detect_launcher_configs(&root, &root.join(".minecraft"));
        assert!(hits.is_empty());
        let _ = std::fs::remove_dir_all(&root);
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
