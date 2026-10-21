//! Plugin management IPC commands (Phase 2).
//!
//! 所有权限/状态决策都在后端 `PluginManager`，前端只负责展示与触发。

use crate::state::AppState;
use rmccc_core::rmccc_plugin::{
    CapabilityInfo, InstallReport, PluginConfig, PluginContributions, PluginDetail,
    PluginDiagnostic, PluginSummary,
};
use rmccc_error::IpcError;
use serde::Serialize;
use serde_json::Value;
use ts_rs::TS;

#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export_to = "types.ts")]
pub struct PluginList {
    pub plugins: Vec<PluginSummary>,
    pub diagnostics: Vec<PluginDiagnostic>,
}

#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export_to = "types.ts")]
pub struct PluginConfigView {
    pub config: PluginConfig,
    #[ts(type = "unknown")]
    pub schema: Option<Value>,
}

#[tauri::command]
#[tracing::instrument(skip(state))]
pub async fn plugin_list(
    state: tauri::State<'_, AppState>,
    include_removed: Option<bool>,
) -> Result<PluginList, IpcError> {
    let core = state.core.read().await;
    Ok(PluginList {
        plugins: core.plugin_manager.list(include_removed.unwrap_or(false)),
        diagnostics: core.plugin_manager.diagnostics().to_vec(),
    })
}

#[tauri::command]
#[tracing::instrument(skip(state))]
pub async fn plugin_inspect(
    state: tauri::State<'_, AppState>,
    id: String,
) -> Result<PluginDetail, IpcError> {
    let mut core = state.core.write().await;
    Ok(core.plugin_manager.inspect(&id)?)
}

#[tauri::command]
#[tracing::instrument(skip(state))]
pub async fn plugin_install(
    state: tauri::State<'_, AppState>,
    path: String,
) -> Result<InstallReport, IpcError> {
    let mut core = state.core.write().await;
    Ok(core
        .plugin_manager
        .install_from_path(std::path::Path::new(&path))?)
}

#[tauri::command]
#[tracing::instrument(skip(app, state))]
pub async fn plugin_pick_and_install(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
) -> Result<Option<InstallReport>, IpcError> {
    use tauri_plugin_dialog::DialogExt;

    let picked = tauri::async_runtime::spawn_blocking(move || {
        app.dialog()
            .file()
            .add_filter("RMCCC plugin", &["rmcpkg"])
            .blocking_pick_file()
    })
    .await
    .map_err(|e| IpcError {
        code: rmccc_error::ErrorCode::Unknown,
        message: format!("file dialog failed: {e}"),
        retryable: false,
    })?;

    let Some(file) = picked else {
        return Ok(None);
    };
    let path = file.into_path().map_err(|e| IpcError {
        code: rmccc_error::ErrorCode::ValidationError,
        message: format!("invalid path: {e}"),
        retryable: false,
    })?;

    let mut core = state.core.write().await;
    Ok(Some(core.plugin_manager.install_from_path(&path)?))
}

#[tauri::command]
#[tracing::instrument(skip(state))]
pub async fn plugin_enable(
    state: tauri::State<'_, AppState>,
    id: String,
) -> Result<PluginSummary, IpcError> {
    let mut core = state.core.write().await;
    Ok(core.plugin_manager.enable(&id)?)
}

#[tauri::command]
#[tracing::instrument(skip(state))]
pub async fn plugin_disable(
    state: tauri::State<'_, AppState>,
    id: String,
) -> Result<PluginSummary, IpcError> {
    let mut core = state.core.write().await;
    Ok(core.plugin_manager.disable(&id)?)
}

#[tauri::command]
#[tracing::instrument(skip(state))]
pub async fn plugin_remove(
    state: tauri::State<'_, AppState>,
    id: String,
    delete_data: Option<bool>,
) -> Result<(), IpcError> {
    let mut core = state.core.write().await;
    core.plugin_manager
        .uninstall(&id, delete_data.unwrap_or(false))?;
    Ok(())
}

#[tauri::command]
#[tracing::instrument(skip(state))]
pub async fn plugin_rollback(
    state: tauri::State<'_, AppState>,
    id: String,
) -> Result<PluginSummary, IpcError> {
    let mut core = state.core.write().await;
    Ok(core.plugin_manager.rollback(&id)?)
}

#[tauri::command]
#[tracing::instrument(skip(state))]
pub async fn plugin_grant(
    state: tauri::State<'_, AppState>,
    id: String,
    capability: String,
) -> Result<PluginSummary, IpcError> {
    let mut core = state.core.write().await;
    Ok(core.plugin_manager.grant(&id, &capability)?)
}

#[tauri::command]
#[tracing::instrument(skip(state))]
pub async fn plugin_revoke(
    state: tauri::State<'_, AppState>,
    id: String,
    capability: String,
) -> Result<PluginSummary, IpcError> {
    let mut core = state.core.write().await;
    Ok(core.plugin_manager.revoke(&id, &capability)?)
}

#[tauri::command]
#[tracing::instrument(skip(state))]
pub async fn plugin_config_get(
    state: tauri::State<'_, AppState>,
    id: String,
) -> Result<PluginConfigView, IpcError> {
    let mut core = state.core.write().await;
    let (config, schema) = core.plugin_manager.config_get(&id)?;
    Ok(PluginConfigView { config, schema })
}

#[tauri::command]
#[tracing::instrument(skip(state))]
pub async fn plugin_config_set(
    state: tauri::State<'_, AppState>,
    id: String,
    values: Value,
) -> Result<PluginConfig, IpcError> {
    let mut core = state.core.write().await;
    Ok(core.plugin_manager.config_set(&id, values)?)
}

#[tauri::command]
#[tracing::instrument(skip(state))]
pub async fn plugin_config_reset(
    state: tauri::State<'_, AppState>,
    id: String,
) -> Result<PluginConfig, IpcError> {
    let mut core = state.core.write().await;
    Ok(core.plugin_manager.config_reset(&id)?)
}

#[tauri::command]
#[tracing::instrument(skip(state))]
pub async fn plugin_capabilities(
    state: tauri::State<'_, AppState>,
) -> Result<Vec<CapabilityInfo>, IpcError> {
    let core = state.core.read().await;
    Ok(core.plugin_manager.capability_catalog())
}

#[tauri::command]
#[tracing::instrument(skip(state))]
pub async fn plugin_contributions(
    state: tauri::State<'_, AppState>,
) -> Result<PluginContributions, IpcError> {
    let core = state.core.read().await;
    Ok(core.plugin_manager.contributions())
}
