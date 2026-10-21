//! 插件目录布局（ADR-003 决策 3）。所有路径都从 `data_dir` 推导，不依赖 CWD。

use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct PluginPaths {
    pub data_dir: PathBuf,
    pub plugins_root: PathBuf,
    pub data_root: PathBuf,
    pub state_root: PathBuf,
    pub staging_root: PathBuf,
    pub registry_file: PathBuf,
    pub permissions_file: PathBuf,
    pub audit_file: PathBuf,
}

impl PluginPaths {
    pub fn new(data_dir: &Path) -> Self {
        Self {
            data_dir: data_dir.to_path_buf(),
            plugins_root: rmccc_config::paths::plugins_dir(data_dir),
            data_root: rmccc_config::paths::plugin_data_dir(data_dir),
            state_root: rmccc_config::paths::plugin_state_dir(data_dir),
            staging_root: rmccc_config::paths::plugin_staging_dir(data_dir),
            registry_file: rmccc_config::paths::plugin_registry_path(data_dir),
            permissions_file: rmccc_config::paths::plugin_permissions_path(data_dir),
            audit_file: rmccc_config::paths::plugin_audit_path(data_dir),
        }
    }

    /// `plugins/<id>/`
    pub fn plugin_dir(&self, plugin_id: &str) -> PathBuf {
        self.plugins_root.join(plugin_id)
    }

    /// `plugins/<id>/<version>/`
    pub fn version_dir(&self, plugin_id: &str, version: &str) -> PathBuf {
        self.plugin_dir(plugin_id).join(version)
    }

    /// `plugin-data/<id>/`
    pub fn config_dir(&self, plugin_id: &str) -> PathBuf {
        self.data_root.join(plugin_id)
    }

    /// `plugin-data/<id>/config.json`
    pub fn config_file(&self, plugin_id: &str) -> PathBuf {
        self.config_dir(plugin_id).join("config.json")
    }

    pub fn ensure_base_dirs(&self) -> std::io::Result<()> {
        std::fs::create_dir_all(&self.plugins_root)?;
        std::fs::create_dir_all(&self.data_root)?;
        std::fs::create_dir_all(&self.state_root)?;
        std::fs::create_dir_all(&self.staging_root)?;
        Ok(())
    }
}
