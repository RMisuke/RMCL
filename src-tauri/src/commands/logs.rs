//! Log IPC commands — read and manage log files.

use crate::state::AppState;
use rmccc_error::IpcError;
use std::io::Read;

const MAX_LOG_LINES: usize = 500;

#[tauri::command]
#[tracing::instrument(skip(state))]
pub async fn log_get(
    state: tauri::State<'_, AppState>,
    lines: Option<usize>,
) -> Result<Vec<String>, IpcError> {
    let limit = lines.unwrap_or(100).min(MAX_LOG_LINES);
    let core = state.core.read().await;
    let log_dir = core.data_dir.join("logs");

    let mut latest_log = None;
    if let Ok(entries) = std::fs::read_dir(&log_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().is_some_and(|e| e == "log") {
                if let Ok(meta) = path.metadata() {
                    latest_log = Some((
                        meta.modified().unwrap_or(std::time::SystemTime::UNIX_EPOCH),
                        path,
                    ));
                }
            }
        }
    }

    let log_path = match latest_log {
        Some((_, path)) => path,
        None => return Ok(Vec::new()),
    };

    let mut file = std::fs::File::open(&log_path)?;
    let mut content = String::new();
    file.read_to_string(&mut content)?;

    let all_lines: Vec<String> = content.lines().map(|l| l.to_string()).collect();
    let start = all_lines.len().saturating_sub(limit);
    Ok(all_lines[start..].to_vec())
}

#[tauri::command]
#[tracing::instrument(skip(state))]
pub async fn log_clear(state: tauri::State<'_, AppState>) -> Result<(), IpcError> {
    let core = state.core.read().await;
    let log_dir = core.data_dir.join("logs");

    if let Ok(entries) = std::fs::read_dir(&log_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().is_some_and(|e| e == "log") {
                let _ = std::fs::remove_file(&path);
            }
        }
    }
    Ok(())
}
