//! Version IPC commands.

use crate::state::AppState;
use rmccc_download::provider::DownloadProvider;
use rmccc_download::{DownloadItem, DownloadProgress};
use rmccc_error::IpcError;
use rmccc_version::GameRepository as _;
use rmccc_version::VersionInfo;
use std::sync::Arc;
use tauri::Emitter;

/// 将 `DownloadItem` 的 URL 经 `download_provider` 注入镜像候选（P1-05）。
/// 与 CLI 一致：注入后的 `candidates[0]` 作主 URL，其余作回退候选。
/// Mojang 源下 `inject_url_with_candidates` 返回原 URL，无副作用。
fn inject_mirror(provider: &dyn DownloadProvider, mut item: DownloadItem) -> DownloadItem {
    let candidates = provider.inject_url_with_candidates(&item.url);
    item.url = candidates[0].clone();
    item.candidates = candidates[1..].to_vec();
    item
}

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
    let http_client = state.http_client.clone();
    let app_handle = state.app_handle.clone();
    tokio::spawn(async move {
        if let Err(e) = run_install_background(
            core,
            http_client,
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

/// 后台安装流程：拉取版本元数据 → 构建下载项 → 下载（真实 batch_id 进度）→
/// 等待完成 → 保存版本 JSON + 更新 profile → 发射完成/失败事件。
async fn run_install_background(
    core: Arc<tokio::sync::RwLock<rmccc_core::CoreContext>>,
    http_client: reqwest::Client,
    app_handle: tauri::AppHandle,
    version: String,
    profile_id: String,
) -> Result<(), IpcError> {
    // ── 1. Load profile (short lock) ──
    let (mut profile, game_dir) = {
        let core = core.read().await;
        let profile = core
            .profile_store
            .get(&profile_id)?
            .ok_or_else(|| IpcError {
                code: rmccc_error::ErrorCode::ProfileNotFound,
                message: format!("Profile not found: {profile_id}"),
                retryable: false,
            })?;
        let game_dir = profile.game_dir.clone();
        (profile, game_dir)
    };

    // ── 2. Fetch version metadata (P2-01：走 DI 注入的 version_provider) ──
    let game_version = {
        let core = core.read().await;
        core.version_provider.fetch_version(&version).await?
    };

    // ── 3. Prepare directories ──
    let versions_dir = game_dir.join("versions").join(&version);
    std::fs::create_dir_all(&versions_dir)?;

    // ── 4. Build download items ──
    let client_entry = game_version
        .downloads
        .get("client")
        .ok_or_else(|| IpcError {
            code: rmccc_error::ErrorCode::DownloadFailed,
            message: "Client JAR not found in version metadata".into(),
            retryable: false,
        })?;

    let client_item = DownloadItem {
        url: client_entry.url.clone(),
        dest: versions_dir.join(format!("{}.jar", version)),
        expected_sha1: Some(client_entry.sha1.clone()),
        size: Some(client_entry.size),
        use_cache: true,
        candidates: Vec::new(),
    };

    let lib_items = rmccc_version::libraries::resolve_library_items(
        &game_version.libraries,
        &game_dir.join("libraries"),
    );

    let asset_items = rmccc_version::assets::resolve_asset_items(
        &http_client,
        &game_version.asset_index,
        &game_dir.join("assets"),
    )
    .await?;

    let total_size: u64 = std::iter::once(&client_item)
        .chain(lib_items.iter())
        .chain(asset_items.iter())
        .filter_map(|i| i.size)
        .sum();
    let required = (total_size as f64 * 1.2) as u64;
    rmccc_download::check_disk_space(&game_dir, required)?;

    tracing::info!(
        profile_id = %profile_id,
        version = %version,
        total_items = 1 + lib_items.len() + asset_items.len(),
        total_size_mb = total_size / 1024 / 1024,
        "Starting version install"
    );

    // ── 5. Download with throttled progress events (真实 batch_id, P0-04) ──
    let final_failed = Arc::new(std::sync::Mutex::new(None::<u64>));
    let handle = {
        let core = core.read().await;
        let emitter = Arc::new(crate::events::ThrottledEmitter::new(
            app_handle.clone(),
            core.config.event_throttle_ms,
        ));
        let final_failed_cb = final_failed.clone();

        // 镜像注入（P1-05）：client JAR + libraries + assets 统一注入候选。
        let mut all_items = Vec::with_capacity(1 + lib_items.len() + asset_items.len());
        all_items.push(inject_mirror(core.download_provider.as_ref(), client_item));
        all_items.extend(
            lib_items
                .into_iter()
                .map(|it| inject_mirror(core.download_provider.as_ref(), it)),
        );
        all_items.extend(
            asset_items
                .into_iter()
                .map(|it| inject_mirror(core.download_provider.as_ref(), it)),
        );

        core.download_manager
            .download_batch(
                all_items,
                Arc::new(move |p: DownloadProgress| {
                    emitter.emit_progress(&crate::events::DownloadProgressPayload {
                        batch_id: p.batch_id.clone(),
                        completed_files: p.completed_files,
                        total_files: p.total_files,
                        downloaded_bytes: p.downloaded_bytes,
                        total_bytes: p.total_bytes,
                        current_file: p.current_file.clone(),
                    });
                    // 捕获最终失败数（current_file == None 表示最终回调）
                    // 容忍 poisoned mutex（P1-07：禁 unwrap_used）
                    if p.current_file.is_none() {
                        if let Ok(mut guard) = final_failed_cb.lock() {
                            *guard = Some(p.failed_files);
                        }
                    }
                }),
            )
            .await?
    };
    let batch_id = handle.id.clone();

    // ── 6. Wait for batch completion ──
    {
        let core = core.read().await;
        core.download_manager.wait_for_batch(&batch_id).await;
    }

    // ── 7. 失败检测：有失败文件则发射错误并终止（不更新 profile）──
    let failed = match final_failed.lock() {
        Ok(guard) => guard.unwrap_or(0),
        Err(poisoned) => poisoned.into_inner().unwrap_or(0),
    };
    if failed > 0 {
        tracing::warn!(batch_id = %batch_id, failed_files = failed, "Version install download had failures");
        let _ = app_handle.emit(
            crate::events::DOWNLOAD_ERROR,
            serde_json::json!({
                "batch_id": batch_id,
                "profile_id": profile_id,
                "version": version,
                "failed_files": failed,
            }),
        );
        return Ok(());
    }

    // ── 8. Save version metadata and update profile (short lock) ──
    {
        let version_json_path = versions_dir.join(format!("{}.json", version));
        // Bug #7: Save the original Mojang JSON (camelCase) not the Rust struct (snake_case).
        // Minecraft itself reads versions/{id}/{id}.json and expects the original format.
        if let Some(ref raw) = game_version.raw_json {
            std::fs::write(&version_json_path, raw)?;
        } else {
            // Fallback: serialize from the parsed struct (less ideal but functional)
            let fallback = serde_json::to_string_pretty(&game_version)?;
            std::fs::write(&version_json_path, fallback)?;
        }

        profile.game_version = version.clone();
        let core = core.read().await;
        core.profile_store.save(&profile)?;

        // Refresh the local repository so the new instance appears in the index
        // (launch now resolves from disk, not the network — G1).
        core.game_repository
            .write()
            .map_err(|_| rmccc_error::RmcccError::Other("game repository lock poisoned".into()))?
            .refresh()?;
    }

    // ── 9. Emit completion event（前端据此刷新 UI）──
    let _ = app_handle.emit(
        crate::events::DOWNLOAD_COMPLETE,
        serde_json::json!({
            "batch_id": batch_id,
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
