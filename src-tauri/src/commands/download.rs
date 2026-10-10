//! Download control IPC commands.
//!
//! Wraps `ConcurrentDownloadManager` batch control methods for the frontend.

use crate::state::AppState;
use rmccc_error::IpcError;

#[tauri::command]
#[tracing::instrument(skip(state))]
pub async fn download_cancel(
    state: tauri::State<'_, AppState>,
    batch_id: String,
) -> Result<(), IpcError> {
    let core = state.core.read().await;
    core.download_manager.cancel_batch(&batch_id);
    Ok(())
}

#[tauri::command]
#[tracing::instrument(skip(state))]
pub async fn download_pause(
    state: tauri::State<'_, AppState>,
    batch_id: String,
) -> Result<(), IpcError> {
    let core = state.core.read().await;
    core.download_manager.pause(&batch_id);
    Ok(())
}

#[tauri::command]
#[tracing::instrument(skip(state))]
pub async fn download_resume(
    state: tauri::State<'_, AppState>,
    batch_id: String,
) -> Result<(), IpcError> {
    let core = state.core.read().await;
    core.download_manager.resume(&batch_id);
    Ok(())
}
