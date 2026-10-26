//! Log IPC commands — read and manage log files.

use crate::state::AppState;
use rmccc_error::IpcError;

const MAX_LOG_LINES: usize = 500;

#[tauri::command]
#[tracing::instrument(skip(state))]
pub async fn log_get(
    state: tauri::State<'_, AppState>,
    lines: Option<usize>,
) -> Result<Vec<String>, IpcError> {
    let limit = lines.unwrap_or(100).min(MAX_LOG_LINES);
    let log_dir = {
        let core = state.core.read().await;
        core.data_dir.join("logs")
    };

    // Resolve the newest `.log` path on the async worker (cheap), then hand
    // the whole file read to `spawn_blocking` (perf audit 1.2): a crash log
    // can be tens of MB and was previously fully buffered ×2 (content String
    // + per-line Vec) on a runtime worker.
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

    // Keep only the trailing `limit` lines in memory: a tail window is all
    // the UI shows, so there is no need to materialize the whole file.
    let tail = tokio::task::spawn_blocking(move || -> std::io::Result<Vec<String>> {
        use std::io::BufRead;
        let file = std::fs::File::open(&log_path)?;
        let mut ring: std::collections::VecDeque<String> = std::collections::VecDeque::new();
        for line in std::io::BufReader::new(file).lines() {
            let line = line?;
            if ring.len() == limit {
                ring.pop_front();
            }
            ring.push_back(line);
        }
        Ok(ring.into_iter().collect())
    })
    .await
    .map_err(|e| IpcError {
        code: rmccc_error::ErrorCode::Unknown,
        message: format!("log read join error: {e}"),
        retryable: false,
    })?
    .map_err(IpcError::from)?;
    Ok(tail)
}

#[tauri::command]
#[tracing::instrument(skip(state))]
pub async fn log_clear(state: tauri::State<'_, AppState>) -> Result<(), IpcError> {
    let log_dir = {
        let core = state.core.read().await;
        core.data_dir.join("logs")
    };
    tokio::task::spawn_blocking(move || {
        if let Ok(entries) = std::fs::read_dir(&log_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().is_some_and(|e| e == "log") {
                    let _ = std::fs::remove_file(&path);
                }
            }
        }
    })
    .await
    .map_err(|e| IpcError {
        code: rmccc_error::ErrorCode::Unknown,
        message: format!("log clear join error: {e}"),
        retryable: false,
    })?;
    Ok(())
}
