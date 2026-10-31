//! Event name constants for Rust → Frontend communication.
//!
//! Installation/download progress no longer has per-feature event families:
//! every transaction is a `rmccc_tasks::TaskSnapshot` broadcast as
//! `task://updated`, with `task://sync` for a full-board resync after the
//! frontend reloads or the broadcast channel lags.
//!
//! P2-24：移除 crate 级 allow(dead_code)，仅对预留常量保留单项 allow。

use serde::Serialize;

// ── Event name constants ──

/// One task snapshot changed (payload: `TaskSnapshot`).
pub const TASK_UPDATED: &str = "task://updated";
/// Full board after a lag or frontend reload (payload: `TaskBoard`).
pub const TASK_SYNC: &str = "task://sync";

pub const AUTH_DEVICE_CODE: &str = "auth://device-code";

pub const LAUNCH_STARTED: &str = "launch://started";
pub const LAUNCH_OUTPUT: &str = "launch://output";
pub const LAUNCH_EXITED: &str = "launch://exited";
/// Real launch-stage progress. `100` is emitted only after the game window
/// readiness marker appears in the game's stdout, never on process spawn.
pub const LAUNCH_PROGRESS: &str = "launch://progress";

/// Emitted after the game repository re-scans `versions/`
/// (HMCL `RefreshedGameInstancesEvent`).
pub const INSTANCES_REFRESHED: &str = "instances://refreshed";

// ── AI service (ADR-013 S2)：日志一键分析的流式回传 ──

pub const AI_DELTA: &str = "ai://delta";
pub const AI_DONE: &str = "ai://done";
pub const AI_ERROR: &str = "ai://error";

// ── AI analysis payloads（ADR-013 S2）──

/// Launch progress shown by the home status card and launch bubble.
#[derive(Debug, Clone, Serialize, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
#[ts(export_to = "types.ts")]
pub struct LaunchProgressPayload {
    pub profile_id: String,
    pub percent: u8,
    /// `checking` / `repairing` / `checking-java` / `spawning` /
    /// `waiting-window` / `window-open`.
    pub stage: String,
    pub message: String,
    pub window_opened: bool,
}

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
