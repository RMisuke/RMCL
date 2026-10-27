//! Mod loader IPC commands.

use crate::state::AppState;
use rmccc_error::IpcError;
use rmccc_modloader::{DependencyManager, LoaderVersion, ModLoaderType};
use std::sync::Arc;
use tauri::Emitter;

/// 发射 modloader 安装进度事件（P2-06）。
/// `InstallTask` 无细粒度进度回调，故采用阶段级事件（0/10/50/100%），
/// 前端可据此展示安装中/完成/失败状态。
fn emit_modloader_progress(
    app_handle: &tauri::AppHandle,
    key: &str,
    loader_type: &str,
    game_version: &str,
    stage: &str,
    percent: u8,
    message: &str,
) {
    let _ = app_handle.emit(
        crate::events::MODLOADER_INSTALL_PROGRESS,
        serde_json::json!({
            "key": key,
            "loader_type": loader_type,
            "game_version": game_version,
            "stage": stage,
            "percent": percent,
            "message": message,
        }),
    );
}

#[tauri::command]
#[tracing::instrument(skip(state))]
pub async fn modloader_list(
    state: tauri::State<'_, AppState>,
    loader_type: String,
    game_version: String,
) -> Result<Vec<LoaderVersion>, IpcError> {
    let lt = ModLoaderType::from_str(&loader_type).ok_or_else(|| IpcError {
        code: rmccc_error::ErrorCode::Unknown,
        message: format!("Unknown mod loader type: {loader_type}"),
        retryable: false,
    })?;

    // A6 triage：安装器未完成的 loader 不再出现在可选版本里（analyzer 仍可
    // 识别已安装实例）。返回空列表，让向导自然显示"无可用版本"。
    if !lt.is_installable() {
        tracing::warn!(loader = %loader_type, "loader installer incomplete; returning empty version list");
        return Ok(Vec::new());
    }

    // 统一走 DependencyManager（P1-02）：支持全部 8 种 loader
    let platform = {
        let core = state.core.read().await;
        core.platform.descriptor
    };
    let dm = DependencyManager::new(state.http_client.clone()).with_platform(platform);
    let versions = dm.list_versions(&lt, &game_version).await?;
    Ok(versions)
}

#[tauri::command]
#[tracing::instrument(skip(state), fields(request_id = %uuid::Uuid::new_v4()))]
pub async fn modloader_install(
    state: tauri::State<'_, AppState>,
    key: String,
    loader_type: String,
    loader_version: String,
) -> Result<String, IpcError> {
    let lt = ModLoaderType::from_str(&loader_type).ok_or_else(|| IpcError {
        code: rmccc_error::ErrorCode::Unknown,
        message: format!("Unknown mod loader type: {loader_type}"),
        retryable: false,
    })?;

    let app_handle = state.app_handle.clone();

    // 只读一次实例：Java 探测、基础版本解析、安装、新实例提交、索引刷新
    // 全部由 core 的编排完成。
    let instance = {
        let core = state.core.read().await;
        let mut service = core.instances.lock().map_err(|_| IpcError {
            code: rmccc_error::ErrorCode::InternalError,
            message: "Instance service lock poisoned".into(),
            retryable: false,
        })?;
        service.get(&key)?
    };
    let base_game_version = instance.summary.game_version.clone();
    let base_game_version_for_progress = base_game_version.clone();

    emit_modloader_progress(
        &app_handle,
        &key,
        &loader_type,
        &base_game_version,
        "started",
        0,
        &format!("开始安装 {} ...", lt.display_name()),
    );

    // 安装细节回调：modloader crate 的阶段事件逐条转发为 IPC 进度事件
    //（下载字节 / 安装器 stdout 原行 / 检测产物 / 补库），Task Center 展示。
    // "detail"（安装器 stdout 原行）可达每秒数百条，按 100ms 限流转发；
    // 阶段/百分比里程碑事件不受限流影响（perf audit 2.2）。
    let progress: rmccc_modloader::InstallProgressCallback = {
        let app_handle = app_handle.clone();
        let key = key.clone();
        let loader_type = loader_type.clone();
        let progress_created = std::time::Instant::now();
        let last_detail_ms = std::sync::atomic::AtomicU64::new(0);
        Arc::new(move |event: rmccc_modloader::InstallProgressEvent| {
            if event.stage == "detail" {
                const DETAIL_WINDOW_MS: u64 = 100;
                let now_ms = progress_created.elapsed().as_millis() as u64;
                if now_ms.saturating_sub(last_detail_ms.load(std::sync::atomic::Ordering::Relaxed))
                    < DETAIL_WINDOW_MS
                {
                    return;
                }
                last_detail_ms.store(now_ms, std::sync::atomic::Ordering::Relaxed);
            }
            emit_modloader_progress(
                &app_handle,
                &key,
                &loader_type,
                &base_game_version_for_progress,
                event.stage,
                event.percent,
                &event.message,
            );
        })
    };

    // 统一入口（P1-4）：installer-JAR 与 patch 两条路径的分流已在
    // `rmccc_modloader` 内部，Tauri/CLI 不再各自 match。
    // A2：Forge/NeoForge 安装要运行官方 installer JAR（分钟级）——handle
    // 快照后立即释放读锁，全程不持 core 锁。
    let core = {
        let core = state.core.read().await;
        core.handle()
    };
    let installed = match rmccc_core::modloader::install_loader(
        &core,
        &instance,
        &lt,
        &loader_version,
        Some(progress),
    )
    .await
    {
        Ok(installed) => installed,
        Err(error) => {
            // 失败也要落地：否则 Task Center 的安装行永远悬着。
            emit_modloader_progress(
                &app_handle,
                &key,
                &loader_type,
                &base_game_version,
                "failed",
                100,
                &format!("安装失败：{error}"),
            );
            return Err(error.into());
        }
    };
    tracing::info!(
        key = %key,
        loader = %loader_type,
        version_id = %installed.version_id,
        via_installer_jar = installed.via_installer_jar,
        "Mod loader installed"
    );

    if let Ok(mut service) = core.instances.lock() {
        let _ = service.select_instance(Some(&installed.instance_key));
    }

    // ── 新实例提交与索引刷新已由 core 完成，这里只上报事件 ──
    emit_modloader_progress(
        &app_handle,
        &key,
        &loader_type,
        &installed.version_id,
        "completed",
        100,
        "安装完成",
    );

    let installed_key = installed.instance_key.clone();
    let _ = app_handle.emit(
        crate::events::INSTANCES_REFRESHED,
        serde_json::json!({ "version": installed.version_id }),
    );

    Ok(installed_key)
}
