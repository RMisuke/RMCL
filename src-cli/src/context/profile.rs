//! Instance domain: list / inspect / create-from-installed-version / update /
//! select / remove. IDs are `{directory_id}/{instance_id}` keys (ADR-016).

use super::*;

impl CliContext {
    pub async fn profile_list(&self) -> Result<Vec<rmccc_instance::InstanceSummary>> {
        let core = self.core.read().await;
        let mut service = core
            .instances
            .lock()
            .map_err(|_| RmcccError::Other("instance service lock poisoned".to_string()))?;
        service.list(None, false, false)
    }

    /// Create an instance from a version that is already installed.
    ///
    /// Installation itself is `resource download`/`instance install`; creating
    /// metadata for a missing version would produce an unlaunchable ghost.
    pub async fn profile_create(
        &self,
        name: &str,
        game_version: &str,
        _loader: Option<&str>,
        memory_mb: Option<u32>,
    ) -> Result<rmccc_instance::InstanceDetail> {
        let name = name.trim();
        if name.is_empty() {
            return Err(RmcccError::Other(
                "Instance name cannot be empty".to_string(),
            ));
        }
        let directory_id = {
            let core = self.core.read().await;
            let service = core
                .instances
                .lock()
                .map_err(|_| RmcccError::Other("instance service lock poisoned".to_string()))?;
            service.selected_directory().id.to_string()
        };

        let detail = {
            let core = self.core.read().await;
            let mut service = core
                .instances
                .lock()
                .map_err(|_| RmcccError::Other("instance service lock poisoned".to_string()))?;
            service.ensure_instance(&directory_id, game_version)?
        };

        let mut patch = rmccc_instance::InstanceSettingsPatch {
            display_name: Some(name.to_string()),
            ..Default::default()
        };
        if let Some(memory) = memory_mb {
            patch.max_memory_mb = Some(memory);
            patch.override_properties =
                Some(vec![rmccc_instance::fields::MAX_MEMORY_MB.to_string()]);
        }
        let core = self.core.read().await;
        let mut service = core
            .instances
            .lock()
            .map_err(|_| RmcccError::Other("instance service lock poisoned".to_string()))?;
        service.update_settings(&detail.summary.key, patch)
    }

    /// Update selected instance fields; `None` means "leave unchanged".
    #[allow(clippy::too_many_arguments)]
    pub async fn profile_update(
        &self,
        key: &str,
        name: Option<&str>,
        game_version: Option<&str>,
        java_path: Option<&str>,
        jvm_args: Option<Vec<String>>,
        game_args: Option<Vec<String>>,
        memory_mb: Option<u32>,
        icon: Option<&str>,
    ) -> Result<rmccc_instance::InstanceDetail> {
        if let Some(version) = game_version {
            // The instance id is the version id; changing it is a rename, not a
            // settings edit. Keep the CLI honest instead of silently ignoring.
            let current = key.rsplit('/').next().unwrap_or(key);
            if version != current {
                return Err(RmcccError::Validation(
                    "changing game_version is a rename; use `profile` with the new id".to_string(),
                ));
            }
        }

        let mut patch = rmccc_instance::InstanceSettingsPatch::default();
        if let Some(name) = name {
            patch.display_name = Some(name.to_string());
        }
        if let Some(path) = java_path {
            patch.java_type = Some(if path.is_empty() {
                rmccc_instance::JavaType::Auto
            } else {
                rmccc_instance::JavaType::Custom
            });
            patch.custom_java_path = Some(path.to_string());
        }
        if let Some(args) = jvm_args {
            patch.jvm_args = Some(args);
        }
        if let Some(args) = game_args {
            patch.game_args = Some(args);
        }
        if let Some(memory) = memory_mb {
            patch.max_memory_mb = Some(memory);
        }
        if let Some(icon) = icon {
            patch.icon = Some(if icon.is_empty() {
                rmccc_instance::InstanceIcon::Auto
            } else if icon.starts_with("data:") {
                rmccc_instance::InstanceIcon::DataUrl(icon.to_string())
            } else {
                rmccc_instance::InstanceIcon::File(PathBuf::from(icon))
            });
        }

        let core = self.core.read().await;
        let mut service = core
            .instances
            .lock()
            .map_err(|_| RmcccError::Other("instance service lock poisoned".to_string()))?;
        service.update_settings(key, patch)
    }

    pub async fn profile_delete(&self, key: &str, cleanup: bool) -> Result<()> {
        let core = self.core.read().await;
        let mut service = core
            .instances
            .lock()
            .map_err(|_| RmcccError::Other("instance service lock poisoned".to_string()))?;
        service.remove(key, cleanup)
    }

    pub async fn profile_select(&self, key: &str) -> Result<()> {
        let core = self.core.read().await;
        let mut service = core
            .instances
            .lock()
            .map_err(|_| RmcccError::Other("instance service lock poisoned".to_string()))?;
        service.select_instance(Some(key))
    }

    pub async fn profile_active(&self) -> Result<Option<rmccc_instance::InstanceDetail>> {
        let key = {
            let core = self.core.read().await;
            let service = core
                .instances
                .lock()
                .map_err(|_| RmcccError::Other("instance service lock poisoned".to_string()))?;
            service.selected_instance().map(str::to_string)
        };
        match key {
            Some(key) => self.get_instance(&key).await.map(Some),
            None => Ok(None),
        }
    }
}
