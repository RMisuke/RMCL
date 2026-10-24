//! MCP 服务器管理 IPC（ADR-013 S4/S5，设计 §8 的 9 条 `mcp_*`）。
//!
//! 服务器本体在 [`crate::mcp`]；本文件是 GUI 设置页（ui-design 07 §2）的
//! 管理面：开关 / 令牌 / 工具策略 / 会话 / 确认卡 / 审计视图。
//! 前端接线随整体前端重构落地（`ui-design/07`），因此这 9 条命令暂列
//! `ipc_tests::BACKEND_ONLY` 白名单。

use crate::mcp::{McpHandle, Policy};
use crate::state::AppState;
use rmccc_error::{ErrorCode, IpcError};
use serde::Serialize;
use ts_rs::TS;

/// 服务器状态视图（含本机用户复制配置所需的令牌；仅 IPC，不经 MCP 层外发）。
#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export_to = "types.ts")]
pub struct McpStatusView {
    pub enabled: bool,
    pub port: Option<u16>,
    pub token: String,
    pub endpoint: String,
    pub sessions: Vec<McpSessionView>,
    pub pending_confirmations: usize,
}

#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export_to = "types.ts")]
pub struct McpSessionView {
    pub session_id: String,
    pub client: String,
    pub connected_at: String,
}

/// 工具视图：风险档 + 默认策略 + （若有）用户覆盖。
#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export_to = "types.ts")]
pub struct McpToolView {
    pub name: String,
    pub description: String,
    /// `read_only` | `operate` | `destructive`
    pub risk: String,
    pub default_policy: Policy,
    pub override_policy: Option<Policy>,
}

/// 待确认操作（确认卡数据）。
#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export_to = "types.ts")]
pub struct McpConfirmationView {
    pub id: String,
    pub tool: String,
    pub session_id: Option<String>,
    pub status: String,
    #[ts(type = "unknown")]
    pub payload: serde_json::Value,
    pub created_at: String,
}

/// 审计记录视图（`mcp_recent_calls`，已脱敏）。
#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export_to = "types.ts")]
pub struct McpRecentCallView {
    pub ts: String,
    pub session_id: String,
    pub tool_id: String,
    pub risk_level: String,
    #[ts(type = "unknown")]
    pub input: serde_json::Value,
    pub outcome: String,
    pub duration_ms: u64,
    pub error: Option<String>,
}

fn status_view(handle: &McpHandle) -> Result<McpStatusView, IpcError> {
    let token = handle.token()?;
    Ok(McpStatusView {
        enabled: handle.is_enabled(),
        port: handle.port(),
        endpoint: handle
            .port()
            .map(|p| format!("http://127.0.0.1:{p}/mcp"))
            .unwrap_or_default(),
        token,
        sessions: handle
            .list_sessions()
            .into_iter()
            .map(|(id, client, connected_at)| McpSessionView {
                session_id: id,
                client,
                connected_at,
            })
            .collect(),
        pending_confirmations: handle.pending_confirmations().len(),
    })
}

fn validation_error(message: impl Into<String>) -> IpcError {
    IpcError {
        code: ErrorCode::ValidationError,
        message: message.into(),
        retryable: false,
    }
}

#[tauri::command]
#[tracing::instrument(skip(state))]
pub async fn mcp_status(state: tauri::State<'_, AppState>) -> Result<McpStatusView, IpcError> {
    status_view(&state.mcp)
}

#[tauri::command]
#[tracing::instrument(skip(state))]
pub async fn mcp_set_enabled(
    state: tauri::State<'_, AppState>,
    enabled: bool,
) -> Result<McpStatusView, IpcError> {
    let handle = &state.mcp;
    if enabled {
        handle.start(state.app_handle.clone())?;
    } else {
        handle.stop();
    }
    status_view(handle)
}

#[tauri::command]
#[tracing::instrument(skip(state))]
pub async fn mcp_regenerate_token(
    state: tauri::State<'_, AppState>,
) -> Result<McpStatusView, IpcError> {
    state.mcp.regenerate_token()?;
    status_view(&state.mcp)
}

#[tauri::command]
#[tracing::instrument(skip(state))]
pub async fn mcp_tools_list(
    state: tauri::State<'_, AppState>,
) -> Result<Vec<McpToolView>, IpcError> {
    Ok(crate::mcp::tools::catalog()
        .iter()
        .map(|tool| {
            let default = match tool.risk {
                crate::mcp::tools::ToolRisk::Destructive => Policy::Confirm,
                _ => Policy::Allow,
            };
            McpToolView {
                name: tool.name.to_string(),
                description: tool.description.to_string(),
                risk: match tool.risk {
                    crate::mcp::tools::ToolRisk::ReadOnly => "read_only",
                    crate::mcp::tools::ToolRisk::Operate => "operate",
                    crate::mcp::tools::ToolRisk::Destructive => "destructive",
                }
                .to_string(),
                default_policy: default,
                override_policy: state.mcp.policy_override(tool.name),
            }
        })
        .collect())
}

#[tauri::command]
#[tracing::instrument(skip(state))]
pub async fn mcp_tools_set(
    state: tauri::State<'_, AppState>,
    tool: String,
    policy: Option<String>,
) -> Result<(), IpcError> {
    if crate::mcp::tools::catalog().iter().all(|t| t.name != tool) {
        return Err(validation_error(format!("unknown tool: {tool}")));
    }
    let parsed = match policy.as_deref() {
        None | Some("default") => None,
        Some(text) => Some(Policy::parse(text).ok_or_else(|| {
            validation_error(format!(
                "invalid policy '{text}' (allow|confirm|deny|default)"
            ))
        })?),
    };
    state
        .mcp
        .set_policy_override(&tool, parsed)
        .map_err(IpcError::from)
}

#[tauri::command]
#[tracing::instrument(skip(state))]
pub async fn mcp_sessions_disconnect(
    state: tauri::State<'_, AppState>,
    session_id: Option<String>,
) -> Result<usize, IpcError> {
    Ok(match session_id {
        Some(id) => usize::from(state.mcp.drop_session(&id)),
        None => state.mcp.drop_all_sessions(),
    })
}

#[tauri::command]
#[tracing::instrument(skip(state))]
pub async fn mcp_confirmations_list(
    state: tauri::State<'_, AppState>,
) -> Result<Vec<McpConfirmationView>, IpcError> {
    Ok(state
        .mcp
        .pending_confirmations()
        .into_iter()
        .map(|(id, tool, snapshot)| McpConfirmationView {
            id,
            tool: tool.to_string(),
            session_id: snapshot.session_id.clone(),
            status: McpHandle::snapshot_status_name(snapshot.status).to_string(),
            payload: snapshot.payload,
            created_at: snapshot.created_at,
        })
        .collect())
}

#[tauri::command]
#[tracing::instrument(skip(state))]
pub async fn mcp_confirmation_resolve(
    state: tauri::State<'_, AppState>,
    id: String,
    allow: bool,
) -> Result<bool, IpcError> {
    Ok(state.mcp.resolve_confirmation(&id, allow))
}

#[tauri::command]
#[tracing::instrument(skip(state))]
pub async fn mcp_recent_calls(
    state: tauri::State<'_, AppState>,
    limit: Option<u32>,
) -> Result<Vec<McpRecentCallView>, IpcError> {
    let limit = limit.unwrap_or(50).min(500) as usize;
    Ok(state
        .mcp
        .read_audit(limit)
        .map_err(IpcError::from)?
        .into_iter()
        .map(|record| McpRecentCallView {
            ts: record.ts,
            session_id: record.session_id,
            tool_id: record.tool_id,
            risk_level: record.risk_level.as_str().to_string(),
            input: record.input,
            outcome: serde_json::to_value(record.outcome)
                .ok()
                .and_then(|v| v.as_str().map(str::to_string))
                .unwrap_or_else(|| "unknown".into()),
            duration_ms: record.duration_ms,
            error: record.error,
        })
        .collect())
}
