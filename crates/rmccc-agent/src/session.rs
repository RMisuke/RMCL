//! Agent 会话事件流与任务快照（Phase 6.3，ADR-010 决策 1/2/6）。
//!
//! ```text
//! <data_dir>/agent/
//!   sessions/<session-id>.jsonl   ← 一行一个 SessionEvent
//!   tasks/<task-id>.json          ← 待确认的计划（ModInstallPlan 等）
//!   audit.jsonl                   ← 工具调用审计（crate::audit，跨会话）
//! ```
//!
//! # 不变量
//!
//! 1. **白名单事件**：[`SessionEventKind`] 就是全部可落盘的事件；没有 `raw_prompt` 字段。
//!    提示词/模型输出只落 `sha256` + 字符数，正文默认不写盘（ADR-010 决策 2）。
//! 2. **写盘前脱敏**：工具输入经 [`crate::redact`] 处理，错误摘要同样脱敏并截断，
//!    磁盘上不应存在凭据原文。
//! 3. **id 不可信**：session id / task id 可能来自模型或 UI，必须先过
//!    [`validate_id`]（只允许 `[A-Za-z0-9_-]`、≤64 字符），否则 `../../x` 会越出目录。
//! 4. **可裁剪**：[`SessionStore::purge_expired`] 按文件 mtime 裁剪，默认保留
//!    [`DEFAULT_RETENTION_DAYS`] 天；[`SessionStore::clear_all`] 是"清除 Agent 历史"。
//! 5. 会话/任务文件创建后经 `PlatformFs::restrict_owner_only` 收紧（Windows/Linux 为
//!    `UnsupportedPlatform`，记 deferred）。

use crate::audit::AuditLog;
use crate::redact::{redact_json, redact_text};
use crate::tools::{RiskLevel, ToolResult};
use rmccc_error::{Result, RmcccError};
use rmccc_platform::PlatformFs;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::io::{ErrorKind, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, SystemTime};

/// `agent/` 目录名（`{data_dir}/agent`）。
pub const AGENT_DIR: &str = "agent";
/// 会话事件目录名。
pub const SESSIONS_DIR: &str = "sessions";
/// 任务快照目录名。
pub const TASKS_DIR: &str = "tasks";
/// 审计文件名。
pub const AUDIT_FILE: &str = "audit.jsonl";
/// 默认保留天数（ADR-010 决策 6）。
pub const DEFAULT_RETENTION_DAYS: u64 = 30;

/// id 最大长度。UUID v4 为 36 字符，留出人类可读 id 的余量。
const MAX_ID_LEN: usize = 64;
/// 错误摘要截断长度（字符数）。
const MAX_ERROR_CHARS: usize = 512;
/// 审批摘要截断长度（字符数）。
const MAX_SUMMARY_CHARS: usize = 240;

/// 会话事件白名单（ADR-010 决策 2）。
///
/// 序列化为 `{"kind": "tool_call", ...}`；**新增变体等于改协议**，需先更新 ADR-010。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SessionEventKind {
    /// 会话开始。只记录非敏感的编排元数据。
    SessionStarted {
        provider: String,
        model: String,
        profile_id: Option<String>,
    },
    /// 用户消息：**只有哈希与长度**。
    UserMessage {
        text_sha256: String,
        text_chars: usize,
    },
    /// 模型输出：**只有哈希与长度**。
    ModelResponse {
        text_sha256: String,
        text_chars: usize,
    },
    /// 工具调用请求（输入已脱敏）。
    ToolCall {
        tool_id: String,
        risk_level: RiskLevel,
        input: Value,
    },
    /// 工具结果：只记录成功与否、耗时、输出哈希与字节数。
    ToolResult {
        tool_id: String,
        ok: bool,
        duration_ms: u64,
        output_sha256: String,
        output_bytes: usize,
    },
    /// 请求用户确认（摘要已脱敏并截断）。
    ApprovalRequested {
        task_id: String,
        tool_id: String,
        risk_level: RiskLevel,
        summary: String,
    },
    /// 用户对确认请求的答复。
    ApprovalResolved {
        task_id: String,
        approved: bool,
        reason: Option<String>,
    },
    /// 错误（`category` 为机器可读分类；字段名不能叫 `kind`，会与内部 tag 冲突）。
    Error { category: String, message: String },
    /// 会话结束。
    SessionEnded { reason: String, tool_calls: u32 },
}

impl SessionEventKind {
    pub fn session_started(
        provider: impl Into<String>,
        model: impl Into<String>,
        profile_id: Option<String>,
    ) -> Self {
        Self::SessionStarted {
            provider: provider.into(),
            model: model.into(),
            profile_id,
        }
    }

    /// 用户消息：正文不落盘，只留哈希与长度。
    pub fn user_message(text: &str) -> Self {
        Self::UserMessage {
            text_sha256: sha256_hex(text.as_bytes()),
            text_chars: text.chars().count(),
        }
    }

    /// 模型输出：正文不落盘，只留哈希与长度。
    pub fn model_response(text: &str) -> Self {
        Self::ModelResponse {
            text_sha256: sha256_hex(text.as_bytes()),
            text_chars: text.chars().count(),
        }
    }

    /// 工具调用：输入在这里统一脱敏，调用方不需要记得。
    pub fn tool_call(tool_id: impl Into<String>, risk_level: RiskLevel, input: &Value) -> Self {
        Self::ToolCall {
            tool_id: tool_id.into(),
            risk_level,
            input: redact_json(input, crate::redact::user_home().as_deref()),
        }
    }

    /// 工具结果：只留输出的哈希与字节数，**不落输出内容**。
    pub fn tool_result(result: &ToolResult, duration: Duration) -> Self {
        let (output_sha256, output_bytes) = match serde_json::to_vec(&result.output) {
            Ok(bytes) => (sha256_hex(&bytes), bytes.len()),
            Err(_) => (String::new(), 0),
        };
        Self::ToolResult {
            tool_id: result.tool_id.clone(),
            ok: result.ok,
            duration_ms: to_millis(duration),
            output_sha256,
            output_bytes,
        }
    }

    /// 审批请求：摘要脱敏 + 截断，避免把文件内容带进事件流。
    pub fn approval_requested(
        task_id: impl Into<String>,
        tool_id: impl Into<String>,
        risk_level: RiskLevel,
        summary: &str,
    ) -> Self {
        Self::ApprovalRequested {
            task_id: task_id.into(),
            tool_id: tool_id.into(),
            risk_level,
            summary: truncate_chars(
                &redact_text(summary, crate::redact::user_home().as_deref()),
                MAX_SUMMARY_CHARS,
            ),
        }
    }

    pub fn approval_resolved(
        task_id: impl Into<String>,
        approved: bool,
        reason: Option<&str>,
    ) -> Self {
        Self::ApprovalResolved {
            task_id: task_id.into(),
            approved,
            reason: reason.map(|text| {
                truncate_chars(
                    &redact_text(text, crate::redact::user_home().as_deref()),
                    MAX_ERROR_CHARS,
                )
            }),
        }
    }

    /// 错误：脱敏 + 截断。`category` 应为稳定分类（如 `tool_error` / `llm_timeout`）。
    pub fn error(category: impl Into<String>, message: &str) -> Self {
        Self::Error {
            category: category.into(),
            message: truncate_chars(
                &redact_text(message, crate::redact::user_home().as_deref()),
                MAX_ERROR_CHARS,
            ),
        }
    }

    pub fn session_ended(reason: impl Into<String>, tool_calls: u32) -> Self {
        Self::SessionEnded {
            reason: reason.into(),
            tool_calls,
        }
    }
}

/// 一条会话事件：时间戳 + 会话 id + 白名单载荷（扁平序列化，便于 grep）。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SessionEvent {
    /// RFC3339 时间戳（UTC）。
    pub ts: String,
    pub session_id: String,
    #[serde(flatten)]
    pub event: SessionEventKind,
}

/// 会话列表项（不返回事件正文，避免 UI 一次拉全量历史）。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SessionSummary {
    pub session_id: String,
    pub started_at: String,
    pub last_event_at: String,
    pub event_count: usize,
    pub tool_calls: u32,
    pub ended: bool,
}

/// 任务快照状态。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskStatus {
    /// 已生成、等待用户确认（Phase 6.6 的 dry-run 落点）。
    Proposed,
    Approved,
    Rejected,
    Applied,
    Failed,
}

/// 待确认任务的快照（`agent/tasks/<task-id>.json`）。
///
/// `payload` 是结构化计划（例如 `ModInstallPlan` 的 JSON 形态），**写盘前脱敏**。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TaskSnapshot {
    pub task_id: String,
    pub session_id: Option<String>,
    /// 计划类型，如 `mod_install_plan`。
    pub kind: String,
    pub created_at: String,
    pub updated_at: String,
    pub status: TaskStatus,
    pub payload: Value,
}

impl TaskSnapshot {
    /// 新建 `proposed` 快照；id 立即校验（可能来自模型）。
    pub fn proposed(
        task_id: impl Into<String>,
        session_id: Option<String>,
        kind: impl Into<String>,
        payload: &Value,
    ) -> Result<Self> {
        let task_id = task_id.into();
        validate_id("task id", &task_id)?;
        if let Some(session_id) = session_id.as_deref() {
            validate_id("session id", session_id)?;
        }
        let now = chrono::Utc::now().to_rfc3339();
        Ok(Self {
            task_id,
            session_id,
            kind: kind.into(),
            created_at: now.clone(),
            updated_at: now,
            status: TaskStatus::Proposed,
            payload: redact_json(payload, None),
        })
    }

    pub fn with_status(mut self, status: TaskStatus) -> Self {
        self.status = status;
        self.updated_at = chrono::Utc::now().to_rfc3339();
        self
    }
}

/// 裁剪/清除结果。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PurgeReport {
    pub sessions_removed: usize,
    pub tasks_removed: usize,
    pub bytes_freed: u64,
}

impl PurgeReport {
    pub fn total_removed(&self) -> usize {
        self.sessions_removed + self.tasks_removed
    }
}

/// 会话与任务存储。无数据库，纯 JSONL/JSON，崩溃后仍可读。
pub struct SessionStore {
    data_dir: PathBuf,
    fs: Arc<dyn PlatformFs>,
    /// 串行化 JSONL 追加：两个线程交错 write 会产生交错坏行，而读取端
    /// 跳过坏行等于审计/会话记录无声丢失（审计 P2-29）。
    append_lock: std::sync::Mutex<()>,
}

impl SessionStore {
    /// 打开（必要时创建）`<data_dir>/agent/{sessions,tasks}`。
    ///
    /// 目录创建后收紧为仅属主可进入（`0700`，ADR-010 决策 1）——
    /// 此前只有文件是 0600，目录保持 0755 可被同机其他用户列目录名。
    pub fn open(data_dir: impl AsRef<Path>, fs: Arc<dyn PlatformFs>) -> Result<Self> {
        let data_dir = data_dir.as_ref().to_path_buf();
        let store = Self {
            data_dir,
            fs,
            append_lock: std::sync::Mutex::new(()),
        };
        for dir in [store.agent_dir(), store.sessions_dir(), store.tasks_dir()] {
            std::fs::create_dir_all(&dir)?;
            store.fs.restrict_owner_only_dir(&dir)?;
        }
        Ok(store)
    }

    pub fn data_dir(&self) -> &Path {
        &self.data_dir
    }

    pub fn agent_dir(&self) -> PathBuf {
        self.data_dir.join(AGENT_DIR)
    }

    pub fn sessions_dir(&self) -> PathBuf {
        self.agent_dir().join(SESSIONS_DIR)
    }

    pub fn tasks_dir(&self) -> PathBuf {
        self.agent_dir().join(TASKS_DIR)
    }

    pub fn audit_path(&self) -> PathBuf {
        self.agent_dir().join(AUDIT_FILE)
    }

    /// 打开 `agent/audit.jsonl`（与插件审计并列，互不覆盖）。
    pub fn open_audit(&self) -> Result<AuditLog> {
        AuditLog::open(self.audit_path(), self.fs.clone())
    }

    /// 新建会话并写入 `session_started`。
    pub fn create_session(
        &self,
        provider: &str,
        model: &str,
        profile_id: Option<&str>,
    ) -> Result<String> {
        let session_id = uuid::Uuid::new_v4().to_string();
        self.append(
            &session_id,
            SessionEventKind::session_started(
                provider.to_string(),
                model.to_string(),
                profile_id.map(str::to_string),
            ),
        )?;
        Ok(session_id)
    }

    /// 追加一个事件，返回写盘后的记录（含实际时间戳）。
    pub fn append(&self, session_id: &str, event: SessionEventKind) -> Result<SessionEvent> {
        let path = self.session_path(session_id)?;
        let record = SessionEvent {
            ts: chrono::Utc::now().to_rfc3339(),
            session_id: session_id.to_string(),
            event,
        };
        let line = serde_json::to_string(&record)?;
        if line.contains('\n') {
            return Err(RmcccError::Validation(
                "session event must serialize to a single line".into(),
            ));
        }
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let _guard = self
            .append_lock
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)?;
        file.write_all(line.as_bytes())?;
        file.write_all(b"\n")?;
        file.sync_data()?;
        // 新建文件后收紧权限（create 分支不走 open 的显式设置）。
        self.fs.restrict_owner_only(&path)?;
        Ok(record)
    }

    /// 读取会话全部事件；无法解析的行跳过（与审计日志一致：坏行不该让历史不可读）。
    pub fn read(&self, session_id: &str) -> Result<Vec<SessionEvent>> {
        let path = self.session_path(session_id)?;
        let raw = std::fs::read_to_string(&path).map_err(|error| {
            if error.kind() == ErrorKind::NotFound {
                RmcccError::Io(std::io::Error::new(
                    ErrorKind::NotFound,
                    format!("unknown agent session: {session_id}"),
                ))
            } else {
                RmcccError::Io(error)
            }
        })?;
        Ok(raw
            .lines()
            .filter(|line| !line.trim().is_empty())
            .filter_map(|line| serde_json::from_str::<SessionEvent>(line).ok())
            .collect())
    }

    /// 列出会话（按最后事件时间倒序）。损坏到没有任何可解析事件的会话不出现。
    pub fn list(&self) -> Result<Vec<SessionSummary>> {
        let mut summaries = Vec::new();
        for path in self.files_in(&self.sessions_dir(), "jsonl")? {
            let Some(session_id) = path.file_stem().and_then(|stem| stem.to_str()) else {
                continue;
            };
            let Ok(events) = self.read(session_id) else {
                continue;
            };
            let (Some(first), Some(last)) = (events.first(), events.last()) else {
                continue;
            };
            summaries.push(SessionSummary {
                session_id: session_id.to_string(),
                started_at: first.ts.clone(),
                last_event_at: last.ts.clone(),
                event_count: events.len(),
                tool_calls: events
                    .iter()
                    .filter(|event| matches!(event.event, SessionEventKind::ToolCall { .. }))
                    .count()
                    .min(u32::MAX as usize) as u32,
                ended: events
                    .iter()
                    .any(|event| matches!(event.event, SessionEventKind::SessionEnded { .. })),
            });
        }
        summaries.sort_by(|a, b| b.last_event_at.cmp(&a.last_event_at));
        Ok(summaries)
    }

    /// 写入任务快照（原子替换，`payload` 已脱敏）。
    pub fn write_task(&self, snapshot: &TaskSnapshot) -> Result<()> {
        validate_id("task id", &snapshot.task_id)?;
        let path = self.task_path(&snapshot.task_id)?;
        let payload = TaskSnapshot {
            payload: redact_json(&snapshot.payload, crate::redact::user_home().as_deref()),
            ..snapshot.clone()
        };
        let bytes = serde_json::to_vec_pretty(&payload)?;
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, &bytes)?;
        self.fs.restrict_owner_only(&tmp)?;
        self.fs.atomic_replace(&tmp, &path)?;
        Ok(())
    }

    /// 读取任务快照；不存在返回 `Ok(None)`，损坏返回错误（不静默丢弃计划）。
    pub fn read_task(&self, task_id: &str) -> Result<Option<TaskSnapshot>> {
        let path = self.task_path(task_id)?;
        match std::fs::read_to_string(&path) {
            Ok(raw) => Ok(Some(serde_json::from_str(&raw)?)),
            Err(error) if error.kind() == ErrorKind::NotFound => Ok(None),
            Err(error) => Err(RmcccError::Io(error)),
        }
    }

    /// 列出任务快照（按创建时间倒序）。
    pub fn list_tasks(&self) -> Result<Vec<TaskSnapshot>> {
        let mut tasks = Vec::new();
        for path in self.files_in(&self.tasks_dir(), "json")? {
            let Some(task_id) = path.file_stem().and_then(|stem| stem.to_str()) else {
                continue;
            };
            if let Some(task) = self.read_task(task_id)? {
                tasks.push(task);
            }
        }
        tasks.sort_by(|a, b| b.created_at.cmp(&a.created_at));
        Ok(tasks)
    }

    /// 按 mtime 裁剪超过保留期的会话与任务文件（ADR-010 决策 6）。
    pub fn purge_expired(&self, retention: Duration, now: SystemTime) -> Result<PurgeReport> {
        let mut report = PurgeReport::default();
        for (dir, extension, is_session) in [
            (self.sessions_dir(), "jsonl", true),
            (self.tasks_dir(), "json", false),
        ] {
            for path in self.files_in(&dir, extension)? {
                let modified = std::fs::metadata(&path)
                    .and_then(|meta| meta.modified())
                    .unwrap_or(now);
                if !is_expired(modified, now, retention) {
                    continue;
                }
                let size = std::fs::metadata(&path).map(|meta| meta.len()).unwrap_or(0);
                if std::fs::remove_file(&path).is_ok() {
                    report.bytes_freed += size;
                    if is_session {
                        report.sessions_removed += 1;
                    } else {
                        report.tasks_removed += 1;
                    }
                }
            }
        }
        Ok(report)
    }

    /// "清除 Agent 历史"：删除会话与任务文件，**保留审计日志**，并把清除动作
    /// 本身写进审计（ADR-010 决策 6）。审计写不进去则整体失败——销毁性操作
    /// 必须留下痕迹。
    pub fn clear_all(&self) -> Result<PurgeReport> {
        let mut report = PurgeReport::default();
        for (dir, extension, is_session) in [
            (self.sessions_dir(), "jsonl", true),
            (self.tasks_dir(), "json", false),
        ] {
            for path in self.files_in(&dir, extension)? {
                let size = std::fs::metadata(&path).map(|meta| meta.len()).unwrap_or(0);
                if std::fs::remove_file(&path).is_ok() {
                    report.bytes_freed += size;
                    if is_session {
                        report.sessions_removed += 1;
                    } else {
                        report.tasks_removed += 1;
                    }
                }
            }
        }
        let audit = self.open_audit()?;
        let record = crate::audit::AuditRecord::new(
            "system",
            "agent.clear_history",
            crate::tools::RiskLevel::Destructive,
            &serde_json::json!({
                "sessions_removed": report.sessions_removed,
                "tasks_removed": report.tasks_removed,
                "bytes_freed": report.bytes_freed,
            }),
            crate::audit::AuditOutcome::Ok,
            std::time::Duration::from_millis(0),
            None,
        );
        audit.append(&record)?;
        Ok(report)
    }

    fn session_path(&self, session_id: &str) -> Result<PathBuf> {
        validate_id("session id", session_id)?;
        Ok(self.sessions_dir().join(format!("{session_id}.jsonl")))
    }

    fn task_path(&self, task_id: &str) -> Result<PathBuf> {
        validate_id("task id", task_id)?;
        Ok(self.tasks_dir().join(format!("{task_id}.json")))
    }

    /// 列出目录下指定扩展名的文件；目录不存在时返回空列表（首次运行）。
    fn files_in(&self, dir: &Path, extension: &str) -> Result<Vec<PathBuf>> {
        let entries = match std::fs::read_dir(dir) {
            Ok(entries) => entries,
            Err(error) if error.kind() == ErrorKind::NotFound => return Ok(Vec::new()),
            Err(error) => return Err(RmcccError::Io(error)),
        };
        let mut paths = Vec::new();
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().is_some_and(|ext| ext == extension) && path.is_file() {
                paths.push(path);
            }
        }
        paths.sort();
        Ok(paths)
    }
}

/// id 校验：只允许 `[A-Za-z0-9_-]`，长度 1..=[`MAX_ID_LEN`]。
///
/// 这是路径安全的**唯一**保证点：`../`、`/`、`.`、空串全部拒绝。
pub fn validate_id(label: &str, id: &str) -> Result<()> {
    if id.is_empty() || id.len() > MAX_ID_LEN {
        return Err(RmcccError::Validation(format!(
            "{label} must be 1..={MAX_ID_LEN} characters"
        )));
    }
    let valid = id
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_');
    if !valid {
        return Err(RmcccError::Validation(format!(
            "{label} contains disallowed characters: {id}"
        )));
    }
    Ok(())
}

/// 是否超过保留期。`modified` 晚于 `now`（时钟回拨）视为未过期。
pub fn is_expired(modified: SystemTime, now: SystemTime, retention: Duration) -> bool {
    now.duration_since(modified)
        .map(|age| age > retention)
        .unwrap_or(false)
}

fn sha256_hex(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    let digest = Sha256::digest(bytes);
    let mut out = String::with_capacity(digest.len() * 2);
    for byte in digest {
        out.push_str(&format!("{byte:02x}"));
    }
    out
}

/// 按字符边界截断（不切开 UTF-8）。
fn truncate_chars(text: &str, max_chars: usize) -> String {
    match text.char_indices().nth(max_chars) {
        Some((index, _)) => format!("{}…", &text[..index]),
        None => text.to_string(),
    }
}

fn to_millis(duration: Duration) -> u64 {
    duration.as_millis().min(u128::from(u64::MAX)) as u64
}

#[cfg(test)]
mod tests {
    use super::*;
    use rmccc_platform::Platform;
    use serde_json::json;
    use std::time::UNIX_EPOCH;

    fn temp_store(tag: &str) -> (PathBuf, SessionStore) {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let dir = std::env::temp_dir().join(format!(
            "rmccc-agent-session-{tag}-{}-{nanos}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        let store =
            SessionStore::open(&dir, Platform::current().fs.clone()).expect("session store");
        (dir, store)
    }

    #[test]
    fn create_session_writes_started_event() {
        let (dir, store) = temp_store("create");
        let id = store
            .create_session("openai", "gpt-4o-mini", Some("profile-1"))
            .expect("create");
        let events = store.read(&id).expect("read");
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].session_id, id);
        assert!(matches!(
            events[0].event,
            SessionEventKind::SessionStarted { .. }
        ));
        assert!(store.sessions_dir().join(format!("{id}.jsonl")).exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn append_and_read_roundtrip_preserves_order() {
        let (dir, store) = temp_store("roundtrip");
        let id = store.create_session("openai", "m", None).expect("create");
        store
            .append(&id, SessionEventKind::user_message("hi"))
            .expect("append");
        store
            .append(&id, SessionEventKind::model_response("hello"))
            .expect("append");
        store
            .append(&id, SessionEventKind::session_ended("user_closed", 0))
            .expect("append");
        let events = store.read(&id).expect("read");
        assert_eq!(events.len(), 4);
        assert!(matches!(
            events[1].event,
            SessionEventKind::UserMessage { .. }
        ));
        assert!(matches!(
            events[3].event,
            SessionEventKind::SessionEnded { .. }
        ));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn prompt_text_never_reaches_disk() {
        let (dir, store) = temp_store("prompt");
        let id = store.create_session("openai", "m", None).expect("create");
        let secret_prompt = "please install sodium for my private server";
        store
            .append(&id, SessionEventKind::user_message(secret_prompt))
            .expect("append");
        let raw =
            std::fs::read_to_string(store.sessions_dir().join(format!("{id}.jsonl"))).expect("raw");
        assert!(!raw.contains(secret_prompt));
        assert!(!raw.contains("sodium"));
        assert!(raw.contains(&sha256_hex(secret_prompt.as_bytes())));
        assert!(raw.contains("\"text_chars\":"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn tool_call_input_is_redacted() {
        let (dir, store) = temp_store("toolcall");
        let id = store.create_session("openai", "m", None).expect("create");
        store
            .append(
                &id,
                SessionEventKind::tool_call(
                    "mods.dependency_scan",
                    RiskLevel::ReadOnly,
                    &json!({ "api_key": "sk-super-secret", "mods_dir": "/tmp/mods" }),
                ),
            )
            .expect("append");
        let raw =
            std::fs::read_to_string(store.sessions_dir().join(format!("{id}.jsonl"))).expect("raw");
        assert!(!raw.contains("sk-super-secret"));
        assert!(raw.contains("«redacted»"));
        assert!(raw.contains("/tmp/mods"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn tool_result_stores_hash_not_output() {
        let (dir, store) = temp_store("toolresult");
        let id = store.create_session("openai", "m", None).expect("create");
        let result = ToolResult::ok("mods.installed", json!({ "mods": ["sodium.jar"] }));
        store
            .append(
                &id,
                SessionEventKind::tool_result(&result, Duration::from_millis(7)),
            )
            .expect("append");
        let raw =
            std::fs::read_to_string(store.sessions_dir().join(format!("{id}.jsonl"))).expect("raw");
        assert!(!raw.contains("sodium.jar"));
        assert!(raw.contains("\"output_bytes\""));
        assert!(raw.contains("\"duration_ms\":7"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn error_text_is_redacted_and_truncated() {
        let long = format!("Bearer abcdefghijklmnop {}", "x".repeat(600));
        let event = SessionEventKind::error("llm_error", &long);
        let SessionEventKind::Error { category, message } = event else {
            panic!("wrong variant");
        };
        assert_eq!(category, "llm_error");
        assert!(!message.contains("abcdefghijklmnop"));
        assert!(message.chars().count() <= MAX_ERROR_CHARS + 1);
        assert!(message.ends_with('…'));
    }

    #[test]
    fn invalid_session_ids_are_rejected() {
        let (dir, store) = temp_store("badid");
        for bad in ["../escape", "a/b", "", ".", "..", "a b", "a.b"] {
            let error = store
                .append(bad, SessionEventKind::user_message("x"))
                .expect_err("must reject");
            assert!(matches!(error, RmcccError::Validation(_)), "{bad}");
        }
        let too_long = "a".repeat(MAX_ID_LEN + 1);
        assert!(store.read(&too_long).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn unknown_session_read_is_not_found() {
        let (dir, store) = temp_store("missing");
        let error = store.read("no-such-session").expect_err("must error");
        match error {
            RmcccError::Io(io) => assert_eq!(io.kind(), ErrorKind::NotFound),
            other => panic!("unexpected error: {other}"),
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn task_snapshot_roundtrip_and_redaction() {
        let (dir, store) = temp_store("task");
        let snapshot = TaskSnapshot::proposed(
            "task-1",
            None,
            "mod_install_plan",
            &json!({ "download_url": "https://example.com/a.jar?token=secret-value" }),
        )
        .expect("snapshot");
        store.write_task(&snapshot).expect("write");
        let read = store.read_task("task-1").expect("read").expect("present");
        assert_eq!(read.task_id, "task-1");
        assert_eq!(read.status, TaskStatus::Proposed);
        let raw = std::fs::read_to_string(store.tasks_dir().join("task-1.json")).expect("raw");
        assert!(!raw.contains("secret-value"));
        assert_eq!(
            read.payload["download_url"],
            "https://example.com/a.jar?token=«redacted»"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn task_traversal_is_rejected() {
        let error = TaskSnapshot::proposed("../../etc/passwd", None, "x", &json!({}))
            .expect_err("must reject");
        assert!(matches!(error, RmcccError::Validation(_)));
    }

    #[test]
    fn missing_task_reads_as_none() {
        let (dir, store) = temp_store("notask");
        assert_eq!(store.read_task("absent").expect("read"), None);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn list_sessions_is_sorted_by_last_event() {
        let (dir, store) = temp_store("list");
        let old = store.create_session("openai", "m", None).expect("create");
        let new = store.create_session("openai", "m", None).expect("create");
        // 让 old 的最后事件时间更早，再给 new 追加一条新事件。
        std::thread::sleep(Duration::from_millis(5));
        store
            .append(&new, SessionEventKind::session_ended("done", 1))
            .expect("append");
        let summaries = store.list().expect("list");
        assert_eq!(summaries.len(), 2);
        assert_eq!(summaries[0].session_id, new);
        assert!(summaries[0].ended);
        assert_eq!(summaries[1].session_id, old);
        assert!(!summaries[1].ended);
        assert_eq!(summaries[0].event_count, 2);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn purge_expired_only_removes_old_files() {
        let (dir, store) = temp_store("purge");
        let old = store.create_session("openai", "m", None).expect("create");
        let new = store.create_session("openai", "m", None).expect("create");
        let task = TaskSnapshot::proposed("task-old", None, "plan", &json!({})).expect("task");
        store.write_task(&task).expect("write");

        let now = SystemTime::now();
        let ancient = now - Duration::from_secs(60 * 60 * 24 * 90);
        for path in [
            store.sessions_dir().join(format!("{old}.jsonl")),
            store.tasks_dir().join("task-old.json"),
        ] {
            let file = std::fs::OpenOptions::new()
                .write(true)
                .open(&path)
                .expect("open");
            file.set_modified(ancient).expect("set mtime");
        }

        let report = store
            .purge_expired(Duration::from_secs(60 * 60 * 24 * 30), now)
            .expect("purge");
        assert_eq!(report.sessions_removed, 1);
        assert_eq!(report.tasks_removed, 1);
        assert!(report.bytes_freed > 0);
        assert!(store.read(&new).is_ok());
        assert!(store.read(&old).is_err());
        assert_eq!(store.read_task("task-old").expect("read"), None);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn clear_all_keeps_audit_log() {
        let (dir, store) = temp_store("clear");
        let id = store.create_session("openai", "m", None).expect("create");
        let audit = store.open_audit().expect("audit");
        let task =
            TaskSnapshot::proposed("task-1", Some(id.clone()), "plan", &json!({})).expect("task");
        store.write_task(&task).expect("write");

        let report = store.clear_all().expect("clear");
        assert_eq!(report.sessions_removed, 1);
        assert_eq!(report.tasks_removed, 1);
        assert!(store.list().expect("list").is_empty());
        assert!(store.list_tasks().expect("tasks").is_empty());
        assert!(audit.path().exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[cfg(unix)]
    #[test]
    fn session_and_task_files_are_owner_only() {
        use std::os::unix::fs::PermissionsExt;
        let (dir, store) = temp_store("perms");
        let id = store.create_session("openai", "m", None).expect("create");
        let task = TaskSnapshot::proposed("task-perm", None, "plan", &json!({})).expect("task");
        store.write_task(&task).expect("write");
        for path in [
            store.sessions_dir().join(format!("{id}.jsonl")),
            store.tasks_dir().join("task-perm.json"),
        ] {
            let mode = std::fs::metadata(&path).expect("meta").permissions().mode() & 0o777;
            assert_eq!(mode, 0o600, "{} mode {mode:o}", path.display());
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn retention_ignores_future_timestamps() {
        let now = SystemTime::now();
        let future = now + Duration::from_secs(60);
        assert!(!is_expired(future, now, Duration::from_secs(1)));
        assert!(is_expired(
            now - Duration::from_secs(120),
            now,
            Duration::from_secs(60)
        ));
    }
}
