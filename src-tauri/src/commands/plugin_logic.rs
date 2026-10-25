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

    // 审计批次 2：per-plugin 槽位——慢调用只锁自己的 runner，不再串行化
    // 其它插件；pid 槽让 cancel 无需等这把锁（见 plugin_logic_cancel）。
    let slot = state
        .logic
        .entry(plugin_id.clone())
        .or_insert_with(|| {
            std::sync::Arc::new(crate::state::LogicSlot {
                runner: std::sync::Mutex::new(rmccc_logic::LogicRunner::new()),
                pid: std::sync::Mutex::new(None),
            })
        })
        .clone();

    let result: Result<Value, LogicError> = tauri::async_runtime::spawn_blocking(move || {
        let mut runner = slot
            .runner
            .lock()
            .map_err(|_| LogicError::new(LogicErrorCode::Internal, "logic runner lock poisoned"))?;
        let publish_pid = |pid: Option<u32>| -> Result<(), LogicError> {
            match slot.pid.lock() {
                Ok(mut guard) => {
                    *guard = pid;
                    Ok(())
                }
                // pid 槽只影响 cancel 的慢路径判定位，毒化不中止调用本身。
                Err(_) => {
                    tracing::warn!(plugin_id = %plugin_id, "logic pid slot poisoned");
                    Ok(())
                }
            }
        };
        // 显式 load → 发布 PID → call：invoke 内部的 ensure_loaded 此时是
        // no-op，语义与直接 invoke 完全一致。
        runner.load(&plugin_id, &wasm_path, limits)?;
        publish_pid(runner.host_pid(&plugin_id))?;
        let result = runner.invoke(
            &plugin_id,
            &wasm_path,
            &call,
            input.unwrap_or(Value::Null),
            limits,
        );
        publish_pid(None)?;
        result
    })
    .await
    .map_err(|e| internal(format!("logic task failed: {e}")))?;

    result.map_err(map_logic_error)
}

/// Kill the plugin's helper, aborting any in-flight call (best-effort cancel).
///
/// 审计批次 2：cancel 不再与 invoke 争同一把全局锁——
/// - runner 空闲（try_lock 成功）：干净地 cancel（杀进程 + 移除 host）；
/// - 调用进行中（try_lock 失败）：按发布中的 PID 经平台层杀进程组，
///   阻塞中的 `request()` 因管道关闭立即返回，下一次调用自动重生 helper。
#[tauri::command]
pub async fn plugin_logic_cancel(
    state: tauri::State<'_, AppState>,
    plugin_id: String,
) -> Result<bool, IpcError> {
    let Some(slot) = state.logic.get(&plugin_id).map(|e| e.value().clone()) else {
        return Ok(false);
    };

    // 快路径：runner 空闲。
    if let Ok(mut runner) = slot.runner.try_lock() {
        return Ok(runner.cancel(&plugin_id));
    }

    // 慢路径：调用进行中，按 PID 杀（不等锁）。
    let pid = slot.pid.lock().ok().and_then(|p| *p);
    let Some(pid) = pid else {
        // runner 被锁但 PID 未发布（load 阶段）：load 很快，交给下一轮取消。
        return Ok(false);
    };
    tracing::info!(plugin_id = %plugin_id, pid, "Cancelling in-flight logic call by pid");
    match state.platform.process.terminate_tree(pid) {
        Ok(()) => Ok(true),
        Err(e) => {
            // ESRCH（进程已退出）按成功处理；其余错误上报。
            tracing::warn!(plugin_id = %plugin_id, pid, error = ?e, "terminate_tree failed");
            Ok(false)
        }
    }
}
