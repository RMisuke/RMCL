//! Version IPC commands.

use crate::state::AppState;
use rmccc_error::IpcError;
use rmccc_version::VersionInfo;
use std::sync::Arc;
use tauri::Emitter;

#[tauri::command]
#[tracing::instrument(skip(state))]
pub async fn version_list(state: tauri::State<'_, AppState>) -> Result<Vec<VersionInfo>, IpcError> {
    // P2-01：走 DI 注入的 version_provider（fetch 期间持有读锁）
    let versions = {
        let core = state.core.read().await;
        match core.version_provider.fetch_versions().await {
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

    // Write cache in background
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

#[tauri::command]
#[tracing::instrument(skip(state), fields(request_id = %uuid::Uuid::new_v4()))]
pub async fn version_install(
    state: tauri::State<'_, AppState>,
    version: String,
    profile_id: String,
) -> Result<(), IpcError> {
    // 预检查：profile 必须存在（快速失败，避免产生无效的后台任务）
    {
        let core = state.core.read().await;
        if core.profile_store.get(&profile_id)?.is_none() {
            return Err(IpcError {
                code: rmccc_error::ErrorCode::ProfileNotFound,
                message: format!("Profile not found: {profile_id}"),
                retryable: false,
            });
        }
    }

    // 后台执行整个安装流程（下载 + 保存元数据 + 更新 profile），命令立即返回。
    // 进度经 download://progress（携带真实 batch_id），完成后经 download://complete，
    // 失败经 download://error 通知前端（P0-04）。
    let core = state.core.clone();
    let app_handle = state.app_handle.clone();
    tokio::spawn(async move {
        if let Err(e) = run_install_background(
            core,
            app_handle.clone(),
            version.clone(),
            profile_id.clone(),
        )
        .await
        {
            tracing::error!(
                profile_id = %profile_id,
                version = %version,
                "Background version install failed: {e:?}"
            );
            // 失败事件：batch_id 未知时用空串占位，前端可凭 profile_id 关联
            let _ = app_handle.emit(
                crate::events::DOWNLOAD_ERROR,
                serde_json::json!({
                    "batch_id": "",
                    "profile_id": profile_id,
                    "version": version,
                    "error": e.message,
                }),
            );
        }
    });

    Ok(())
}

/// 后台安装流程：一次 `pipeline::install_game` 调用完成全部编排
/// （版本 JSON 落盘、client JAR + 依赖库 + 资源下载、natives 解压），
/// 成功后经 `pipeline::commit_install` 提交 profile 并刷新索引。
///
/// P1-3：此前这里复刻了一份与 pipeline 平行的安装实现（且忘了解压 natives、
/// 且用几秒前的 profile 快照覆盖用户设置），现已全部删除。
async fn run_install_background(
    core: Arc<tokio::sync::RwLock<rmccc_core::CoreContext>>,
    app_handle: tauri::AppHandle,
    version: String,
    profile_id: String,
) -> Result<(), IpcError> {
    // ── 1. Load profile + 事件适配器（短锁）──
    // 完整/失败事件需要真实 batch_id；它只出现在进度回调里，这里记录最后一次值。
    let last_batch_id = Arc::new(std::sync::Mutex::new(None::<String>));
    let (profile, progress_cb) = {
        let core = core.read().await;
        let profile = core
            .profile_store
            .get(&profile_id)?
            .ok_or_else(|| IpcError {
                code: rmccc_error::ErrorCode::ProfileNotFound,
                message: format!("Profile not found: {profile_id}"),
                retryable: false,
            })?;
        let emitter = Arc::new(crate::events::ThrottledEmitter::new(
            app_handle.clone(),
            core.config.event_throttle_ms,
        ));
        let log_pid = profile_id.clone();
        let cb_last_batch = last_batch_id.clone();
        let progress_cb: rmccc_core::pipeline::ProgressCb =
            Arc::new(move |msg: &rmccc_core::pipeline::ProgressMsg| match msg {
                rmccc_core::pipeline::ProgressMsg::Download { progress, .. } => {
                    *cb_last_batch.lock().unwrap() = Some(progress.batch_id.clone());
                    emitter.emit_progress(&crate::events::DownloadProgressPayload {
                        batch_id: progress.batch_id.clone(),
                        completed_files: progress.completed_files,
                        total_files: progress.total_files,
                        downloaded_bytes: progress.downloaded_bytes,
                        total_bytes: progress.total_bytes,
                        current_file: progress.current_file.clone(),
                    });
                }
                rmccc_core::pipeline::ProgressMsg::Stage(stage)
                | rmccc_core::pipeline::ProgressMsg::StageDone(stage) => {
                    tracing::debug!(profile_id = %log_pid, "{stage}");
                }
            });
        (profile, progress_cb)
    };

    // ── 2. Install（编排全部在 pipeline 内，长锁覆盖整个下载过程）──
    let report = {
        let core = core.read().await;
        rmccc_core::pipeline::install_game(&core, &profile, &version, Some(&progress_cb)).await?
    };

    // ── 3. 有失败条目：不提交 profile，上报错误 ──
    let real_batch_id = last_batch_id.lock().unwrap().clone().unwrap_or_default();
    if !report.is_complete() {
        tracing::warn!(
            profile_id = %profile_id,
            version = %version,
            failed_files = report.failed_files,
            "Version install download had failures"
        );
        let _ = app_handle.emit(
            crate::events::DOWNLOAD_ERROR,
            serde_json::json!({
                "batch_id": real_batch_id,
                "profile_id": profile_id,
                "version": version,
                "failed_files": report.failed_files,
            }),
        );
        return Ok(());
    }

    // ── 4. 提交：profile 指向新版本 + 刷新本地索引 ──
    {
        let core = core.read().await;
        rmccc_core::pipeline::commit_install(&core, &profile_id, &version)?;
    }

    // ── 5. Emit completion event（前端据此刷新 UI）──
    let _ = app_handle.emit(
        crate::events::DOWNLOAD_COMPLETE,
        serde_json::json!({
            "batch_id": real_batch_id,
            "profile_id": profile_id,
            "version": version,
        }),
    );
    // 实例索引已更新（HMCL RefreshedGameInstancesEvent 等价物）。
    let _ = app_handle.emit(
        crate::events::INSTANCES_REFRESHED,
        serde_json::json!({ "version": version }),
    );

    tracing::info!(profile_id = %profile_id, version = %version, "Version install complete");
    Ok(())
}
