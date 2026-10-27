//! Registered game directories (`.minecraft`-equivalent roots).
//!
//! HMCL keeps game directories in separate stores (`local-game-directories.json`
//! / `user-game-directories.json`) and selects one active directory; instances
//! are only visible inside the selected directory. RMCCC uses a single registry
//! with a `selected` marker — simpler, while keeping the same invariant: an
//! instance key always names its directory, so the same version id may exist in
//! several roots without ambiguity.

use crate::id::GameDirectoryId;
use rmccc_config::schema::{ConfigSchema, SchemaVersion};
use rmccc_config::AppConfig;
use rmccc_error::{Result, RmcccError};
use rmccc_platform::PlatformFs;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use ts_rs::TS;

/// Whether a directory is owned by the launcher or merely registered.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export_to = "types.ts")]
pub enum GameDirectoryKind {
    /// Created and managed by RMCCC (the default `.minecraft` root).
    #[default]
    Managed,
    /// An existing game directory (HMCL, another launcher, a shared folder).
    External,
}

impl GameDirectoryKind {
    /// Stable wire name.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Managed => "managed",
            Self::External => "external",
        }
    }
}

/// A registered game directory.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export_to = "types.ts")]
pub struct GameDirectory {
    pub id: GameDirectoryId,
    pub name: String,
    pub path: PathBuf,
    #[serde(default)]
    pub kind: GameDirectoryKind,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

impl GameDirectory {
    /// Display name, falling back to the directory's folder name.
    pub fn display_name(&self) -> &str {
        if self.name.trim().is_empty() {
            self.path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("Game directory")
        } else {
            &self.name
        }
    }
}

/// Current schema for `config/game-directories.json`.
pub fn directories_schema() -> ConfigSchema {
    ConfigSchema::new("game-directories", SchemaVersion::new(1, 0, 0))
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct GameDirectoriesFile {
    #[serde(rename = "$schema", default = "directories_schema")]
    schema: ConfigSchema,
    #[serde(default)]
    selected: Option<GameDirectoryId>,
    /// Selected instance key (`directory/instance`), persisted like HMCL's
    /// `selectedInstance` map.
    #[serde(default)]
    selected_instance: Option<String>,
    #[serde(default)]
    directories: Vec<GameDirectory>,
}

impl Default for GameDirectoriesFile {
    fn default() -> Self {
        Self {
            schema: directories_schema(),
            selected: None,
            selected_instance: None,
            directories: Vec::new(),
        }
    }
}

/// Persistent game-directory registry.
pub struct GameDirectoryStore {
    path: PathBuf,
    fs: Arc<dyn PlatformFs>,
    file: GameDirectoriesFile,
}

impl GameDirectoryStore {
    /// Load the registry, creating it from `AppConfig` on first run.
    ///
    /// A malformed registry is backed up and rebuilt from the configured roots;
    /// it must never block startup (the same policy as other RMCCC settings).
    pub fn load(data_dir: &Path, config: &AppConfig, fs: Arc<dyn PlatformFs>) -> Result<Self> {
        let path = rmccc_config::paths::config_dir(data_dir).join("game-directories.json");
        let mut store = Self {
            path,
            fs,
            file: GameDirectoriesFile::default(),
        };

        let mut fresh = !store.path.exists();
        if !fresh {
            match std::fs::read_to_string(&store.path)
                .map_err(RmcccError::from)
                .and_then(|text| {
                    serde_json::from_str::<GameDirectoriesFile>(&text).map_err(RmcccError::from)
                }) {
                Ok(file) => store.file = file,
                Err(e) => {
                    tracing::warn!(
                        path = %store.path.display(),
                        error = %e,
                        "Game directory registry is unreadable; rebuilding from AppConfig"
                    );
                    backup_invalid(&store.path);
                    // Rebuild as a fresh registry so configured extra roots are
                    // seeded again after corruption.
                    fresh = true;
                }
            }
        }

        let changed = store.ensure_configured(config, fresh)?;
        if changed {
            store.save()?;
        }
        Ok(store)
    }

    /// Path of the registry file.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// All registered directories, in registration order.
    pub fn list(&self) -> &[GameDirectory] {
        &self.file.directories
    }

    /// Look up a directory by id.
    pub fn get(&self, id: &GameDirectoryId) -> Option<&GameDirectory> {
        self.file.directories.iter().find(|d| &d.id == id)
    }

    /// Look up a directory by canonical path.
    pub fn find_by_path(&self, path: &Path) -> Option<&GameDirectory> {
        let canonical = canonical_or_self(path);
        self.file
            .directories
            .iter()
            .find(|d| canonical_or_self(&d.path) == canonical)
    }

    /// Currently selected directory.
    ///
    /// Falls back to the first registered directory, so callers can rely on a
    /// non-empty registry (the store guarantees at least one entry).
    pub fn selected(&self) -> Option<&GameDirectory> {
        self.file
            .selected
            .as_ref()
            .and_then(|id| self.get(id))
            .or_else(|| self.file.directories.first())
    }

    /// Currently selected instance key, if it still exists.
    pub fn selected_instance(&self) -> Option<&str> {
        self.file.selected_instance.as_deref()
    }

    /// Persist the selected directory.
    pub fn select(&mut self, id: &GameDirectoryId) -> Result<()> {
        if self.get(id).is_none() {
            return Err(RmcccError::Validation(format!(
                "unknown game directory: {id}"
            )));
        }
        if self.file.selected.as_ref() == Some(id) {
            return Ok(());
        }
        self.file.selected = Some(id.clone());
        self.save()
    }

    /// Persist the selected instance key (or clear it).
    pub fn select_instance(&mut self, key: Option<String>) -> Result<()> {
        if self.file.selected_instance == key {
            return Ok(());
        }
        self.file.selected_instance = key;
        self.save()
    }

    /// Add a directory. The path is canonicalised and may not be registered
    /// twice. `Managed` directories are created when missing; `External` ones
    /// must already exist.
    pub fn add(
        &mut self,
        name: impl Into<String>,
        path: &Path,
        kind: GameDirectoryKind,
    ) -> Result<GameDirectory> {
        let name = validate_directory_name(name.into())?;
        if kind == GameDirectoryKind::Managed {
            std::fs::create_dir_all(path)?;
        }
        if !path.is_dir() {
            return Err(RmcccError::Validation(format!(
                "game directory does not exist: {}",
                path.display()
            )));
        }
        let canonical = canonical_or_self(path);
        if let Some(existing) = self.find_by_path(&canonical) {
            return Err(RmcccError::Validation(format!(
                "game directory already registered as '{}': {}",
                existing.display_name(),
                canonical.display()
            )));
        }

        let directory = GameDirectory {
            id: GameDirectoryId::generate(),
            name,
            path: canonical,
            kind,
            created_at: chrono::Utc::now(),
        };
        self.file.directories.push(directory.clone());
        if self.file.selected.is_none() {
            self.file.selected = Some(directory.id.clone());
        }
        self.save()?;
        Ok(directory)
    }

    /// Update a directory's display name and/or path.
    ///
    /// Changing the path of the selected directory is allowed; instances are
    /// re-scanned by the caller. Removing files is never part of this operation.
    pub fn update(
        &mut self,
        id: &GameDirectoryId,
        name: Option<String>,
        path: Option<PathBuf>,
    ) -> Result<GameDirectory> {
        let canonical = if let Some(path) = path {
            if !path.is_dir() {
                return Err(RmcccError::Validation(format!(
                    "game directory does not exist: {}",
                    path.display()
                )));
            }
            let canonical = canonical_or_self(&path);
            if self
                .file
                .directories
                .iter()
                .any(|d| &d.id != id && canonical_or_self(&d.path) == canonical)
            {
                return Err(RmcccError::Validation(format!(
                    "game directory already registered: {}",
                    canonical.display()
                )));
            }
            Some(canonical)
        } else {
            None
        };

        let directory = self
            .file
            .directories
            .iter_mut()
            .find(|d| &d.id == id)
            .ok_or_else(|| RmcccError::Validation(format!("unknown game directory: {id}")))?;

        if let Some(name) = name {
            directory.name = validate_directory_name(name)?;
        }
        if let Some(path) = canonical {
            directory.path = path;
        }
        let updated = directory.clone();
        self.save()?;
        Ok(updated)
    }

    /// Remove a directory from the registry (never deletes files).
    ///
    /// The last directory cannot be removed — the instance model requires at
    /// least one place to look for versions.
    pub fn remove(&mut self, id: &GameDirectoryId) -> Result<()> {
        if self.file.directories.len() <= 1 {
            return Err(RmcccError::Validation(
                "cannot remove the last game directory".to_string(),
            ));
        }
        let before = self.file.directories.len();
        self.file.directories.retain(|d| &d.id != id);
        if self.file.directories.len() == before {
            return Err(RmcccError::Validation(format!(
                "unknown game directory: {id}"
            )));
        }
        if self.file.selected.as_ref() == Some(id) {
            self.file.selected = self.file.directories.first().map(|d| d.id.clone());
        }
        self.save()
    }

    fn ensure_configured(&mut self, config: &AppConfig, seed_extras: bool) -> Result<bool> {
        let mut changed = false;

        if self.file.directories.is_empty() {
            let root = config.resolved_game_root();
            std::fs::create_dir_all(&root)?;
            let canonical = canonical_or_self(&root);
            self.file.directories.push(GameDirectory {
                id: GameDirectoryId::default_id(),
                name: "Default".to_string(),
                path: canonical,
                kind: GameDirectoryKind::Managed,
                created_at: chrono::Utc::now(),
            });
            self.file.selected = Some(GameDirectoryId::default_id());
            changed = true;
        }

        // `extra_game_roots` is a legacy seed, not an ongoing source of truth:
        // re-reading it on every boot would resurrect a directory the user
        // deliberately removed from the registry.
        if seed_extras {
            for root in &config.extra_game_roots {
                if !root.is_dir() {
                    tracing::warn!(
                        path = %root.display(),
                        "Configured extra game root is missing; skipping registration"
                    );
                    continue;
                }
                if self.find_by_path(root).is_none() {
                    let name = root
                        .file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or("External")
                        .to_string();
                    self.file.directories.push(GameDirectory {
                        id: GameDirectoryId::generate(),
                        name,
                        path: canonical_or_self(root),
                        kind: GameDirectoryKind::External,
                        created_at: chrono::Utc::now(),
                    });
                    changed = true;
                }
            }
        }

        if self
            .file
            .selected
            .as_ref()
            .is_none_or(|id| self.get(id).is_none())
        {
            self.file.selected = self.file.directories.first().map(|d| d.id.clone());
            changed = true;
        }

        if let Some(key) = self.file.selected_instance.clone() {
            if crate::id::InstanceKey::parse(&key).is_err() {
                self.file.selected_instance = None;
                changed = true;
            }
        }

        Ok(changed)
    }

    /// Persist the registry atomically.
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

fn validate_directory_name(name: String) -> Result<String> {
    let name = name.trim().to_string();
    if name.is_empty() {
        return Err(RmcccError::Validation(
            "game directory name must not be empty".to_string(),
        ));
    }
    if name.chars().count() > 64 {
        return Err(RmcccError::Validation(
            "game directory name is too long (max 64)".to_string(),
        ));
    }
    if name.chars().any(char::is_control) {
        return Err(RmcccError::Validation(
            "game directory name contains control characters".to_string(),
        ));
    }
    Ok(name)
}

fn canonical_or_self(path: &Path) -> PathBuf {
    std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

fn backup_invalid(path: &Path) {
    let backup = path.with_extension("json.invalid");
    if let Err(e) = std::fs::rename(path, &backup) {
        tracing::warn!(
            path = %path.display(),
            error = %e,
            "Unable to back up invalid game directory registry"
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rmccc_config::schema::SchemaVersion;

    fn temp(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "rmccc-dirs-{tag}-{}-{}",
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

    fn fs() -> Arc<dyn PlatformFs> {
        rmccc_platform::Platform::current().fs
    }

    #[test]
    fn creates_default_and_extra_directories() {
        let data = temp("init");
        let extra = data.join("hmcl");
        std::fs::create_dir_all(&extra).unwrap();
        let config = AppConfig {
            data_dir: Some(data.clone()),
            extra_game_roots: vec![extra.clone()],
            ..Default::default()
        };

        let store = GameDirectoryStore::load(&data, &config, fs()).unwrap();
        assert_eq!(store.list().len(), 2);
        assert_eq!(store.selected().unwrap().id, GameDirectoryId::default_id());
        assert!(store.find_by_path(&extra).is_some());

        // Reload is idempotent (does not duplicate the extra root).
        let store = GameDirectoryStore::load(&data, &config, fs()).unwrap();
        assert_eq!(store.list().len(), 2);
        let _ = std::fs::remove_dir_all(&data);
    }

    #[test]
    fn add_update_remove_and_last_directory_guard() {
        let data = temp("crud");
        let config = AppConfig {
            data_dir: Some(data.clone()),
            ..Default::default()
        };
        let mut store = GameDirectoryStore::load(&data, &config, fs()).unwrap();
        let other = data.join("other");
        std::fs::create_dir_all(&other).unwrap();
        let added = store
            .add("Other", &other, GameDirectoryKind::External)
            .unwrap();

        // Duplicate path rejected.
        assert!(store
            .add("Again", &other, GameDirectoryKind::External)
            .is_err());

        let renamed = store
            .update(&added.id, Some("Renamed".into()), None)
            .unwrap();
        assert_eq!(renamed.name, "Renamed");

        store.remove(&added.id).unwrap();
        assert_eq!(store.list().len(), 1);
        assert!(store.remove(&GameDirectoryId::default_id()).is_err());
        let _ = std::fs::remove_dir_all(&data);
    }

    #[test]
    fn schema_marker_is_current() {
        let schema = directories_schema();
        assert_eq!(schema.id, "game-directories");
        assert_eq!(schema.version, SchemaVersion::new(1, 0, 0));
    }
}
