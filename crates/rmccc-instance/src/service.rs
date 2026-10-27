//! `InstanceService` — the single read/write facade for game directories and
//! version-backed instances.
//!
//! It owns one `DefaultGameRepository` per registered game directory (lazily
//! created and refreshed) and the settings stores. IPC, CLI, launch and mod
//! management all go through this type instead of touching `profiles/` or
//! `versions/` directly.

use crate::compat::{detect_layout, scan_candidates, DirectoryCandidate, GameDirectoryOverview};
use crate::directory::{GameDirectory, GameDirectoryKind, GameDirectoryStore};
use crate::global::GlobalGameSettingsStore;
use crate::id::{GameDirectoryId, InstanceId, InstanceKey};
use crate::migrate::{migrate_legacy_profiles, MigrationReport};
use crate::settings::{
    fields, EffectiveGameSettings, GlobalGameSettings, InstanceSettings, InstanceSettingsPatch,
    IsolationPolicy,
};
use crate::summary::{
    InstanceDetail, InstanceManifestInfo, InstancePaths, InstanceSummary, LoaderInfo,
    ResolvedInstanceSettings,
};
use rmccc_config::AppConfig;
use rmccc_error::{Result, RmcccError};
use rmccc_platform::Platform;
use rmccc_version::{file_layout, DefaultGameRepository, GameRepository as _, ResolvedVersion};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// Instance lifecycle facade.
pub struct InstanceService {
    data_dir: PathBuf,
    platform: Arc<Platform>,
    directories: GameDirectoryStore,
    global: GlobalGameSettingsStore,
    /// Lazily-created repository per directory. Keeping them warm avoids
    /// re-parsing every version JSON on every IPC call.
    repositories: HashMap<GameDirectoryId, DefaultGameRepository>,
}

impl InstanceService {
    /// Load the directory registry, global preset, and legacy-profile migration.
    ///
    /// A broken/unreadable game directory only logs a warning at startup; it
    /// must not prevent the launcher from opening (the same policy as HMCL).
    pub fn new(
        data_dir: &Path,
        config: &AppConfig,
        platform: Arc<Platform>,
        selected_legacy_profile: Option<&str>,
    ) -> Result<Self> {
        let directories = GameDirectoryStore::load(data_dir, config, platform.fs.clone())?;
        let global = GlobalGameSettingsStore::load(data_dir, config, platform.fs.clone())?;
        let mut service = Self {
            data_dir: data_dir.to_path_buf(),
            platform,
            directories,
            global,
            repositories: HashMap::new(),
        };

        if let Err(e) = migrate_legacy_profiles(
            data_dir,
            &mut service.directories,
            selected_legacy_profile,
            service.platform.fs.clone(),
        ) {
            tracing::warn!(error = %e, "Legacy profile migration failed; continuing");
        }

        if let Some(dir) = service.directories.selected().map(|d| d.id.clone()) {
            if let Err(e) = service.repo_for(&dir) {
                tracing::warn!(
                    directory = %dir,
                    error = %e,
                    "Initial game repository refresh failed"
                );
            }
        }
        Ok(service)
    }

    // ── Directory registry ──────────────────────────────────────────

    /// Data directory used for migration markers/backups.
    pub fn data_dir(&self) -> &Path {
        &self.data_dir
    }

    /// All registered game directories.
    pub fn directories(&self) -> &[GameDirectory] {
        self.directories.list()
    }

    /// Selected game directory (always present after construction).
    pub fn selected_directory(&self) -> &GameDirectory {
        self.directories
            .selected()
            .expect("GameDirectoryStore guarantees one directory")
    }

    /// Selected instance key, if it still exists.
    pub fn selected_instance(&self) -> Option<&str> {
        self.directories.selected_instance()
    }

    /// Look up a directory.
    pub fn directory(&self, id: &str) -> Result<&GameDirectory> {
        let id = GameDirectoryId::parse(id)?;
        self.directories
            .get(&id)
            .ok_or_else(|| RmcccError::Validation(format!("unknown game directory: {id}")))
    }

    /// Cheap sidebar overview: each directory with its layout and instance count.
    ///
    /// Never fails on a broken/missing directory: the row still shows up (so the
    /// user can fix or remove it) with a filesystem count fallback.
    pub fn overviews(&mut self) -> Vec<GameDirectoryOverview> {
        let selected = self.selected_directory().id.clone();
        let directories: Vec<GameDirectory> = self.directories.list().to_vec();
        directories
            .into_iter()
            .map(|directory| {
                let instance_count = match self.repo_for(&directory.id) {
                    Ok(repo) => repo.get_instance_ids().len(),
                    Err(e) => {
                        tracing::warn!(
                            directory = %directory.id,
                            path = %directory.path.display(),
                            error = %e,
                            "Falling back to filesystem instance count"
                        );
                        crate::compat::count_instances(&directory.path)
                    }
                };
                GameDirectoryOverview {
                    layout: detect_layout(&directory.path),
                    instance_count,
                    is_selected: directory.id == selected,
                    directory,
                }
            })
            .collect()
    }

    /// Discover standard game directories registered by HMCL/PCL/the official
    /// launcher, plus portable `.minecraft` locations.
    pub fn scan_directories(&self) -> Vec<DirectoryCandidate> {
        let registered: Vec<PathBuf> = self
            .directories
            .list()
            .iter()
            .map(|d| d.path.clone())
            .collect();
        let mut candidates = scan_candidates(&registered);
        // Registered roots keep their RMCCC display name (the folder name may
        // be a generic `instances`/`.minecraft`).
        for candidate in &mut candidates {
            if !candidate.registered {
                continue;
            }
            if let Some(directory) = self
                .directories
                .list()
                .iter()
                .find(|d| same_path(&d.path, &candidate.path))
            {
                candidate.name = directory.name.clone();
            }
        }
        candidates
    }

    /// Register a directory.
    pub fn add_directory(
        &mut self,
        name: impl Into<String>,
        path: &Path,
        kind: GameDirectoryKind,
    ) -> Result<GameDirectory> {
        let directory = self.directories.add(name, path, kind)?;
        self.repositories.remove(&directory.id);
        Ok(directory)
    }

    /// Update a directory's name and/or path.
    pub fn update_directory(
        &mut self,
        id: &str,
        name: Option<String>,
        path: Option<PathBuf>,
    ) -> Result<GameDirectory> {
        let id = GameDirectoryId::parse(id)?;
        let directory = self.directories.update(&id, name, path)?;
        self.repositories.remove(&id);
        Ok(directory)
    }

    /// Remove a directory from the registry (never deletes files).
    pub fn remove_directory(&mut self, id: &str) -> Result<()> {
        let id = GameDirectoryId::parse(id)?;
        self.directories.remove(&id)?;
        self.repositories.remove(&id);
        Ok(())
    }

    /// Select the active game directory.
    pub fn select_directory(&mut self, id: &str) -> Result<GameDirectory> {
        let id = GameDirectoryId::parse(id)?;
        self.directories.select(&id)?;
        self.repo_for(&id)?;
        Ok(self
            .directories
            .get(&id)
            .cloned()
            .expect("selected directory exists"))
    }

    /// Persist the selected instance key.
    pub fn select_instance(&mut self, key: Option<&str>) -> Result<()> {
        if let Some(key) = key {
            let parsed = InstanceKey::parse(key)?;
            let exists = {
                let repo = self.repo_for(&parsed.directory)?;
                repo.has_instance(parsed.id.as_str())
            };
            if !exists {
                return Err(RmcccError::ProfileNotFound(key.to_string()));
            }
            self.directories.select(&parsed.directory)?;
        }
        self.directories.select_instance(key.map(str::to_string))
    }

    // ── Global preset ───────────────────────────────────────────────

    /// Current global game-settings preset.
    pub fn global_settings(&self) -> &GlobalGameSettings {
        self.global.settings()
    }

    /// Replace the global preset.
    pub fn update_global_settings(&mut self, settings: GlobalGameSettings) -> Result<()> {
        self.global.replace(settings)
    }

    // ── Listing / detail ────────────────────────────────────────────

    /// List instances.
    ///
    /// `directory = None` uses the selected directory; `all = true` returns
    /// every registered directory. Hidden instances (manifest `hidden` or the
    /// instance settings `hidden` flag) are filtered unless `include_hidden`.
    pub fn list(
        &mut self,
        directory: Option<&str>,
        all: bool,
        include_hidden: bool,
    ) -> Result<Vec<InstanceSummary>> {
        let ids: Vec<GameDirectoryId> = if all {
            self.directories
                .list()
                .iter()
                .map(|d| d.id.clone())
                .collect()
        } else if let Some(id) = directory {
            vec![GameDirectoryId::parse(id)?]
        } else {
            vec![self.selected_directory().id.clone()]
        };

        let mut out = Vec::new();
        for id in ids {
            let directory = match self.directories.get(&id).cloned() {
                Some(d) => d,
                None => {
                    if all {
                        tracing::warn!(directory = %id, "Skipping unknown game directory");
                        continue;
                    }
                    return Err(RmcccError::Validation(format!(
                        "unknown game directory: {id}"
                    )));
                }
            };

            let instance_ids = {
                let repo = self.repo_for(&id)?;
                repo.get_instance_ids()
            };

            for instance_id in instance_ids {
                match self.describe(&directory, &instance_id) {
                    Ok((summary, _)) => {
                        if include_hidden || !summary.hidden {
                            out.push(summary);
                        }
                    }
                    Err(e) => tracing::warn!(
                        directory = %id,
                        instance = %instance_id,
                        error = %e,
                        "Skipping unreadable instance"
                    ),
                }
            }
        }

        out.sort_by(|a, b| {
            b.last_played.cmp(&a.last_played).then_with(|| {
                a.display_name
                    .to_lowercase()
                    .cmp(&b.display_name.to_lowercase())
            })
        });
        Ok(out)
    }

    /// Like [`Self::list`], but returns full details (settings + paths) so a
    /// list UI can render the settings form without N+1 IPC calls. Still does
    /// **not** run a completeness scan.
    pub fn list_details(
        &mut self,
        directory: Option<&str>,
        all: bool,
        include_hidden: bool,
    ) -> Result<Vec<InstanceDetail>> {
        let ids: Vec<GameDirectoryId> = if all {
            self.directories
                .list()
                .iter()
                .map(|d| d.id.clone())
                .collect()
        } else if let Some(id) = directory {
            vec![GameDirectoryId::parse(id)?]
        } else {
            vec![self.selected_directory().id.clone()]
        };

        let mut out = Vec::new();
        for id in ids {
            let directory = match self.directories.get(&id).cloned() {
                Some(d) => d,
                None => {
                    if all {
                        continue;
                    }
                    return Err(RmcccError::Validation(format!(
                        "unknown game directory: {id}"
                    )));
                }
            };
            let instance_ids = {
                let repo = self.repo_for(&id)?;
                repo.get_instance_ids()
            };
            for instance_id in instance_ids {
                match self.describe(&directory, &instance_id) {
                    Ok((summary, detail)) => {
                        if include_hidden || !summary.hidden {
                            out.push(detail);
                        }
                    }
                    Err(e) => tracing::warn!(
                        directory = %id,
                        instance = %instance_id,
                        error = %e,
                        "Skipping unreadable instance"
                    ),
                }
            }
        }

        out.sort_by(|a, b| {
            b.summary
                .last_played
                .cmp(&a.summary.last_played)
                .then_with(|| {
                    a.summary
                        .display_name
                        .to_lowercase()
                        .cmp(&b.summary.display_name.to_lowercase())
                })
        });
        Ok(out)
    }

    /// Load one instance by its `directory/id` key.
    pub fn get(&mut self, key: &str) -> Result<InstanceDetail> {
        let key = InstanceKey::parse(key)?;
        let directory = self
            .directories
            .get(&key.directory)
            .cloned()
            .ok_or_else(|| {
                RmcccError::Validation(format!("unknown game directory: {}", key.directory))
            })?;
        let (_, detail) = self.describe(&directory, key.id.as_str())?;
        Ok(detail)
    }

    /// Resolved settings (single source of truth for the UI).
    pub fn resolved_settings(&mut self, key: &str) -> Result<ResolvedInstanceSettings> {
        let detail = self.get(key)?;
        Ok(ResolvedInstanceSettings::from(&detail.effective))
    }

    /// All on-disk paths for an instance.
    pub fn paths(&mut self, key: &str) -> Result<InstancePaths> {
        Ok(self.get(key)?.paths)
    }

    /// Update an instance's settings overlay.
    pub fn update_settings(
        &mut self,
        key: &str,
        patch: InstanceSettingsPatch,
    ) -> Result<InstanceDetail> {
        let key = InstanceKey::parse(key)?;
        let directory = self
            .directories
            .get(&key.directory)
            .cloned()
            .ok_or_else(|| {
                RmcccError::Validation(format!("unknown game directory: {}", key.directory))
            })?;
        let instance_root = file_layout::instance_root(&directory.path, key.id.as_str());
        if !file_layout::instance_json(&directory.path, key.id.as_str()).exists() {
            return Err(RmcccError::ProfileNotFound(key.as_key()));
        }
        if is_read_only(&instance_root) {
            return Err(RmcccError::Validation(format!(
                "instance directory is read-only: {}",
                instance_root.display()
            )));
        }

        let loaded = InstanceSettings::load(&instance_root);
        let mut settings = loaded.settings;
        if loaded.warning.is_some() {
            crate::settings::backup_instance_settings(&instance_root);
        }
        settings.apply(patch)?;
        if settings.created_at.is_none() {
            settings.created_at = Some(chrono::Utc::now());
        }
        settings.save(&instance_root, self.platform.fs.as_ref())?;
        self.get(&key.as_key())
    }

    /// Persist the selected instance's last-played timestamp.
    pub fn touch(&mut self, key: &str) -> Result<()> {
        self.mutate_metadata(key, |settings| {
            settings.last_played = Some(chrono::Utc::now());
        })
    }

    /// Add completed play time in seconds.
    pub fn add_play_time(&mut self, key: &str, seconds: u64) -> Result<()> {
        if seconds == 0 {
            return Ok(());
        }
        self.mutate_metadata(key, |settings| {
            settings.play_time_seconds = settings.play_time_seconds.saturating_add(seconds);
            settings.last_played = Some(chrono::Utc::now());
        })
    }

    // ── Lifecycle ───────────────────────────────────────────────────

    /// Create/ensure the settings overlay for a version that has just been
    /// installed (or imported from disk).
    ///
    /// The instance is pinned to the isolation policy that applies at creation
    /// time (HMCL `applyDefaultIsolationSettingForNewInstance`), so changing
    /// the global default later does not move existing saves/mods.
    pub fn ensure_instance(&mut self, directory_id: &str, id: &str) -> Result<InstanceDetail> {
        let directory_id = GameDirectoryId::parse(directory_id)?;
        let id = InstanceId::parse(id)?;
        let directory = self
            .directories
            .get(&directory_id)
            .cloned()
            .ok_or_else(|| {
                RmcccError::Validation(format!("unknown game directory: {directory_id}"))
            })?;
        let instance_root = file_layout::instance_root(&directory.path, id.as_str());
        if !file_layout::instance_json(&directory.path, id.as_str()).exists() {
            return Err(RmcccError::Validation(format!(
                "version '{}' is not installed in {}",
                id,
                directory.path.display()
            )));
        }
        // The version JSON may have just been written by an installer (or by
        // an external tool): re-scan before trusting the cached index.
        {
            let repo = self.repo_for(&directory_id)?;
            repo.refresh()?;
        }

        let loaded = InstanceSettings::load(&instance_root);
        if !loaded.exists {
            let modded = {
                let repo = self.repo_for(&directory_id)?;
                repo.is_modded(id.as_str())
            };
            let mut settings = loaded.settings;
            settings.created_at = Some(chrono::Utc::now());
            settings.isolation = Some(pinned_isolation(
                self.global.settings().default_isolation,
                modded,
            ));
            // Read-only external roots can still be used with defaults; only
            // the settings overlay cannot be persisted.
            if let Err(e) = settings.save(&instance_root, self.platform.fs.as_ref()) {
                tracing::warn!(
                    instance = %id,
                    path = %instance_root.display(),
                    error = %e,
                    "Instance settings are not writable; using defaults"
                );
            }
            self.repo_for(&directory_id)?;
        }

        self.get(&InstanceKey::new(directory_id, id).as_key())
    }

    /// Rename an instance id (folder + inner json/jar + child references).
    pub fn rename(&mut self, key: &str, new_id: &str) -> Result<InstanceDetail> {
        let key = InstanceKey::parse(key)?;
        let new_id = InstanceId::parse(new_id)?;
        let old_id = key.id.clone();
        if new_id == old_id {
            return self.get(&key.as_key());
        }

        {
            let repo = self.repo_for(&key.directory)?;
            if repo
                .get_instance_ids()
                .iter()
                .any(|id| id.eq_ignore_ascii_case(new_id.as_str()))
            {
                return Err(RmcccError::Validation(format!(
                    "an instance named '{new_id}' already exists"
                )));
            }
            repo.rename_instance(old_id.as_str(), new_id.as_str())?;
        }

        let new_key = InstanceKey::new(key.directory.clone(), new_id.clone());
        let directory = self
            .directories
            .get(&key.directory)
            .cloned()
            .expect("validated directory");
        let new_root = file_layout::instance_root(&directory.path, new_id.as_str());

        // If the display name was simply the old id, let it follow the new id.
        let loaded = InstanceSettings::load(&new_root);
        if loaded.settings.display_name.as_deref() == Some(old_id.as_str()) {
            let mut settings = loaded.settings;
            settings.display_name = None;
            let _ = settings.save(&new_root, self.platform.fs.as_ref());
        }

        if self.directories.selected_instance() == Some(key.as_key().as_str()) {
            self.directories.select_instance(Some(new_key.as_key()))?;
        }

        self.get(&new_key.as_key())
    }

    /// Duplicate an instance. `copy_saves = false` omits `saves/`.
    pub fn duplicate(
        &mut self,
        key: &str,
        new_id: &str,
        copy_saves: bool,
    ) -> Result<InstanceDetail> {
        let key = InstanceKey::parse(key)?;
        let new_id = InstanceId::parse(new_id)?;
        if new_id == key.id {
            return Err(RmcccError::Validation(
                "the duplicate needs a different instance id".to_string(),
            ));
        }

        {
            let repo = self.repo_for(&key.directory)?;
            if repo
                .get_instance_ids()
                .iter()
                .any(|id| id.eq_ignore_ascii_case(new_id.as_str()))
            {
                return Err(RmcccError::Validation(format!(
                    "an instance named '{new_id}' already exists"
                )));
            }
            repo.duplicate_instance_with(key.id.as_str(), new_id.as_str(), copy_saves)?;
        }

        let directory = self
            .directories
            .get(&key.directory)
            .cloned()
            .expect("validated directory");
        let new_root = file_layout::instance_root(&directory.path, new_id.as_str());

        // A copy is a new instance: its display name defaults to the new id,
        // and running-directory overrides that pointed at the source are
        // cleared (HMCL duplicate semantics).
        let loaded = InstanceSettings::load(&new_root);
        let mut settings = loaded.settings;
        settings.display_name = None;
        settings.created_at = Some(chrono::Utc::now());
        settings.last_played = None;
        settings.play_time_seconds = 0;
        settings.hidden = false;
        settings
            .override_properties
            .remove(fields::RUNNING_DIRECTORY);
        settings.running_directory = None;
        if let Err(e) = settings.save(&new_root, self.platform.fs.as_ref()) {
            tracing::warn!(
                instance = %new_id,
                error = %e,
                "Duplicated instance but could not rewrite its settings"
            );
        }

        self.get(&InstanceKey::new(key.directory, new_id).as_key())
    }

    /// Remove an instance.
    ///
    /// `delete_files = false` hides it (settings `hidden = true`) while leaving
    /// the version directory intact — the only honest meaning of "remove the
    /// instance but keep the files" once the version folder *is* the instance.
    /// `delete_files = true` deletes `versions/{id}` and its settings.
    pub fn remove(&mut self, key: &str, delete_files: bool) -> Result<()> {
        let key = InstanceKey::parse(key)?;
        let directory = self
            .directories
            .get(&key.directory)
            .cloned()
            .ok_or_else(|| {
                RmcccError::Validation(format!("unknown game directory: {}", key.directory))
            })?;
        let instance_root = file_layout::instance_root(&directory.path, key.id.as_str());
        if !instance_root.is_dir() {
            return Err(RmcccError::ProfileNotFound(key.as_key()));
        }

        if delete_files {
            {
                let repo = self.repo_for(&key.directory)?;
                repo.remove_instance(key.id.as_str())?;
            }
            self.repo_for(&key.directory)?;
        } else {
            let loaded = InstanceSettings::load(&instance_root);
            let mut settings = loaded.settings;
            settings.hidden = true;
            settings.save(&instance_root, self.platform.fs.as_ref())?;
        }

        if self.directories.selected_instance() == Some(key.as_key().as_str()) {
            self.directories.select_instance(None)?;
        }
        Ok(())
    }

    // ── Launch / repair helpers ─────────────────────────────────────

    /// Return the resolved launch view together with the detail payload.
    pub fn resolved_version(&mut self, key: &str) -> Result<(InstanceDetail, ResolvedVersion)> {
        let key = InstanceKey::parse(key)?;
        let detail = self.get(&key.as_key())?;
        let resolved = {
            let repo = self.repo_for(&key.directory)?;
            repo.get_resolved(key.id.as_str())
                .map_err(|e| RmcccError::Other(e.to_string()))?
        };
        Ok((detail, resolved))
    }

    /// Game directory path for install/repair orchestration.
    pub fn game_dir(&self, directory_id: &str) -> Result<PathBuf> {
        Ok(self.directory(directory_id)?.path.clone())
    }

    /// Resolve a version that lives under `game_dir` (registered or not).
    ///
    /// Used by install/modloader orchestration to find a parent manifest
    /// without going through the network provider. Unregistered roots are
    /// scanned with a temporary repository (external installers may write to a
    /// root that has not been registered yet).
    pub fn resolved_in_root(&mut self, game_dir: &Path, id: &str) -> Result<ResolvedVersion> {
        if let Some(directory) = self.directories.find_by_path(game_dir) {
            let directory_id = directory.id.clone();
            let repo = self.repo_for(&directory_id)?;
            return repo
                .get_resolved(id)
                .map_err(|e| RmcccError::Other(e.to_string()));
        }

        let mut repo =
            DefaultGameRepository::with_platform(game_dir.to_path_buf(), self.platform.descriptor);
        repo.refresh()?;
        repo.get_resolved(id)
            .map_err(|e| RmcccError::Other(e.to_string()))
    }

    /// Refresh one directory (or the selected one) and return its instances.
    pub fn refresh(
        &mut self,
        directory: Option<&str>,
        include_hidden: bool,
    ) -> Result<Vec<InstanceSummary>> {
        let id = match directory {
            Some(id) => GameDirectoryId::parse(id)?,
            None => self.selected_directory().id.clone(),
        };
        {
            let repo = self.repo_for(&id)?;
            repo.refresh()?;
        }
        self.list(Some(id.as_str()), false, include_hidden)
    }

    // ── Internals ───────────────────────────────────────────────────

    /// Get (creating/refreshing if needed) the repository for a directory.
    fn repo_for(&mut self, id: &GameDirectoryId) -> Result<&mut DefaultGameRepository> {
        if !self.repositories.contains_key(id) {
            let directory =
                self.directories.get(id).cloned().ok_or_else(|| {
                    RmcccError::Validation(format!("unknown game directory: {id}"))
                })?;
            let mut repo =
                DefaultGameRepository::with_platform(directory.path, self.platform.descriptor);
            repo.refresh()?;
            self.repositories.insert(id.clone(), repo);
        }
        Ok(self
            .repositories
            .get_mut(id)
            .expect("repository inserted above"))
    }

    fn describe(
        &mut self,
        directory: &GameDirectory,
        id: &str,
    ) -> Result<(InstanceSummary, InstanceDetail)> {
        let instance_root = file_layout::instance_root(&directory.path, id);
        let json_path = file_layout::instance_json(&directory.path, id);
        if !json_path.exists() {
            return Err(RmcccError::ProfileNotFound(format!(
                "{}/{}",
                directory.id, id
            )));
        }

        let (resolved, manifest, base_version, modded) = {
            let repo = self.repo_for(&directory.id)?;
            if !repo.has_instance(id) {
                return Err(RmcccError::ProfileNotFound(format!(
                    "{}/{}",
                    directory.id, id
                )));
            }
            let resolved = repo
                .get_resolved(id)
                .map_err(|e| RmcccError::Other(e.to_string()))?;
            let manifest = repo
                .get_manifest(id)
                .cloned()
                .ok_or_else(|| RmcccError::ProfileNotFound(format!("{}/{}", directory.id, id)))?;
            let mut base = repo.get_game_version(id);
            if base == id {
                if let Some(from_patch) = rmccc_version::loader::base_version(&resolved.standalone)
                {
                    base = from_patch;
                }
            }
            let modded = repo.is_modded(id);
            (resolved, manifest, base, modded)
        };

        let loaded = InstanceSettings::load(&instance_root);
        let settings = loaded.settings;
        let global = self.global.settings().clone();
        let effective = settings.resolve(&global);
        let paths = paths_for(&directory.path, id, &effective, modded, &self.platform);

        let loader =
            rmccc_version::loader::detect_loader(&resolved.standalone).map(|l| LoaderInfo {
                kind: l.kind,
                version: (!l.version.is_empty()).then_some(l.version),
            });
        let hidden = manifest.hidden || settings.hidden;
        let created_at = settings.created_at.or_else(|| {
            std::fs::metadata(&instance_root)
                .and_then(|m| m.created())
                .ok()
                .map(chrono::DateTime::<chrono::Utc>::from)
        });
        let settings_read_only = is_read_only(&instance_root);
        let has_jar = resolved
            .launch
            .jar
            .as_deref()
            .map(|jar| file_layout::instance_jar(&directory.path, jar).exists())
            .unwrap_or_else(|| file_layout::instance_jar(&directory.path, id).exists());

        let summary = InstanceSummary {
            key: InstanceKey::new(directory.id.clone(), InstanceId::parse(id)?).as_key(),
            directory_id: directory.id.to_string(),
            directory_name: directory.display_name().to_string(),
            id: id.to_string(),
            display_name: settings.display_name_or(id).to_string(),
            icon: settings.icon.clone(),
            game_version: base_version,
            release_type: release_type_name(&manifest.release_type),
            loader,
            modded,
            is_modpack: instance_root.join("modpack.cfg").exists()
                || instance_root.join("modpack.json").exists(),
            hidden,
            has_jar,
            has_json: true,
            missing_count: None,
            path: instance_root,
            run_dir: paths.run_dir.clone(),
            mods_dir: paths.mods_dir.clone(),
            isolation: effective.isolation,
            isolated: paths.run_dir != paths.game_dir,
            created_at,
            last_played: settings.last_played,
            play_time_seconds: settings.play_time_seconds,
            favorite: settings.favorite,
            settings_read_only,
        };

        let manifest_info = InstanceManifestInfo {
            id: manifest.id.clone(),
            main_class: resolved.launch.main_class.clone(),
            release_type: release_type_name(&resolved.launch.release_type),
            asset_index: resolved.launch.asset_index.id.clone(),
            java_major: resolved
                .launch
                .java_version
                .as_ref()
                .map(|j| j.major_version),
            library_count: resolved.launch.libraries.len(),
            patch_count: manifest.patches.len(),
            inherits_from: manifest.inherits_from.clone(),
            root: manifest.root.unwrap_or(false),
            hidden: manifest.hidden,
            has_inheritance: manifest.inherits_from.is_some() || !manifest.patches.is_empty(),
        };

        let detail = InstanceDetail {
            summary: summary.clone(),
            directory: directory.clone(),
            settings,
            effective,
            paths,
            manifest: manifest_info,
            settings_exists: loaded.exists,
            warning: loaded.warning,
        };
        Ok((summary, detail))
    }

    fn mutate_metadata(
        &mut self,
        key: &str,
        edit: impl FnOnce(&mut InstanceSettings),
    ) -> Result<()> {
        let key = InstanceKey::parse(key)?;
        let directory = self
            .directories
            .get(&key.directory)
            .cloned()
            .ok_or_else(|| {
                RmcccError::Validation(format!("unknown game directory: {}", key.directory))
            })?;
        let instance_root = file_layout::instance_root(&directory.path, key.id.as_str());
        if !instance_root.is_dir() {
            return Err(RmcccError::ProfileNotFound(key.as_key()));
        }
        if is_read_only(&instance_root) {
            tracing::warn!(
                instance = %key,
                "Instance directory is read-only; metadata not persisted"
            );
            return Ok(());
        }
        let loaded = InstanceSettings::load(&instance_root);
        let mut settings = loaded.settings;
        if settings.created_at.is_none() {
            settings.created_at = Some(chrono::Utc::now());
        }
        edit(&mut settings);
        settings.save(&instance_root, self.platform.fs.as_ref())
    }
}

fn pinned_isolation(policy: IsolationPolicy, modded: bool) -> IsolationPolicy {
    match policy {
        IsolationPolicy::Modded if modded => IsolationPolicy::Always,
        IsolationPolicy::Modded => IsolationPolicy::Never,
        other => other,
    }
}

fn same_path(a: &Path, b: &Path) -> bool {
    let canon = |p: &Path| std::fs::canonicalize(p).unwrap_or_else(|_| p.to_path_buf());
    canon(a) == canon(b)
}

fn paths_for(
    game_dir: &Path,
    id: &str,
    effective: &EffectiveGameSettings,
    modded: bool,
    platform: &Platform,
) -> InstancePaths {
    let instance_dir = file_layout::instance_root(game_dir, id);
    let run_dir = run_dir_for(game_dir, id, effective, modded);
    InstancePaths {
        game_dir: game_dir.to_path_buf(),
        instance_dir: instance_dir.clone(),
        mods_dir: run_dir.join("mods"),
        saves_dir: run_dir.join("saves"),
        resourcepacks_dir: run_dir.join("resourcepacks"),
        shaderpacks_dir: run_dir.join("shaderpacks"),
        screenshots_dir: run_dir.join("screenshots"),
        logs_dir: run_dir.join("logs"),
        config_dir: run_dir.join("config"),
        json_file: file_layout::instance_json(game_dir, id),
        jar_file: file_layout::instance_jar(game_dir, id),
        settings_file: InstanceSettings::file_path(&instance_dir),
        natives_dir: file_layout::native_dir(game_dir, id, &platform.descriptor),
        run_dir,
    }
}

fn run_dir_for(
    game_dir: &Path,
    id: &str,
    effective: &EffectiveGameSettings,
    modded: bool,
) -> PathBuf {
    // 1. Explicit running-directory setting (absolute or relative to the root).
    if let Some(dir) = &effective.running_directory {
        return if dir.is_absolute() {
            dir.clone()
        } else {
            game_dir.join(dir)
        };
    }
    // 2. Isolation policy. This uses the *instance* id, not the base game
    //    version — the old code used `game_version` and isolated the wrong
    //    folder for every loader instance.
    rmccc_version::isolation::run_directory(game_dir, id, effective.isolation.into(), modded)
}

fn release_type_name(release: &rmccc_version::ReleaseType) -> String {
    match release {
        rmccc_version::ReleaseType::Release => "release",
        rmccc_version::ReleaseType::Snapshot => "snapshot",
        rmccc_version::ReleaseType::OldBeta => "old_beta",
        rmccc_version::ReleaseType::OldAlpha => "old_alpha",
    }
    .to_string()
}

fn is_read_only(path: &Path) -> bool {
    std::fs::metadata(path)
        .map(|meta| meta.permissions().readonly())
        .unwrap_or(false)
}

/// Convenience helpers used by CLI/commands.
impl InstanceService {
    /// Key for a directory + id without checking existence.
    pub fn key(directory_id: &str, id: &str) -> Result<String> {
        Ok(InstanceKey::new(
            GameDirectoryId::parse(directory_id)?,
            InstanceId::parse(id)?,
        )
        .as_key())
    }

    /// Migrated legacy report path (for diagnostics/tests).
    pub fn migration_marker(&self) -> PathBuf {
        rmccc_config::paths::state_dir(&self.data_dir).join(crate::migrate::MIGRATION_MARKER)
    }

    /// Result of the last migration (re-reads the marker).
    pub fn migration_report(&self) -> Option<MigrationReport> {
        let text = std::fs::read_to_string(self.migration_marker()).ok()?;
        serde_json::from_str(&text).ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::InstanceSettingsPatch;
    use std::sync::Arc;

    fn temp(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "rmccc-svc-{tag}-{}-{}",
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

    fn write_version(root: &Path, id: &str, json: &str) {
        let dir = root.join("versions").join(id);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join(format!("{id}.json")), json).unwrap();
        std::fs::write(dir.join(format!("{id}.jar")), b"jar").unwrap();
    }

    fn vanilla(id: &str) -> String {
        format!(
            r#"{{
                "id": "{id}",
                "type": "release",
                "mainClass": "net.minecraft.client.main.Main",
                "assetIndex": {{"id": "17", "url": "https://example.com/17.json", "sha1": "abc", "size": 1, "totalSize": 1}},
                "downloads": {{"client": {{"url": "https://example.com/{id}.jar", "sha1": "def", "size": 2}}}},
                "libraries": [{{"name": "com.mojang:test:1.0"}}],
                "javaVersion": {{"majorVersion": 21}}
            }}"#
        )
    }

    fn service(tag: &str) -> (PathBuf, InstanceService) {
        let data = temp(tag);
        let config = AppConfig {
            data_dir: Some(data.clone()),
            ..Default::default()
        };
        std::fs::create_dir_all(config.resolved_game_root()).unwrap();
        let service = InstanceService::new(
            &data,
            &config,
            Arc::new(rmccc_platform::Platform::current()),
            None,
        )
        .unwrap();
        (data, service)
    }

    #[test]
    fn list_details_is_scoped_to_the_selected_directory() {
        let (data, mut service) = service("scoped");
        write_version(&data.join("instances"), "1.20.4", &vanilla("1.20.4"));
        let external = data.join("external");
        std::fs::create_dir_all(&external).unwrap();
        write_version(&external, "1.21", &vanilla("1.21"));
        let external_dir = service
            .add_directory("External", &external, GameDirectoryKind::External)
            .unwrap();
        let default_id = service.selected_directory().id.clone();
        service.refresh(Some(default_id.as_str()), true).unwrap();

        let default_list = service
            .list_details(Some(default_id.as_str()), false, false)
            .unwrap();
        assert_eq!(default_list.len(), 1);
        assert_eq!(default_list[0].summary.id, "1.20.4");

        let external_list = service
            .list_details(Some(external_dir.id.as_str()), false, false)
            .unwrap();
        assert_eq!(external_list.len(), 1);
        assert_eq!(external_list[0].summary.id, "1.21");

        assert_eq!(service.list_details(None, true, false).unwrap().len(), 2);

        service.select_directory(external_dir.id.as_str()).unwrap();
        let selected = service.list_details(None, false, false).unwrap();
        assert_eq!(selected.len(), 1);
        assert_eq!(selected[0].summary.id, "1.21");
    }

    #[test]
    fn lists_settings_and_paths() {
        let (data, mut service) = service("list");
        let root = service.selected_directory().path.clone();
        write_version(&root, "1.20.4", &vanilla("1.20.4"));
        service.refresh(None, false).unwrap();
        service.ensure_instance("default", "1.20.4").unwrap();

        service
            .update_settings(
                "default/1.20.4",
                InstanceSettingsPatch {
                    display_name: Some("Survival".into()),
                    max_memory_mb: Some(8192),
                    override_properties: Some(vec![fields::MAX_MEMORY_MB.into()]),
                    ..Default::default()
                },
            )
            .unwrap();

        let list = service.list(None, false, false).unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].display_name, "Survival");
        assert_eq!(list[0].game_version, "1.20.4");
        assert_eq!(list[0].missing_count, None);

        let detail = service.get("default/1.20.4").unwrap();
        assert_eq!(detail.effective.max_memory_mb, 8192);
        assert!(detail.settings_exists);
        assert_eq!(
            detail.paths.instance_dir,
            root.join("versions").join("1.20.4")
        );
        let _ = std::fs::remove_dir_all(&data);
    }

    #[test]
    fn rename_updates_child_inherits_from_and_settings_follow() {
        let (data, mut service) = service("rename");
        let root = service.selected_directory().path.clone();
        write_version(&root, "1.20.4", &vanilla("1.20.4"));
        write_version(
            &root,
            "1.20.4-fabric",
            r#"{"id":"1.20.4-fabric","type":"release","inheritsFrom":"1.20.4","patches":[{"id":"fabric","version":"0.16.9","mainClass":"net.fabricmc.loader.impl.launch.knot.KnotClient"}]}"#,
        );
        service.refresh(None, false).unwrap();
        service.ensure_instance("default", "1.20.4").unwrap();
        service
            .update_settings(
                "default/1.20.4",
                InstanceSettingsPatch {
                    display_name: Some("Vanilla".into()),
                    ..Default::default()
                },
            )
            .unwrap();

        service.rename("default/1.20.4", "1.20.4-base").unwrap();
        assert!(!service.get("default/1.20.4").is_ok());
        let detail = service.get("default/1.20.4-base").unwrap();
        assert_eq!(detail.summary.display_name, "Vanilla");
        // Child still resolves after its parent was renamed.
        let child = service.get("default/1.20.4-fabric").unwrap();
        assert_eq!(child.manifest.inherits_from.as_deref(), Some("1.20.4-base"));
        let _ = std::fs::remove_dir_all(&data);
    }

    #[test]
    fn duplicate_resets_metadata_and_hides_without_deleting() {
        let (data, mut service) = service("dup");
        let root = service.selected_directory().path.clone();
        write_version(&root, "1.20.4", &vanilla("1.20.4"));
        service.refresh(None, false).unwrap();
        service.ensure_instance("default", "1.20.4").unwrap();
        service
            .update_settings(
                "default/1.20.4",
                InstanceSettingsPatch {
                    display_name: Some("Original".into()),
                    ..Default::default()
                },
            )
            .ok();

        service
            .duplicate("default/1.20.4", "1.20.4-copy", false)
            .unwrap();
        let copy = service.get("default/1.20.4-copy").unwrap();
        assert_eq!(copy.summary.display_name, "1.20.4-copy");
        assert!(!copy.paths.instance_dir.join("saves").exists());
        assert_eq!(copy.summary.play_time_seconds, 0);

        service.remove("default/1.20.4-copy", false).unwrap();
        assert!(!service
            .list(None, false, false)
            .unwrap()
            .iter()
            .any(|i| i.id == "1.20.4-copy"));
        assert!(root
            .join("versions")
            .join("1.20.4-copy")
            .join("1.20.4-copy.json")
            .exists());

        service.remove("default/1.20.4-copy", true).unwrap();
        assert!(!root.join("versions").join("1.20.4-copy").exists());
        let _ = std::fs::remove_dir_all(&data);
    }

    #[test]
    fn isolation_uses_instance_id_not_base_version() {
        let (data, mut service) = service("iso");
        let root = service.selected_directory().path.clone();
        write_version(&root, "1.20.4", &vanilla("1.20.4"));
        write_version(
            &root,
            "1.20.4-fabric",
            r#"{"id":"1.20.4-fabric","type":"release","inheritsFrom":"1.20.4","patches":[{"id":"fabric","version":"0.16.9"}]}"#,
        );
        service.refresh(None, false).unwrap();
        service.ensure_instance("default", "1.20.4-fabric").unwrap();
        let detail = service.get("default/1.20.4-fabric").unwrap();
        assert_eq!(detail.summary.game_version, "1.20.4");
        assert_eq!(
            detail.paths.run_dir,
            root.join("versions").join("1.20.4-fabric")
        );
        let _ = std::fs::remove_dir_all(&data);
    }
}
