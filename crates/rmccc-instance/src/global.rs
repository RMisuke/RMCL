//! Persistent global game-settings preset (`config/game-settings.json`).
//!
//! HMCL keeps one or more `GameSettings.Preset` objects and each instance
//! points at one via `parent`. RMCCC currently ships a single default preset;
//! the schema keeps a `presets` map so multi-preset support can be added
//! without another file migration.

use crate::settings::GlobalGameSettings;
use rmccc_config::schema::{ConfigSchema, SchemaVersion};
use rmccc_config::AppConfig;
use rmccc_error::{Result, RmcccError};
use rmccc_platform::PlatformFs;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// Id of the built-in default preset.
pub const DEFAULT_PRESET_ID: &str = "default";

/// Current schema for `config/game-settings.json`.
pub fn game_settings_schema() -> ConfigSchema {
    ConfigSchema::new("game-settings", SchemaVersion::new(1, 0, 0))
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct GameSettingsFile {
    #[serde(rename = "$schema", default = "game_settings_schema")]
    schema: ConfigSchema,
    #[serde(default = "default_preset_id")]
    default_preset: String,
    #[serde(default)]
    presets: BTreeMap<String, GlobalGameSettings>,
}

fn default_preset_id() -> String {
    DEFAULT_PRESET_ID.to_string()
}

impl Default for GameSettingsFile {
    fn default() -> Self {
        Self {
            schema: game_settings_schema(),
            default_preset: default_preset_id(),
            presets: BTreeMap::new(),
        }
    }
}

/// Global game-settings store.
pub struct GlobalGameSettingsStore {
    path: PathBuf,
    fs: Arc<dyn PlatformFs>,
    file: GameSettingsFile,
}

impl GlobalGameSettingsStore {
    /// Load the store, seeding the default preset from `AppConfig` on first run.
    pub fn load(data_dir: &Path, config: &AppConfig, fs: Arc<dyn PlatformFs>) -> Result<Self> {
        let path = rmccc_config::paths::config_dir(data_dir).join("game-settings.json");
        let mut store = Self {
            path,
            fs,
            file: GameSettingsFile::default(),
        };

        if store.path.exists() {
            match std::fs::read_to_string(&store.path)
                .map_err(RmcccError::from)
                .and_then(|text| {
                    serde_json::from_str::<GameSettingsFile>(&text).map_err(RmcccError::from)
                }) {
                Ok(file) => store.file = file,
                Err(e) => {
                    tracing::warn!(
                        path = %store.path.display(),
                        error = %e,
                        "Global game settings are unreadable; rebuilding from AppConfig"
                    );
                    let backup = store.path.with_extension("json.invalid");
                    let _ = std::fs::rename(&store.path, backup);
                }
            }
        }

        let mut changed = false;
        if store.file.presets.is_empty() {
            store.file.presets.insert(
                store.file.default_preset.clone(),
                GlobalGameSettings::from_app_config(config),
            );
            changed = true;
        }
        if !store.file.presets.contains_key(&store.file.default_preset) {
            store.file.default_preset = store
                .file
                .presets
                .keys()
                .next()
                .cloned()
                .unwrap_or_else(default_preset_id);
            changed = true;
        }
        if changed {
            store.save()?;
        }
        Ok(store)
    }

    /// Path of the store file.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// The default preset.
    pub fn settings(&self) -> &GlobalGameSettings {
        self.file
            .presets
            .get(&self.file.default_preset)
            .expect("default preset is guaranteed by load")
    }

    /// Replace the default preset and persist it.
    pub fn replace(&mut self, settings: GlobalGameSettings) -> Result<()> {
        self.file
            .presets
            .insert(self.file.default_preset.clone(), settings);
        self.save()
    }

    /// Persist the store atomically.
    pub fn save(&mut self) -> Result<()> {
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let json = serde_json::to_string_pretty(&self.file)?;
        let tmp = self.path.with_extension("json.tmp");
        std::fs::write(&tmp, json)?;
        self.fs.atomic_replace(&tmp, &self.path)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "rmccc-gsettings-{tag}-{}-{}",
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
    fn seeds_from_app_config_and_round_trips() {
        let data = temp("seed");
        let config = AppConfig {
            data_dir: Some(data.clone()),
            default_max_memory_mb: 6144,
            default_jvm_args: vec!["-Dfoo=bar".into()],
            default_isolation: "always".into(),
            ..Default::default()
        };
        let platform = rmccc_platform::Platform::current();
        let mut store = GlobalGameSettingsStore::load(&data, &config, platform.fs.clone()).unwrap();
        assert_eq!(store.settings().max_memory_mb, 6144);
        assert_eq!(store.settings().jvm_args, vec!["-Dfoo=bar"]);
        assert_eq!(
            store.settings().default_isolation,
            crate::settings::IsolationPolicy::Always
        );

        let mut settings = store.settings().clone();
        settings.max_memory_mb = 8192;
        store.replace(settings).unwrap();

        let reloaded = GlobalGameSettingsStore::load(&data, &config, platform.fs).unwrap();
        assert_eq!(reloaded.settings().max_memory_mb, 8192);
        let _ = std::fs::remove_dir_all(&data);
    }
}
