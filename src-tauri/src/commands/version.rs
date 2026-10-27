//! Version catalogue IPC commands.
//!
//! Installation lives in `instance_install` (it creates a version-backed
//! instance). This module only exposes the remote version list.

use crate::state::AppState;
use rmccc_error::IpcError;
use rmccc_version::VersionInfo;

#[tauri::command]
#[tracing::instrument(skip(state))]
pub async fn version_list(state: tauri::State<'_, AppState>) -> Result<Vec<VersionInfo>, IpcError> {
    // P2-01：走 DI 注入的 version_provider。A2：短锁克隆 Arc，网络请求
    // 期间不持 core 读锁（否则 settings_set 等写命令要等清单拉完）。
    let provider = {
        let core = state.core.read().await;
        core.version_provider.clone()
    };
    let versions = {
        match provider.fetch_versions().await {
            Ok(versions) => versions,
            Err(e) if e.is_retryable() => {
                tracing::warn!("Network error fetching versions, trying cache: {e}");
                let cache_dir = rmccc_config::paths::cache_dir();
                let cache = rmccc_version::cache::VersionCache::new(&cache_dir, 3600);
                cache.read()?.unwrap_or_default()
            }
            Err(e) => return Err(e.into()),
        }
    };

    // Write cache in background.
    let versions_for_cache = versions.clone();
    let cache_dir = rmccc_config::paths::cache_dir();
    tokio::spawn(async move {
        let cache = rmccc_version::cache::VersionCache::new(&cache_dir, 3600);
        if let Err(e) = cache.write(&versions_for_cache) {
            tracing::warn!("Failed to update version cache: {e}");
        }
    });

    Ok(versions)
}
