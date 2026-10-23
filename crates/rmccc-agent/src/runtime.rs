//! Agent 执行循环与审批中介（Phase 6.5，指南 §10.3，里程碑 M7）。
//!
//! # 安全边界（ADR-010）
//!
//! - **模型输出不是指令**：模型文本只被解析出受约束的工具调用请求
//!   （严格的 ```rmccc-tool 围栏 + JSON），任何其他内容都只是给用户看的回复。
//! - **每次调用都过注册表**：Schema 校验 → 风险门禁 → 审计，与本 crate 其余部分同一入口。
//! - **审批链**：声明 `confirmation_policy != never` 的工具在执行前生成
//!   [`TaskSnapshot`]（dry-run 计划进 `agent/tasks/`），由 UI 决断后才会执行；
//!   等待期间可取消。决断本身写 `approval_resolved` 事件。
//! - **会话可追溯**：每一步（用户消息 / 模型响应 / 工具调用与结果 / 审批 / 错误）
//!   都进 [`SessionStore`] 事件流，提示词与输出只存哈希。
//! - Phase 6.5 第一批仍只放行 `read_only` 工具（[`ToolRegistry`] 的既有门禁）；
//!   写操作即使审批通过也会被注册表拒绝，直到写工具在后续切片注册。

use crate::llm::{self};
use crate::llm::{CancelToken, ChatMessage, ChatRole, LlmProvider};
use crate::session::{SessionEventKind, SessionStore, TaskSnapshot, TaskStatus};
use crate::tools::{ConfirmationPolicy, RiskLevel, ToolRegistry};
use rmccc_error::{Result, RmcccError};
use serde_json::{json, Value};
use std::io::ErrorKind;
use std::sync::Arc;
use std::time::Duration;

/// 模型输出工具调用的围栏标记。刻意选一个不常见的语言标签，
/// 避免与普通 markdown 代码块混淆。
const TOOL_FENCE: &str = "```rmccc-tool";

/// 一次回合的最终状态。
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TurnStatus {
    /// 模型给出最终答复（无更多工具调用）。
    Completed,
    /// 达到迭代上限，仍未收敛。
    MaxIterations,
    /// 用户或调用方取消。
    Cancelled,
    /// 审批等待超时。
    ApprovalTimeout,
}

/// 一回合的汇总（供 IPC/UI 展示）。
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct TurnOutput {
    pub session_id: String,
    pub status: TurnStatus,
    /// 模型的各轮文本（最后一轮即最终答复）。
    pub replies: Vec<String>,
    pub tool_calls: usize,
    /// 本回合实际发送给远程模型的实例上下文（已脱敏）。
    /// 6.7 隐私要求"发送范围清晰且可见"——UI 据此展示。
    pub context_sent: Option<Value>,
}

/// 执行循环参数。
#[derive(Debug, Clone)]
pub struct AgentServiceConfig {
    /// 单回合最多几次"模型 → 工具"迭代。
    pub max_iterations: usize,
    /// 构建请求时保留的最近消息条数（上下文窗口控制）。
    pub max_history: usize,
    /// 单个工具反馈注入对话前的截断长度（字符）。
    pub tool_feedback_max_chars: usize,
    /// 单次审批等待上限。
    pub approval_timeout: Duration,
    /// 审批轮询间隔。
    pub approval_poll_interval: Duration,
    /// 会话记录里的模型名（`session_started` 事件用）。
    pub model: String,
}

impl Default for AgentServiceConfig {
    fn default() -> Self {
        Self {
            max_iterations: 8,
            max_history: 24,
            tool_feedback_max_chars: 16_000,
            approval_timeout: Duration::from_secs(10 * 60),
            approval_poll_interval: Duration::from_millis(50),
            model: String::new(),
        }
    }
}

/// 单个工具调用在循环里的落点。
enum StepOutcome {
    /// 把这一行反馈给模型，继续迭代。
    Feedback(String),
    /// 回合到此为止（取消 / 审批超时）。
    Stop(TurnStatus),
}

/// 审批中介：`proposed → approved/rejected` 的任务快照推进 + 事件留痕。
pub struct ApprovalBroker {
    store: Arc<SessionStore>,
}

/// `wait_resolution` 的结果（取消用 `Err` 表达，与 [`CancelToken`] 语义一致）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Resolution {
    Approved,
    Rejected,
    TimedOut,
}

impl ApprovalBroker {
    pub fn new(store: Arc<SessionStore>) -> Self {
        Self { store }
    }

    /// 生成待确认任务并写 `approval_requested` 事件。
    pub fn propose(
        &self,
        session_id: &str,
        tool_id: &str,
        risk_level: RiskLevel,
        summary: &str,
        payload: &Value,
    ) -> Result<TaskSnapshot> {
        let task_id = uuid::Uuid::new_v4().to_string();
        let snapshot = TaskSnapshot::proposed(
            task_id,
            Some(session_id.to_string()),
            format!("tool_call:{tool_id}"),
            payload,
        )?;
        self.store.write_task(&snapshot)?;
        self.store.append(
            session_id,
            SessionEventKind::approval_requested(
                snapshot.task_id.clone(),
                tool_id,
                risk_level,
                summary,
            ),
        )?;
        Ok(snapshot)
    }

    /// 用户决断：写快照 + `approval_resolved` 事件。
    ///
    /// 非 `proposed` 状态（已决断/不存在）直接报错，不静默重复决断。
    pub fn resolve(
        &self,
        task_id: &str,
        approved: bool,
        reason: Option<&str>,
    ) -> Result<TaskSnapshot> {
        let snapshot = self
            .store
            .read_task(task_id)?
            .ok_or_else(|| not_found_task(task_id))?;
        if snapshot.status != TaskStatus::Proposed {
            return Err(RmcccError::Validation(format!(
                "task {task_id} is already {:?}",
                snapshot.status
            )));
        }
        let updated = snapshot.clone().with_status(if approved {
            TaskStatus::Approved
        } else {
            TaskStatus::Rejected
        });
        self.store.write_task(&updated)?;
        if let Some(session_id) = &snapshot.session_id {
            self.store.append(
                session_id,
                SessionEventKind::approval_resolved(task_id, approved, reason),
            )?;
        }
        Ok(updated)
    }

    /// 轮询等待某个任务被决断；取消返回 `Err`。
    pub async fn wait_resolution(
        &self,
        task_id: &str,
        timeout: Duration,
        poll_interval: Duration,
        cancel: &CancelToken,
    ) -> Result<Resolution> {
        let deadline = tokio::time::Instant::now() + timeout;
        loop {
            if cancel.is_cancelled() {
                return Err(RmcccError::Other("agent run cancelled".into()));
            }
            let snapshot = self
                .store
                .read_task(task_id)?
                .ok_or_else(|| not_found_task(task_id))?;
            match snapshot.status {
                TaskStatus::Approved => return Ok(Resolution::Approved),
                TaskStatus::Rejected => return Ok(Resolution::Rejected),
                _ => {}
            }
            if tokio::time::Instant::now() >= deadline {
                return Ok(Resolution::TimedOut);
            }
            tokio::time::sleep(poll_interval).await;
        }
    }

    /// 仍待决断的任务（按创建时间倒序）。
    pub fn pending(&self) -> Result<Vec<TaskSnapshot>> {
        Ok(self
            .store
            .list_tasks()?
            .into_iter()
            .filter(|task| task.status == TaskStatus::Proposed)
            .collect())
    }
}

/// Agent 执行循环（Phase 6.5）。
///
/// 持有会话存储、只读工具注册表与 LLM provider，把"模型响应 → 受校验的
/// 工具调用 → 规范化结果 → （需要的）审批 → 审计"编排成一个可取消的回合。
pub struct AgentService {
    store: Arc<SessionStore>,
    registry: Arc<ToolRegistry>,
    provider: Arc<dyn LlmProvider>,
    broker: ApprovalBroker,
    config: AgentServiceConfig,
}

impl AgentService {
    pub fn new(
        store: Arc<SessionStore>,
        registry: Arc<ToolRegistry>,
        provider: Arc<dyn LlmProvider>,
        config: AgentServiceConfig,
    ) -> Self {
        let broker = ApprovalBroker::new(store.clone());
        Self {
            store,
            registry,
            provider,
            broker,
            config,
        }
    }

    pub fn store(&self) -> &SessionStore {
        &self.store
    }

    pub fn broker(&self) -> &ApprovalBroker {
        &self.broker
    }

    /// 跑一整个回合：用户消息 → （模型 ↔ 工具）× N → 最终答复。
    pub async fn run_turn(
        &self,
        message: &str,
        profile_id: Option<&str>,
        cancel: CancelToken,
    ) -> Result<TurnOutput> {
        self.run_turn_with_context(message, profile_id, None, cancel)
            .await
    }

    /// 同 [`Self::run_turn`]，并注入宿主提供的实例上下文（6.7）。
    ///
    /// 上下文在进入提示词**之前**脱敏（home 前缀 → `~`），并原样出现在
    /// [`TurnOutput::context_sent`] 里供 UI 展示——发送范围对用户可见。
    pub async fn run_turn_with_context(
        &self,
        message: &str,
        profile_id: Option<&str>,
        context: Option<Value>,
        cancel: CancelToken,
    ) -> Result<TurnOutput> {
        if self.config.model.is_empty() {
            return Err(RmcccError::Validation(
                "agent service has no model configured".into(),
            ));
        }
        let session_id =
            self.store
                .create_session(self.provider.id(), &self.config.model, profile_id)?;
        let mut tool_calls_total = 0usize;
        let mut replies = Vec::new();
        let mut status = TurnStatus::Completed;

        // 6.7：上下文进提示词前脱敏；同时在回合输出里保留一份供 UI 展示。
        let context_sent = context
            .map(|value| crate::redact::redact_json(&value, crate::redact::user_home().as_deref()));

        let outcome = self
            .run_turn_inner(
                &session_id,
                message,
                context_sent.as_ref(),
                &mut replies,
                &mut tool_calls_total,
                &mut status,
                &cancel,
            )
            .await;

        let reason = match (&outcome, status) {
            (Err(_), _) => "error".to_string(),
            (_, TurnStatus::Cancelled) => "cancelled".to_string(),
            (_, TurnStatus::MaxIterations) => "max_iterations".to_string(),
            (_, TurnStatus::ApprovalTimeout) => "approval_timeout".to_string(),
            (_, TurnStatus::Completed) => "completed".to_string(),
        };
        // 会话收尾事件尽力而为；写失败不吞掉原始错误。
        if let Err(event_error) = self.store.append(
            &session_id,
            SessionEventKind::session_ended(reason, tool_calls_total as u32),
        ) {
            tracing::error!(session = %session_id, error = %event_error, "failed to append session_ended");
        }
        outcome?;
        Ok(TurnOutput {
            session_id,
            status,
            replies,
            tool_calls: tool_calls_total,
            context_sent,
        })
    }

    #[allow(clippy::too_many_arguments)]
    async fn run_turn_inner(
        &self,
        session_id: &str,
        message: &str,
        context: Option<&Value>,
        replies: &mut Vec<String>,
        tool_calls_total: &mut usize,
        status: &mut TurnStatus,
        cancel: &CancelToken,
    ) -> Result<()> {
        self.store
            .append(session_id, SessionEventKind::user_message(message))?;

        let mut history = vec![ChatMessage::system(system_prompt(&self.registry, context))];
        history.push(ChatMessage::user(message));

        for _iteration in 0..self.config.max_iterations {
            if cancel.is_cancelled() {
                *status = TurnStatus::Cancelled;
                return Ok(());
            }

            let request = llm::ChatRequest::new(trim_history(&history, self.config.max_history));
            let stream = self
                .provider
                .stream_chat(request, cancel.clone())
                .map_err(|e| RmcccError::Other(format!("LLM request failed: {e}")))?;
            let outcome = llm::collect(stream)
                .await
                .map_err(|e| RmcccError::Other(format!("LLM stream failed: {e}")))?;
            if cancel.is_cancelled() {
                *status = TurnStatus::Cancelled;
                return Ok(());
            }

            self.store
                .append(session_id, SessionEventKind::model_response(&outcome.text))?;
            history.push(ChatMessage {
                role: ChatRole::Assistant,
                content: outcome.text.clone(),
            });

            let calls = parse_tool_calls(&outcome.text)?;
            if calls.is_empty() {
                replies.push(outcome.text);
                return Ok(());
            }

            let mut feedback = Vec::new();
            for call in calls {
                *tool_calls_total += 1;
                match self.execute_call(session_id, &call, cancel).await? {
                    StepOutcome::Feedback(line) => feedback.push(line),
                    StepOutcome::Stop(stop) => {
                        *status = stop;
                        return Ok(());
                    }
                }
            }
            history.push(ChatMessage::user(feedback.join("\n")));
        }

        *status = TurnStatus::MaxIterations;
        Ok(())
    }

    /// 执行一个解析出的工具调用请求（含按策略走审批），返回循环落点。
    async fn execute_call(
        &self,
        session_id: &str,
        call: &ToolCallRequest,
        cancel: &CancelToken,
    ) -> Result<StepOutcome> {
        let Some(descriptor) = self.registry.descriptor(&call.tool) else {
            // 模型点名了未注册工具：不执行，把错误反馈给模型。
            self.store
                .append(
                    session_id,
                    SessionEventKind::error("tool_error", &format!("unknown tool {}", call.tool)),
                )
                .ok();
            return Ok(StepOutcome::Feedback(unknown_tool_feedback(&call.tool)));
        };

        // 写工具走两阶段：审阅载荷（dry-run 计划）→ 审批 → invoke_approved。
        // `never` 确认策略的写工具直接拒绝（写操作必须经用户）。
        let is_write = descriptor.risk_level != RiskLevel::ReadOnly;
        let needs_confirmation = descriptor.confirmation_policy != ConfirmationPolicy::Never;
        if is_write && !needs_confirmation {
            self.store
                .append(
                    session_id,
                    SessionEventKind::error(
                        "tool_error",
                        &format!("write tool {} must require confirmation", descriptor.id),
                    ),
                )
                .ok();
            return Ok(StepOutcome::Feedback(format!(
                "{{\"tool\":\"{}\",\"ok\":false,\"error\":\"write tool must require confirmation\"}}",
                descriptor.id
            )));
        }

        // 审批链：策略不是 never 的工具需要用户决断（dry-run 计划进任务快照）。
        let mut approved_task: Option<crate::session::TaskSnapshot> = None;
        if needs_confirmation {
            // 写工具的审阅载荷由工具自己生成（如重新扫描得到的 dry-run 计划）；
            // 只读工具用 {tool, input} 形状。
            let payload = if is_write {
                self.registry
                    .approval_payload(&descriptor.id, &call.input)
                    .unwrap_or_else(|error| {
                        Some(json!({ "tool": descriptor.id, "prepare_error": error.to_string() }))
                    })
                    .unwrap_or_else(|| json!({ "tool": descriptor.id, "input": call.input }))
            } else {
                json!({ "tool": descriptor.id, "input": call.input })
            };
            let summary = format!(
                "调用 {}（{}）",
                descriptor.name,
                descriptor.risk_level.as_str()
            );
            let proposed = self.broker.propose(
                session_id,
                &descriptor.id,
                descriptor.risk_level,
                &summary,
                &payload,
            )?;
            match self
                .broker
                .wait_resolution(
                    &proposed.task_id,
                    self.config.approval_timeout,
                    self.config.approval_poll_interval,
                    cancel,
                )
                .await
            {
                Ok(Resolution::Approved) => {
                    approved_task = self.store.read_task(&proposed.task_id)?;
                }
                Ok(Resolution::Rejected) => {
                    return Ok(StepOutcome::Feedback(rejected_feedback(&descriptor.id)));
                }
                Ok(Resolution::TimedOut) => {
                    self.store
                        .append(
                            session_id,
                            SessionEventKind::error(
                                "approval_timeout",
                                &format!("task {} timed out", proposed.task_id),
                            ),
                        )
                        .ok();
                    return Ok(StepOutcome::Stop(TurnStatus::ApprovalTimeout));
                }
                Err(_) if cancel.is_cancelled() => {
                    return Ok(StepOutcome::Stop(TurnStatus::Cancelled));
                }
                Err(error) => return Err(error),
            }
        }

        self.store.append(
            session_id,
            SessionEventKind::tool_call(descriptor.id.clone(), descriptor.risk_level, &call.input),
        )?;

        let started = std::time::Instant::now();
        let result = match approved_task {
            // 写工具：唯一执行通道是 invoke_approved（校验批准快照）。
            Some(ref task) => {
                self.registry
                    .invoke_approved(session_id, task, &descriptor.id, &call.input)
            }
            None => self
                .registry
                .invoke_in_session(session_id, &descriptor.id, &call.input),
        };
        let duration = started.elapsed();

        // 写工具执行完把任务推进到 applied / failed（单次执行，不可重放）。
        if let Some(task) = approved_task.as_ref() {
            let new_status = if result.is_ok() {
                crate::session::TaskStatus::Applied
            } else {
                crate::session::TaskStatus::Failed
            };
            if let Ok(Some(snapshot)) = self.store.read_task(&task.task_id) {
                if snapshot.status == crate::session::TaskStatus::Approved {
                    let _ = self.store.write_task(&snapshot.with_status(new_status));
                }
            }
        }

        let line = match &result {
            Ok(tool_result) => {
                self.store
                    .append(
                        session_id,
                        SessionEventKind::tool_result(tool_result, duration),
                    )
                    .ok();
                let output_text =
                    serde_json::to_string(&tool_result.output).unwrap_or_else(|_| "{}".into());
                truncate_chars(&output_text, self.config.tool_feedback_max_chars)
            }
            Err(error) => {
                self.store
                    .append(
                        session_id,
                        SessionEventKind::error("tool_error", &error.to_string()),
                    )
                    .ok();
                truncate_chars(&error.to_string(), self.config.tool_feedback_max_chars)
            }
        };
        let ok = result.is_ok();
        Ok(StepOutcome::Feedback(format!(
            "{{\"tool\":\"{}\",\"ok\":{},\"output\":{}}}",
            descriptor.id, ok, line
        )))
    }
}

fn not_found_task(task_id: &str) -> RmcccError {
    RmcccError::Io(std::io::Error::new(
        ErrorKind::NotFound,
        format!("unknown agent task: {task_id}"),
    ))
}

fn unknown_tool_feedback(tool: &str) -> String {
    format!("{{\"tool\":\"{tool}\",\"ok\":false,\"error\":\"unknown tool\"}}")
}

fn rejected_feedback(tool: &str) -> String {
    format!("{{\"tool\":\"{tool}\",\"ok\":false,\"error\":\"rejected by user\"}}")
}

/// 一个解析出的工具调用请求。
#[derive(Debug, Clone, PartialEq)]
pub struct ToolCallRequest {
    pub tool: String,
    pub input: Value,
}

/// 从模型文本解析 ```rmccc-tool 围栏里的工具调用。
///
/// 严格契约：每块必须恰好是 `{"tool": <id>, "input": <object>}`；
/// 其他任何形状都是解析错误（宁可让这一轮失败，也不能把任意 JSON 当调用执行）。
pub fn parse_tool_calls(text: &str) -> Result<Vec<ToolCallRequest>> {
    let mut calls = Vec::new();
    let mut rest = text;
    while let Some(start) = rest.find(TOOL_FENCE) {
        let after_fence = &rest[start + TOOL_FENCE.len()..];
        let Some(end) = after_fence.find("```") else {
            return Err(RmcccError::Validation(
                "tool call fence is not closed".into(),
            ));
        };
        let body = after_fence[..end].trim();
        let value: Value = serde_json::from_str(body)
            .map_err(|e| RmcccError::Validation(format!("tool call is not valid JSON: {e}")))?;
        let tool = value
            .get("tool")
            .and_then(Value::as_str)
            .ok_or_else(|| RmcccError::Validation("tool call missing 'tool'".into()))?
            .to_string();
        let input = value
            .get("input")
            .cloned()
            .ok_or_else(|| RmcccError::Validation("tool call missing 'input'".into()))?;
        if !input.is_object() {
            return Err(RmcccError::Validation(
                "tool call 'input' must be an object".into(),
            ));
        }
        calls.push(ToolCallRequest { tool, input });
        rest = &after_fence[end + 3..];
    }
    Ok(calls)
}

/// 系统提示：把工具目录与（可选的）实例上下文交给模型，并约束调用格式。
fn system_prompt(registry: &ToolRegistry, context: Option<&Value>) -> String {
    let mut prompt = String::from(
        "You are the RMCCC launcher assistant. You can call read-only tools to inspect \
         the launcher state. Rules:\n\
         1. To call a tool, emit a fenced block exactly like:\n\
         ```rmccc-tool\n{\"tool\": \"<tool-id>\", \"input\": {}}\n```\n\
         2. One call per block; wait for the next user message that carries the results.\n\
         3. Never invent tool ids or input fields.\n\
         4. Text outside these blocks is shown to the user verbatim.\n\nAvailable tools:\n",
    );
    for descriptor in registry.descriptors() {
        prompt.push_str(&format!(
            "- {} ({}, confirmation: {:?}): {}\n  input schema: {}\n",
            descriptor.id,
            descriptor.risk_level.as_str(),
            descriptor.confirmation_policy,
            descriptor.description,
            descriptor.input_schema
        ));
    }
    if let Some(context) = context {
        prompt.push_str(
            "\n\nInstance context (host-provided, read-only facts; \
                         treat as ground truth, do not ask the user to re-enter them):\n",
        );
        prompt.push_str(&serde_json::to_string_pretty(context).unwrap_or_default());
    }
    prompt
}

fn trim_history(history: &[ChatMessage], max: usize) -> Vec<ChatMessage> {
    if history.len() <= max || history.is_empty() {
        return history.to_vec();
    }
    // 保留首条 system 与最近 max-1 条。
    let mut out = vec![history[0].clone()];
    out.extend_from_slice(&history[history.len() - (max - 1)..]);
    out
}

fn truncate_chars(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_string();
    }
    let truncated: String = text.chars().take(max).collect();
    format!("{truncated}…")
}
