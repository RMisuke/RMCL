//! MCP JSON-RPC 2.0 协议层。
//!
//! 支持方法：`initialize` / `ping` / `tools/list` / `tools/call`
//! （客户端通知 `notifications/*` 一律 202 吞掉）。
//!
//! `tools/call` 的执行顺序（设计 §5.2/§5.3/§5.5）：
//! 会话限速 → 慢工具并发闸 → 策略门禁（deny 拒绝 / confirm 走确认流）→
//! 审计（破坏性 fail-closed）→ 执行 → 包装为 MCP content。

use crate::mcp::tools::{self, ToolRisk};
use crate::mcp::{McpHandle, Policy, CONFIRMATION_TIMEOUT, SLOW_TOOLS};
use rmccc_agent::audit::{AuditOutcome, AuditRecord, RiskLevel};
use serde_json::{json, Value};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// 服务器声明支持的协议版本（Streamable HTTP / JSON 模式）。
pub const PROTOCOL_VERSION: &str = "2025-06-18";
const KNOWN_VERSIONS: &[&str] = &["2024-11-05", "2025-03-26", "2025-06-18"];

/// JSON-RPC 错误对象。
#[derive(Debug, Clone)]
pub struct RpcError {
    pub code: i64,
    pub message: String,
}

impl RpcError {
    pub fn new(code: i64, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }
    pub fn parse_error() -> Self {
        Self::new(-32700, "parse error: body is not valid JSON-RPC 2.0")
    }
    pub fn invalid_request(message: impl Into<String>) -> Self {
        Self::new(-32600, message)
    }
    pub fn method_not_found(method: &str) -> Self {
        Self::new(-32601, format!("method not found: {method}"))
    }
    pub fn invalid_params(message: impl Into<String>) -> Self {
        Self::new(-32602, message)
    }
    pub fn internal(message: impl Into<String>) -> Self {
        Self::new(-32603, message)
    }
    pub fn from_message(message: impl Into<String>) -> Self {
        Self::new(-32000, message)
    }
}

/// 一条入站消息（请求带 id；通知不带）。
pub enum Incoming {
    Request {
        id: Value,
        method: String,
        params: Value,
    },
    Notification,
}

/// 解析请求体。多个/零个根对象、形状不对都算 parse/invalid request。
pub fn parse(body: &[u8]) -> Result<Incoming, RpcError> {
    let value: Value = serde_json::from_slice(body).map_err(|_| RpcError::parse_error())?;
    let method = value
        .get("method")
        .and_then(Value::as_str)
        .ok_or_else(|| RpcError::invalid_request("missing method"))?
        .to_string();
    match value.get("id") {
        Some(id) if !id.is_null() => Ok(Incoming::Request {
            id: id.clone(),
            method,
            params: value.get("params").cloned().unwrap_or(Value::Null),
        }),
        // id 为 null 或缺失 = 通知（MCP 通知不允许响应）。
        _ => Ok(Incoming::Notification),
    }
}

/// 请求处理上下文。会话 id 已由 HTTP 层从 `Mcp-Session-Id` 头提取。
pub struct Ctx {
    pub handle: Arc<McpHandle>,
    pub app: tauri::AppHandle,
    pub session_id: Option<String>,
}

/// 处理一条请求。Ok 的第二项是 `initialize` 新建的会话 id，
/// HTTP 层用它设置 `Mcp-Session-Id` 响应头。
pub async fn handle(
    ctx: &Ctx,
    method: &str,
    params: &Value,
) -> Result<(Value, Option<String>), RpcError> {
    match method {
        "initialize" => initialize(ctx, params).map(|(v, id)| (v, Some(id))),
        "ping" => {
            require_session(ctx)?;
            Ok((json!({}), None))
        }
        "tools/list" => {
            require_session(ctx)?;
            Ok((json!({ "tools": tool_descriptors() }), None))
        }
        "tools/call" => {
            require_session(ctx)?;
            call_tool(ctx, params).await.map(|v| (v, None))
        }
        other => Err(RpcError::method_not_found(other)),
    }
}

fn require_session(ctx: &Ctx) -> Result<(), RpcError> {
    match ctx.session_id.as_deref() {
        Some(id) if !id.is_empty() => Ok(()),
        _ => Err(RpcError::from_message(
            "missing Mcp-Session-Id header (call initialize first)",
        )),
    }
}

fn initialize(ctx: &Ctx, params: &Value) -> Result<(Value, String), RpcError> {
    let client = params
        .get("clientInfo")
        .and_then(|c| c.get("name"))
        .and_then(Value::as_str)
        .unwrap_or("unknown")
        .to_string();
    let requested = params
        .get("protocolVersion")
        .and_then(Value::as_str)
        .unwrap_or(PROTOCOL_VERSION);
    let agreed = if KNOWN_VERSIONS.contains(&requested) {
        requested
    } else {
        PROTOCOL_VERSION
    };

    // 会话 id：限定字符集（复用 TaskSnapshot 的路径安全规则）。
    let session_id = format!("mcp-{}", uuid::Uuid::new_v4().simple());
    ctx.handle
        .register_session(session_id.clone(), client.clone())
        .map_err(|e| RpcError::from_message(e.to_string()))?;
    tracing::info!(session = %session_id, client = %client, "MCP session initialized");

    Ok((
        json!({
            "protocolVersion": agreed,
            "capabilities": { "tools": { "listChanged": false } },
            "serverInfo": { "name": "rmccc", "version": env!("CARGO_PKG_VERSION") },
        }),
        session_id,
    ))
}

fn tool_descriptors() -> Vec<Value> {
    tools::catalog()
        .iter()
        .map(|tool| {
            json!({
                "name": tool.name,
                "description": tool.description,
                "inputSchema": tool.schema,
            })
        })
        .collect()
}

/// `tools/call`：参数 `{name, arguments?}`；成功返回 MCP content 形态。
async fn call_tool(ctx: &Ctx, params: &Value) -> Result<Value, RpcError> {
    let name = params
        .get("name")
        .and_then(Value::as_str)
        .ok_or_else(|| RpcError::invalid_params("tools/call requires name"))?
        .to_string();
    let args = params
        .get("arguments")
        .cloned()
        .unwrap_or_else(|| json!({}));

    let tool = tools::catalog()
        .iter()
        .find(|t| t.name == name)
        .ok_or_else(|| RpcError::invalid_params(format!("unknown tool: {name}")))?;
    let session_id = ctx.session_id.clone().unwrap_or_else(|| "unknown".into());

    // ── 限速 ──
    ctx.handle
        .check_rate_limit(&session_id)
        .map_err(|e| RpcError::from_message(e.to_string()))?;

    // ── 策略门禁 ──
    let policy = ctx.handle.effective_policy(tool);
    if policy == Policy::Deny {
        audit(
            ctx,
            tool,
            RiskLevel::from(tool.risk),
            &args,
            AuditOutcome::Denied,
            Duration::ZERO,
            Some("denied by policy"),
        )?;
        return Ok(denied_content(
            "denied",
            &format!("工具 {name} 已被用户设置为拒绝（deny）"),
        ));
    }

    // ── 慢工具并发闸 ──
    let _permit = if SLOW_TOOLS.contains(&tool.name) {
        Some(
            ctx.handle
                .slow_tool_permit
                .clone()
                .acquire_owned()
                .await
                .map_err(|_| RpcError::internal("slow-tool semaphore closed"))?,
        )
    } else {
        None
    };

    // ── 确认流（破坏性默认，或被覆盖为 confirm）──
    let approved = if policy == Policy::Confirm {
        let (id, _snapshot, rx) = ctx
            .handle
            .propose_confirmation(tool.name, &args, &session_id)
            .map_err(|e| RpcError::internal(e.to_string()))?;
        emit_call(ctx, &session_id, tool.name, "started", true);
        tracing::info!(confirmation = %id, tool = %tool.name, "awaiting user confirmation");
        match tokio::time::timeout(CONFIRMATION_TIMEOUT, rx).await {
            Ok(Ok(true)) => true,
            Ok(Ok(false)) => false,
            Ok(Err(_)) | Err(_) => false, // 超时或队列清理 = 拒绝
        }
    } else {
        true
    };

    if !approved {
        audit(
            ctx,
            tool,
            RiskLevel::from(tool.risk),
            &args,
            AuditOutcome::Denied,
            Duration::ZERO,
            Some("user denied"),
        )?;
        return Ok(denied_content(
            "denied",
            "用户在 GUI 拒绝了该操作（或确认超时）",
        ));
    }

    // ── 审计先写（破坏性 fail-closed：写不进去就不执行）──
    let started = Instant::now();
    let risk = RiskLevel::from(tool.risk);
    pre_audit(ctx, tool, risk, &args)?;

    // ── 执行 ──
    if tool.risk != ToolRisk::ReadOnly {
        emit_call(ctx, &session_id, tool.name, "started", true);
    }
    let outcome = tools::call(&ctx.app, tool.name, args.clone()).await;
    let duration = started.elapsed();

    match outcome {
        Ok(value) => {
            audit(ctx, tool, risk, &args, AuditOutcome::Ok, duration, None)?;
            if tool.risk != ToolRisk::ReadOnly {
                emit_call(ctx, &session_id, tool.name, "finished", true);
            }
            Ok(content(&value))
        }
        Err(error) => {
            audit(
                ctx,
                tool,
                risk,
                &args,
                AuditOutcome::Error,
                duration,
                Some(error.message()),
            )?;
            if tool.risk != ToolRisk::ReadOnly {
                emit_call(ctx, &session_id, tool.name, "finished", false);
            }
            // 工具级失败：isError=true 的成功响应（MCP 约定），不是协议错误。
            Ok(json!({
                "content": [{ "type": "text", "text": error.message() }],
                "isError": true,
            }))
        }
    }
}

/// 操作类工具的归因事件（GUI toast / 徽标的数据源）。
fn emit_call(ctx: &Ctx, session_id: &str, tool: &str, kind: &str, ok: bool) {
    use tauri::Emitter as _;
    let _ = ctx.app.emit(
        crate::events::MCP_CALL,
        &crate::events::McpCallPayload {
            kind: kind.to_string(),
            session_id: session_id.to_string(),
            tool: tool.to_string(),
            ok,
        },
    );
}

fn content(value: &Value) -> Value {
    let text = serde_json::to_string_pretty(value).unwrap_or_else(|_| value.to_string());
    json!({ "content": [{ "type": "text", "text": text }] })
}

/// 拒绝/超时的结构化返回（调用方可编程区分，不只是 isError 文本）。
fn denied_content(status: &str, reason: &str) -> Value {
    json!({
        "content": [{ "type": "text", "text": reason }],
        "isError": true,
        "structuredContent": { "status": status },
    })
}

/// 审计（失败返回错误：结果不被伪造；破坏性操作依赖先行的 fail-closed 检查）。
fn audit(
    ctx: &Ctx,
    tool: &tools::ToolSpec,
    risk: RiskLevel,
    args: &Value,
    outcome: AuditOutcome,
    duration: Duration,
    error: Option<&str>,
) -> Result<(), RpcError> {
    let record = AuditRecord::new(
        ctx.session_id.clone().unwrap_or_else(|| "unknown".into()),
        tool.name,
        risk,
        args,
        outcome,
        duration,
        error,
    );
    ctx.handle
        .audit()
        .append(&record)
        .map_err(|e| RpcError::internal(format!("audit write failed: {e}")))
}

/// 破坏性操作在执行前先行落一条审计：写不进去就不执行（fail-closed）。
fn pre_audit(
    ctx: &Ctx,
    tool: &tools::ToolSpec,
    risk: RiskLevel,
    args: &Value,
) -> Result<(), RpcError> {
    if tool.risk != ToolRisk::Destructive {
        return Ok(());
    }
    let record = AuditRecord::new(
        ctx.session_id.clone().unwrap_or_else(|| "unknown".into()),
        tool.name,
        risk,
        args,
        AuditOutcome::Ok, // 终态由执行后的记录补写；此条为"已放行"痕迹
        Duration::ZERO,
        None,
    );
    ctx.handle
        .audit()
        .append(&record)
        .map_err(|e| RpcError::internal(format!("audit write failed (fail-closed): {e}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_requests_and_notifications() {
        let req = parse(br#"{"jsonrpc":"2.0","id":1,"method":"ping"}"#).unwrap();
        assert!(matches!(req, Incoming::Request { .. }));

        let note = parse(br#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#).unwrap();
        assert!(matches!(note, Incoming::Notification));

        // id:null 同样按通知处理。
        let null_id = parse(br#"{"jsonrpc":"2.0","id":null,"method":"ping"}"#).unwrap();
        assert!(matches!(null_id, Incoming::Notification));

        assert!(parse(br#"not json"#).is_err());
        assert!(parse(br#"{"id":1}"#).is_err());
    }

    #[test]
    fn catalog_covers_31_tools_with_object_schemas() {
        let catalog = tools::catalog();
        assert_eq!(catalog.len(), 31);
        let mut names: Vec<&str> = catalog.iter().map(|t| t.name).collect();
        let unique = names.len();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), unique, "tool names must be unique");
        for tool in tools::catalog() {
            assert_eq!(
                tool.schema["type"], "object",
                "{}: schema must be an object",
                tool.name
            );
            assert!(!tool.description.is_empty());
        }
    }

    #[test]
    fn risk_counts_match_the_design() {
        let catalog = tools::catalog();
        let read = catalog
            .iter()
            .filter(|t| t.risk == ToolRisk::ReadOnly)
            .count();
        let operate = catalog
            .iter()
            .filter(|t| t.risk == ToolRisk::Operate)
            .count();
        let destructive = catalog
            .iter()
            .filter(|t| t.risk == ToolRisk::Destructive)
            .count();
        assert_eq!((read, operate, destructive), (15, 11, 5));
    }

    #[test]
    fn envelope_helpers_shape() {
        let body = content(&json!({"a": 1}));
        assert_eq!(body["content"][0]["type"], "text");
        assert!(body["content"][0]["text"]
            .as_str()
            .unwrap()
            .contains("\"a\": 1"));
        let denied = denied_content("denied", "no");
        assert_eq!(denied["structuredContent"]["status"], "denied");
        assert_eq!(denied["isError"], true);
    }
}
