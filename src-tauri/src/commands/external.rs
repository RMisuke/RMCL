//! External URL opening — wraps `tauri-plugin-opener` with validation.
//!
//! Also owns the native video picker used by the appearance settings: the
//! chosen MP4 is validated once, then whitelisted for the duration of this
//! session in the asset-protocol scope so the WebView can stream it locally
//! (with Range support) without exposing any other file on disk.

use std::path::{Path, PathBuf};

use rmccc_error::{ErrorCode, IpcError};
use tauri::Manager;

/// Open an external URL in the system default browser (via tauri-plugin-opener).
///
/// Only `http`/`https` schemes are accepted; arbitrary protocol handlers
/// (`file:`, `javascript:`, custom schemes) are rejected. Failures are
/// returned as `IpcError` and logged — never panics.
#[tauri::command]
#[tracing::instrument(skip(url))]
pub fn open_external_url(url: String) -> Result<(), IpcError> {
    let trimmed = url.trim();
    if !(trimmed.starts_with("https://") || trimmed.starts_with("http://")) {
        tracing::warn!(url = %trimmed, "Rejected non-http(s) URL open request");
        return Err(IpcError {
            code: rmccc_error::ErrorCode::ValidationError,
            message: "Only http/https URLs are allowed".into(),
            retryable: false,
        });
    }

    match tauri_plugin_opener::open_url(trimmed, None::<&str>) {
        Ok(()) => {
            tracing::info!(url = %trimmed, "Opened external URL");
            Ok(())
        }
        Err(e) => {
            tracing::warn!(url = %trimmed, error = ?e, "Failed to open external URL");
            Err(IpcError {
                code: rmccc_error::ErrorCode::Unknown,
                message: format!("Failed to open URL: {e}"),
                retryable: false,
            })
        }
    }
}

/// Native directory picker (tauri-plugin-dialog). Returns `None` on cancel.
///
/// Read-only browsing target for extra game roots — the picked path is only
/// stored when the user confirms adding it in the UI.
#[tauri::command]
#[tracing::instrument(skip(app))]
pub async fn dialog_pick_directory(app: tauri::AppHandle) -> Result<Option<String>, IpcError> {
    use tauri_plugin_dialog::DialogExt;

    let picked =
        tauri::async_runtime::spawn_blocking(move || app.dialog().file().blocking_pick_folder())
            .await
            .map_err(|e| IpcError {
                code: rmccc_error::ErrorCode::Unknown,
                message: format!("directory dialog failed: {e}"),
                retryable: false,
            })?;

    match picked {
        Some(path) => {
            let path = path.into_path().map_err(|e| IpcError {
                code: rmccc_error::ErrorCode::ValidationError,
                message: format!("invalid path: {e}"),
                retryable: false,
            })?;
            Ok(Some(path.display().to_string()))
        }
        None => Ok(None),
    }
}

/// Resolve and validate a user-picked background video.
///
/// Only existing regular `.mp4` files are accepted; symlinks are resolved so
/// the asset-protocol scope matches the real file the WebView opens.
fn checked_background_video(path: &Path) -> Result<PathBuf, IpcError> {
    let resolved = std::fs::canonicalize(path).map_err(|e| IpcError {
        code: ErrorCode::ValidationError,
        message: format!("video file is not accessible: {e}"),
        retryable: false,
    })?;
    let metadata = std::fs::metadata(&resolved).map_err(|e| IpcError {
        code: ErrorCode::ValidationError,
        message: format!("video file is not accessible: {e}"),
        retryable: false,
    })?;
    if !metadata.is_file() {
        return Err(IpcError {
            code: ErrorCode::ValidationError,
            message: "background video must be a file".into(),
            retryable: false,
        });
    }
    let is_mp4 = resolved
        .extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| ext.eq_ignore_ascii_case("mp4"));
    if !is_mp4 {
        return Err(IpcError {
            code: ErrorCode::ValidationError,
            message: "only local MP4 videos are supported".into(),
            retryable: false,
        });
    }
    Ok(resolved)
}

fn allow_asset_read(app: &tauri::AppHandle, path: &Path) -> Result<(), IpcError> {
    app.asset_protocol_scope()
        .allow_file(path)
        .map_err(|e| IpcError {
            code: ErrorCode::ValidationError,
            message: format!("could not grant video access: {e}"),
            retryable: false,
        })
}

/// Whitelist an already-known background video (called again on every launch,
/// because the asset-protocol scope is in-memory only). Returns the resolved
/// path so the frontend can build the `asset://` URL from the canonical file.
#[tauri::command]
#[tracing::instrument(skip(app, path))]
pub async fn background_video_allow(
    app: tauri::AppHandle,
    path: String,
) -> Result<String, IpcError> {
    let resolved = checked_background_video(Path::new(&path))?;
    allow_asset_read(&app, &resolved)?;
    tracing::info!(file = ?resolved.file_name(), "Local background video allowed");
    Ok(resolved.display().to_string())
}

/// Native MP4 picker for the background video. Returns `None` on cancel and
/// whitelists the chosen file immediately.
#[tauri::command]
#[tracing::instrument(skip(app))]
pub async fn dialog_pick_background_video(
    app: tauri::AppHandle,
) -> Result<Option<String>, IpcError> {
    use tauri_plugin_dialog::DialogExt;

    let dialog_app = app.clone();
    let picked = tauri::async_runtime::spawn_blocking(move || {
        dialog_app
            .dialog()
            .file()
            .add_filter("MP4 video", &["mp4"])
            .blocking_pick_file()
    })
    .await
    .map_err(|e| IpcError {
        code: ErrorCode::Unknown,
        message: format!("video dialog failed: {e}"),
        retryable: false,
    })?;

    let Some(file) = picked else {
        return Ok(None);
    };
    let path = file.into_path().map_err(|e| IpcError {
        code: ErrorCode::ValidationError,
        message: format!("invalid path: {e}"),
        retryable: false,
    })?;
    let resolved = checked_background_video(&path)?;
    allow_asset_read(&app, &resolved)?;
    tracing::info!(file = ?resolved.file_name(), "Local background video picked");
    Ok(Some(resolved.display().to_string()))
}
