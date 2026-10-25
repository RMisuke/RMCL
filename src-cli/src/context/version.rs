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
        profile_id: &str,
        version: &str,
        progress: Option<&ProgressCb>,
    ) -> Result<()> {
        let profile = self.get_profile(profile_id).await?;
        let core = self.core.read().await;
        let handle = core.handle();
        let report = pipeline::install_game(&handle, &profile, version, progress).await?;
        if !report.is_complete() {
            return Err(RmcccError::DownloadFailed {
                url: version.to_string(),
                reason: format!(
                    "{} of {} files failed to download",
                    report.failed_files, report.total_files
                ),
            });
        }
        // 提交（写 game_version + 刷新索引）与 GUI 共用同一路径。
        pipeline::commit_install(&handle, profile_id, version)?;
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
}
