//! 工具调用审计（ADR-010 决策 3）。
//!
//! 追加写 JSONL：一行一条 [`AuditRecord`]。默认字段都是白名单，
//! **不记录** api_key、完整命令输出或文件内容；输入在构造记录时经 [`crate::redact`] 处理。
//!
//! 审计写入失败的处理原则：
//!
//! - 只读工具：错误返回给调用方，调用结果不被伪造；
//! - 写操作（Phase 6.5 起）：必须 fail-closed（审计写不进去就不执行）。
//!   本模块只提供机制，策略在调用方。

use crate::redact::redact_json;
use crate::tools::RiskLevel;
use rmccc_error::{Result, RmcccError};
use rmccc_platform::PlatformFs;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

/// 审计结果分类。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuditOutcome {
    Ok,
    Denied,
    Error,
}

/// 一条工具调用记录。字段是白名单：新增字段需要先过 ADR-010。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AuditRecord {
    /// RFC3339 时间戳（UTC）。
    pub ts: String,
    pub session_id: String,
    pub tool_id: String,
    pub risk_level: RiskLevel,
    /// 已脱敏的输入。
    pub input: Value,
    pub outcome: AuditOutcome,
    pub duration_ms: u64,
    /// 已脱敏的错误摘要（`outcome = error` 时）。
    pub error: Option<String>,
}

impl AuditRecord {
    /// 构造记录：输入与错误摘要在这里脱敏，避免调用方忘记。
    pub fn new(
        session_id: impl Into<String>,
        tool_id: impl Into<String>,
        risk_level: RiskLevel,
        input: &Value,
        outcome: AuditOutcome,
        duration: Duration,
        error: Option<&str>,
    ) -> Self {
        Self {
            ts: chrono::Utc::now().to_rfc3339(),
            session_id: session_id.into(),
            tool_id: tool_id.into(),
            risk_level,
            input: redact_json(input, None),
            outcome,
            duration_ms: duration.as_millis().min(u128::from(u64::MAX)) as u64,
            error: error.map(|text| crate::redact::redact_text(text, None)),
        }
    }
}

/// 追加写审计日志。
pub struct AuditLog {
    path: PathBuf,
    fs: Arc<dyn PlatformFs>,
}

impl AuditLog {
    /// 打开（必要时创建父目录），新文件权限收紧到属主可读写（ADR-010）。
    pub fn open(path: impl AsRef<Path>, fs: Arc<dyn PlatformFs>) -> Result<Self> {
        let path = path.as_ref().to_path_buf();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        if !path.exists() {
            std::fs::write(&path, b"")?;
            fs.restrict_owner_only(&path)?;
        }
        Ok(Self { path, fs })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// 追加一条记录（同步 I/O；只读工具路径可接受，写操作应走队列）。
    pub fn append(&self, record: &AuditRecord) -> Result<()> {
        use std::io::Write;
        let line = serde_json::to_string(record)?;
        if line.contains('\n') {
            return Err(RmcccError::Validation(
                "audit record must serialize to a single line".into(),
            ));
        }
        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)?;
        file.write_all(line.as_bytes())?;
        file.write_all(b"\n")?;
        file.sync_data()?;
        // 新建文件后补一次权限收紧（open 的 create 不会走 `open()` 分支）。
        self.fs.restrict_owner_only(&self.path)?;
        Ok(())
    }

    /// 读取全部记录（诊断/导出）。无法解析的行会被跳过而不是让整个日志不可读。
    pub fn read_all(&self) -> Result<Vec<AuditRecord>> {
        let raw = std::fs::read_to_string(&self.path)?;
        Ok(raw
            .lines()
            .filter(|line| !line.trim().is_empty())
            .filter_map(|line| serde_json::from_str::<AuditRecord>(line).ok())
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rmccc_platform::Platform;
    use serde_json::json;

    fn temp_log(tag: &str) -> (PathBuf, AuditLog) {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let dir = std::env::temp_dir().join(format!(
            "rmccc-agent-audit-{tag}-{}-{nanos}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        let path = dir.join("audit.jsonl");
        let log = AuditLog::open(&path, Platform::current().fs.clone()).expect("audit log");
        (dir, log)
    }

    #[test]
    fn append_and_read_roundtrip() {
        let (dir, log) = temp_log("roundtrip");
        let record = AuditRecord::new(
            "session-1",
            "mods.installed",
            RiskLevel::ReadOnly,
            &json!({ "mods_dir": "/tmp/mods" }),
            AuditOutcome::Ok,
            Duration::from_millis(12),
            None,
        );
        log.append(&record).expect("append");
        let records = log.read_all().expect("read");
        assert_eq!(records.len(), 1);
        assert_eq!(records[0], record);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn secrets_never_reach_disk() {
        let (dir, log) = temp_log("secrets");
        let record = AuditRecord::new(
            "session-1",
            "demo",
            RiskLevel::ReadOnly,
            &json!({ "api_key": "sk-super-secret", "note": "Bearer abcdefghijklmnop" }),
            AuditOutcome::Error,
            Duration::from_millis(1),
            Some("failed with sk-super-secret"),
        );
        log.append(&record).expect("append");
        let raw = std::fs::read_to_string(log.path()).expect("raw");
        assert!(!raw.contains("sk-super-secret"));
        assert!(!raw.contains("abcdefghijklmnop"));
        assert!(raw.contains("«redacted»"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn corrupt_lines_are_skipped() {
        let (dir, log) = temp_log("corrupt");
        std::fs::write(log.path(), "not json\n{\"ts\":\"x\"}\n").expect("write");
        assert!(log.read_all().expect("read").is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
