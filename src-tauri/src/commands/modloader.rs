//! Mod loader IPC commands.

use crate::state::AppState;
use rmccc_error::IpcError;
use rmccc_modloader::{DependencyManager, LoaderVersion, ModLoaderType};
use tauri::Emitter;

/// 发射 modloader 安装进度事件（P2-06）。
/// `InstallTask` 无细粒度进度回调，故采用阶段级事件（0/10/50/100%），
/// 前端可据此展示安装中/完成/失败状态。
fn emit_modloader_progress(
    app_handle: &tauri::AppHandle,
    profile_id: &str,
    loader_type: &str,
    game_version: &str,
    stage: &str,
    percent: u8,
    message: &str,
) {
    let _ = app_handle.emit(
        crate::events::MODLOADER_INSTALL_PROGRESS,
        serde_json::json!({
            "profile_id": profile_id,
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

    // 统一走 DependencyManager（P1-02）：支持全部 8 种 loader
    let dm = DependencyManager::new(state.http_client.clone());
    let versions = dm.list_versions(&lt, &game_version).await?;
    Ok(versions)
}

#[tauri::command]
#[tracing::instrument(skip(state), fields(request_id = %uuid::Uuid::new_v4()))]
pub async fn modloader_install(
    state: tauri::State<'_, AppState>,
    profile_id: String,
    loader_type: String,
    loader_version: String,
) -> Result<(), IpcError> {
    let lt = ModLoaderType::from_str(&loader_type).ok_or_else(|| IpcError {
        code: rmccc_error::ErrorCode::Unknown,
        message: format!("Unknown mod loader type: {loader_type}"),
        retryable: false,
    })?;

    let app_handle = state.app_handle.clone();

    // 只读一次 profile：Java 探测、基础版本解析、安装、profile 提交、索引刷新
    // 全部由 core 的编排完成。
    let profile = {
        let core = state.core.read().await;
        core.profile_store
            .get(&profile_id)?
            .ok_or_else(|| IpcError {
                code: rmccc_error::ErrorCode::ProfileNotFound,
                message: format!("Profile not found: {profile_id}"),
                retryable: false,
            })?
    };
    let base_game_version = profile.game_version.clone();

    emit_modloader_progress(
        &app_handle,
        &profile_id,
        &loader_type,
        &base_game_version,
        "started",
        0,
        &format!("开始安装 {} ...", lt.display_name()),
    );

    // 统一入口（P1-4）：installer-JAR 与 patch 两条路径的分流已在
    // `rmccc_modloader` 内部，Tauri/CLI 不再各自 match。
    let installed = {
        let core = state.core.read().await;
        rmccc_core::modloader::install_loader(&core, &profile, &lt, &loader_version).await?
    };
    tracing::info!(
        profile_id = %profile_id,
        loader = %loader_type,
        version_id = %installed.version_id,
        via_installer_jar = installed.via_installer_jar,
        "Mod loader installed"
    );

    // ── profile 提交与索引刷新已由 core 完成，这里只上报事件 ──
    emit_modloader_progress(
        &app_handle,
        &profile_id,
        &loader_type,
        &installed.version_id,
        "completed",
        100,
        "安装完成",
    );

    let _ = app_handle.emit(
        crate::events::INSTANCES_REFRESHED,
        serde_json::json!({ "version": installed.version_id }),
    );

    Ok(())
}
