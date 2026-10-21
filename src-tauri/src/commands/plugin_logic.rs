//! Logic plugin execution (Phase 5, ADR-007).
//!
//! The launcher never links a WASM engine: [`LogicRunner`] spawns
//! `rmccc-logic-host` per plugin and enforces the wall-clock budget by killing
//! it. Authorization happens here, once per call, against the live registry and
//! permission state — the helper has no idea what a capability is.

use rmccc_error::{ErrorCode, IpcError};
use rmccc_logic::protocol::{LogicError, LogicErrorCode};
use rmccc_logic::LogicLimits;
use serde_json::Value;

use crate::state::AppState;

/// Calls a logic plugin may receive in v1: read-only, no side effects.
pub const ALLOWED_CALLS: &[&str] = &["compat.check", "mods.source.search"];

/// Capability a plugin must hold to be executed at all.
pub const LOGIC_CAPABILITY: &str = "logic.invoke";

/// Whether `call` is part of the v1 allowlist.
pub fn is_allowed_call(call: &str) -> bool {
    ALLOWED_CALLS.contains(&call)
}

/// Map a helper/protocol failure onto the IPC error vocabulary.
///
/// The bridge keeps its own wire codes (ADR-007 §24.2); this is the app-level
/// projection so the UI shows the same toast shape as every other failure.
pub fn map_logic_error(error: LogicError) -> IpcError {
    let code = match error.code {
        LogicErrorCode::BadRequest
        | LogicErrorCode::UnknownMethod
        | LogicErrorCode::ProtocolVersion => ErrorCode::ValidationError,
        LogicErrorCode::NotLoaded | LogicErrorCode::AlreadyLoaded => ErrorCode::PluginStateConflict,
        LogicErrorCode::ImportNotAllowed
        | LogicErrorCode::ModuleTooLarge
        | LogicErrorCode::ModuleInvalid => ErrorCode::PluginPackageInvalid,
        LogicErrorCode::FuelExhausted | LogicErrorCode::Trap | LogicErrorCode::PayloadTooLarge => {
            ErrorCode::InternalError
        }
        LogicErrorCode::Timeout => ErrorCode::InternalError,
        LogicErrorCode::Internal => ErrorCode::InternalError,
    };
    let retryable = matches!(
        error.code,
        LogicErrorCode::Timeout | LogicErrorCode::Internal
    );
    IpcError {
        code,
        message: format!("logic.{}: {}", error.code.as_str(), error.message),
        retryable,
    }
}

fn internal(message: impl Into<String>) -> IpcError {
    IpcError {
        code: ErrorCode::InternalError,
        message: message.into(),
        retryable: false,
    }
}

/// Run one read-only logic call.
///
/// Authorization order matters: enabled state → `logic.invoke` capability →
/// call allowlist → manifest entry → installed path. Nothing runs before all of
/// them pass, and every call repeats the whole chain (a revoke takes effect on
/// the next call, not on the next app start).
#[tauri::command]
#[tracing::instrument(skip(state, input), fields(call = %call))]
pub async fn plugin_logic_invoke(
    state: tauri::State<'_, AppState>,
    plugin_id: String,
    call: String,
    input: Option<Value>,
    timeout_ms: Option<u64>,
) -> Result<Value, IpcError> {
    if !is_allowed_call(&call) {
        return Err(IpcError {
            code: ErrorCode::ValidationError,
            message: format!("logic call `{call}` is not in the v1 allowlist"),
            retryable: false,
        });
    }

    let (wasm_path, limits) = {
        let core = state.core.read().await;
        core.plugin_manager
            .ensure_enabled(&plugin_id)
            .map_err(IpcError::from)?;
        core.plugin_manager
            .authorize(&plugin_id, LOGIC_CAPABILITY, None, None)
            .map_err(IpcError::from)?;

        let entry = core
            .plugin_manager
            .manifest(&plugin_id)
            .and_then(|manifest| manifest.entry.logic.clone())
            .ok_or_else(|| IpcError {
                code: ErrorCode::PluginStateConflict,
                message: format!("plugin {plugin_id} declares no entry.logic"),
                retryable: false,
            })?;
        let dir = core
            .plugin_manager
            .installed_dir(&plugin_id)
            .ok_or_else(|| internal("installed directory is missing"))?;
        let path = dir.join(entry);
        if !path.is_file() {
            return Err(internal(format!(
                "logic module is missing: {}",
                path.display()
            )));
        }
        let mut limits = LogicLimits::default();
        if let Some(timeout) = timeout_ms {
            limits.timeout_ms = timeout;
        }
        (path, limits.clamped())
    };

    let runner = state.logic.clone();
    let result: Result<Value, LogicError> = tauri::async_runtime::spawn_blocking(move || {
        let mut runner = runner
            .lock()
            .map_err(|_| LogicError::new(LogicErrorCode::Internal, "logic runner lock poisoned"))?;
        runner.invoke(
            &plugin_id,
            &wasm_path,
            &call,
            input.unwrap_or(Value::Null),
            limits,
        )
    })
    .await
    .map_err(|e| internal(format!("logic task failed: {e}")))?;

    result.map_err(map_logic_error)
}

/// Kill the plugin's helper, aborting any in-flight call (best-effort cancel).
#[tauri::command]
pub async fn plugin_logic_cancel(
    state: tauri::State<'_, AppState>,
    plugin_id: String,
) -> Result<bool, IpcError> {
    let runner = state.logic.clone();
    let cancelled = tauri::async_runtime::spawn_blocking(move || {
        runner
            .lock()
            .map(|mut runner| runner.cancel(&plugin_id))
            .unwrap_or(false)
    })
    .await
    .map_err(|e| internal(format!("cancel task failed: {e}")))?;
    Ok(cancelled)
}
