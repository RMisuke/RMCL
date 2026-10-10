//! External URL opening — wraps `tauri-plugin-opener` with validation.

use rmccc_error::IpcError;

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
