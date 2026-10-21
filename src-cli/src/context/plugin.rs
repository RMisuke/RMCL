//! Plugin manager facade (Phase 2) — one method per `plugin` subcommand.

use super::*;
use rmccc_plugin::ManifestContext;
use std::path::Path;

impl CliContext {
    pub async fn plugin_list(&self, all: bool) -> Result<Value> {
        let core = self.core.read().await;
        let plugins = core.plugin_manager.list(all);
        let diagnostics = core.plugin_manager.diagnostics();
        Ok(json!({
            "plugins": plugins,
            "count": plugins.len(),
            "diagnostics": diagnostics,
        }))
    }

    pub async fn plugin_inspect(&self, id: &str) -> Result<Value> {
        let mut core = self.core.write().await;
        let detail = core.plugin_manager.inspect(id)?;
        Ok(serde_json::to_value(detail)?)
    }

    pub async fn plugin_install(&self, path: &str, enable: bool) -> Result<Value> {
        let mut core = self.core.write().await;
        let report = core
            .plugin_manager
            .install_from_path_with(Path::new(path), enable)?;
        Ok(serde_json::to_value(report)?)
    }

    pub async fn plugin_enable(&self, id: &str) -> Result<Value> {
        let mut core = self.core.write().await;
        Ok(serde_json::to_value(core.plugin_manager.enable(id)?)?)
    }

    pub async fn plugin_disable(&self, id: &str) -> Result<Value> {
        let mut core = self.core.write().await;
        Ok(serde_json::to_value(core.plugin_manager.disable(id)?)?)
    }

    pub async fn plugin_remove(&self, id: &str, delete_data: bool) -> Result<Value> {
        let mut core = self.core.write().await;
        core.plugin_manager.uninstall(id, delete_data)?;
        Ok(json!({ "id": id, "removed": true, "data_deleted": delete_data }))
    }

    pub async fn plugin_rollback(&self, id: &str) -> Result<Value> {
        let mut core = self.core.write().await;
        Ok(serde_json::to_value(core.plugin_manager.rollback(id)?)?)
    }

    pub async fn plugin_grant(&self, id: &str, capability: &str) -> Result<Value> {
        let mut core = self.core.write().await;
        Ok(serde_json::to_value(
            core.plugin_manager.grant(id, capability)?,
        )?)
    }

    pub async fn plugin_revoke(&self, id: &str, capability: &str) -> Result<Value> {
        let mut core = self.core.write().await;
        Ok(serde_json::to_value(
            core.plugin_manager.revoke(id, capability)?,
        )?)
    }

    pub async fn plugin_config_get(&self, id: &str) -> Result<Value> {
        let mut core = self.core.write().await;
        let (config, schema) = core.plugin_manager.config_get(id)?;
        Ok(json!({ "config": config, "schema": schema }))
    }

    pub async fn plugin_config_set(&self, id: &str, values: &str) -> Result<Value> {
        let values: Value = serde_json::from_str(values)
            .map_err(|e| RmcccError::Validation(format!("values 必须是合法 JSON：{e}")))?;
        let mut core = self.core.write().await;
        let config = core.plugin_manager.config_set(id, values)?;
        Ok(serde_json::to_value(config)?)
    }

    pub async fn plugin_config_reset(&self, id: &str) -> Result<Value> {
        let mut core = self.core.write().await;
        Ok(serde_json::to_value(core.plugin_manager.config_reset(id)?)?)
    }

    pub async fn plugin_config_path(&self, id: &str) -> Result<Value> {
        let paths = rmccc_plugin::PluginPaths::new(&self.data_dir);
        Ok(json!({
            "id": id,
            "path": paths.config_file(id).display().to_string(),
        }))
    }

    pub async fn plugin_capabilities(&self) -> Result<Value> {
        let core = self.core.read().await;
        let capabilities = core.plugin_manager.capability_catalog();
        Ok(json!({ "capabilities": capabilities, "count": capabilities.len() }))
    }

    /// 把 examples/plugins 目录打包成 `.rmcpkg`（开发工具）。
    pub async fn plugin_pack(&self, dir: &str, out: &str) -> Result<Value> {
        let core = self.core.read().await;
        // 用管理器自己的 core 版本（rmccc-core 的版本），不要用 CLI crate 版本：
        // 两者不同步时，打包会生成"自己都装不上"的包。
        let ctx = ManifestContext::new(
            &core.plugin_manager.core_version().to_string(),
            core.plugin_manager.platform_id(),
        )?;
        rmccc_plugin::package::pack_directory(Path::new(dir), Path::new(out), &ctx)?;
        let size = std::fs::metadata(out).map(|m| m.len()).unwrap_or(0);
        Ok(json!({ "dir": dir, "out": out, "bytes": size }))
    }
}
