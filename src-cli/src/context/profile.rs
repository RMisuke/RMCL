//! 实例（profile）域：增删改查、默认实例、游戏文件清理。
//!
//! `impl CliContext` 的一个分片（P2-1：把 1,450 行的 God class 按域拆开）。
//! 方法名 / 签名与拆分前完全一致，命令层无需改动。

use super::*;

impl CliContext {
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
}
