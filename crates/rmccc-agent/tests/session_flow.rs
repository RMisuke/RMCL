//! 端到端：按 ADR-010 的会话流程走一遍公开 API（不碰私有字段）。
//!
//! 这个测试存在的意义是锁定"外部调用方"看到的契约：
//! `SessionStore` → 事件流 → 任务快照 → 审计 → 清除历史，
//! 以及**磁盘上不出现提示词正文与凭据原文**。

use rmccc_agent::session::{
    SessionEvent, SessionEventKind, SessionStore, TaskSnapshot, TaskStatus,
};
use rmccc_agent::tools::RiskLevel;
use rmccc_agent::{AuditLog, AuditOutcome, AuditRecord};
use rmccc_platform::Platform;
use serde_json::json;
use std::path::PathBuf;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

fn temp_store(tag: &str) -> (PathBuf, SessionStore) {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let dir = std::env::temp_dir().join(format!(
        "rmccc-agent-flow-{tag}-{}-{nanos}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    let store = SessionStore::open(&dir, Platform::current().fs.clone()).expect("session store");
    (dir, store)
}

/// 读出 `agent/` 下所有文件的全文，用于"磁盘不该有明文"断言。
fn all_disk_text(store: &SessionStore) -> String {
    let mut out = String::new();
    let mut stack = vec![store.agent_dir()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if let Ok(text) = std::fs::read_to_string(&path) {
                out.push_str(&text);
                out.push('\n');
            }
        }
    }
    out
}

#[test]
fn read_only_session_flow_is_traceable_and_private() {
    let (dir, store) = temp_store("flow");
    let audit = store.open_audit().expect("audit");

    let session_id = store
        .create_session("openai", "gpt-4o-mini", Some("profile-1"))
        .expect("create session");

    let prompt = "check my instance for missing sodium dependencies";
    store
        .append(&session_id, SessionEventKind::user_message(prompt))
        .expect("user message");

    let scan_input = json!({ "mods_dir": "/tmp/instance/mods" });
    store
        .append(
            &session_id,
            SessionEventKind::tool_call("mods.dependency_scan", RiskLevel::ReadOnly, &scan_input),
        )
        .expect("tool call");
    audit
        .append(&AuditRecord::new(
            &session_id,
            "mods.dependency_scan",
            RiskLevel::ReadOnly,
            &scan_input,
            AuditOutcome::Ok,
            Duration::from_millis(12),
            None,
        ))
        .expect("audit");

    let task = TaskSnapshot::proposed(
        "task-1",
        Some(session_id.clone()),
        "mod_install_plan",
        &json!({
            "dry_run": true,
            "requires_confirmation": true,
            "download_url": "https://cdn.example.com/sodium.jar?token=abc123secret",
        }),
    )
    .expect("task");
    store.write_task(&task).expect("write task");
    store
        .append(
            &session_id,
            SessionEventKind::approval_requested(
                "task-1",
                "mods.dependency_scan",
                RiskLevel::ReadOnly,
                "install sodium (dry-run)",
            ),
        )
        .expect("approval requested");
    store
        .append(
            &session_id,
            SessionEventKind::approval_resolved("task-1", true, None),
        )
        .expect("approval resolved");
    store
        .append(&session_id, SessionEventKind::session_ended("completed", 1))
        .expect("end");

    // 1) 事件流完整且有序（外部依次读到每一个白名单事件）。
    let events = store.read(&session_id).expect("read session");
    assert_eq!(events.len(), 6);
    assert!(matches!(
        events[0].event,
        SessionEventKind::SessionStarted { .. }
    ));
    assert!(matches!(
        events[5].event,
        SessionEventKind::SessionEnded { .. }
    ));

    // 2) 列表视图能回答"这个会话发生了什么"。
    let summaries = store.list().expect("list");
    assert_eq!(summaries.len(), 1);
    assert_eq!(summaries[0].session_id, session_id);
    assert_eq!(summaries[0].event_count, 6);
    assert_eq!(summaries[0].tool_calls, 1);
    assert!(summaries[0].ended);

    // 3) 任务快照可读，状态可推进，token 已脱敏。
    let stored = store
        .read_task("task-1")
        .expect("read task")
        .expect("present");
    assert_eq!(stored.status, TaskStatus::Proposed);
    assert_eq!(
        stored.payload["download_url"],
        "https://cdn.example.com/sodium.jar?token=«redacted»"
    );
    store
        .write_task(&stored.clone().with_status(TaskStatus::Approved))
        .expect("update task");
    assert_eq!(
        store
            .read_task("task-1")
            .expect("read")
            .expect("present")
            .status,
        TaskStatus::Approved
    );

    // 4) 审计与事件流都落盘，且磁盘上没有提示词正文 / 令牌原文。
    let disk = all_disk_text(&store);
    assert!(disk.contains("mods.dependency_scan"));
    assert!(!disk.contains(prompt));
    assert!(!disk.contains("sodium dependencies"));
    assert!(!disk.contains("abc123secret"));
    assert!(audit.read_all().expect("audit read").len() == 1);

    // 5) "清除 Agent 历史"删会话与任务，保留审计（清除动作仍可审计）。
    let report = store.clear_all().expect("clear");
    assert_eq!(report.sessions_removed, 1);
    assert_eq!(report.tasks_removed, 1);
    assert!(store.list().expect("list").is_empty());
    assert_eq!(store.read_task("task-1").expect("read"), None);
    let _ = AuditLog::open(audit.path(), Platform::current().fs.clone());
    assert!(audit.path().exists());

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn events_roundtrip_through_serialized_form() {
    let event = SessionEvent {
        ts: "2026-10-22T00:00:00Z".into(),
        session_id: "session-1".into(),
        event: SessionEventKind::tool_call(
            "versions.installed",
            RiskLevel::ReadOnly,
            &json!({ "versions_dir": "/tmp/versions" }),
        ),
    };
    let line = serde_json::to_string(&event).expect("serialize");
    assert!(line.contains("\"kind\":\"tool_call\""));
    let back: SessionEvent = serde_json::from_str(&line).expect("deserialize");
    assert_eq!(back, event);
}
