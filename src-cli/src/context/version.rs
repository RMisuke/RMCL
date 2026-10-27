//! 版本域：远程版本列表、本地仓库索引、单版本元数据。
//!
//! `impl CliContext` 的一个分片（P2-1：把 1,450 行的 God class 按域拆开）。
//! 方法名 / 签名与拆分前完全一致，命令层无需改动。

use super::*;

impl CliContext {
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
        key: &str,
        version: &str,
        progress: Option<&ProgressCb>,
    ) -> Result<()> {
        let instance = self.get_instance(key).await?;
        let core = self.core.read().await;
        let handle = core.handle();
        let report =
            pipeline::install_game(&handle, &instance.paths.game_dir, version, progress).await?;
        if !report.is_complete() {
            return Err(RmcccError::DownloadFailed {
                url: version.to_string(),
                reason: format!(
                    "{} of {} files failed to download",
                    report.failed_files, report.total_files
                ),
            });
        }
        // 提交（创建实例设置 + 固定隔离 + 刷新索引）与 GUI 共用同一路径。
        pipeline::commit_install(&handle, &instance.summary.directory_id, version)?;
        tracing::info!(key = %key, version = %version, "Version installed");
        Ok(())
    }

    /// List locally installed instances in the selected directory.
    pub async fn version_local_list(&self) -> Result<Vec<Value>> {
        let core = self.core.read().await;
        let mut service = core
            .instances
            .lock()
            .map_err(|_| RmcccError::Other("instance service lock poisoned".to_string()))?;
        let summaries = service.list(None, false, true)?;
        Ok(summaries
            .into_iter()
            .map(|item| {
                json!({
                    "key": item.key,
                    "id": item.id,
                    "base_version": item.game_version,
                    "loader": item.loader,
                    "inherits_from": null,
                    "library_count": null,
                })
            })
            .collect())
    }

    pub async fn version_local_get(&self, id: &str) -> Result<rmccc_version::GameVersion> {
        let core = self.core.read().await;
        let mut service = core
            .instances
            .lock()
            .map_err(|_| RmcccError::Other("instance service lock poisoned".to_string()))?;
        let directory = service.selected_directory().id.clone();
        let key = rmccc_instance::InstanceService::key(directory.as_str(), id)?;
        let (_, resolved) = service.resolved_version(&key)?;
        Ok(resolved.standalone)
    }

    pub async fn version_local_delete(&self, id: &str) -> Result<()> {
        let core = self.core.read().await;
        let mut service = core
            .instances
            .lock()
            .map_err(|_| RmcccError::Other("instance service lock poisoned".to_string()))?;
        let directory = service.selected_directory().id.clone();
        let key = rmccc_instance::InstanceService::key(directory.as_str(), id)?;
        service.remove(&key, true)
    }
}
