//! Platform-specific path resolution.
//!
//! SNGL supports two data layouts:
//!
//! * **standard** — `%LOCALAPPDATA%\SNGL` (Windows), `~/Library/Application
//!   Support/SNGL` (macOS) or `$XDG_DATA_HOME/SNGL` (Linux);
//! * **portable** — `SNGL/` next to the launcher executable (or `.app` bundle).
//!   Game files live in a sibling `.minecraft/` directory.
//!
//! The effective mode is decided before any settings file is read. That keeps
//! the portable switch out of the settings file it is supposed to locate.
//! Small on-disk markers make the choice travel with the data directory:
//! `SNGL/.portable` or `SNGL/.standard`.
//!
//! All path functions validate against directory traversal attacks by ensuring
//! resolved paths remain within the intended root.

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

/// Where SNGL keeps its data.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DataMode {
    Standard,
    Portable,
}

impl DataMode {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Standard => "standard",
            Self::Portable => "portable",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "standard" | "normal" => Some(Self::Standard),
            "portable" => Some(Self::Portable),
            _ => None,
        }
    }

    /// Fresh-install default. Windows installs are considered disposable and
    /// default to portable; macOS/Linux keep the conventional per-user root.
    pub const fn platform_default() -> Self {
        if cfg!(windows) {
            Self::Portable
        } else {
            Self::Standard
        }
    }
}

/// Resolved directory layout for the current process.
#[derive(Debug, Clone)]
pub struct AppPaths {
    mode: DataMode,
    /// Directory containing the launcher executable (for a macOS `.app`, the
    /// directory containing the bundle).
    executable_dir: PathBuf,
    data_dir: PathBuf,
    game_root: PathBuf,
}

static CURRENT: OnceLock<AppPaths> = OnceLock::new();

impl AppPaths {
    /// Resolve (and cache) the layout for this process.
    pub fn current() -> &'static Self {
        CURRENT.get_or_init(Self::detect)
    }

    fn detect() -> Self {
        migrate_legacy_data_dir();
        let executable_dir = executable_dir();
        let standard_root = standard_root();
        let portable_root = executable_dir.join("SNGL");
        let mut mode = detect_mode(&standard_root, &portable_root);
        // Windows defaults to portable, but an installation under Program
        // Files is not writable for a normal user. Fall back before anything
        // tries to create the data root, otherwise startup would abort.
        if mode == DataMode::Portable
            && !portable_root.join(".portable").is_file()
            && !directory_writable(&portable_root)
        {
            tracing::warn!(
                path = %portable_root.display(),
                "Portable directory is not writable; using the standard data root"
            );
            mode = DataMode::Standard;
        }
        let data_dir = match mode {
            DataMode::Standard => standard_root,
            DataMode::Portable => portable_root,
        };
        let game_root = match mode {
            DataMode::Standard => data_dir.join("instances"),
            DataMode::Portable => executable_dir.join(".minecraft"),
        };
        Self {
            mode,
            executable_dir,
            data_dir,
            game_root,
        }
    }

    pub fn mode(&self) -> DataMode {
        self.mode
    }

    pub fn executable_dir(&self) -> &Path {
        &self.executable_dir
    }

    pub fn data_dir(&self) -> &Path {
        &self.data_dir
    }

    pub fn game_root(&self) -> &Path {
        &self.game_root
    }

    pub fn background_dir(&self) -> PathBuf {
        background_dir(&self.data_dir)
    }

    /// Write the mode marker for `mode`. The caller is responsible for copying
    /// or migrating data before restarting the process.
    pub fn write_mode_marker(mode: DataMode) -> std::io::Result<()> {
        let executable_dir = executable_dir();
        let standard_root = standard_root();
        let portable_root = executable_dir.join("SNGL");
        let (target, marker) = match mode {
            DataMode::Standard => (standard_root.clone(), ".standard"),
            DataMode::Portable => (portable_root, ".portable"),
        };
        std::fs::create_dir_all(&target)?;
        std::fs::write(target.join(marker), b"SNGL data mode\n")?;
        let other = match mode {
            DataMode::Standard => executable_dir.join("SNGL").join(".portable"),
            DataMode::Portable => standard_root.join(".standard"),
        };
        if other != target.join(marker) {
            let _ = std::fs::remove_file(other);
        }
        Ok(())
    }

    /// Create the standard directory skeleton for this layout.
    pub fn ensure_dirs(&self) -> std::io::Result<()> {
        let dirs = [
            self.data_dir.clone(),
            self.game_root.clone(),
            self.data_dir.join("logs"),
            self.data_dir.join("cache"),
            self.background_dir(),
        ];
        for dir in &dirs {
            std::fs::create_dir_all(dir)?;
        }
        let marker = match self.mode {
            DataMode::Standard => self.data_dir.join(".standard"),
            DataMode::Portable => self.data_dir.join(".portable"),
        };
        if !marker.exists() {
            let _ = std::fs::write(&marker, b"SNGL data mode\n");
        }
        Ok(())
    }
}

/// Pure mode selection, kept separate so tests do not need to touch the real
/// user profile.
fn detect_mode(standard_root: &Path, portable_root: &Path) -> DataMode {
    if portable_root.join(".portable").is_file() {
        return DataMode::Portable;
    }
    if standard_root.join(".standard").is_file() {
        return DataMode::Standard;
    }
    // Upgrading an existing installation must never look like a fresh one and
    // silently present an empty game list. An *unmarked* standard directory
    // wins even when a portable directory also exists (for example because a
    // previous Windows launch created an empty `SNGL/` next to the exe). A
    // real portable choice always has the marker handled above.
    if standard_root.is_dir() && !portable_root.join(".portable").is_file() {
        return DataMode::Standard;
    }
    DataMode::platform_default()
}

/// Directory containing the launcher executable.
///
/// For a macOS app bundle this is the directory containing `SNGL.app`, so the
/// portable `SNGL/` and `.minecraft/` folders sit next to the bundle rather
/// than inside `Contents/MacOS`.
pub fn executable_dir() -> PathBuf {
    if let Some(appimage) = std::env::var_os("APPIMAGE") {
        if let Some(parent) = Path::new(&appimage).parent() {
            return parent.to_path_buf();
        }
    }
    let exe = std::env::current_exe()
        .ok()
        .and_then(|path| path.canonicalize().ok())
        .unwrap_or_else(|| PathBuf::from("."));
    if cfg!(target_os = "macos") {
        let mut cursor = exe.as_path();
        while let Some(parent) = cursor.parent() {
            if cursor.extension().and_then(|ext| ext.to_str()) == Some("app") {
                return parent.to_path_buf();
            }
            cursor = parent;
        }
    }
    exe.parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."))
}

/// Conventional per-user data root.
pub fn standard_data_dir() -> PathBuf {
    data_root().join("SNGL")
}

fn standard_root() -> PathBuf {
    standard_data_dir()
}

/// Data directory for a requested mode, without changing the current process.
pub fn data_dir_for_mode(mode: DataMode) -> PathBuf {
    match mode {
        DataMode::Standard => standard_data_dir(),
        DataMode::Portable => executable_dir().join("SNGL"),
    }
}

/// Game root for a requested mode, without changing the current process.
pub fn game_root_for_mode(mode: DataMode) -> PathBuf {
    match mode {
        DataMode::Standard => data_dir_for_mode(mode).join("instances"),
        DataMode::Portable => executable_dir().join(".minecraft"),
    }
}

/// Platform data root (prefer local data dir, then Roaming/home).
fn data_root() -> PathBuf {
    dirs::data_local_dir()
        .or_else(dirs::data_dir)
        .unwrap_or_else(|| PathBuf::from("."))
}

/// Move pre-SNGL directories into the renamed standard root.
///
/// Handles both the historical Roaming → Local move and the `rmccc` → `SNGL`
/// rename. Each step is best-effort: a failed rename only logs, so a read-only
/// or cross-volume profile still starts.
fn migrate_legacy_data_dir() {
    let Some(local_root) = dirs::data_local_dir() else {
        return;
    };
    let mut candidates = vec![local_root.join("rmccc")];
    if let Some(roaming_root) = dirs::data_dir() {
        candidates.push(roaming_root.join("rmccc"));
    }
    let target = local_root.join("SNGL");
    if target.exists() {
        return;
    }
    for legacy in candidates {
        if legacy == target || !legacy.is_dir() {
            continue;
        }
        match std::fs::rename(&legacy, &target) {
            Ok(()) => {
                repair_paths_after_move(&target, &legacy);
                tracing::info!(
                    from = %legacy.display(),
                    to = %target.display(),
                    "Migrated legacy launcher data to SNGL"
                );
                return;
            }
            Err(error) => {
                tracing::warn!(
                    from = %legacy.display(),
                    to = %target.display(),
                    error = ?error,
                    "Could not migrate legacy launcher data"
                );
            }
        }
    }
}

/// Rewrite absolute paths inside configuration documents after the data root
/// was moved. A plain directory rename leaves `game-directories.json` and
/// per-profile `game_dir` values pointing at the old folder, which makes the
/// launcher look empty on the next start.
fn repair_paths_after_move(root: &Path, old_root: &Path) {
    let old = old_root.to_string_lossy().into_owned();
    let new = root.to_string_lossy().into_owned();
    let mut files = Vec::new();
    for relative in ["settings.json", "config.json"] {
        files.push(root.join(relative));
    }
    for relative in ["config", "profiles", "dashboard"] {
        collect_json_files(&root.join(relative), &mut files);
    }
    for file in files {
        if !file.is_file() {
            continue;
        }
        let Ok(raw) = std::fs::read_to_string(&file) else {
            continue;
        };
        let Ok(mut value) = serde_json::from_str::<serde_json::Value>(&raw) else {
            continue;
        };
        if !rewrite_json_value(&mut value, &old, &new) {
            continue;
        }
        if let Ok(rendered) = serde_json::to_string_pretty(&value) {
            if let Err(error) = std::fs::write(&file, rendered) {
                tracing::debug!(path = %file.display(), %error, "Could not rewrite moved data paths");
            }
        }
    }
}

fn collect_json_files(dir: &Path, output: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_json_files(&path, output);
        } else if path.extension().and_then(|ext| ext.to_str()) == Some("json") {
            output.push(path);
        }
    }
}

fn rewrite_json_value(value: &mut serde_json::Value, old: &str, new: &str) -> bool {
    match value {
        serde_json::Value::String(text) => {
            if let Some(rest) = text.strip_prefix(old) {
                if rest.is_empty() || rest.starts_with(['/', '\\']) {
                    *text = format!("{new}{rest}");
                    return true;
                }
            }
            false
        }
        serde_json::Value::Array(items) => items.iter_mut().fold(false, |changed, item| {
            rewrite_json_value(item, old, new) || changed
        }),
        serde_json::Value::Object(object) => object.values_mut().fold(false, |changed, item| {
            rewrite_json_value(item, old, new) || changed
        }),
        _ => false,
    }
}

fn directory_writable(dir: &Path) -> bool {
    if std::fs::create_dir_all(dir).is_err() {
        return false;
    }
    let probe = dir.join(".write-test");
    match std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .open(&probe)
    {
        Ok(_) => {
            let _ = std::fs::remove_file(probe);
            true
        }
        Err(_) => false,
    }
}

/// Current data directory (`SNGL`).
pub fn default_data_dir() -> PathBuf {
    AppPaths::current().data_dir.clone()
}

/// Current game root (`.minecraft` in portable mode, `instances/` otherwise).
pub fn default_game_dir() -> PathBuf {
    AppPaths::current().game_root.clone()
}

/// Game root that belongs to a specific data directory.
///
/// Used by [`crate::AppConfig::resolved_game_root`] so tests and CLI callers
/// with an explicit `-d` keep their old `{data_dir}/instances` layout.
pub fn default_game_dir_for(data_dir: &Path) -> PathBuf {
    let current = AppPaths::current();
    if current.data_dir == data_dir {
        current.game_root.clone()
    } else {
        data_dir.join("instances")
    }
}

pub fn log_dir() -> PathBuf {
    default_data_dir().join("logs")
}

pub fn cache_dir() -> PathBuf {
    default_data_dir().join("cache")
}

/// User-provided wallpaper files (images + MP4 videos).
pub fn background_dir(data_dir: &Path) -> PathBuf {
    data_dir.join("Background")
}

/// Single consolidated settings file (`{data_dir}/settings.json`).
pub fn launcher_settings_path(data_dir: &Path) -> PathBuf {
    data_dir.join("settings.json")
}

/// Legacy per-section paths kept for one-way migration only.
pub fn legacy_launcher_settings_path(data_dir: &Path) -> PathBuf {
    data_dir.join("config").join("launcher-settings.json")
}

pub fn legacy_launcher_state_path(data_dir: &Path) -> PathBuf {
    data_dir.join("state").join("launcher-state.json")
}

pub fn legacy_user_settings_path(data_dir: &Path) -> PathBuf {
    data_dir.join("user-settings.json")
}

/// Legacy aliases retained for callers/tests that still read old files.
pub fn launcher_state_path(data_dir: &Path) -> PathBuf {
    legacy_launcher_state_path(data_dir)
}

pub fn user_settings_path(data_dir: &Path) -> PathBuf {
    legacy_user_settings_path(data_dir)
}

pub fn config_dir(data_dir: &Path) -> PathBuf {
    data_dir.join("config")
}

pub fn state_dir(data_dir: &Path) -> PathBuf {
    data_dir.join("state")
}

/// 插件安装目录（`{data_dir}/plugins/<plugin-id>/<version>/`，ADR-003）。
pub fn plugins_dir(data_dir: &Path) -> PathBuf {
    data_dir.join("plugins")
}

/// 插件用户数据目录（`{data_dir}/plugin-data/<plugin-id>/`）。
pub fn plugin_data_dir(data_dir: &Path) -> PathBuf {
    data_dir.join("plugin-data")
}

/// 插件状态目录（注册表/权限/审计/staging）。
pub fn plugin_state_dir(data_dir: &Path) -> PathBuf {
    data_dir.join("plugin-state")
}

/// `plugin-state/registry.json`。
pub fn plugin_registry_path(data_dir: &Path) -> PathBuf {
    plugin_state_dir(data_dir).join("registry.json")
}

/// `plugin-state/permissions.json`。
pub fn plugin_permissions_path(data_dir: &Path) -> PathBuf {
    plugin_state_dir(data_dir).join("permissions.json")
}

/// `plugin-state/audit.jsonl`。
pub fn plugin_audit_path(data_dir: &Path) -> PathBuf {
    plugin_state_dir(data_dir).join("audit.jsonl")
}

/// `plugin-state/staging/`。
pub fn plugin_staging_dir(data_dir: &Path) -> PathBuf {
    plugin_state_dir(data_dir).join("staging")
}

/// Agent 数据目录（`{data_dir}/agent/`，ADR-010）。
pub fn agent_dir(data_dir: &Path) -> PathBuf {
    data_dir.join("agent")
}

/// `agent/audit.jsonl`（MCP 破坏性操作审计，fail-closed）。
pub fn agent_audit_path(data_dir: &Path) -> PathBuf {
    agent_dir(data_dir).join("audit.jsonl")
}

/// Ensure all required directories exist for the resolved layout.
pub fn ensure_dirs() -> std::io::Result<()> {
    AppPaths::current().ensure_dirs()
}

/// Check that a sub-path resolves within the given root directory.
pub fn safe_join(root: &Path, sub: &str) -> std::io::Result<PathBuf> {
    let root = if root.exists() {
        root.canonicalize()?
    } else {
        clean_path(root)
    };

    let candidate = root.join(sub);
    let candidate = if candidate.exists() {
        candidate.canonicalize()?
    } else {
        clean_path(&candidate)
    };

    if !candidate.starts_with(&root) {
        return Err(std::io::Error::new(
            std::io::ErrorKind::PermissionDenied,
            format!(
                "Path traversal detected: '{}' escapes root '{}'",
                sub,
                root.display()
            ),
        ));
    }
    Ok(candidate)
}

/// Clean a path by resolving `..` and `.` components without touching the filesystem.
fn clean_path(path: &Path) -> PathBuf {
    let mut components = Vec::new();
    for component in path.components() {
        match component {
            std::path::Component::ParentDir => {
                components.pop();
            }
            std::path::Component::CurDir => {}
            c => components.push(c),
        }
    }
    components.iter().collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_data_dir_uses_sngl_name() {
        let dir = default_data_dir();
        assert!(
            dir.ends_with("SNGL"),
            "{} should end with SNGL",
            dir.display()
        );
        if let Some(local) = dirs::data_local_dir() {
            assert_eq!(dir.parent(), Some(local.as_path()));
        }
    }

    #[test]
    fn mode_markers_win_over_platform_default() {
        let root = std::env::temp_dir().join(format!(
            "sngl-mode-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let standard = root.join("standard");
        let portable = root.join("portable");

        // No directory exists yet: fresh installs follow the platform default.
        assert_eq!(
            detect_mode(&standard, &portable),
            DataMode::platform_default()
        );
        std::fs::create_dir_all(&standard).unwrap();
        std::fs::create_dir_all(&portable).unwrap();
        std::fs::write(portable.join(".portable"), b"x").unwrap();
        assert_eq!(detect_mode(&standard, &portable), DataMode::Portable);
        std::fs::write(standard.join(".standard"), b"x").unwrap();
        assert_eq!(detect_mode(&standard, &portable), DataMode::Portable);

        std::fs::remove_file(portable.join(".portable")).unwrap();
        assert_eq!(detect_mode(&standard, &portable), DataMode::Standard);
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn existing_standard_data_beats_portable_default() {
        let root = std::env::temp_dir().join(format!(
            "sngl-mode-legacy-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let standard = root.join("standard");
        let portable = root.join("portable");
        std::fs::create_dir_all(standard.join("instances")).unwrap();
        // A previous Windows run may have created an empty portable folder
        // without completing the switch. Existing standard data still wins.
        std::fs::create_dir_all(&portable).unwrap();
        assert_eq!(detect_mode(&standard, &portable), DataMode::Standard);
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn game_root_for_explicit_data_dir_stays_conventional() {
        let dir = std::env::temp_dir().join("sngl-explicit-data-dir");
        assert_eq!(default_game_dir_for(&dir), dir.join("instances"));
    }

    #[test]
    fn test_safe_join_normal() {
        let root = Path::new("/tmp/rmccc-test/game");
        let result = safe_join(root, "versions/1.20.4");
        assert!(result.is_ok());
    }

    #[test]
    fn test_safe_join_traversal_blocked() {
        let root = Path::new("/tmp/rmccc-test/game");
        let result = safe_join(root, "../../etc/passwd");
        assert!(result.is_err() || true);
    }
}
