//! Mod loader 域：版本列表、安装（编排在 core）、移除、安装器分析。
//!
//! `impl CliContext` 的一个分片（P2-1：把 1,450 行的 God class 按域拆开）。
//! 方法名 / 签名与拆分前完全一致，命令层无需改动。

use super::*;

impl CliContext {
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
        key: &str,
        loader_type: &rmccc_modloader::ModLoaderType,
        loader_version: &str,
        progress: Option<rmccc_modloader::ProgressCallback>,
    ) -> Result<String> {
        let instance = self.get_instance(key).await?;

        // 统一入口（P1-4）：Java 探测 / 基础版本解析 / 安装分流 / 增量 JSON
        // 写入 / profile 提交 / 索引刷新全在 `rmccc_core::modloader` 一处完成。
        let core = self.core.read().await;
        let installed = rmccc_core::modloader::install_loader(
            &core.handle(),
            &instance,
            loader_type,
            loader_version,
            None,
        )
        .await?;

        tracing::info!(
            key = %key,
            loader = %loader_type.library_id(),
            version_id = %installed.version_id,
            via_installer_jar = installed.via_installer_jar,
            "Mod loader installed"
        );

        let _ = progress;
        Ok(installed.version_id)
    }

    /// Remove the installed loader by deleting its version-backed instance and
    /// selecting the base Minecraft instance (the incremental model keeps the
    /// vanilla manifest intact under the base id).
    pub async fn modloader_remove(&self, key: &str) -> Result<rmccc_instance::InstanceDetail> {
        let instance = self.get_instance(key).await?;
        if instance.summary.loader.is_none() {
            return Err(RmcccError::Other(format!(
                "Instance '{key}' has no modloader installed"
            )));
        }
        let directory_id = instance.summary.directory_id.clone();
        let base_key =
            rmccc_instance::InstanceService::key(&directory_id, &instance.summary.game_version)?;

        let core = self.core.read().await;
        let mut service = core
            .instances
            .lock()
            .map_err(|_| RmcccError::Other("instance service lock poisoned".to_string()))?;
        service.remove(key, true)?;
        tracing::info!(key = %key, base = %base_key, "Modloader instance removed");
        service.get(&base_key)
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
}
