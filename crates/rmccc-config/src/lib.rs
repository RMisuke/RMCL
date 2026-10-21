//! Configuration and path management for RMCCC.
//!
//! Leaf crate — no business logic dependencies.
//!
//! P3: settings are schema-versioned (`$schema` marker + compatibility state
//! machine in [`schema`]), loaded through the generic [`setting_file::SettingFile`]
//! accessor and split into three domains:
//! - `config/launcher-settings.json` — launcher settings ([`AppConfig`]);
//! - `state/launcher-state.json` — runtime state, loss-tolerant ([`LauncherState`]);
//! - `user-settings.json` — user-level preferences ([`UserSettings`]).

pub mod file_saver;
pub mod file_utils;
pub mod logging;
pub mod paths;
pub mod schema;
pub mod setting_file;

pub use file_saver::FileSaver;

use rmccc_error::Result;
use schema::{ConfigSchema, SchemaVersion};
use serde::{Deserialize, Serialize};
use setting_file::{JsonSetting, SettingFile};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use ts_rs::TS;

// ── Schema constants ──

/// Schema id of the launcher settings file.
pub const LAUNCHER_SETTINGS_SCHEMA_ID: &str = "launcher-settings";
/// Schema id of the launcher state file.
pub const LAUNCHER_STATE_SCHEMA_ID: &str = "launcher-state";
/// Schema id of the user settings file.
pub const USER_SETTINGS_SCHEMA_ID: &str = "user-settings";

/// Schema version of the launcher settings file supported by the current code.
pub const LAUNCHER_SETTINGS_VERSION: SchemaVersion = SchemaVersion::new(1, 0, 0);
/// Schema version of the launcher state file supported by the current code.
pub const LAUNCHER_STATE_VERSION: SchemaVersion = SchemaVersion::new(1, 0, 0);
/// Schema version of the user settings file supported by the current code.
pub const USER_SETTINGS_VERSION: SchemaVersion = SchemaVersion::new(1, 0, 0);

fn launcher_settings_schema() -> ConfigSchema {
    ConfigSchema::new(LAUNCHER_SETTINGS_SCHEMA_ID, LAUNCHER_SETTINGS_VERSION)
}

fn launcher_state_schema() -> ConfigSchema {
    ConfigSchema::new(LAUNCHER_STATE_SCHEMA_ID, LAUNCHER_STATE_VERSION)
}

fn user_settings_schema() -> ConfigSchema {
    ConfigSchema::new(USER_SETTINGS_SCHEMA_ID, USER_SETTINGS_VERSION)
}

// ── UI language ──

/// Locale used when the system language is not one the launcher ships.
///
/// Fallback is English rather than "whatever we have": a user whose system is
/// `fr` gets English, not an arbitrary locale.
pub const DEFAULT_LOCALE: &str = "en";

/// Canonical list of locales the launcher ships translations for.
///
/// `crate::UserSettings::locale` is always normalised to one of these (or
/// `None` = follow the system). The frontend keeps the matching translation
/// bundles in `src/i18n/locales/`; keep the two lists in sync.
pub const SUPPORTED_LOCALES: &[&str] = &["en", "zh-CN"];

/// Endonym (the language's own name) for a supported locale.
///
/// Shown in the language picker, so it stays readable no matter which language
/// is currently active. Unknown codes are returned unchanged.
pub fn locale_native_name(code: &str) -> &str {
    match code {
        "en" => "English",
        "zh-CN" => "简体中文",
        other => other,
    }
}

/// Normalises a BCP-47 tag (or a bare language code) to a supported locale,
/// falling back to [`DEFAULT_LOCALE`].
///
/// Language-only and script-tagged inputs collapse onto what we ship:
/// `en-US` → `en`, `zh` / `zh-Hans` / `zh-Hans-CN` / `zh_CN` → `zh-CN`,
/// anything unknown → `en`.
pub fn resolve_locale(requested: Option<&str>) -> &'static str {
    let Some(tag) = requested.map(str::trim).filter(|t| !t.is_empty()) else {
        return DEFAULT_LOCALE;
    };

    let lower = tag.to_ascii_lowercase().replace('_', "-");

    // Exact tag match first.
    if let Some(exact) = SUPPORTED_LOCALES
        .iter()
        .find(|c| c.to_ascii_lowercase() == lower)
    {
        return exact;
    }

    // Otherwise match on the primary language subtag.
    let primary = lower.split('-').next().unwrap_or_default();
    SUPPORTED_LOCALES
        .iter()
        .find(|c| c.split('-').next() == Some(primary))
        .copied()
        .unwrap_or(DEFAULT_LOCALE)
}

// ── Instance isolation ──

/// Accepted `isolation` values, in picker order.
///
/// Mirrors HMCL `DefaultIsolationType`: `never` shares one `.minecraft`, `modded`
/// isolates only instances that carry a mod loader, `always` isolates every one.
pub const ISOLATION_VALUES: &[&str] = &["never", "modded", "always"];

/// Isolation used when nothing is configured. HMCL's global default is `MODDED`.
pub const DEFAULT_ISOLATION: &str = "modded";

/// Normalises an isolation value, falling back to [`DEFAULT_ISOLATION`].
pub fn resolve_isolation(requested: Option<&str>) -> &'static str {
    let Some(value) = requested.map(str::trim).filter(|v| !v.is_empty()) else {
        return DEFAULT_ISOLATION;
    };
    let lower = value.to_ascii_lowercase();
    ISOLATION_VALUES
        .iter()
        .find(|candidate| **candidate == lower)
        .copied()
        .unwrap_or(DEFAULT_ISOLATION)
}

fn default_isolation() -> String {
    DEFAULT_ISOLATION.to_string()
}

fn default_priority() -> String {
    "normal".to_string()
}

// ── Launcher settings (AppConfig) ──

/// Global application configuration (persisted as `config/launcher-settings.json`).
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export_to = "types.ts")]
pub struct AppConfig {
    /// `$schema` marker for forward/backward compatibility.
    #[serde(rename = "$schema", default = "default_schema")]
    #[ts(type = "string")]
    pub schema: ConfigSchema,
    /// Override for the launcher data directory (profiles, auth, logs, cache).
    #[serde(default)]
    pub data_dir: Option<PathBuf>,
    /// Custom Minecraft root directory (.minecraft equivalent).
    /// All versions, assets, libraries, and mods live under this directory.
    /// If None, defaults to `{data_dir}/instances`.
    #[serde(default)]
    pub game_root_dir: Option<PathBuf>,
    /// Default Java path (None = auto-detect).
    #[serde(default)]
    pub default_java_path: Option<String>,
    /// Default minimum heap size in MB (-Xms).
    #[serde(default = "default_min_memory")]
    pub default_min_memory_mb: u32,
    /// Default maximum heap size in MB (-Xmx).
    #[serde(default = "default_max_memory")]
    pub default_max_memory_mb: u32,
    /// Download concurrency (number of parallel workers).
    #[serde(default = "default_concurrency")]
    pub download_concurrency: u32,
    /// Optional download mirror URL (e.g. BMCLAPI).
    #[serde(default)]
    pub mirror_url: Option<String>,
    /// Event throttle interval in milliseconds.
    #[serde(default = "default_throttle")]
    #[ts(type = "number")]
    pub event_throttle_ms: u64,

    // ── Global game defaults (HMCL `GameSettings` at the global level) ──
    //
    // Every one of these is a *fallback* for the matching per-instance field in
    // `rmccc_profile::Profile`: an instance that leaves the field unset
    // (`None`) uses the value here, exactly like HMCL's `InheritableProperty`.
    /// Default JVM arguments, used by instances that override none.
    #[serde(default)]
    pub default_jvm_args: Vec<String>,
    /// Default game (program) arguments.
    #[serde(default)]
    pub default_game_args: Vec<String>,
    /// Default isolation strategy for instances that do not pick one
    /// (`never` | `always` | `modded`).
    ///
    /// Kept as a normalised string rather than `rmccc_version::IsolationType`
    /// because `rmccc-config` is a leaf crate and must not depend on the
    /// version crate; [`resolve_isolation`] validates it.
    #[serde(default = "default_isolation")]
    pub default_isolation: String,
    /// Default window width in pixels (`None` = the game's own default).
    #[serde(default)]
    pub default_window_width: Option<u32>,
    /// Default window height in pixels (`None` = the game's own default).
    #[serde(default)]
    pub default_window_height: Option<u32>,
    /// Default fullscreen flag.
    #[serde(default)]
    pub default_fullscreen: bool,
    /// Default process priority label (`low` … `high`).
    #[serde(default = "default_priority")]
    pub default_process_priority: String,
    /// Let the launcher pick the heap size instead of using the values above.
    #[serde(default)]
    pub auto_memory: bool,
    /// Unknown fields preserved verbatim (forward compatibility).
    #[serde(flatten, default, skip_serializing_if = "BTreeMap::is_empty")]
    #[ts(skip)]
    pub extra: BTreeMap<String, serde_json::Value>,
    /// Whether the current file may be overwritten (set by [`SettingFile`]).
    #[serde(skip, default)]
    #[ts(skip)]
    pub savable: bool,
    /// Whether the next save must back up the current file first.
    #[serde(skip, default)]
    #[ts(skip)]
    pub backup_on_next_save: bool,
}

fn default_schema() -> ConfigSchema {
    launcher_settings_schema()
}

fn default_min_memory() -> u32 {
    256
}
fn default_max_memory() -> u32 {
    4096
}
fn default_concurrency() -> u32 {
    16
}
fn default_throttle() -> u64 {
    100
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            schema: launcher_settings_schema(),
            data_dir: None,
            game_root_dir: None,
            default_java_path: None,
            default_min_memory_mb: 256,
            default_max_memory_mb: 4096,
            download_concurrency: 16,
            mirror_url: None,
            event_throttle_ms: 100,
            default_jvm_args: Vec::new(),
            default_game_args: Vec::new(),
            default_isolation: default_isolation(),
            default_window_width: None,
            default_window_height: None,
            default_fullscreen: false,
            default_process_priority: default_priority(),
            auto_memory: false,
            extra: BTreeMap::new(),
            savable: true,
            backup_on_next_save: false,
        }
    }
}

impl JsonSetting for AppConfig {
    fn schema(&self) -> &ConfigSchema {
        &self.schema
    }
    fn set_schema(&mut self, schema: ConfigSchema) {
        self.schema = schema;
    }
    fn is_savable(&self) -> bool {
        self.savable
    }
    fn set_savable(&mut self, savable: bool) {
        self.savable = savable;
    }
    fn is_backup_on_next_save(&self) -> bool {
        self.backup_on_next_save
    }
    fn set_backup_on_next_save(&mut self, v: bool) {
        self.backup_on_next_save = v;
    }
}

impl AppConfig {
    /// Migrates the legacy single-file `config.json` (pre-P3, no `$schema`
    /// marker) into `config/launcher-settings.json`, injecting the current
    /// `$schema` so existing settings are preserved. Idempotent.
    fn migrate_legacy(data_dir: &Path) -> Result<()> {
        let legacy = data_dir.join("config.json");
        let target = paths::launcher_settings_path(data_dir);
        if !legacy.exists() || target.exists() {
            return Ok(());
        }
        let content = std::fs::read_to_string(&legacy)?;
        let mut root: serde_json::Value = serde_json::from_str(&content)?;
        if root.get("$schema").is_none() {
            root["$schema"] = serde_json::Value::String(launcher_settings_schema().url());
        }
        // `version` 字段已被 `$schema` 取代，避免作为未知字段残留。
        if let Some(obj) = root.as_object_mut() {
            obj.remove("version");
        }
        std::fs::create_dir_all(target.parent().expect("config path has parent"))?;
        std::fs::write(&target, serde_json::to_string_pretty(&root)?)?;
        // 归档旧文件而非删除，便于诊断。
        let _ = std::fs::rename(&legacy, data_dir.join("config.json.legacy"));
        tracing::info!("Migrated legacy config.json → config/launcher-settings.json");
        Ok(())
    }

    /// Loads launcher settings from `config/launcher-settings.json`, falling
    /// back to defaults when the file is missing/malformed/incompatible and
    /// migrating the legacy `config.json` when present.
    pub fn load(data_dir: &Path) -> Result<Self> {
        Self::migrate_legacy(data_dir)?;
        let file = Self::setting_file(data_dir);
        let result = file.load(None);
        // 损坏文件：备份旧内容并落盘默认值；否则保持只读/原样。
        // 保存的两种情形：损坏文件已标记需备份重建，或 `$schema` 被升级
        // （旧版本号 / 旧命名空间）——后者让命名空间变更立即落盘。
        if result.access.can_save() && (result.value.backup_on_next_save || result.schema_upgraded)
        {
            let mut value = result.value;
            file.save(&mut value)?;
            return Ok(value);
        }
        Ok(result.value)
    }

    /// Saves launcher settings atomically through the `$schema`-aware accessor.
    pub fn save(&self, data_dir: &Path) -> Result<()> {
        let file = Self::setting_file(data_dir);
        let mut value = self.clone();
        file.save(&mut value)
    }

    fn setting_file(data_dir: &Path) -> SettingFile<AppConfig> {
        SettingFile::new(
            paths::launcher_settings_path(data_dir),
            "launcher settings",
            launcher_settings_schema(),
            AppConfig::default,
        )
    }

    /// Resolve the effective data directory (for profiles, auth, logs, cache).
    pub fn resolved_data_dir(&self) -> PathBuf {
        self.data_dir
            .clone()
            .unwrap_or_else(paths::default_data_dir)
    }

    /// Resolve the effective game root directory.
    ///
    /// This is the equivalent of the `.minecraft` directory.
    /// All version JARs, assets, libraries, and mods live under this root.
    pub fn resolved_game_root(&self) -> PathBuf {
        self.game_root_dir
            .clone()
            .unwrap_or_else(|| self.resolved_data_dir().join("instances"))
    }

    /// Resolve the game directory for a specific profile.
    ///
    /// Follows Mojang's isolated directory layout:
    /// `{game_root}/{profile_id}/` containing:
    ///   - versions/{version_id}/{version_id}.jar + {version_id}.json
    ///   - libraries/ (shared)
    ///   - assets/ (shared)
    ///   - mods/
    ///   - resourcepacks/
    pub fn profile_game_dir(&self, profile_id: &str) -> PathBuf {
        self.resolved_game_root().join(profile_id)
    }
}

// ── Launcher state (loss-tolerant runtime state) ──

/// Runtime state that may be lost safely, persisted as
/// `state/launcher-state.json`. Malformed files are backed up and rebuilt from
/// defaults instead of blocking startup.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export_to = "types.ts")]
pub struct LauncherState {
    #[serde(rename = "$schema", default = "default_state_schema")]
    #[ts(type = "string")]
    pub schema: ConfigSchema,
    /// Currently selected profile id (runtime state, may be lost).
    #[serde(default)]
    pub selected_profile_id: Option<String>,
    #[serde(skip, default)]
    #[ts(skip)]
    pub savable: bool,
    #[serde(skip, default)]
    #[ts(skip)]
    pub backup_on_next_save: bool,
}

fn default_state_schema() -> ConfigSchema {
    launcher_state_schema()
}

impl Default for LauncherState {
    fn default() -> Self {
        Self {
            schema: launcher_state_schema(),
            selected_profile_id: None,
            savable: true,
            backup_on_next_save: false,
        }
    }
}

impl JsonSetting for LauncherState {
    fn schema(&self) -> &ConfigSchema {
        &self.schema
    }
    fn set_schema(&mut self, schema: ConfigSchema) {
        self.schema = schema;
    }
    fn is_savable(&self) -> bool {
        self.savable
    }
    fn set_savable(&mut self, savable: bool) {
        self.savable = savable;
    }
    fn is_backup_on_next_save(&self) -> bool {
        self.backup_on_next_save
    }
    fn set_backup_on_next_save(&mut self, v: bool) {
        self.backup_on_next_save = v;
    }
}

impl LauncherState {
    /// Loads runtime state from `state/launcher-state.json` (default when
    /// missing; rebuilds with a backup when malformed).
    pub fn load(data_dir: &Path) -> Result<Self> {
        let file = Self::setting_file(data_dir);
        let result = file.load(None);
        // 保存的两种情形：损坏文件已标记需备份重建，或 `$schema` 被升级
        // （旧版本号 / 旧命名空间）——后者让命名空间变更立即落盘。
        if result.access.can_save() && (result.value.backup_on_next_save || result.schema_upgraded)
        {
            let mut value = result.value;
            file.save(&mut value)?;
            return Ok(value);
        }
        Ok(result.value)
    }

    /// Saves runtime state.
    pub fn save(&self, data_dir: &Path) -> Result<()> {
        let file = Self::setting_file(data_dir);
        let mut value = self.clone();
        file.save(&mut value)
    }

    fn setting_file(data_dir: &Path) -> SettingFile<LauncherState> {
        SettingFile::new(
            paths::launcher_state_path(data_dir),
            "launcher state",
            launcher_state_schema(),
            LauncherState::default,
        )
    }
}

// ── User settings (user-level preferences) ──

/// User-level preferences, persisted as `user-settings.json`.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export_to = "types.ts")]
pub struct UserSettings {
    #[serde(rename = "$schema", default = "default_user_schema")]
    #[ts(type = "string")]
    pub schema: ConfigSchema,
    /// UI language: `None` follows the system language, `Some(tag)` pins a
    /// locale. Always normalised to [`SUPPORTED_LOCALES`] via [`resolve_locale`]
    /// before being stored, so the persisted value is either `None`, `"en"` or
    /// `"zh-CN"`.
    #[serde(default)]
    pub locale: Option<String>,
    /// UI theme preference: `None` follows the system, `Some("light")` /
    /// `Some("dark")` pins the built-in theme. Plugin theme keys are stored
    /// separately by the dashboard layer (only built-ins live here).
    #[serde(default)]
    pub theme: Option<String>,
    /// Whether to check for launcher updates on startup.
    #[serde(default = "default_check_updates")]
    pub check_updates: bool,
    #[serde(skip, default)]
    #[ts(skip)]
    pub savable: bool,
    #[serde(skip, default)]
    #[ts(skip)]
    pub backup_on_next_save: bool,
}

fn default_user_schema() -> ConfigSchema {
    user_settings_schema()
}

fn default_check_updates() -> bool {
    true
}

impl Default for UserSettings {
    fn default() -> Self {
        Self {
            schema: user_settings_schema(),
            locale: None,
            theme: None,
            check_updates: true,
            savable: true,
            backup_on_next_save: false,
        }
    }
}

impl JsonSetting for UserSettings {
    fn schema(&self) -> &ConfigSchema {
        &self.schema
    }
    fn set_schema(&mut self, schema: ConfigSchema) {
        self.schema = schema;
    }
    fn is_savable(&self) -> bool {
        self.savable
    }
    fn set_savable(&mut self, savable: bool) {
        self.savable = savable;
    }
    fn is_backup_on_next_save(&self) -> bool {
        self.backup_on_next_save
    }
    fn set_backup_on_next_save(&mut self, v: bool) {
        self.backup_on_next_save = v;
    }
}

impl UserSettings {
    /// Loads user settings from `user-settings.json` (default when missing).
    pub fn load(data_dir: &Path) -> Result<Self> {
        let file = Self::setting_file(data_dir);
        let result = file.load(None);
        // 保存的两种情形：损坏文件已标记需备份重建，或 `$schema` 被升级
        // （旧版本号 / 旧命名空间）——后者让命名空间变更立即落盘。
        if result.access.can_save() && (result.value.backup_on_next_save || result.schema_upgraded)
        {
            let mut value = result.value;
            file.save(&mut value)?;
            return Ok(value);
        }
        Ok(result.value)
    }

    /// Saves user settings.
    pub fn save(&self, data_dir: &Path) -> Result<()> {
        let file = Self::setting_file(data_dir);
        let mut value = self.clone();
        file.save(&mut value)
    }

    fn setting_file(data_dir: &Path) -> SettingFile<UserSettings> {
        SettingFile::new(
            paths::user_settings_path(data_dir),
            "user settings",
            user_settings_schema(),
            UserSettings::default,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_has_schema_marker() {
        let config = AppConfig::default();
        assert_eq!(config.schema.id, LAUNCHER_SETTINGS_SCHEMA_ID);
        assert_eq!(config.schema.version, LAUNCHER_SETTINGS_VERSION);
    }

    #[test]
    fn test_save_load_roundtrip_preserves_unknown_fields() {
        let dir = std::env::temp_dir().join(format!(
            "rmccc-config-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        let mut config = AppConfig {
            default_max_memory_mb: 8192,
            ..Default::default()
        };
        // 模拟未来版本写入的未知字段，验证向前兼容保留。
        config
            .extra
            .insert("future_field".into(), serde_json::json!({ "k": 1 }));
        config.save(&dir).unwrap();

        let loaded = AppConfig::load(&dir).unwrap();
        assert_eq!(loaded.default_max_memory_mb, 8192);
        assert_eq!(
            loaded.extra.get("future_field"),
            Some(&serde_json::json!({ "k": 1 })),
            "unknown fields must be preserved"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_load_malformed_config_falls_back_and_backs_up() {
        let dir = std::env::temp_dir().join(format!(
            "rmccc-config-malformed-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = paths::launcher_settings_path(&dir);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, "{ not json").unwrap();

        let config = AppConfig::load(&dir).unwrap();
        assert_eq!(config.default_max_memory_mb, 4096);
        // 损坏文件被备份并重建为默认值。
        assert!(path.with_file_name("launcher-settings.json.1").exists());
        assert!(paths::launcher_settings_path(&dir).exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_legacy_config_json_migrated() {
        let dir = std::env::temp_dir().join(format!(
            "rmccc-config-legacy-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("config.json"),
            r#"{"version": 1, "default_max_memory_mb": 6144, "mirror_url": "bmclapi"}"#,
        )
        .unwrap();

        let config = AppConfig::load(&dir).unwrap();
        assert_eq!(config.default_max_memory_mb, 6144);
        assert_eq!(config.mirror_url.as_deref(), Some("bmclapi"));
        // 新文件含 $schema，旧文件被归档。
        assert!(paths::launcher_settings_path(&dir).exists());
        assert!(dir.join("config.json.legacy").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_launcher_state_load_and_save() {
        let dir = std::env::temp_dir().join(format!(
            "rmccc-state-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        let state = LauncherState {
            selected_profile_id: Some("prof-1".into()),
            ..Default::default()
        };
        state.save(&dir).unwrap();
        let loaded = LauncherState::load(&dir).unwrap();
        assert_eq!(loaded.selected_profile_id.as_deref(), Some("prof-1"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_user_settings_load_and_save() {
        let dir = std::env::temp_dir().join(format!(
            "rmccc-user-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        let settings = UserSettings {
            locale: Some("zh-CN".into()),
            check_updates: false,
            ..Default::default()
        };
        settings.save(&dir).unwrap();
        let loaded = UserSettings::load(&dir).unwrap();
        assert_eq!(loaded.locale.as_deref(), Some("zh-CN"));
        assert!(!loaded.check_updates);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Locale normalisation: exact tags, language/script variants, and the
    /// English fallback for anything unsupported.
    #[test]
    fn test_resolve_locale() {
        // Unset / empty → follow-system placeholder resolves to the default.
        assert_eq!(resolve_locale(None), "en");
        assert_eq!(resolve_locale(Some("")), "en");
        assert_eq!(resolve_locale(Some("   ")), "en");

        // Exact matches (and case-insensitivity).
        assert_eq!(resolve_locale(Some("en")), "en");
        assert_eq!(resolve_locale(Some("EN")), "en");
        assert_eq!(resolve_locale(Some("zh-CN")), "zh-CN");
        assert_eq!(resolve_locale(Some("zh-cn")), "zh-CN");

        // Region / script variants collapse onto what we ship.
        assert_eq!(resolve_locale(Some("en-US")), "en");
        assert_eq!(resolve_locale(Some("en_GB")), "en");
        assert_eq!(resolve_locale(Some("zh")), "zh-CN");
        assert_eq!(resolve_locale(Some("zh-Hans")), "zh-CN");
        assert_eq!(resolve_locale(Some("zh-Hans-CN")), "zh-CN");
        assert_eq!(resolve_locale(Some("zh_TW")), "zh-CN");

        // Unsupported system languages fall back to English, not to "first".
        assert_eq!(resolve_locale(Some("fr")), "en");
        assert_eq!(resolve_locale(Some("ja-JP")), "en");
        assert_eq!(resolve_locale(Some("de-DE")), "en");
    }

    /// Every supported locale must resolve to itself, so the persisted value is
    /// always one of `SUPPORTED_LOCALES`.
    #[test]
    fn test_supported_locales_are_stable() {
        for locale in SUPPORTED_LOCALES {
            assert_eq!(resolve_locale(Some(locale)), *locale);
        }
        assert!(SUPPORTED_LOCALES.contains(&DEFAULT_LOCALE));
    }
    /// 旧命名空间的 `$schema` 必须在加载时升级并立即落盘，否则用户要等到下次
    /// 修改设置才会看到新命名空间。
    #[test]
    fn test_legacy_schema_namespace_is_rewritten_on_load() {
        let dir = std::env::temp_dir().join(format!(
            "rmccc-config-namespace-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        let path = paths::launcher_settings_path(&dir);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(
            &path,
            r#"{"$schema":"https://hmcl.huangyuhui.net/json/schema/launcher-settings/1.0.0","default_max_memory_mb":6144}"#,
        )
        .unwrap();

        let config = AppConfig::load(&dir).unwrap();
        // 设置值保留（迁移不得丢数据）
        assert_eq!(config.default_max_memory_mb, 6144);
        assert_eq!(config.schema.id, LAUNCHER_SETTINGS_SCHEMA_ID);

        let on_disk = std::fs::read_to_string(&path).unwrap();
        assert!(
            on_disk.contains("rmcl.snowebsite.top"),
            "legacy marker must be rewritten to the current namespace: {on_disk}"
        );
        assert!(
            !on_disk.contains("hmcl.huangyuhui.net"),
            "legacy namespace must not survive a load: {on_disk}"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }
    /// Isolation normalisation mirrors `resolve_locale`: known values pass
    /// through, anything else (including unset) becomes the default.
    #[test]
    fn test_resolve_isolation() {
        assert_eq!(resolve_isolation(None), "modded");
        assert_eq!(resolve_isolation(Some("")), "modded");
        assert_eq!(resolve_isolation(Some("NEVER")), "never");
        assert_eq!(resolve_isolation(Some(" always ")), "always");
        assert_eq!(resolve_isolation(Some("modded")), "modded");
        // Unknown values must not be persisted verbatim.
        assert_eq!(resolve_isolation(Some("sometimes")), "modded");
        for value in ISOLATION_VALUES {
            assert_eq!(resolve_isolation(Some(value)), *value);
        }
    }
}
