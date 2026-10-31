//! Unified task-center IPC commands.
//!
//! Every install/download transaction is created in
//! [`crate::state::AppState::tasks`]. The frontend reads the board once after
//! mount and then applies `task://updated` snapshots; controls are addressed by
//! task id, never by an internal download batch id.

use crate::state::AppState;
use rmccc_error::{ErrorCode, IpcError};
use rmccc_tasks::{TaskBoard, TaskControlError, TaskErrorView, TaskSnapshot};
use serde::Serialize;
use ts_rs::TS;

/// Returned as soon as a task has been scheduled.
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export_to = "types.ts")]
pub struct TaskStart {
    pub task_id: String,
}

impl TaskStart {
    pub fn new(task_id: impl Into<String>) -> Self {
        Self {
            task_id: task_id.into(),
        }
    }
}

/// Convert any IPC-facing error into the task error payload.
pub(crate) fn error_view(message: impl Into<String>) -> TaskErrorView {
    TaskErrorView {
        message: message.into(),
        code: None,
        retryable: false,
    }
}

pub(crate) fn ipc_error_view(error: &IpcError) -> TaskErrorView {
    TaskErrorView {
        message: error.message.clone(),
        code: Some(format!("{:?}", error.code)),
        retryable: error.retryable,
    }
}

fn control_error(error: TaskControlError) -> IpcError {
    match error {
        TaskControlError::NotFound => IpcError {
            code: ErrorCode::NotFound,
            message: "任务不存在或已被清理".into(),
            retryable: false,
        },
        TaskControlError::NotRunning => IpcError {
            code: ErrorCode::ValidationError,
            message: "任务已结束，无法再操作".into(),
            retryable: false,
        },
        TaskControlError::NotPausable => IpcError {
            code: ErrorCode::ValidationError,
            message: "当前阶段不可暂停".into(),
            retryable: false,
        },
        TaskControlError::NotResumable => IpcError {
            code: ErrorCode::ValidationError,
            message: "任务未处于暂停状态".into(),
            retryable: false,
        },
    }
}

/// Active tasks + bounded recent history (survives a WebView reload).
#[tauri::command]
#[tracing::instrument(skip(state))]
pub async fn task_list(state: tauri::State<'_, AppState>) -> Result<TaskBoard, IpcError> {
    Ok(state.tasks.board())
}

/// One task snapshot, for deep links / debugging.
#[tauri::command]
#[tracing::instrument(skip(state))]
pub async fn task_get(
    state: tauri::State<'_, AppState>,
    task_id: String,
) -> Result<Option<TaskSnapshot>, IpcError> {
    Ok(state.tasks.snapshot(&task_id))
}

/// Full detail log (installer stdout etc.); the snapshot only carries a preview.
#[tauri::command]
#[tracing::instrument(skip(state))]
pub async fn task_log(
    state: tauri::State<'_, AppState>,
    task_id: String,
) -> Result<Vec<String>, IpcError> {
    Ok(state.tasks.log(&task_id))
}

#[tauri::command]
#[tracing::instrument(skip(state))]
pub async fn task_pause(
    state: tauri::State<'_, AppState>,
    task_id: String,
) -> Result<(), IpcError> {
    state.tasks.request_pause(&task_id).map_err(control_error)
}

#[tauri::command]
#[tracing::instrument(skip(state))]
pub async fn task_resume(
    state: tauri::State<'_, AppState>,
    task_id: String,
) -> Result<(), IpcError> {
    state.tasks.request_resume(&task_id).map_err(control_error)
}

#[tauri::command]
#[tracing::instrument(skip(state))]
pub async fn task_cancel(
    state: tauri::State<'_, AppState>,
    task_id: String,
) -> Result<(), IpcError> {
    state.tasks.request_cancel(&task_id).map_err(control_error)
}

#[tauri::command]
#[tracing::instrument(skip(state))]
pub async fn task_clear_history(state: tauri::State<'_, AppState>) -> Result<(), IpcError> {
    state.tasks.clear_history();
    Ok(())
}
