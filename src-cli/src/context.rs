//! CLI context — the single backend facade used by every command handler.
//!
//! Wraps [`rmccc_core::CoreContext`] (the DI container) and exposes one method
//! per core capability so that:
//! - CLI commands map 1:1 to backend capabilities (no business logic in the
//!   presentation layer),
//! - the Tauri command layer and the CLI share the exact same orchestration
//!   (both delegate to `rmccc_core::pipeline` / the DI providers),
//! - launch lifecycle (running games, exit signals) is observable and
//!   controllable for state-sync (`launch status` / `stop` / `wait`).
//!
//! All methods return `rmccc_error::Result<T>`; the dispatcher converts to
//! [`crate::error::CliError`] with a stable error code + exit code.

use rmccc_auth::Account;
use rmccc_core::pipeline::{self, GameOutputCb, ProgressCb};
use rmccc_core::CoreContext;
use rmccc_error::{Result, RmcccError};
use rmccc_version::GameRepository as _;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// A currently running game process (for state observation).
#[derive(Debug, Clone)]
pub struct RunningGame {
    pub profile_id: String,
    pub pid: u32,
}

/// Launch entry callback fired when the game process exits.
pub type CliExitCb = Arc<dyn Fn(i32, rmccc_launch::ExitType) + Send + Sync>;

/// Options accepted by [`CliContext::launch_start`].
#[derive(Default, Clone)]
pub struct LaunchStartOptions {
    /// Skip the pre-launch completeness check & auto-repair.
    pub no_repair: bool,
    /// Game stdout/stderr line callback.
    pub output: Option<GameOutputCb>,
    /// Pipeline progress callback.
    pub progress: Option<ProgressCb>,
    /// Callback invoked on process exit (event stream / status sync).
    pub on_exit: Option<CliExitCb>,
}

/// Unified backend API facade used by CLI commands.
#[derive(Clone)]
pub struct CliContext {
    pub core: Arc<tokio::sync::RwLock<CoreContext>>,
    pub http_client: reqwest::Client,
    pub data_dir: PathBuf,
    /// profile_id → OS pid of a running game (state sync).
    pub running_games: Arc<dashmap::DashMap<String, u32>>,
    /// profile_id → exit-code watch channel (allows `launch wait`).
    pub exit_signals: Arc<dashmap::DashMap<String, tokio::sync::watch::Receiver<Option<i32>>>>,
}

impl CliContext {
    pub fn new(data_dir: &Path) -> Result<Self> {
        std::fs::create_dir_all(data_dir)?;
        let core = CoreContext::new(data_dir)?;
        let http_client = core.http_client.clone();
        Ok(Self {
            core: Arc::new(tokio::sync::RwLock::new(core)),
            http_client,
            data_dir: data_dir.to_path_buf(),
            running_games: Arc::new(dashmap::DashMap::new()),
            exit_signals: Arc::new(dashmap::DashMap::new()),
        })
    }

    /// Load a profile or fail with `ProfileNotFound`.
    pub async fn get_profile(&self, id: &str) -> Result<rmccc_profile::Profile> {
        let core = self.core.read().await;
        core.profile_store
            .get(id)?
            .ok_or_else(|| RmcccError::ProfileNotFound(id.to_string()))
    }

    // ────────────────────────── Auth ──────────────────────────

    pub async fn auth_login_microsoft(
        &self,
        progress: Option<rmccc_auth::ProgressCallback>,
    ) -> Result<rmccc_auth::AuthSession> {
        let session = {
            let core = self.core.read().await;
            let account = core
                .account_repo
                .factory("microsoft")?
                .create(rmccc_auth::CreateAccountContext {
                    additional_data: None,
                    progress,
                })
                .await?;
            core.account_repo.save_account(account.as_ref())?;
            account.to_session()
        };
        Ok(session)
    }

    pub async fn auth_login_offline(&self, username: &str) -> Result<rmccc_auth::AuthSession> {
        let session = {
            let core = self.core.read().await;
            let account = core
                .account_repo
                .factory("offline")?
                .create(rmccc_auth::CreateAccountContext {
                    additional_data: Some(json!({ "username": username })),
                    progress: None,
                })
                .await?;
            core.account_repo.save_account(account.as_ref())?;
            account.to_session()
        };
        Ok(session)
    }

    /// Log out the active account.
    pub async fn auth_logout(&self) -> Result<()> {
        let core = self.core.read().await;
        if let Some(active) = core.account_repo.load_active()? {
            core.account_repo.remove(&active.account_id)?;
        }
        Ok(())
    }

    /// Log out a specific account by id.
    pub async fn auth_logout_account(&self, account_id: &str) -> Result<()> {
        let core = self.core.read().await;
        core.account_repo.remove(account_id)
    }

    /// Refresh the active session (None when no active account).
    pub async fn auth_refresh(&self) -> Result<Option<rmccc_auth::AuthSession>> {
        let core = self.core.read().await;
        match core.account_repo.load_active()? {
            Some(s) => Ok(Some(core.refresh_session_if_needed(&s).await?)),
            None => Ok(None),
        }
    }

    /// Refresh a specific account session.
    pub async fn auth_refresh_account(
        &self,
        account_id: &str,
    ) -> Result<Option<rmccc_auth::AuthSession>> {
        let core = self.core.read().await;
        let session = core.account_repo.load(account_id)?;
        match session {
            Some(s) => Ok(Some(core.refresh_session_if_needed(&s).await?)),
            None => Ok(None),
        }
    }

    pub async fn auth_status(&self) -> Result<Option<rmccc_auth::AuthSession>> {
        let core = self.core.read().await;
        core.account_repo.load_active()
    }

    /// List persisted account metadata (plaintext — no tokens ever cross).
    pub async fn auth_list(&self) -> Result<Vec<rmccc_auth::AccountMetadata>> {
        let core = self.core.read().await;
        core.account_repo.list_metadata()
    }

    /// Select an account as the active one.
    pub async fn auth_select(&self, account_id: &str) -> Result<()> {
        let core = self.core.read().await;
        core.account_repo.select(account_id)
    }

    // ────────────────────────── Config / state ──────────────────────────

    /// Read the launcher settings; `key` restricts to one field.
    pub async fn config_get(&self, key: Option<&str>) -> Result<Value> {
        let core = self.core.read().await;
        let v = serde_json::to_value(&core.config)?;
        match key {
            Some(k) => Ok(v.get(k).cloned().unwrap_or(Value::Null)),
            None => Ok(v),
        }
    }

    /// Structured settings update (whitelist + type/range validation).
    /// Returns the updated config as JSON.
    pub async fn config_set(&self, field: &str, value: &str) -> Result<Value> {
        let mut core = self.core.write().await;
        match field {
            "data_dir" => core.config.data_dir = Some(PathBuf::from(value)),
            "game_root_dir" => core.config.game_root_dir = Some(PathBuf::from(value)),
            "default_java_path" => core.config.default_java_path = parse_optional_str(value)?,
            "default_min_memory_mb" => {
                core.config.default_min_memory_mb = parse_bounded_u32(value, 128, 65536)?
            }
            "default_max_memory_mb" => {
                core.config.default_max_memory_mb = parse_bounded_u32(value, 256, 65536)?
            }
            "download_concurrency" => {
                core.config.download_concurrency = parse_bounded_u32(value, 1, 64)?
            }
            "mirror_url" => core.config.mirror_url = parse_optional_str(value)?,
            "event_throttle_ms" => core.config.event_throttle_ms = parse_u64_field(value)?,
            other => {
                return Err(RmcccError::Other(format!(
                    "Unknown settings field '{other}' (see `rmcli config set --help`)"
                )))
            }
        }
        core.config.save(&core.data_dir)?;
        Ok(serde_json::to_value(&core.config)?)
    }

    /// Resolved data dir + game root (for `config path`).
    pub async fn config_paths(&self) -> Result<Value> {
        let core = self.core.read().await;
        Ok(json!({
            "data_dir": core.data_dir.display().to_string(),
            "resolved_data_dir": core.config.resolved_data_dir().display().to_string(),
            "game_root_dir": core.config.resolved_game_root().display().to_string(),
            "default_java_path": core.config.default_java_path,
        }))
    }

    pub async fn state_get(&self, key: Option<&str>) -> Result<Value> {
        let core = self.core.read().await;
        let v = serde_json::to_value(&core.launcher_state)?;
        match key {
            Some(k) => Ok(v.get(k).cloned().unwrap_or(Value::Null)),
            None => Ok(v),
        }
    }

    pub async fn state_set(&self, field: &str, value: &str) -> Result<Value> {
        let mut core = self.core.write().await;
        match field {
            "selected_profile_id" => {
                core.launcher_state.selected_profile_id = parse_optional_str(value)?
            }
            other => {
                return Err(RmcccError::Other(format!(
                    "Unknown state field '{other}' (only 'selected_profile_id')"
                )))
            }
        }
        core.launcher_state.save(&core.data_dir)?;
        Ok(serde_json::to_value(&core.launcher_state)?)
    }

    pub async fn user_get(&self, key: Option<&str>) -> Result<Value> {
        let settings = rmccc_config::UserSettings::load(&self.data_dir)?;
        let v = serde_json::to_value(&settings)?;
        match key {
            Some(k) => Ok(v.get(k).cloned().unwrap_or(Value::Null)),
            None => Ok(v),
        }
    }

    pub async fn user_set(&self, field: &str, value: &str) -> Result<Value> {
        let mut settings = rmccc_config::UserSettings::load(&self.data_dir)?;
        match field {
            "locale" => settings.locale = parse_optional_str(value)?,
            "check_updates" => settings.check_updates = parse_bool(value)?,
            other => {
                return Err(RmcccError::Other(format!(
                    "Unknown user-settings field '{other}' (locale | check_updates)"
                )))
            }
        }
        settings.save(&self.data_dir)?;
        Ok(serde_json::to_value(&settings)?)
    }

    // ────────────────────────── Profile ──────────────────────────

    pub async fn profile_list(&self) -> Result<Vec<rmccc_profile::Profile>> {
        self.core.read().await.profile_store.list()
    }

    pub async fn profile_create(
        &self,
        name: &str,
        game_version: &str,
        loader: Option<&str>,
        memory_mb: Option<u32>,
    ) -> Result<rmccc_profile::Profile> {
        if name.trim().is_empty() {
            return Err(RmcccError::Other(
                "Profile name cannot be empty".to_string(),
            ));
        }
        if name.chars().count() > 64 {
            return Err(RmcccError::Other(format!(
                "Profile name too long (max 64 chars, got {})",
                name.chars().count()
            )));
        }
        let mod_loader = loader
            .and_then(rmccc_modloader::ModLoaderType::from_str)
            .map(|lt| rmccc_profile::ModLoaderConfig {
                loader_type: lt,
                loader_version: String::new(),
            });
        let core = self.core.write().await;
        let id = uuid::Uuid::new_v4().to_string();
        // Shared game root, matching what `GameRepository` indexes and where
        // installs land (see the Tauri `profile_create` for the full rationale).
        let game_dir = core.config.resolved_game_root();
        let profile = rmccc_profile::Profile {
            id,
            name: name.to_string(),
            game_version: game_version.to_string(),
            mod_loader,
            memory_mb: memory_mb.unwrap_or(core.config.default_max_memory_mb),
            game_dir,
            ..Default::default()
        };
        core.profile_store.save(&profile)?;
        Ok(profile)
    }

    /// Update select profile fields; `None` means "leave unchanged".
    #[allow(clippy::too_many_arguments)]
    pub async fn profile_update(
        &self,
        id: &str,
        name: Option<&str>,
        game_version: Option<&str>,
        java_path: Option<&str>,
        jvm_args: Option<Vec<String>>,
        game_args: Option<Vec<String>>,
        memory_mb: Option<u32>,
        icon: Option<&str>,
    ) -> Result<rmccc_profile::Profile> {
        let core = self.core.write().await;
        let mut profile = core
            .profile_store
            .get(id)?
            .ok_or_else(|| RmcccError::ProfileNotFound(id.to_string()))?;
        if let Some(name) = name {
            profile.name = name.to_string();
        }
        if let Some(gv) = game_version {
            profile.game_version = gv.to_string();
        }
        if let Some(jp) = java_path {
            profile.java_path = if jp.is_empty() {
                None
            } else {
                Some(jp.to_string())
            };
        }
        if let Some(args) = jvm_args {
            profile.jvm_args = args;
        }
        if let Some(args) = game_args {
            profile.game_args = args;
        }
        if let Some(mem) = memory_mb {
            profile.memory_mb = mem;
        }
        if let Some(icon) = icon {
            profile.icon = if icon.is_empty() {
                None
            } else {
                Some(icon.to_string())
            };
        }
        core.profile_store.save(&profile)?;
        Ok(profile)
    }

    pub async fn profile_delete(&self, id: &str, cleanup: bool) -> Result<()> {
        let core = self.core.read().await;
        core.profile_store.delete(id, cleanup)
    }

    pub async fn profile_select(&self, id: &str) -> Result<()> {
        // Validate the profile exists first.
        self.get_profile(id).await?;
        let mut core = self.core.write().await;
        core.launcher_state.selected_profile_id = Some(id.to_string());
        core.launcher_state.save(&core.data_dir)?;
        Ok(())
    }

    pub async fn profile_active(&self) -> Result<Option<rmccc_profile::Profile>> {
        let core = self.core.read().await;
        match &core.launcher_state.selected_profile_id {
            Some(id) => core.profile_store.get(id),
            None => Ok(None),
        }
    }

    // ────────────────────────── Version ──────────────────────────

    pub async fn version_list(&self, refresh: bool) -> Result<Vec<rmccc_version::VersionInfo>> {
        let versions = {
            let core = self.core.read().await;
            match core.version_provider.fetch_versions().await {
                Ok(v) => v,
                Err(e) if e.is_retryable() && !refresh => {
                    tracing::warn!("Network error fetching versions, using cache: {e}");
                    let cache =
                        rmccc_version::cache::VersionCache::new(&self.data_dir.join("cache"), 3600);
                    cache.read()?.unwrap_or_default()
                }
                Err(e) => return Err(e),
            }
        };
        // Refresh the on-disk cache in the background.
        if !versions.is_empty() {
            let cache_dir = self.data_dir.join("cache");
            let list = versions.clone();
            tokio::spawn(async move {
                let cache = rmccc_version::cache::VersionCache::new(&cache_dir, 3600);
                if let Err(e) = cache.write(&list) {
                    tracing::warn!("Failed to update version cache: {e}");
                }
            });
        }
        Ok(versions)
    }

    pub async fn version_get(&self, id: &str) -> Result<rmccc_version::GameVersion> {
        let core = self.core.read().await;
        core.version_provider.fetch_version(id).await
    }

    /// Install a version into a profile (client jar + libraries + assets +
    /// natives) via the canonical `pipeline::install_game` orchestration, then
    /// update the profile and refresh the local repository.
    pub async fn version_install(
        &self,
        profile_id: &str,
        version: &str,
        progress: Option<&ProgressCb>,
    ) -> Result<()> {
        let profile = self.get_profile(profile_id).await?;
        {
            let core = self.core.read().await;
            pipeline::install_game(&core, &profile, version, progress).await?;
        }
        let core = self.core.read().await;
        let mut p = core
            .profile_store
            .get(profile_id)?
            .ok_or_else(|| RmcccError::ProfileNotFound(profile_id.to_string()))?;
        p.game_version = version.to_string();
        core.profile_store.save(&p)?;
        core.game_repository
            .write()
            .map_err(|_| RmcccError::Other("game repository lock poisoned".to_string()))?
            .refresh()?;
        tracing::info!(profile_id = %profile_id, version = %version, "Version installed");
        Ok(())
    }

    /// List locally installed versions (from the local repository index).
    pub async fn version_local_list(&self) -> Result<Vec<Value>> {
        let core = self.core.read().await;
        let repo = core
            .game_repository
            .read()
            .map_err(|_| RmcccError::Other("game repository lock poisoned".to_string()))?;
        let ids = repo.get_instance_ids();
        let mut out = Vec::new();
        for id in ids {
            let manifest = repo.get_manifest(&id).cloned().unwrap_or_default();
            let base = repo.get_game_version(&id);
            out.push(json!({
                "id": id,
                "base_version": base,
                "main_class": manifest.main_class,
                "inherits_from": manifest.inherits_from,
                "library_count": manifest.libraries.len(),
            }));
        }
        Ok(out)
    }

    pub async fn version_local_get(&self, id: &str) -> Result<rmccc_version::GameVersion> {
        let core = self.core.read().await;
        let repo = core
            .game_repository
            .read()
            .map_err(|_| RmcccError::Other("game repository lock poisoned".to_string()))?;
        repo.get_manifest(id)
            .cloned()
            .ok_or_else(|| RmcccError::Other(format!("Local version '{id}' is not installed")))
    }

    pub async fn version_local_delete(&self, id: &str) -> Result<()> {
        let core = self.core.read().await;
        let mut repo = core
            .game_repository
            .write()
            .map_err(|_| RmcccError::Other("game repository lock poisoned".to_string()))?;
        repo.remove_instance(id)
    }

    // ────────────────────────── Resource ──────────────────────────

    /// Install game resources for a profile (canonical pipeline).
    pub async fn resource_install(
        &self,
        profile_id: &str,
        version: &str,
        progress: Option<&ProgressCb>,
    ) -> Result<()> {
        let profile = self.get_profile(profile_id).await?;
        let core = self.core.read().await;
        pipeline::install_game(&core, &profile, version, progress).await?;
        // Refresh the local index so the new files are observed.
        core.game_repository
            .write()
            .map_err(|_| RmcccError::Other("game repository lock poisoned".to_string()))?
            .refresh()?;
        Ok(())
    }

    /// Completeness check against the local repository (offline).
    pub async fn resource_check(&self, profile_id: &str) -> Result<Value> {
        let profile = self.get_profile(profile_id).await?;
        let core = self.core.read().await;
        let (game_root, resolved) = {
            let repo = core
                .game_repository
                .read()
                .map_err(|_| RmcccError::Other("game repository lock poisoned".to_string()))?;
            let resolved = repo
                .get_resolved(&profile.game_version)
                .map_err(|e| RmcccError::Other(e.to_string()))?;
            (repo.game_dir().to_path_buf(), resolved)
        };
        let missing = rmccc_version::completion::check_completion(&game_root, &resolved);
        let items: Vec<Value> = missing
            .iter()
            .map(|m| {
                json!({
                    "kind": format!("{:?}", m.kind),
                    "description": m.description,
                    "path": m.path.display().to_string(),
                })
            })
            .collect();
        Ok(json!({
            "profile_id": profile_id,
            "game_version": profile.game_version,
            "complete": items.is_empty(),
            "missing_count": items.len(),
            "missing": items,
        }))
    }

    /// Repair missing resources (HMCL checkGameCompletionAsync equivalent).
    /// Returns whether anything was repaired.
    pub async fn resource_repair(
        &self,
        profile_id: &str,
        progress: Option<&ProgressCb>,
    ) -> Result<bool> {
        let profile = self.get_profile(profile_id).await?;
        let core = self.core.read().await;
        pipeline::ensure_launch_ready(&core, &profile, progress).await
    }

    /// SHA1 integrity verification for a profile's game files.
    pub async fn resource_verify(&self, profile_id: &str) -> Result<Value> {
        let profile = self.get_profile(profile_id).await?;
        let game_dir = &profile.game_dir;
        let version_id = &profile.game_version;

        let provider: &dyn rmccc_version::VersionProvider =
            &rmccc_version::MojangVersionProvider::new(self.http_client.clone());
        let game_version = rmccc_version::resolve_version(version_id, provider).await?;

        let mut verified = 0u64;
        let mut failed: Vec<Value> = Vec::new();
        let mut missing: Vec<Value> = Vec::new();

        // Client JAR.
        if let Some(client_dl) = game_version.downloads.get("client") {
            let jar_path = game_dir
                .join("versions")
                .join(version_id)
                .join(format!("{version_id}.jar"));
            match verify_sha1(&jar_path, &client_dl.sha1) {
                Ok(true) => verified += 1,
                Ok(false) => failed.push(json!({
                    "file": jar_path.display().to_string(),
                    "reason": "sha1_mismatch",
                })),
                Err(_) => missing.push(json!({
                    "file": jar_path.display().to_string(),
                    "reason": "missing",
                })),
            }
        }

        // Libraries (sample check up to 50 to bound runtime).
        let libs_dir = game_dir.join("libraries");
        let mut lib_checked = 0u64;
        let mut lib_missing = 0u64;
        for lib in game_version
            .libraries
            .iter()
            .filter(|l| rmccc_version::libraries::library_applies(l))
        {
            if lib_checked >= 50 {
                break;
            }
            let lib_path = make_library_path(&lib.name, &libs_dir);
            if lib_path.exists() {
                lib_checked += 1;
            } else {
                lib_missing += 1;
            }
        }

        let assets_dir = game_dir.join("assets").join("objects");
        let asset_count = walk_dir_top(&assets_dir).len() as u64;

        let ok = failed.is_empty() && missing.is_empty();
        Ok(json!({
            "profile_id": profile_id,
            "game_version": version_id,
            "verified": verified + lib_checked,
            "verified_files": verified,
            "libraries_checked": lib_checked,
            "libraries_missing": lib_missing,
            "asset_objects": asset_count,
            "failed": failed,
            "missing": missing,
            "ok": ok,
        }))
    }

    // ────────────────────────── Modloader ──────────────────────────

    pub async fn modloader_list(
        &self,
        loader_type: &rmccc_modloader::ModLoaderType,
        game_version: &str,
    ) -> Result<Vec<rmccc_modloader::LoaderVersion>> {
        let dm = rmccc_modloader::DependencyManager::new(self.http_client.clone());
        dm.list_versions(loader_type, game_version).await
    }

    /// Install a modloader into a profile. Returns the resulting version id.
    pub async fn modloader_install(
        &self,
        profile_id: &str,
        loader_type: &rmccc_modloader::ModLoaderType,
        loader_version: &str,
        progress: Option<rmccc_modloader::ProgressCallback>,
    ) -> Result<String> {
        let (profile, game_dir) = {
            let p = self.get_profile(profile_id).await?;
            (p.clone(), p.game_dir.clone())
        };

        let required_java = rmccc_version::java_version_for_mc(&profile.game_version);
        let java_install = rmccc_launch::find_java(required_java)?;

        // Forge / NeoForge use the installer-JAR provider (needs java_path);
        // the remaining loaders use the unified DependencyManager.
        let version_id = match loader_type {
            rmccc_modloader::ModLoaderType::Forge | rmccc_modloader::ModLoaderType::NeoForge => {
                let core = self.core.read().await;
                let provider = core.modloader_provider(loader_type)?;
                provider
                    .install(
                        &profile.game_version,
                        loader_version,
                        &game_dir,
                        &java_install.path,
                    )
                    .await?
            }
            _ => {
                let dm = rmccc_modloader::DependencyManager::new(self.http_client.clone());
                let provider = rmccc_version::MojangVersionProvider::new(self.http_client.clone());
                let base_version =
                    rmccc_version::resolve_version(&profile.game_version, &provider).await?;
                let updated = dm
                    .install(
                        loader_type,
                        &profile.game_version,
                        &base_version,
                        loader_version,
                        &game_dir,
                        &java_install.path,
                    )
                    .await?;
                let version_dir = game_dir.join("versions").join(&updated.id);
                std::fs::create_dir_all(&version_dir)?;
                let json_path = version_dir.join(format!("{}.json", updated.id));
                std::fs::write(&json_path, serde_json::to_string_pretty(&updated)?)?;
                updated.id
            }
        };

        // Update the profile with the installed loader.
        {
            let core = self.core.write().await;
            if let Ok(Some(ref mut p)) = core.profile_store.get(profile_id) {
                p.mod_loader = Some(rmccc_profile::ModLoaderConfig {
                    loader_type: loader_type.clone(),
                    loader_version: loader_version.to_string(),
                });
                p.game_version = version_id.clone();
                let _ = core.profile_store.save(p);
            }
            let _ = core.game_repository.write().map(|mut r| r.refresh());
        }
        let _ = progress;
        Ok(version_id)
    }

    /// Remove the installed modloader from a profile (reset to the base
    /// Minecraft version — the incremental model keeps the vanilla manifest
    /// intact under the base id).
    pub async fn modloader_remove(&self, profile_id: &str) -> Result<rmccc_profile::Profile> {
        let core = self.core.write().await;
        let mut profile = core
            .profile_store
            .get(profile_id)?
            .ok_or_else(|| RmcccError::ProfileNotFound(profile_id.to_string()))?;

        if profile.mod_loader.is_none() {
            return Err(RmcccError::Other(format!(
                "Profile '{profile_id}' has no modloader installed"
            )));
        }

        // Resolve the base Minecraft version (root of the inheritance chain).
        let base = {
            let repo = core
                .game_repository
                .read()
                .map_err(|_| RmcccError::Other("game repository lock poisoned".to_string()))?;
            repo.get_game_version(&profile.game_version)
        };

        let removed_version = profile.game_version.clone();
        profile.game_version = base;
        profile.mod_loader = None;
        core.profile_store.save(&profile)?;

        // Clean up the patched version directory (optional, non-fatal).
        let patched_dir = profile.game_dir.join("versions").join(&removed_version);
        if patched_dir.exists() {
            let _ = std::fs::remove_dir_all(&patched_dir);
        }

        tracing::info!(
            profile_id = %profile_id,
            removed = %removed_version,
            base = %profile.game_version,
            "Modloader removed"
        );
        Ok(profile)
    }

    /// Analyze which modloaders are installed in a version (for diagnostics).
    pub async fn modloader_analyze(&self, version_id: &str) -> Result<Value> {
        use rmccc_modloader::LibraryType;
        let game_version = self.version_local_get(version_id).await?;
        let analyzer = rmccc_modloader::LibraryAnalyzer::analyze(&game_version, &game_version.id);
        let mut detected: Vec<Value> = Vec::new();
        for ty in [
            LibraryType::Fabric,
            LibraryType::Forge,
            LibraryType::NeoForge,
            LibraryType::Quilt,
            LibraryType::LiteLoader,
            LibraryType::OptiFine,
            LibraryType::Cleanroom,
            LibraryType::LegacyFabric,
        ] {
            if analyzer.has(ty) {
                detected.push(json!({
                    "loader": ty.patch_id(),
                    "version": analyzer.get_version(ty),
                }));
            }
        }
        Ok(json!({
            "version": version_id,
            "installed_loaders": detected,
            "has_mod_launcher": analyzer.has_mod_launcher(),
        }))
    }

    // ────────────────────────── Mods ──────────────────────────

    /// Build a fresh Modrinth source on the shared HTTP client. The DI
    /// container's `mod_sources` list is the canonical place where sources are
    /// registered; a fresh instance keeps the facade free of borrow problems
    /// while sharing the exact same HTTP client (connection pooling / proxy).
    fn mod_source(&self) -> Result<Box<dyn rmccc_mods::ModSource>> {
        let source: Box<dyn rmccc_mods::ModSource> = Box::new(
            rmccc_mods::modrinth::ModrinthSource::new(self.http_client.clone()),
        );
        Ok(source)
    }

    pub async fn mod_search(
        &self,
        query: &str,
        game_version: Option<&str>,
        loader: Option<&str>,
        page: u32,
        limit: u32,
    ) -> Result<rmccc_mods::ModSearchResult> {
        let source = self.mod_source()?;
        source
            .search(&rmccc_mods::ModSearchQuery {
                query: query.to_string(),
                game_version: game_version.map(str::to_string),
                loader: loader.map(str::to_string),
                page,
                limit: limit.min(100),
            })
            .await
    }

    pub async fn mod_versions(&self, mod_id: &str) -> Result<Vec<rmccc_mods::ModVersion>> {
        let source = self.mod_source()?;
        source.list_versions(mod_id).await
    }

    pub async fn mod_download_url(&self, version_id: &str) -> Result<String> {
        let source = self.mod_source()?;
        source.get_download_url(version_id).await
    }

    pub async fn mod_list(&self, profile_id: &str) -> Result<Value> {
        let profile = self.get_profile(profile_id).await?;
        let mods_dir = profile.game_dir.join("mods");
        let mut active = Vec::new();
        let mut disabled = Vec::new();
        if let Ok(entries) = std::fs::read_dir(&mods_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_file() && path.extension().is_some_and(|e| e == "jar") {
                    active.push(
                        path.file_name()
                            .unwrap_or_default()
                            .to_string_lossy()
                            .to_string(),
                    );
                }
            }
        }
        let disabled_dir = mods_dir.join("disabled");
        if let Ok(entries) = std::fs::read_dir(&disabled_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_file() && path.extension().is_some_and(|e| e == "jar") {
                    disabled.push(
                        path.file_name()
                            .unwrap_or_default()
                            .to_string_lossy()
                            .to_string(),
                    );
                }
            }
        }
        active.sort();
        disabled.sort();
        Ok(json!({
            "profile_id": profile_id,
            "mods_dir": mods_dir.display().to_string(),
            "active": active,
            "disabled": disabled,
            "count": active.len() + disabled.len(),
        }))
    }

    pub async fn mod_install(
        &self,
        profile_id: &str,
        version: rmccc_mods::ModVersion,
    ) -> Result<String> {
        let profile = self.get_profile(profile_id).await?;
        let source = self.mod_source()?;
        let mods_dir = profile.game_dir.join("mods");
        std::fs::create_dir_all(&mods_dir)?;

        let filename = version
            .filename
            .clone()
            .filter(|f| !f.trim().is_empty())
            .map(|f| sanitize_filename(&f))
            .unwrap_or_else(|| format!("{}.jar", sanitize_filename(&version.name)));
        let filename = if filename.ends_with(".jar") {
            filename
        } else {
            format!("{filename}.jar")
        };
        validate_mod_filename(&filename)?;

        let dest = mods_dir.join(&filename);
        source.install(&version, &dest).await?;
        tracing::info!(profile_id = %profile_id, filename = %filename, "Mod installed");
        Ok(filename)
    }

    /// Install a local jar file into a profile's mods directory (copies the
    /// file with a sanitized name derived from the source filename).
    pub async fn mod_install_local(&self, profile_id: &str, src_path: &str) -> Result<String> {
        let profile = self.get_profile(profile_id).await?;
        let src = Path::new(src_path);
        if !src.is_file() {
            return Err(RmcccError::Other(format!(
                "Local mod file not found: {src_path}"
            )));
        }
        let stem = src
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();
        let filename = sanitize_filename(&stem);
        let filename = if filename.ends_with(".jar") {
            filename
        } else {
            format!("{filename}.jar")
        };
        validate_mod_filename(&filename)?;

        let mods_dir = profile.game_dir.join("mods");
        std::fs::create_dir_all(&mods_dir)?;
        let dest = mods_dir.join(&filename);
        std::fs::copy(src, &dest)?;
        tracing::info!(profile_id = %profile_id, filename = %filename, "Local mod installed");
        Ok(filename)
    }

    pub async fn mod_remove(&self, profile_id: &str, filename: &str) -> Result<()> {
        validate_mod_filename(filename)?;
        let profile = self.get_profile(profile_id).await?;
        let mods_dir = profile.game_dir.join("mods");
        let active = mods_dir.join(filename);
        let disabled = mods_dir.join("disabled").join(filename);
        if active.exists() {
            std::fs::remove_file(&active)?;
        } else if disabled.exists() {
            std::fs::remove_file(&disabled)?;
        } else {
            return Err(RmcccError::Other(format!("Mod file not found: {filename}")));
        }
        Ok(())
    }

    pub async fn mod_toggle(&self, profile_id: &str, filename: &str, enabled: bool) -> Result<()> {
        validate_mod_filename(filename)?;
        let profile = self.get_profile(profile_id).await?;
        let mods_dir = profile.game_dir.join("mods");
        let disabled_dir = mods_dir.join("disabled");
        let (src, dst) = if enabled {
            (disabled_dir.join(filename), mods_dir.join(filename))
        } else {
            (mods_dir.join(filename), disabled_dir.join(filename))
        };
        if !src.exists() {
            return Err(RmcccError::Other(format!("Mod file not found: {filename}")));
        }
        if let Some(parent) = dst.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::rename(&src, &dst)?;
        Ok(())
    }

    // ────────────────────────── Launch ──────────────────────────

    /// Start a game process (HMCL-aligned orchestration), returning its pid.
    ///
    /// Guards against duplicate launches, runs the pre-launch completeness
    /// check (unless `no_repair`), detects Java, resolves auth (offline
    /// fallback) and builds [`rmccc_launch::DefaultLauncher`] with a
    /// state-syncing [`ProcessListener`]. The process is registered in
    /// [`Self::running_games`] and its exit code is published to the exit
    /// signal channel so `launch status` / `launch wait` can observe it.
    pub async fn launch_start(&self, profile_id: &str, opts: &LaunchStartOptions) -> Result<u32> {
        // Guard: duplicate launch.
        if self.running_games.contains_key(profile_id) {
            return Err(RmcccError::Other(format!(
                "Profile '{profile_id}' is already running"
            )));
        }

        // Phase 1: profile + local resolved manifest.
        let profile = self.get_profile(profile_id).await?;
        let (_game_root, resolved, _http_client) = {
            let core = self.core.read().await;
            let repo = core
                .game_repository
                .read()
                .map_err(|_| RmcccError::Other("game repository lock poisoned".to_string()))?;
            let resolved = repo
                .get_resolved(&profile.game_version)
                .map_err(|e| RmcccError::Other(e.to_string()))?;
            (
                repo.game_dir().to_path_buf(),
                resolved,
                core.http_client.clone(),
            )
        };
        let game_version = resolved.launch;

        // Phase 2: pre-launch completeness check & auto-repair.
        if !opts.no_repair {
            let core = self.core.read().await;
            let repaired =
                pipeline::ensure_launch_ready(&core, &profile, opts.progress.as_ref()).await?;
            if repaired {
                tracing::info!(profile_id = %profile_id, "Pre-launch resources repaired");
            }
        }

        // Phase 3: detect Java.
        let required_java = game_version
            .java_version
            .as_ref()
            .map(|j| j.major_version)
            .unwrap_or_else(|| rmccc_version::java_version_for_mc(&profile.game_version));
        let java_install = match &profile.java_path {
            Some(jh) => {
                let home = PathBuf::from(jh);
                let bin = if home.is_file() {
                    home
                } else {
                    home.join("bin").join("java")
                };
                if !bin.exists() {
                    return Err(RmcccError::JavaNotFound);
                }
                rmccc_launch::JavaInstallation {
                    path: bin,
                    major_version: required_java,
                    vendor: String::new(),
                    is_64bit: true,
                }
            }
            None => rmccc_launch::find_java(required_java)?,
        };

        // Phase 4: resolve auth (offline fallback aligned with HMCL).
        let (username, uuid, access_token, user_type) = {
            let core = self.core.read().await;
            match core.log_in_active_account().await? {
                Some(info) => (info.username, info.uuid, info.access_token, info.user_type),
                None => {
                    let info = rmccc_auth::OfflineAccount::new("Player").play_offline();
                    (info.username, info.uuid, info.access_token, info.user_type)
                }
            }
        };
        let auth = rmccc_launch::AuthInfo::new(username, uuid, access_token, user_type);
        let options = rmccc_launch::LaunchOptions::from_profile(&profile, java_install.clone());

        // Phase 5: state-syncing process listener.
        let (exit_tx, exit_rx) = tokio::sync::watch::channel(None);
        self.exit_signals.insert(profile_id.to_string(), exit_rx);
        let listener: Arc<dyn rmccc_launch::ProcessListener> = Arc::new(CliProcessListener {
            profile_id: profile_id.to_string(),
            running_games: self.running_games.clone(),
            exit_tx,
            output: opts.output.clone(),
            on_exit: opts.on_exit.clone(),
        });

        let launcher = rmccc_launch::DefaultLauncher::new(
            profile.game_version.clone(),
            profile.game_dir.clone(),
            game_version,
            auth,
            options,
            Some(listener),
        );

        // Phase 6: launch (natives + log4j + spawn).
        let managed = launcher
            .launch()
            .await
            .map_err(|e| RmcccError::Other(e.to_string()))?;
        let pid = managed.pid();
        self.running_games.insert(profile_id.to_string(), pid);

        // Phase 7: update last_played.
        let core = self.core.read().await;
        core.profile_store.touch(profile_id)?;

        tracing::info!(profile_id = %profile_id, pid = pid, "Game launched");
        Ok(pid)
    }

    /// Block until the given profile's game exits; returns its exit code.
    /// Returns `None` when the profile is not running and has no recorded exit.
    pub async fn launch_wait(&self, profile_id: &str) -> Result<Option<i32>> {
        // Already exited and recorded?
        if !self.running_games.contains_key(profile_id) {
            if let Some(recv) = self.exit_signals.get(profile_id) {
                if let Some(code) = *recv.borrow() {
                    return Ok(Some(code));
                }
            }
            return Ok(None);
        }
        let recv = match self.exit_signals.get(profile_id) {
            Some(r) => r.clone(),
            None => return Ok(None),
        };
        let mut recv = recv;
        loop {
            if let Some(code) = *recv.borrow() {
                return Ok(Some(code));
            }
            if recv.changed().await.is_err() {
                return Ok(None);
            }
        }
    }

    /// Terminate a running game (whole process group on Unix).
    pub async fn launch_stop(&self, profile_id: &str) -> Result<()> {
        let pid = self
            .running_games
            .get(profile_id)
            .map(|g| *g)
            .ok_or_else(|| RmcccError::Other(format!("Profile '{profile_id}' is not running")))?;
        tracing::info!(profile_id = %profile_id, pid = pid, "Stopping game process");
        kill_process_group(pid);
        self.running_games.remove(profile_id);
        Ok(())
    }

    /// List running games.
    pub async fn launch_status(&self) -> Result<Vec<RunningGame>> {
        let mut out: Vec<RunningGame> = self
            .running_games
            .iter()
            .map(|g| RunningGame {
                profile_id: g.key().clone(),
                pid: *g.value(),
            })
            .collect();
        out.sort_by(|a, b| a.profile_id.cmp(&b.profile_id));
        Ok(out)
    }

    /// Detect Java installations on the system.
    pub async fn launch_java(&self) -> Result<Vec<rmccc_launch::JavaInstallation>> {
        Ok(rmccc_launch::detect_java_installations())
    }

    /// Dry-run: generate the exact command line a launch would use (no spawn).
    pub async fn launch_command(&self, profile_id: &str) -> Result<Value> {
        let profile = self.get_profile(profile_id).await?;
        let game_version = {
            let core = self.core.read().await;
            let repo = core
                .game_repository
                .read()
                .map_err(|_| RmcccError::Other("game repository lock poisoned".to_string()))?;
            repo.get_resolved(&profile.game_version)
                .map_err(|e| RmcccError::Other(e.to_string()))?
                .launch
        };
        let required_java = game_version
            .java_version
            .as_ref()
            .map(|j| j.major_version)
            .unwrap_or_else(|| rmccc_version::java_version_for_mc(&profile.game_version));
        let java_install = match &profile.java_path {
            Some(jh) => {
                let home = PathBuf::from(jh);
                let bin = if home.is_file() {
                    home
                } else {
                    home.join("bin").join("java")
                };
                rmccc_launch::JavaInstallation {
                    path: bin,
                    major_version: required_java,
                    vendor: String::new(),
                    is_64bit: true,
                }
            }
            None => rmccc_launch::find_java(required_java)?,
        };
        let (username, uuid, access_token, user_type) = {
            let core = self.core.read().await;
            match core.log_in_active_account().await? {
                Some(info) => (info.username, info.uuid, info.access_token, info.user_type),
                None => {
                    let info = rmccc_auth::OfflineAccount::new("Player").play_offline();
                    (info.username, info.uuid, info.access_token, info.user_type)
                }
            }
        };
        let auth = rmccc_launch::AuthInfo::new(username, uuid, access_token, user_type);
        let options = rmccc_launch::LaunchOptions::from_profile(&profile, java_install.clone());
        let launcher = rmccc_launch::DefaultLauncher::new(
            profile.game_version.clone(),
            profile.game_dir.clone(),
            game_version,
            auth,
            options,
            None,
        );
        let command = launcher
            .generate_command_line()
            .map_err(|e| RmcccError::Other(e.to_string()))?;
        Ok(json!({
            "profile_id": profile_id,
            "java": java_install.path.display().to_string(),
            "main_class": command.main_class,
            "jvm_args": command.jvm_args,
            "game_args": command.game_args,
            "native_dir": command.java_native_folder.display().to_string(),
        }))
    }

    // ────────────────────────── System ──────────────────────────

    /// Overall launcher status snapshot (for `system info`).
    pub async fn system_info(&self) -> Result<Value> {
        let core = self.core.read().await;
        let active_account = core.account_repo.load_active()?;
        let profiles = core.profile_store.list()?;
        let local_versions = {
            let repo = core
                .game_repository
                .read()
                .map_err(|_| RmcccError::Other("game repository lock poisoned".to_string()))?;
            repo.get_instance_ids().len()
        };
        let running = self.running_games.len();
        let java = rmccc_launch::detect_java_installations();

        Ok(json!({
            "data_dir": core.data_dir.display().to_string(),
            "game_root_dir": core.config.resolved_game_root().display().to_string(),
            "environment": std::env::var("RMCCC_ENV").unwrap_or_else(|_| "prod".to_string()),
            "active_account": active_account.map(|a| json!({
                "provider": a.provider,
                "username": a.username,
                "uuid": a.uuid,
                "expires_at": a.expires_at,
            })),
            "active_profile": core.launcher_state.selected_profile_id,
            "profiles": profiles.len(),
            "local_versions": local_versions,
            "running_games": running,
            "java_installations": java.len(),
            "version": env!("CARGO_PKG_VERSION"),
        }))
    }
}

// ────────────────────────── Helpers ──────────────────────────

/// Process listener that syncs CLI state (running games + exit signals).
struct CliProcessListener {
    profile_id: String,
    running_games: Arc<dashmap::DashMap<String, u32>>,
    exit_tx: tokio::sync::watch::Sender<Option<i32>>,
    output: Option<GameOutputCb>,
    on_exit: Option<CliExitCb>,
}

impl rmccc_launch::ProcessListener for CliProcessListener {
    fn on_log(&self, line: String, _is_stderr: bool) {
        if let Some(cb) = &self.output {
            cb(&line);
        }
    }
    fn on_exit(&self, exit_code: i32, exit_type: rmccc_launch::ExitType) {
        self.running_games.remove(&self.profile_id);
        let _ = self.exit_tx.send(Some(exit_code));
        if let Some(cb) = &self.on_exit {
            cb(exit_code, exit_type);
        }
    }
}

fn parse_bounded_u32(value: &str, min: u32, max: u32) -> Result<u32> {
    let v: u32 = value
        .parse()
        .map_err(|_| RmcccError::Other(format!("'{value}' is not a valid u32")))?;
    Ok(v.clamp(min, max))
}

fn parse_u64_field(value: &str) -> Result<u64> {
    value
        .parse()
        .map_err(|_| RmcccError::Other(format!("'{value}' is not a valid u64")))
}

fn parse_bool(value: &str) -> Result<bool> {
    match value.to_ascii_lowercase().as_str() {
        "true" | "1" | "yes" | "on" => Ok(true),
        "false" | "0" | "no" | "off" => Ok(false),
        other => Err(RmcccError::Other(format!(
            "'{other}' is not a valid boolean"
        ))),
    }
}

fn parse_optional_str(value: &str) -> Result<Option<String>> {
    if value.is_empty() {
        Ok(None)
    } else {
        Ok(Some(value.to_string()))
    }
}

/// Sanitize a filename: keep ASCII alphanumerics and `-_.`, replace the rest.
fn sanitize_filename(name: &str) -> String {
    name.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || "-_.".contains(c) {
                c
            } else {
                '_'
            }
        })
        .collect()
}

/// Reject path traversal / invalid characters in user-supplied filenames.
fn validate_mod_filename(name: &str) -> Result<()> {
    if name.is_empty()
        || name.contains('/')
        || name.contains('\\')
        || name.contains("..")
        || name.contains('\0')
    {
        return Err(RmcccError::Other(format!("Invalid mod filename: {name}")));
    }
    Ok(())
}

/// Verify a file's SHA1 against an expected hash.
fn verify_sha1(path: &Path, expected: &str) -> std::io::Result<bool> {
    use sha1::{Digest, Sha1};
    let data = std::fs::read(path)?;
    let mut hasher = Sha1::new();
    hasher.update(&data);
    let actual = format!("{:x}", hasher.finalize());
    Ok(actual.to_lowercase() == expected.to_lowercase())
}

/// Convert a Maven coordinate into a library file path.
fn make_library_path(name: &str, libs_dir: &Path) -> PathBuf {
    let parts: Vec<&str> = name.split(':').collect();
    if parts.len() < 3 {
        return libs_dir.join(name);
    }
    let group = parts[0].replace('.', "/");
    let artifact = parts[1];
    let version = parts[2];
    libs_dir.join(format!(
        "{}/{}/{}/{}-{}.jar",
        group, artifact, version, artifact, version
    ))
}

/// Top-level entry names of a directory.
fn walk_dir_top(dir: &Path) -> Vec<String> {
    let mut names = Vec::new();
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            names.push(entry.file_name().to_string_lossy().to_string());
        }
    }
    names
}

/// Kill a process group (whole Java tree) on Unix; taskkill on Windows.
pub fn kill_process_group(pid: u32) {
    #[cfg(unix)]
    unsafe {
        // SAFETY: kill(-pid, SIGKILL) signals the process group created by
        // spawn_game's process_group(0); terminates Java and its children.
        libc::kill(-(pid as libc::pid_t), libc::SIGKILL);
    }
    #[cfg(windows)]
    {
        let _ = std::process::Command::new("taskkill")
            .args(["/PID", &pid.to_string(), "/F"])
            .output();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "rmccc-cli-test-{tag}-{}-{}",
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

    #[tokio::test]
    async fn profile_crud_roundtrip() {
        let dir = temp_dir("profile");
        let ctx = CliContext::new(&dir).unwrap();

        let created = ctx
            .profile_create("My Instance", "1.21", Some("fabric"), Some(2048))
            .await
            .unwrap();
        assert_eq!(created.name, "My Instance");
        assert_eq!(created.memory_mb, 2048);
        assert!(created.mod_loader.is_some());

        let listed = ctx.profile_list().await.unwrap();
        assert_eq!(listed.len(), 1);

        let updated = ctx
            .profile_update(
                &created.id,
                Some("Renamed"),
                None,
                None,
                None,
                None,
                Some(4096),
                None,
            )
            .await
            .unwrap();
        assert_eq!(updated.name, "Renamed");
        assert_eq!(updated.memory_mb, 4096);

        ctx.profile_delete(&created.id, false).await.unwrap();
        assert!(ctx.profile_list().await.unwrap().is_empty());

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn profile_create_validates_name() {
        let dir = temp_dir("validate");
        let ctx = CliContext::new(&dir).unwrap();
        assert!(ctx.profile_create("", "1.21", None, None).await.is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn config_set_get_roundtrip() {
        let dir = temp_dir("config");
        let ctx = CliContext::new(&dir).unwrap();
        ctx.config_set("default_max_memory_mb", "8192")
            .await
            .unwrap();
        let v = ctx.config_get(Some("default_max_memory_mb")).await.unwrap();
        assert_eq!(v, json!(8192));
        assert!(ctx.config_set("no_such_field", "1").await.is_err());
        assert!(ctx
            .config_set("default_max_memory_mb", "abc")
            .await
            .is_err());

        let paths = ctx.config_paths().await.unwrap();
        assert_eq!(
            paths["game_root_dir"],
            dir.join("instances").display().to_string()
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn auth_offline_login_and_logout() {
        let dir = temp_dir("auth");
        let ctx = CliContext::new(&dir).unwrap();
        let session = ctx.auth_login_offline("Steve").await.unwrap();
        assert_eq!(session.username, "Steve");

        let status = ctx.auth_status().await.unwrap();
        assert_eq!(status.as_ref().map(|s| s.username.as_str()), Some("Steve"));

        let list = ctx.auth_list().await.unwrap();
        assert_eq!(list.len(), 1);

        ctx.auth_logout().await.unwrap();
        assert!(ctx.auth_status().await.unwrap().is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn state_and_user_settings() {
        let dir = temp_dir("state");
        let ctx = CliContext::new(&dir).unwrap();
        let profile = ctx.profile_create("p", "1.21", None, None).await.unwrap();
        ctx.profile_select(&profile.id).await.unwrap();
        let state = ctx.state_get(None).await.unwrap();
        assert_eq!(state["selected_profile_id"], profile.id);

        ctx.user_set("locale", "zh-CN").await.unwrap();
        let user = ctx.user_get(Some("locale")).await.unwrap();
        assert_eq!(user, "zh-CN");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn filename_validation_blocks_traversal() {
        assert!(validate_mod_filename("../evil.jar").is_err());
        assert!(validate_mod_filename("a/b.jar").is_err());
        assert!(validate_mod_filename("ok-mod-1.0.jar").is_ok());
        assert!(validate_mod_filename("").is_err());
    }
}
