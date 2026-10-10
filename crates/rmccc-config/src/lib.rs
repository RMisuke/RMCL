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
        if result.access.can_save() && result.value.backup_on_next_save {
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
        if result.access.can_save() && result.value.backup_on_next_save {
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
    /// UI language (`zh-CN`, `en-US`, ...); empty = follow system.
    #[serde(default)]
    pub locale: Option<String>,
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
        if result.access.can_save() && result.value.backup_on_next_save {
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
}
