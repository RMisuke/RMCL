//! 确认快照原语（ADR-013：为 MCP 破坏性操作的两阶段确认保留）。
//!
//! Phase 6.5 的 `ApprovalBroker` / `SessionStore` 已随内建 Agent 平台退役；
//! [`TaskSnapshot`] 是当时验证过的两阶段原语（proposed → approved/rejected →
//! applied/failed，防重放），MCP 切片（S4/S5）的确认流（`kind = "mcp:<tool>"`）
//! 直接复用本结构与语义。
//!
//! # 不变量
//!
//! 1. **id 不可信**：task/session id 可能来自外部工具或模型，必须先过
//!    [`validate_id`]（只允许 `[A-Za-z0-9_-]`、≤64 字符）。
//! 2. **构造时脱敏**：`payload` 经 [`rmccc_llm::redact::redact_json`] 处理，
//!    磁盘上不应出现凭据原文。

use rmccc_error::{Result, RmcccError};
use rmccc_llm::redact::redact_json;
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// id 最大长度。UUID v4 为 36 字符，留出人类可读 id 的余量。
const MAX_ID_LEN: usize = 64;

/// 确认快照状态（两阶段：proposed → approved/rejected → applied/failed）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskStatus {
    /// 已生成、等待用户确认。
    Proposed,
    Approved,
    Rejected,
    Applied,
    Failed,
}

/// 待确认操作的快照。
///
/// `payload` 是结构化载荷（例如 `ModInstallPlan` 的 JSON 形态；MCP 时代为
/// 工具调用参数），**构造时脱敏**。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TaskSnapshot {
    pub task_id: String,
    /// 发起来源（MCP 会话 id / `"user"` / 插件 id）。
    pub session_id: Option<String>,
    /// 载荷类型：`mod_install_plan`；MCP 时代为 `mcp:<tool>`。
    pub kind: String,
    pub created_at: String,
    pub updated_at: String,
    pub status: TaskStatus,
    pub payload: Value,
}

impl TaskSnapshot {
    /// 新建 `proposed` 快照；id 立即校验（可能来自外部）。
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

/// id 校验：只允许 `[A-Za-z0-9_-]`、≤64 字符。
///
/// session/task id 可能被拼进文件路径或审计记录，`../../x` 一类输入必须在这里
/// 拦下（ADR-010 不变量 3）。
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

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn proposed_snapshot_redacts_payload() {
        let snapshot = TaskSnapshot::proposed(
            "task-1",
            Some("mcp-zcode-9f2a".into()),
            "mcp:profiles.delete",
            &json!({ "api_key": "sk-secret", "profile_id": "p1" }),
        )
        .expect("snapshot");
        assert_eq!(snapshot.status, TaskStatus::Proposed);
        assert_eq!(snapshot.payload["api_key"], "«redacted»");
        assert_eq!(snapshot.payload["profile_id"], "p1");
    }

    #[test]
    fn status_transitions_and_roundtrip() {
        let snapshot = TaskSnapshot::proposed("task-2", None, "mod_install_plan", &json!({}))
            .expect("snapshot")
            .with_status(TaskStatus::Approved)
            .with_status(TaskStatus::Applied);
        assert_eq!(snapshot.status, TaskStatus::Applied);
        let text = serde_json::to_string(&snapshot).expect("serialize");
        let back: TaskSnapshot = serde_json::from_str(&text).expect("deserialize");
        assert_eq!(back, snapshot);
        assert_eq!(back.payload, json!({}));
    }

    #[test]
    fn ids_are_validated() {
        assert!(TaskSnapshot::proposed("../escape", None, "k", &json!({})).is_err());
        assert!(TaskSnapshot::proposed("", None, "k", &json!({})).is_err());
        assert!(TaskSnapshot::proposed("task-1", Some("bad id".into()), "k", &json!({})).is_err());
        assert!(validate_id("task id", &"a".repeat(65)).is_err());
        assert!(validate_id("task id", "Mcp_Zcode-01").is_ok());
    }
}
