//! Phase 6.5 端到端：AgentService 执行循环 + ApprovalBroker。
//!
//! 用脚本化的假 LLM provider 走完整回合，锁定外部契约：
//! - 直接工具调用 → 结果回灌 → 最终答复（事件流 + 审计完整）
//! - 需要确认的工具 → 任务快照 proposed → 异步决断 → approved / rejected
//! - 取消与迭代上限
//! - 模型输出的解析契约（严格围栏）

use rmccc_agent::llm::{
    CancelToken, ChatRequest, LlmEvent, LlmProvider, LlmStream, ModelCapabilities,
};
use rmccc_agent::runtime::{
    AgentService, AgentServiceConfig, Resolution, ToolCallRequest, TurnStatus,
};
use rmccc_agent::session::{SessionEventKind, SessionStore, TaskStatus};
use rmccc_agent::tools::{
    AgentTool, AgentToolDescriptor, ConfirmationPolicy, RiskLevel, ToolRegistry,
};
use rmccc_agent::{AgentRoots, ApprovalBroker};
use rmccc_platform::Platform;
use serde_json::{json, Value};
use std::sync::Arc;
use std::time::Duration;

// ── 测试替身 ──

/// 脚本化 provider：第 N 次请求返回预设响应。
struct ScriptedProvider {
    id: String,
    responses: std::sync::Mutex<Vec<String>>,
}

impl LlmProvider for ScriptedProvider {
    fn id(&self) -> &str {
        &self.id
    }

    fn capabilities(&self) -> ModelCapabilities {
        ModelCapabilities::default()
    }

    fn stream_chat(
        &self,
        _request: ChatRequest,
        _cancel: CancelToken,
    ) -> Result<LlmStream, rmccc_agent::llm::LlmError> {
        let text = self
            .responses
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .pop()
            .unwrap_or_default();
        let (tx, rx) = tokio::sync::mpsc::channel(4);
        tokio::spawn(async move {
            let _ = tx.send(LlmEvent::Delta(text)).await;
            let _ = tx
                .send(LlmEvent::Done {
                    finish_reason: None,
                })
                .await;
        });
        Ok(rx)
    }
}

struct ProbeTool {
    id: String,
    descriptor: AgentToolDescriptor,
}

impl ProbeTool {
    fn new(id: &str, policy: ConfirmationPolicy) -> Self {
        Self {
            id: id.to_string(),
            descriptor: AgentToolDescriptor {
                id: id.to_string(),
                plugin_id: None,
                name: id.to_string(),
                description: "probe tool".into(),
                input_schema: json!({
                    "type": "object", "properties": {}, "additionalProperties": false
                }),
                output_schema: json!({ "type": "object" }),
                required_capabilities: vec![],
                risk_level: RiskLevel::ReadOnly,
                timeout_ms: 5_000,
                confirmation_policy: policy,
            },
        }
    }
}

impl AgentTool for ProbeTool {
    fn descriptor(&self) -> &AgentToolDescriptor {
        &self.descriptor
    }

    fn execute(&self, _input: &Value) -> rmccc_error::Result<Value> {
        Ok(json!({ "probed": self.id }))
    }
}

fn temp_store(tag: &str) -> Arc<SessionStore> {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let dir = std::env::temp_dir().join(format!(
        "rmccc-agent-runtime-{tag}-{}-{nanos}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    Arc::new(SessionStore::open(&dir, Platform::current().fs.clone()).expect("store"))
}

fn service(
    store: Arc<SessionStore>,
    registry: ToolRegistry,
    provider: Arc<dyn LlmProvider>,
) -> AgentService {
    AgentService::new(
        store,
        Arc::new(registry),
        provider,
        AgentServiceConfig {
            model: "test-model".into(),
            ..Default::default()
        },
    )
}

// ── 解析契约 ──

#[test]
fn parse_tool_calls_strict_contract() {
    let text = "thinking...\n```rmccc-tool\n{\"tool\":\"mods.installed\",\"input\":{}}\n```\n\
                more text\n```rmccc-tool\n{\"tool\":\"versions.installed\",\"input\":{}}\n```";
    let calls = rmccc_agent::runtime::parse_tool_calls(text).expect("parse");
    assert_eq!(calls.len(), 2);
    assert_eq!(calls[0].tool, "mods.installed");
    assert_eq!(calls[1].tool, "versions.installed");

    // 未闭合围栏 / 缺字段 / input 非对象 → 全部拒绝。
    assert!(rmccc_agent::runtime::parse_tool_calls("```rmccc-tool\n{\"tool\":\"x\"}").is_err());
    assert!(
        rmccc_agent::runtime::parse_tool_calls("```rmccc-tool\n{\"tool\":\"x\"}\n```").is_err()
    );
    assert!(rmccc_agent::runtime::parse_tool_calls(
        "```rmccc-tool\n{\"tool\":\"x\",\"input\":[1]}\n```"
    )
    .is_err());
    // 普通文本不含围栏 → 空调用 = 最终答复。
    assert!(rmccc_agent::runtime::parse_tool_calls("just an answer")
        .unwrap()
        .is_empty());
}

#[test]
fn tool_call_request_roundtrip() {
    let request = ToolCallRequest {
        tool: "demo.echo".into(),
        input: json!({ "text": "hi" }),
    };
    assert_eq!(request.clone().tool, "demo.echo");
}

// ── 回合端到端 ──

#[tokio::test]
async fn direct_tool_call_feeds_result_back_and_completes() {
    let store = temp_store("direct");
    let audit = store.open_audit().expect("audit");
    let roots = AgentRoots {
        versions_dir: std::env::temp_dir().join("rmccc-agent-runtime-no-versions"),
        mods_dir: std::env::temp_dir().join("rmccc-agent-runtime-no-mods"),
    };
    std::fs::create_dir_all(&roots.versions_dir).expect("versions dir");
    let registry = rmccc_agent::read_only_registry(&roots)
        .expect("registry")
        .with_audit(Arc::new(audit));

    // 第一次响应：调用工具；第二次：最终答复。
    let provider = Arc::new(ScriptedProvider {
        id: "scripted".into(),
        responses: std::sync::Mutex::new(vec![
            "All done, nothing missing.".to_string(),
            "```rmccc-tool\n{\"tool\":\"versions.installed\",\"input\":{}}\n```".to_string(),
        ]),
    });
    let svc = service(store.clone(), registry, provider);
    let out = svc
        .run_turn("what versions are installed?", None, CancelToken::new())
        .await
        .expect("turn");

    assert_eq!(out.status, TurnStatus::Completed);
    assert_eq!(out.tool_calls, 1);
    assert_eq!(out.replies, vec!["All done, nothing missing."]);

    // 事件流：started → user → model → tool_call → tool_result → model → ended。
    let events = store.read(&out.session_id).expect("events");
    let kinds: Vec<&str> = events
        .iter()
        .map(|event| match &event.event {
            SessionEventKind::SessionStarted { .. } => "started",
            SessionEventKind::UserMessage { .. } => "user",
            SessionEventKind::ModelResponse { .. } => "model",
            SessionEventKind::ToolCall { .. } => "tool_call",
            SessionEventKind::ToolResult { .. } => "tool_result",
            SessionEventKind::ApprovalRequested { .. } => "approval_requested",
            SessionEventKind::ApprovalResolved { .. } => "approval_resolved",
            SessionEventKind::Error { .. } => "error",
            SessionEventKind::SessionEnded { .. } => "ended",
        })
        .collect::<Vec<&str>>();
    assert_eq!(
        kinds,
        vec![
            "started",
            "user",
            "model",
            "tool_call",
            "tool_result",
            "model",
            "ended"
        ]
    );

    let _ = std::fs::remove_dir_all(store.data_dir());
}

#[tokio::test]
async fn confirmation_tool_waits_for_broker_decision() {
    let store = temp_store("approval");
    let mut registry = ToolRegistry::new();
    registry
        .register(Arc::new(ProbeTool::new(
            "demo.confirm",
            ConfirmationPolicy::Always,
        )))
        .expect("register");
    let provider = Arc::new(ScriptedProvider {
        id: "scripted".into(),
        responses: std::sync::Mutex::new(vec![
            "approved path done".to_string(),
            "```rmccc-tool\n{\"tool\":\"demo.confirm\",\"input\":{}}\n```".to_string(),
        ]),
    });
    let svc = Arc::new(service(store.clone(), registry, provider));
    // 另一个"用户"在看到任务后批准。
    let store_for_user = store.clone();
    let user = tokio::spawn(async move {
        for _ in 0..100 {
            if let Ok(pending) = broker_for_user(&store_for_user) {
                if !pending.is_empty() {
                    return pending[0].task_id.clone();
                }
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        panic!("no pending task appeared");
    });
    fn broker_for_user(
        store: &Arc<SessionStore>,
    ) -> rmccc_error::Result<Vec<rmccc_agent::session::TaskSnapshot>> {
        ApprovalBroker::new(store.clone()).pending()
    }

    let svc_for_run = svc.clone();
    let run = tokio::spawn(async move {
        svc_for_run
            .run_turn("probe with confirmation", None, CancelToken::new())
            .await
    });

    let task_id = user.await.expect("user task");
    svc.broker().resolve(&task_id, true, None).expect("approve");

    let out = run.await.expect("join").expect("turn");
    assert_eq!(out.status, TurnStatus::Completed);
    assert_eq!(out.tool_calls, 1);
    assert_eq!(out.replies, vec!["approved path done"]);

    let task = store.read_task(&task_id).expect("read").expect("present");
    assert_eq!(task.status, TaskStatus::Approved);

    // 事件流里有 approval_requested 与 approval_resolved。
    let events = store.read(&out.session_id).expect("events");
    assert!(events
        .iter()
        .any(|event| matches!(event.event, SessionEventKind::ApprovalRequested { .. })));
    assert!(events
        .iter()
        .any(|event| matches!(event.event, SessionEventKind::ApprovalResolved { .. })));

    let _ = std::fs::remove_dir_all(store.data_dir());
}

#[tokio::test]
async fn rejected_confirmation_blocks_execution() {
    let store = temp_store("rejected");
    let mut registry = ToolRegistry::new();
    registry
        .register(Arc::new(ProbeTool::new(
            "demo.confirm",
            ConfirmationPolicy::Always,
        )))
        .expect("register");
    let provider = Arc::new(ScriptedProvider {
        id: "scripted".into(),
        responses: std::sync::Mutex::new(vec![
            "understood, stopped".to_string(),
            "```rmccc-tool\n{\"tool\":\"demo.confirm\",\"input\":{}}\n```".to_string(),
        ]),
    });
    let svc = Arc::new(service(store.clone(), registry, provider));

    let store_for_user = store.clone();
    let user = tokio::spawn(async move {
        for _ in 0..100 {
            let broker = ApprovalBroker::new(store_for_user.clone());
            if let Ok(pending) = broker.pending() {
                if !pending.is_empty() {
                    return pending[0].task_id.clone();
                }
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        panic!("no pending task appeared");
    });

    let svc_for_run = svc.clone();
    let run = tokio::spawn(async move {
        svc_for_run
            .run_turn("probe with confirmation", None, CancelToken::new())
            .await
    });

    let task_id = user.await.expect("user task");
    svc.broker()
        .resolve(&task_id, false, Some("no"))
        .expect("reject");

    let out = run.await.expect("join").expect("turn");
    assert_eq!(out.status, TurnStatus::Completed);
    assert_eq!(out.replies, vec!["understood, stopped"]);

    // 工具没执行：事件流只有 tool_call 被拒绝路径——确认没有 tool_result。
    let events = store.read(&out.session_id).expect("events");
    assert!(events
        .iter()
        .all(|event| !matches!(event.event, SessionEventKind::ToolResult { .. })));

    let _ = std::fs::remove_dir_all(store.data_dir());
}

#[tokio::test]
async fn max_iterations_stops_the_loop() {
    let store = temp_store("maxiter");
    let mut registry = ToolRegistry::new();
    registry
        .register(Arc::new(ProbeTool::new(
            "demo.confirm",
            ConfirmationPolicy::Never,
        )))
        .expect("register");
    // 模型永远只发工具调用，不给最终答复。
    let provider = Arc::new(ScriptedProvider {
        id: "scripted".into(),
        responses: std::sync::Mutex::new(
            (0..16)
                .map(|_| "```rmccc-tool\n{\"tool\":\"demo.confirm\",\"input\":{}}\n```".to_string())
                .collect::<Vec<String>>(),
        ),
    });
    let svc = AgentService::new(
        store.clone(),
        Arc::new(registry),
        provider,
        AgentServiceConfig {
            model: "test-model".into(),
            max_iterations: 3,
            ..Default::default()
        },
    );
    let out = svc
        .run_turn("loop forever", None, CancelToken::new())
        .await
        .expect("turn");
    assert_eq!(out.status, TurnStatus::MaxIterations);
    assert_eq!(out.tool_calls, 3);
    assert_eq!(out.replies.len(), 0);

    let _ = std::fs::remove_dir_all(store.data_dir());
}

#[tokio::test]
async fn cancel_stops_the_turn() {
    let store = temp_store("cancel");
    let mut registry = ToolRegistry::new();
    registry
        .register(Arc::new(ProbeTool::new(
            "demo.confirm",
            ConfirmationPolicy::Always,
        )))
        .expect("register");
    let provider = Arc::new(ScriptedProvider {
        id: "scripted".into(),
        responses: std::sync::Mutex::new(vec![
            "never reached".to_string(),
            "```rmccc-tool\n{\"tool\":\"demo.confirm\",\"input\":{}}\n```".to_string(),
        ]),
    });
    let svc = Arc::new(service(store.clone(), registry, provider));

    let store_for_user = store.clone();
    let cancel = CancelToken::new();
    let cancel_for_user = cancel.clone();
    let user = tokio::spawn(async move {
        // 等任务出现后立刻取消回合（模拟用户关掉窗口）。
        for _ in 0..100 {
            let broker = ApprovalBroker::new(store_for_user.clone());
            if let Ok(pending) = broker.pending() {
                if !pending.is_empty() {
                    cancel_for_user.cancel();
                    return;
                }
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    });

    let svc_for_run = svc.clone();
    let run = tokio::spawn(async move { svc_for_run.run_turn("cancel me", None, cancel).await });

    user.await.expect("user");
    let out = run.await.expect("join").expect("turn");
    assert_eq!(out.status, TurnStatus::Cancelled);

    // 任务仍是 proposed（没有静默批准）。
    let pending = svc.broker().pending().expect("pending");
    assert_eq!(pending.len(), 1);

    let _ = std::fs::remove_dir_all(store.data_dir());
}

#[tokio::test]
async fn resolve_is_idempotent_guarded_and_auditable() {
    let store = temp_store("resolve");
    let broker = ApprovalBroker::new(store.clone());
    let snapshot = broker
        .propose(
            "session-1",
            "demo.tool",
            RiskLevel::ReadOnly,
            "summary",
            &json!({ "k": "v" }),
        )
        .expect("propose");

    broker
        .resolve(&snapshot.task_id, true, None)
        .expect("approve");
    // 二次决断被拒绝。
    assert!(broker.resolve(&snapshot.task_id, true, None).is_err());
    // 未知任务报 NotFound 语义错误。
    assert!(broker.resolve("no-such-task", true, None).is_err());

    let resolution = broker
        .wait_resolution(
            &snapshot.task_id,
            Duration::from_secs(1),
            Duration::from_millis(10),
            &CancelToken::new(),
        )
        .await
        .expect("resolution");
    assert_eq!(resolution, Resolution::Approved);

    let _ = std::fs::remove_dir_all(store.data_dir());
}
