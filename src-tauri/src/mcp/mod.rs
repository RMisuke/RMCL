//! MCP 服务器（ADR-013 S4/S5）：把启动器能力以 31 个工具开放给外部 Agent。
//!
//! - 传输：Streamable HTTP（JSON 模式），仅回环 + Bearer Token（见 [`http`]）；
//! - 工具：只读 15 / 操作 11 / 破坏性 5（目录见 [`tools`]）；
//! - 确认流：破坏性工具复用 [`rmccc_agent::confirm::TaskSnapshot`] 两阶段语义，
//!   GUI 经 `mcp_confirmation_resolve` 放行/拒绝，120s 超时即拒绝；
//! - 归因：每次 `tools/call` 写审计（`{data}/agent/audit.jsonl`，ADR-010 脱敏规则），
//!   操作类工具额外发 `mcp://call` 事件供 GUI 呈现；
//! - 默认**关闭**：`mcp_set_enabled(true)` 才监听；总开关关闭即断开全部会话。

pub mod http;
pub mod protocol;
pub mod tools;

use crate::events;
use rmccc_agent::audit::{AuditLog, AuditRecord};
use rmccc_agent::confirm::{TaskSnapshot, TaskStatus};
use rmccc_core::rmccc_platform::PlatformFs;
use rmccc_error::{Result, RmcccError};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tauri::Emitter as _;
use tokio::sync::watch;
use ts_rs::TS;

/// 令牌文件：`<data>/mcp-token`（64 位 hex；0600）。
pub const TOKEN_FILE: &str = "mcp-token";
/// 端点文件：`<data>/mcp-endpoint`（stdio 桥读取；`http://127.0.0.1:<port>/mcp`）。
pub const ENDPOINT_FILE: &str = "mcp-endpoint";
/// 工具策略覆盖：`<data>/mcp-policies.json`。
pub const POLICIES_FILE: &str = "mcp-policies.json";

/// 生成 64 hex 字符的 Bearer Token（4 × UUID v4，去除连字符）。
fn new_token() -> String {
    (0..4)
        .map(|_| uuid::Uuid::new_v4().simple().to_string())
        .collect()
}

/// 会话并发上限。
const MAX_SESSIONS: usize = 4;
/// 每会话调用限速（次/分钟）。
const CALLS_PER_MINUTE: usize = 60;
/// 破坏性操作等待用户确认的超时。
pub const CONFIRMATION_TIMEOUT: Duration = Duration::from_secs(120);
/// 慢工具（`instance.check_completion`）全局并发 1。
const SLOW_TOOLS: &[&str] = &["instance.check_completion"];

/// 每个工具的执行策略。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
#[ts(export_to = "types.ts")]
pub enum Policy {
    /// 直接执行。
    Allow,
    /// 必须经 GUI 确认卡放行（破坏性工具默认）。
    Confirm,
    /// 拒绝执行。
    Deny,
}

impl Policy {
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "allow" => Some(Policy::Allow),
            "confirm" => Some(Policy::Confirm),
            "deny" => Some(Policy::Deny),
            _ => None,
        }
    }
}

/// 一条待确认的破坏性调用。
///
/// `snapshot` 是脱敏后的展示载荷（确认卡/IPC 视图用）；`args` 是原始参数，
/// 仅在用户放行后交给执行器，二者不同盘。
pub struct PendingConfirmation {
    pub snapshot: TaskSnapshot,
    pub tool: &'static str,
    pub session_id: String,
    created: Instant,
    resolve: tokio::sync::oneshot::Sender<bool>,
}

/// 一条在线会话（限速状态内嵌）。
pub struct Session {
    pub client: String,
    pub connected_at: String,
    calls: VecDeque<Instant>,
}

/// 服务器运行态（`enabled = true` 时存在）。
struct Running {
    port: u16,
    token: String,
    shutdown: watch::Sender<bool>,
    sessions: HashMap<String, Session>,
}

/// 共享句柄：HTTP 服务、IPC 命令与工具执行共用。
pub struct McpHandle {
    data_dir: PathBuf,
    fs: Arc<dyn PlatformFs>,
    inner: std::sync::Mutex<McpInner>,
    /// Tauri 句柄（事件发射）；AppState::new 构造后立即注入。
    app: std::sync::Mutex<Option<tauri::AppHandle>>,
    /// 慢工具并发闸（`instance.check_completion` 并发 1）。
    pub slow_tool_permit: Arc<tokio::sync::Semaphore>,
    audit: AuditLog,
}

struct McpInner {
    running: Option<Running>,
    policies: HashMap<String, Policy>,
    confirmations: HashMap<String, PendingConfirmation>,
}

impl McpHandle {
    pub fn new(data_dir: &Path, fs: Arc<dyn PlatformFs>) -> Result<Self> {
        let audit = AuditLog::open(data_dir.join("agent").join("audit.jsonl"), fs.clone())?;
        let policies = Self::load_policies(&data_dir.join(POLICIES_FILE));
        Ok(Self {
            data_dir: data_dir.to_path_buf(),
            fs,
            inner: std::sync::Mutex::new(McpInner {
                running: None,
                policies,
                confirmations: HashMap::new(),
            }),
            app: std::sync::Mutex::new(None),
            slow_tool_permit: Arc::new(tokio::sync::Semaphore::new(1)),
            audit,
        })
    }

    /// 注入 Tauri 句柄（AppState::new 在拿到 app_handle 后调用）。
    pub fn set_app(&self, app: tauri::AppHandle) {
        *self.app.lock().unwrap_or_else(|p| p.into_inner()) = Some(app);
    }

    fn load_policies(path: &Path) -> HashMap<String, Policy> {
        let Ok(text) = std::fs::read_to_string(path) else {
            return HashMap::new();
        };
        serde_json::from_str::<HashMap<String, Policy>>(&text).unwrap_or_default()
    }

    fn save_policies(&self, policies: &HashMap<String, Policy>) -> Result<()> {
        let path = self.data_dir.join(POLICIES_FILE);
        let text = serde_json::to_string_pretty(policies).unwrap_or_else(|_| "{}".into());
        std::fs::write(&path, text)?;
        self.fs.restrict_owner_only(&path)?;
        Ok(())
    }

    // ── 生命周期 ──

    pub fn is_enabled(&self) -> bool {
        self.inner
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .running
            .is_some()
    }

    pub fn port(&self) -> Option<u16> {
        self.inner
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .running
            .as_ref()
            .map(|r| r.port)
    }

    /// 当前 Bearer Token（设置页展示/复制用；只经 IPC 给本机用户）。
    pub fn token(&self) -> Result<String> {
        let path = self.data_dir.join(TOKEN_FILE);
        if let Ok(text) = std::fs::read_to_string(&path) {
            let token = text.trim().to_string();
            if !token.is_empty() {
                return Ok(token);
            }
        }
        self.regenerate_token()
    }

    /// 重新生成令牌并断开全部会话（服务器保持运行；旧客户端立即 401）。
    pub fn regenerate_token(&self) -> Result<String> {
        let token = new_token();
        let path = self.data_dir.join(TOKEN_FILE);
        std::fs::write(&path, &token)?;
        self.fs.restrict_owner_only(&path)?;
        let mut inner = self.inner.lock().unwrap_or_else(|p| p.into_inner());
        if let Some(running) = inner.running.as_mut() {
            running.sessions.clear();
            running.token = token.clone();
        }
        Ok(token)
    }

    /// 启动 HTTP 监听（幂等：已开启则返回当前端口）。
    pub fn start(self: &Arc<Self>, app: tauri::AppHandle) -> Result<u16> {
        let mut inner = self.inner.lock().unwrap_or_else(|p| p.into_inner());
        if let Some(running) = inner.running.as_ref() {
            return Ok(running.port);
        }
        let token = self.token()?;
        // 127.0.0.1:0 —— 内核分配空闲端口（设计 §5.1）。
        let listener = std::net::TcpListener::bind(("127.0.0.1", 0))?;
        listener.set_nonblocking(true)?;
        let port = listener.local_addr()?.port();

        let (shutdown_tx, shutdown_rx) = watch::channel(false);
        let handle = Arc::clone(self);
        let app_for_task = app.clone();
        let _task = tauri::async_runtime::spawn(async move {
            http::serve(listener, handle, app_for_task, shutdown_rx).await;
        });

        let endpoint = format!("http://127.0.0.1:{port}/mcp");
        std::fs::write(self.data_dir.join(ENDPOINT_FILE), &endpoint)?;
        self.fs
            .restrict_owner_only(&self.data_dir.join(ENDPOINT_FILE))?;

        inner.running = Some(Running {
            port,
            token,
            shutdown: shutdown_tx,
            sessions: HashMap::new(),
        });
        tracing::info!(port, "MCP server listening on loopback");
        Ok(port)
    }

    /// 停止监听并断开全部会话（幂等）。
    pub fn stop(&self) {
        let mut inner = self.inner.lock().unwrap_or_else(|p| p.into_inner());
        if let Some(running) = inner.running.take() {
            let _ = running.shutdown.send(true);
            let _ = std::fs::remove_file(self.data_dir.join(ENDPOINT_FILE));
            tracing::info!(port = running.port, "MCP server stopped");
        }
    }

    // ── 会话 ──

    /// `initialize` 时登记会话；并发超限返回错误。
    pub fn register_session(&self, session_id: String, client: String) -> Result<()> {
        let mut inner = self.inner.lock().unwrap_or_else(|p| p.into_inner());
        let Some(running) = inner.running.as_mut() else {
            return Err(RmcccError::Validation("MCP server is not running".into()));
        };
        if running.sessions.len() >= MAX_SESSIONS {
            return Err(RmcccError::Validation(format!(
                "too many MCP sessions (max {MAX_SESSIONS})"
            )));
        }
        running.sessions.insert(
            session_id.clone(),
            Session {
                client,
                connected_at: chrono::Utc::now().to_rfc3339(),
                calls: VecDeque::new(),
            },
        );
        drop(inner);
        self.emit_session(&session_id, "connected");
        Ok(())
    }

    /// 限速检查（每会话 60 次/分钟）+ 活跃心跳。
    pub fn check_rate_limit(&self, session_id: &str) -> Result<()> {
        let mut inner = self.inner.lock().unwrap_or_else(|p| p.into_inner());
        let Some(running) = inner.running.as_mut() else {
            return Err(RmcccError::Validation("MCP server is not running".into()));
        };
        let Some(session) = running.sessions.get_mut(session_id) else {
            return Err(RmcccError::Validation(
                "unknown MCP session (initialize first)".into(),
            ));
        };
        let now = Instant::now();
        while session
            .calls
            .front()
            .is_some_and(|t| now.duration_since(*t) > Duration::from_secs(60))
        {
            session.calls.pop_front();
        }
        if session.calls.len() >= CALLS_PER_MINUTE {
            return Err(RmcccError::Validation(format!(
                "rate limited: max {CALLS_PER_MINUTE} calls/minute per session"
            )));
        }
        session.calls.push_back(now);
        Ok(())
    }

    pub fn drop_session(&self, session_id: &str) -> bool {
        let mut inner = self.inner.lock().unwrap_or_else(|p| p.into_inner());
        let Some(running) = inner.running.as_mut() else {
            return false;
        };
        let removed = running.sessions.remove(session_id).is_some();
        if removed {
            drop(inner);
            self.emit_session(session_id, "disconnected");
        }
        removed
    }

    pub fn drop_all_sessions(&self) -> usize {
        let mut inner = self.inner.lock().unwrap_or_else(|p| p.into_inner());
        let Some(running) = inner.running.as_mut() else {
            return 0;
        };
        let count = running.sessions.len();
        let ids: Vec<String> = running.sessions.keys().cloned().collect();
        running.sessions.clear();
        drop(inner);
        for id in ids {
            self.emit_session(&id, "disconnected");
        }
        count
    }

    /// 会话视图（client 名，如 "ZCode" / "claude"）。
    pub fn list_sessions(&self) -> Vec<(String, String, String)> {
        let inner = self.inner.lock().unwrap_or_else(|p| p.into_inner());
        let Some(running) = inner.running.as_ref() else {
            return Vec::new();
        };
        let mut rows: Vec<(String, String, String)> = running
            .sessions
            .iter()
            .map(|(id, s)| (id.clone(), s.client.clone(), s.connected_at.clone()))
            .collect();
        rows.sort();
        rows
    }

    fn emit_session(&self, session_id: &str, kind: &str) {
        let client = {
            let inner = self.inner.lock().unwrap_or_else(|p| p.into_inner());
            inner
                .running
                .as_ref()
                .and_then(|r| r.sessions.get(session_id))
                .map(|s| s.client.clone())
                .unwrap_or_default()
        };
        let _ = self.app().emit(
            events::MCP_SESSION,
            &events::McpSessionPayload {
                kind: kind.to_string(),
                session_id: session_id.to_string(),
                client,
            },
        );
    }

    fn app(&self) -> tauri::AppHandle {
        // 会话/确认事件需要在无 AppHandle 引用的方法里发射；
        // 句柄在 AppState::new 时注入（见 `set_app`）。
        self.app
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .clone()
            .expect("McpHandle app handle must be set before serving")
    }

    // ── 策略 ──

    /// 生效策略 = 覆盖值 ?? 风险默认（只读 allow / 操作 allow / 破坏性 confirm）。
    pub fn effective_policy(&self, tool: &tools::ToolSpec) -> Policy {
        let inner = self.inner.lock().unwrap_or_else(|p| p.into_inner());
        inner
            .policies
            .get(tool.name)
            .copied()
            .unwrap_or(match tool.risk {
                tools::ToolRisk::Destructive => Policy::Confirm,
                _ => Policy::Allow,
            })
    }

    pub fn policy_override(&self, tool: &str) -> Option<Policy> {
        let inner = self.inner.lock().unwrap_or_else(|p| p.into_inner());
        inner.policies.get(tool).copied()
    }

    pub fn set_policy_override(&self, tool: &str, policy: Option<Policy>) -> Result<()> {
        let mut inner = self.inner.lock().unwrap_or_else(|p| p.into_inner());
        match policy {
            Some(p) => {
                inner.policies.insert(tool.to_string(), p);
            }
            None => {
                inner.policies.remove(tool);
            }
        }
        drop(inner);
        self.save_policies(&{
            let inner = self.inner.lock().unwrap_or_else(|p| p.into_inner());
            inner.policies.clone()
        })
    }

    // ── 确认流（破坏性工具两阶段）──

    /// 创建确认记录并返回接收端（协议层等待放行）。同时发 `mcp://confirmation`。
    pub fn propose_confirmation(
        &self,
        tool: &'static str,
        args: &serde_json::Value,
        session_id: &str,
    ) -> Result<(String, TaskSnapshot, tokio::sync::oneshot::Receiver<bool>)> {
        let id = format!("cfm-{}", uuid::Uuid::new_v4().simple());
        let snapshot = TaskSnapshot::proposed(
            id.clone(),
            Some(session_id.to_string()),
            format!("mcp:{tool}"),
            args,
        )?;
        let (tx, rx) = tokio::sync::oneshot::channel();
        {
            let mut inner = self.inner.lock().unwrap_or_else(|p| p.into_inner());
            inner.confirmations.insert(
                id.clone(),
                PendingConfirmation {
                    snapshot: snapshot.clone(),
                    tool,
                    session_id: session_id.to_string(),
                    created: Instant::now(),
                    resolve: tx,
                },
            );
        }
        let _ = self.app().emit(
            events::MCP_CONFIRMATION,
            &events::McpConfirmationPayload {
                kind: "requested".into(),
                confirmation_id: id,
                tool: tool.to_string(),
                session_id: Some(session_id.to_string()),
            },
        );
        Ok((snapshot.task_id.clone(), snapshot, rx))
    }

    /// 用户在 GUI 放行/拒绝；返回是否存在该记录。
    pub fn resolve_confirmation(&self, id: &str, allow: bool) -> bool {
        let (tool, session_id) = {
            let mut inner = self.inner.lock().unwrap_or_else(|p| p.into_inner());
            match inner.confirmations.remove(id) {
                Some(pending) => {
                    let _ = pending.resolve.send(allow);
                    (pending.tool, pending.session_id)
                }
                None => return false,
            }
        };
        let _ = self.app().emit(
            events::MCP_CONFIRMATION,
            &events::McpConfirmationPayload {
                kind: "resolved".into(),
                confirmation_id: id.to_string(),
                tool: tool.to_string(),
                session_id: Some(session_id),
            },
        );
        true
    }

    /// 超时清理：把到期的确认从队列摘除（等待方收到关闭即视为拒绝）。
    pub fn expire_confirmations(&self) {
        let mut inner = self.inner.lock().unwrap_or_else(|p| p.into_inner());
        let expired: Vec<String> = inner
            .confirmations
            .iter()
            .filter(|(_, c)| c.created.elapsed() > CONFIRMATION_TIMEOUT)
            .map(|(id, _)| id.clone())
            .collect();
        for id in expired {
            if let Some(pending) = inner.confirmations.remove(&id) {
                // drop(sender) —— 等待方 recv() 得 Err，按拒绝处理。
                drop(pending.resolve);
            }
        }
    }

    pub fn pending_confirmations(&self) -> Vec<(String, &'static str, TaskSnapshot)> {
        let inner = self.inner.lock().unwrap_or_else(|p| p.into_inner());
        inner
            .confirmations
            .iter()
            .map(|(id, c)| (id.clone(), c.tool, c.snapshot.clone()))
            .collect()
    }

    /// 确认放行后回写快照终态（审计视图用；快照不落盘，仅审计记录）。
    pub fn snapshot_status_name(status: TaskStatus) -> &'static str {
        match status {
            TaskStatus::Proposed => "proposed",
            TaskStatus::Approved => "approved",
            TaskStatus::Rejected => "rejected",
            TaskStatus::Applied => "applied",
            TaskStatus::Failed => "failed",
        }
    }

    // ── 审计 ──

    pub fn audit(&self) -> &AuditLog {
        &self.audit
    }

    pub fn read_audit(&self, limit: usize) -> Result<Vec<AuditRecord>> {
        let mut records = self.audit.read_all()?;
        let start = records.len().saturating_sub(limit);
        Ok(records.split_off(start))
    }
}
