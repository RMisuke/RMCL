//! WebView-side diagnostics.
//!
//! Release builds ship without the Web Inspector and WKWebView's console is
//! invisible from outside the process, so a frontend crash in production leaves
//! no trace: the window simply keeps whatever was painted last (or nothing at
//! all). The bootstrap guard in `index.html` forwards its own errors here via
//! `invoke`, which puts them in `logs/rmccc.<date>.log` next to the Rust ones.

use rmccc_error::{ErrorCode, IpcError};

/// Longest message accepted from the WebView.
///
/// The guard truncates before sending; this is the hard limit so a runaway
/// `error` handler cannot flood the log appender.
const MAX_MESSAGE_BYTES: usize = 8 * 1024;

/// Record a message sent by the frontend in the application log.
///
/// `level` is restricted to the tracing levels so the log stays greppable; an
/// unknown level is a programming error in the caller and is rejected.
#[tauri::command]
pub fn frontend_log(level: String, message: String) -> Result<(), IpcError> {
    if message.len() > MAX_MESSAGE_BYTES {
        return Err(validation_error(format!(
            "frontend log message is too long ({} bytes, limit {MAX_MESSAGE_BYTES})",
            message.len()
        )));
    }

    match level.as_str() {
        "trace" => tracing::trace!(target: "rmccc::webview", "{message}"),
        "debug" => tracing::debug!(target: "rmccc::webview", "{message}"),
        "info" => tracing::info!(target: "rmccc::webview", "{message}"),
        "warn" => tracing::warn!(target: "rmccc::webview", "{message}"),
        "error" => tracing::error!(target: "rmccc::webview", "{message}"),
        other => {
            return Err(validation_error(format!(
                "unsupported frontend log level: {other}"
            )))
        }
    }

    Ok(())
}

fn validation_error(message: String) -> IpcError {
    IpcError {
        code: ErrorCode::ValidationError,
        message,
        retryable: false,
    }
}
