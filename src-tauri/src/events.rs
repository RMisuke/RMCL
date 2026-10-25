//! Event name constants and throttled emitter for Rust → Frontend communication.
//!
//! Constants and types here are declared for future use in download progress,
//! auth device code flow, and mod loader installation progress events.
//! P2-24：移除 crate 级 allow(dead_code)，仅对预留常量保留单项 allow。

use serde::Serialize;
use std::sync::Mutex;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter};

// ── Event name constants ──

pub const DOWNLOAD_PROGRESS: &str = "download://progress";
pub const DOWNLOAD_COMPLETE: &str = "download://complete";
pub const DOWNLOAD_ERROR: &str = "download://error";
pub const AUTH_DEVICE_CODE: &str = "auth://device-code";

pub const LAUNCH_STARTED: &str = "launch://started";
pub const LAUNCH_OUTPUT: &str = "launch://output";
pub const LAUNCH_EXITED: &str = "launch://exited";

pub const MODLOADER_INSTALL_PROGRESS: &str = "modloader://progress";

/// Emitted after the game repository re-scans `versions/`
/// (HMCL `RefreshedGameInstancesEvent`).
pub const INSTANCES_REFRESHED: &str = "instances://refreshed";

// ── AI service (ADR-013 S2)：日志一键分析的流式回传 ──

pub const AI_DELTA: &str = "ai://delta";
pub const AI_DONE: &str = "ai://done";
pub const AI_ERROR: &str = "ai://error";

// ── Download progress payload ──

#[derive(Debug, Clone, Serialize, ts_rs::TS)]
#[ts(export_to = "types.ts")]
pub struct DownloadProgressPayload {
    pub batch_id: String,
    #[ts(type = "number")]
    pub completed_files: u64,
    #[ts(type = "number")]
    pub total_files: u64,
    #[ts(type = "number")]
    pub downloaded_bytes: u64,
    #[ts(type = "number")]
    pub total_bytes: u64,
    pub current_file: Option<String>,
}

// ── AI analysis payloads（ADR-013 S2）──

#[derive(Debug, Clone, Serialize, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
#[ts(export_to = "types.ts")]
pub struct AiDeltaPayload {
    pub analysis_id: String,
    pub text: String,
}

#[derive(Debug, Clone, Serialize, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
#[ts(export_to = "types.ts")]
pub struct AiDonePayload {
    pub analysis_id: String,
}

#[derive(Debug, Clone, Serialize, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
#[ts(export_to = "types.ts")]
pub struct AiErrorPayload {
    pub analysis_id: String,
    pub message: String,
}

// ── MCP server（ADR-013 S4/S5）：会话 / 确认 / 归因 ──

pub const MCP_SESSION: &str = "mcp://session";
pub const MCP_CONFIRMATION: &str = "mcp://confirmation";
/// 操作类（operate/destructive）MCP 工具调用的归因事件：GUI 据此展示
/// 「外部 Agent 正在……」toast 与徽标（ui-design 07 §3.2；只读工具不发）。
pub const MCP_CALL: &str = "mcp://call";

/// 宿主 → 前端的插件推送中继（S6 `llm.chat` 流式；前端转发进对应 iframe，
/// 消息体即 `rmccc_plugin::ui` 的 `{v, type, ...}` 形态）。
pub const PLUGIN_PUSH: &str = "plugin://push";

#[derive(Debug, Clone, Serialize, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
#[ts(export_to = "types.ts")]
pub struct McpSessionPayload {
    /// `connected` | `disconnected`
    pub kind: String,
    pub session_id: String,
    pub client: String,
}

#[derive(Debug, Clone, Serialize, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
#[ts(export_to = "types.ts")]
pub struct McpConfirmationPayload {
    /// `requested` | `resolved`
    pub kind: String,
    pub confirmation_id: String,
    pub tool: String,
    pub session_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
#[ts(export_to = "types.ts")]
pub struct McpCallPayload {
    /// `started` | `finished`
    pub kind: String,
    pub session_id: String,
    pub tool: String,
    pub ok: bool,
}

#[derive(Debug, Clone, Serialize, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
#[ts(export_to = "types.ts")]
pub struct PluginPushPayload {
    pub plugin_id: String,
    /// 完整的宿主→插件消息（`{v, type: "llm.delta"|"llm.done"|"llm.error", ...}`）。
    #[ts(type = "unknown")]
    pub message: serde_json::Value,
}

// ── Throttled emitter ──

/// Emits download progress events at a controlled rate to avoid flooding the frontend.
///
/// Events are throttled to at most one per `interval` (default 100ms).
/// Completion events (100%) are always emitted immediately.
pub struct ThrottledEmitter {
    app: AppHandle,
    last_emit: Mutex<Instant>,
    interval: Duration,
}

impl ThrottledEmitter {
    pub fn new(app: AppHandle, interval_ms: u64) -> Self {
        Self {
            app,
            last_emit: Mutex::new(Instant::now() - Duration::from_millis(interval_ms)),
            interval: Duration::from_millis(interval_ms),
        }
    }

    /// Emit a download progress event (throttled).
    pub fn emit_progress(&self, payload: &DownloadProgressPayload) {
        // 容忍 poisoned mutex（P1-07：禁 unwrap_used），退化为立即发射
        let mut last = match self.last_emit.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        };

        // Always emit 100% completion
        let is_complete = payload.completed_files >= payload.total_files && payload.total_files > 0;

        if is_complete || last.elapsed() >= self.interval {
            let _ = self.app.emit(DOWNLOAD_PROGRESS, payload);
            *last = Instant::now();
        }
    }
}
