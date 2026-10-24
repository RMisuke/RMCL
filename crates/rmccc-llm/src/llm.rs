//! LLM provider adapter（OpenAI 兼容，ADR-013「轻 AI」内核）。
//!
//! # 为什么是"adapter + 事件流"而不是直接调 HTTP
//!
//! 调用方（内置日志分析、插件 `llm.chat`、未来的 MCP 日志工具）需要三件
//! 与供应商无关的东西：
//!
//! 1. **统一的流式事件**：`Delta` / `Usage` / `Done` / `Error`，不暴露 SSE 细节；
//! 2. **统一的中止、超时、重试、限流**语义——尤其"取消"必须能打断正在收的流；
//! 3. **模型探测**（[`probe_models`]），供设置页"输入地址 → 自动检测可用模型"。
//!
//! 本模块只实现 **OpenAI 兼容** 的 chat completions 流（OpenAI / DeepSeek / GLM /
//! Ollama / llama.cpp 等都遵循同一形状）。新增供应商 = 新增一个 [`LlmProvider`]
//! 实现，不改上层。
//!
//! 明确**不做**（ADR-013）：多轮编排、工具调用（function calling 透传）、
//! 会话存储、Token 计费——那些属于外部 Agent 工具的职责。
//!
//! # 安全
//!
//! - API Key 由 [`crate::credentials`] 解密后传入，不在本模块落盘或打印。
//! - 端点策略（HTTPS / loopback HTTP）在构造时校验一次（ADR-009 决策 5）。
//! - 模型输出一律视为**不可信文本**：本模块不做 Markdown/HTML 渲染。

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use thiserror::Error;
use tokio::sync::{mpsc, Mutex};
use url::Url;

use crate::credentials::{validate_endpoint, ServiceCredential};

/// 默认远程端点（未配置 `base_url` 时）。
pub const DEFAULT_OPENAI_BASE: &str = "https://api.openai.com/v1";

/// 对话角色。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChatRole {
    System,
    User,
    Assistant,
    Tool,
}

impl ChatRole {
    fn as_wire(self) -> &'static str {
        match self {
            ChatRole::System => "system",
            ChatRole::User => "user",
            ChatRole::Assistant => "assistant",
            ChatRole::Tool => "tool",
        }
    }
}

/// 一条消息。`content` 是纯文本；多模态/工具调用留到后续切片。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: ChatRole,
    pub content: String,
}

impl ChatMessage {
    pub fn system(content: impl Into<String>) -> Self {
        Self {
            role: ChatRole::System,
            content: content.into(),
        }
    }

    pub fn user(content: impl Into<String>) -> Self {
        Self {
            role: ChatRole::User,
            content: content.into(),
        }
    }
}

/// 一次对话请求（模型由 provider 持有，避免上层拼供应商字段）。
#[derive(Debug, Clone, Default)]
pub struct ChatRequest {
    pub messages: Vec<ChatMessage>,
    pub temperature: Option<f32>,
    pub max_tokens: Option<u32>,
}

impl ChatRequest {
    pub fn new(messages: Vec<ChatMessage>) -> Self {
        Self {
            messages,
            temperature: None,
            max_tokens: None,
        }
    }

    fn to_wire(&self, model: &str) -> Value {
        let messages: Vec<Value> = self
            .messages
            .iter()
            .map(|m| json!({ "role": m.role.as_wire(), "content": m.content }))
            .collect();
        let mut body = json!({
            "model": model,
            "messages": messages,
            "stream": true,
        });
        if let Some(temperature) = self.temperature {
            body["temperature"] = json!(temperature);
        }
        if let Some(max_tokens) = self.max_tokens {
            body["max_tokens"] = json!(max_tokens);
        }
        body
    }
}

/// 模型能力声明。loop 用它做上下文预算与降级判断。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ModelCapabilities {
    pub context_window: usize,
    pub max_output_tokens: usize,
    pub supports_streaming: bool,
    pub supports_tools: bool,
}

impl Default for ModelCapabilities {
    fn default() -> Self {
        Self {
            context_window: 128_000,
            max_output_tokens: 4_096,
            supports_streaming: true,
            supports_tools: false,
        }
    }
}

/// 流式事件。`Error` 之后 channel 关闭。
#[derive(Debug, Clone, PartialEq)]
pub enum LlmEvent {
    Delta(String),
    Usage {
        prompt_tokens: u32,
        completion_tokens: u32,
    },
    Done {
        finish_reason: Option<String>,
    },
    Error(LlmError),
}

/// 适配器错误。与 `RmcccError` 分开，避免把"用户取消"和"输入非法"混进全局错误码。
#[derive(Debug, Clone, Error, PartialEq, Eq)]
pub enum LlmError {
    #[error("LLM request failed: {0}")]
    Http(String),
    #[error("LLM provider returned HTTP {status}: {body}")]
    Status { status: u16, body: String },
    #[error("LLM request timed out")]
    Timeout,
    #[error("LLM request was cancelled")]
    Cancelled,
    #[error("LLM provider is rate limiting (retry_after={retry_after_ms:?} ms)")]
    RateLimited { retry_after_ms: Option<u64> },
    #[error("LLM provider error: {0}")]
    Provider(String),
    #[error("invalid LLM response: {0}")]
    InvalidResponse(String),
    #[error("invalid LLM configuration: {0}")]
    Validation(String),
}

impl LlmError {
    /// 是否值得重试：网络错误、429、5xx。4xx（除 429）与解析错误不重试。
    pub fn retryable(&self) -> bool {
        match self {
            LlmError::Http(_) | LlmError::Timeout | LlmError::RateLimited { .. } => true,
            LlmError::Status { status, .. } => *status == 429 || *status >= 500,
            _ => false,
        }
    }
}

impl From<LlmError> for rmccc_error::RmcccError {
    fn from(error: LlmError) -> Self {
        match error {
            LlmError::Http(message) => rmccc_error::RmcccError::Network(message),
            LlmError::Timeout => rmccc_error::RmcccError::Timeout(0),
            LlmError::Status { status, body } => {
                rmccc_error::RmcccError::HttpStatus { status, url: body }
            }
            LlmError::RateLimited { .. } | LlmError::Provider(_) | LlmError::InvalidResponse(_) => {
                rmccc_error::RmcccError::Network(error.to_string())
            }
            LlmError::Cancelled | LlmError::Validation(_) => {
                rmccc_error::RmcccError::Validation(error.to_string())
            }
        }
    }
}

/// 取消令牌（克隆共享同一标志）。loop 在用户点"停止"时调用 [`CancelToken::cancel`]。
#[derive(Debug, Clone, Default)]
pub struct CancelToken {
    flag: Arc<AtomicBool>,
}

impl CancelToken {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn cancel(&self) {
        self.flag.store(true, Ordering::SeqCst);
    }

    pub fn is_cancelled(&self) -> bool {
        self.flag.load(Ordering::SeqCst)
    }
}

/// 重试策略（指数退避，封顶 10s）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RetryPolicy {
    pub max_attempts: u32,
    pub base_delay: Duration,
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self {
            max_attempts: 3,
            base_delay: Duration::from_millis(200),
        }
    }
}

impl RetryPolicy {
    /// 第 `attempt` 次失败后应等待多久（attempt 从 1 开始）。
    pub fn delay_for(&self, attempt: u32) -> Duration {
        let factor = 1u64 << attempt.saturating_sub(1).min(5);
        let delay = self.base_delay.saturating_mul(factor as u32);
        delay.min(Duration::from_secs(10))
    }

    /// 只在还有尝试次数、且错误可重试时重试。
    pub fn should_retry(&self, attempt: u32, error: &LlmError) -> bool {
        attempt < self.max_attempts && error.retryable()
    }
}

/// 适配器选项。
#[derive(Debug, Clone)]
pub struct LlmOptions {
    /// 单次请求的总超时（含流式读取）。
    pub timeout: Duration,
    pub retry: RetryPolicy,
    /// 两次请求之间的最小间隔（粗粒度限流，避免用户狂点/loop 失控）。
    pub min_request_interval: Duration,
    pub capabilities: ModelCapabilities,
}

impl Default for LlmOptions {
    fn default() -> Self {
        Self {
            timeout: Duration::from_secs(120),
            retry: RetryPolicy::default(),
            min_request_interval: Duration::ZERO,
            capabilities: ModelCapabilities::default(),
        }
    }
}

/// 事件流接收端。
pub type LlmStream = mpsc::Receiver<LlmEvent>;

/// 供应商无关接口。
pub trait LlmProvider: Send + Sync {
    fn id(&self) -> &str;
    fn capabilities(&self) -> ModelCapabilities;
    /// 启动一次流式请求。返回的 receiver 在 `Done`/`Error` 后关闭；
    /// `cancel` 可在任意时刻打断（包括正在读取响应体）。
    fn stream_chat(&self, request: ChatRequest, cancel: CancelToken)
        -> Result<LlmStream, LlmError>;
}

/// OpenAI 兼容 chat completions 适配器。
pub struct OpenAiCompatibleProvider {
    id: String,
    client: reqwest::Client,
    endpoint: Url,
    api_key: String,
    model: String,
    options: LlmOptions,
    rate_limiter: Arc<RateLimiter>,
}

impl OpenAiCompatibleProvider {
    /// 由已解密的凭据构造。校验：model 必填、端点策略、URL 可拼接。
    pub fn new(credential: &ServiceCredential, options: LlmOptions) -> Result<Self, LlmError> {
        let base = credential
            .base_url
            .as_deref()
            .unwrap_or(DEFAULT_OPENAI_BASE);
        let base = validate_endpoint(base).map_err(|e| LlmError::Validation(e.to_string()))?;
        let endpoint = chat_completions_url(&base)?;
        let model = credential
            .model
            .clone()
            .ok_or_else(|| LlmError::Validation("credential has no model id".into()))?;
        if !options.capabilities.supports_streaming {
            return Err(LlmError::Validation(
                "provider configured without streaming support".into(),
            ));
        }
        let client = reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(10))
            .build()
            .map_err(|e| LlmError::Http(e.to_string()))?;
        Ok(Self {
            id: credential.service.clone(),
            client,
            endpoint,
            api_key: credential.api_key.clone(),
            model,
            rate_limiter: Arc::new(RateLimiter::new(options.min_request_interval)),
            options,
        })
    }

    fn request_body(&self, request: &ChatRequest) -> Value {
        request.to_wire(&self.model)
    }
}

impl LlmProvider for OpenAiCompatibleProvider {
    fn id(&self) -> &str {
        &self.id
    }

    fn capabilities(&self) -> ModelCapabilities {
        self.options.capabilities
    }

    fn stream_chat(
        &self,
        request: ChatRequest,
        cancel: CancelToken,
    ) -> Result<LlmStream, LlmError> {
        if request.messages.is_empty() {
            return Err(LlmError::Validation("chat request has no messages".into()));
        }
        if cancel.is_cancelled() {
            return Err(LlmError::Cancelled);
        }

        let (tx, rx) = mpsc::channel(64);
        let client = self.client.clone();
        let endpoint = self.endpoint.clone();
        let api_key = self.api_key.clone();
        let body = self.request_body(&request);
        let timeout = self.options.timeout;
        let retry = self.options.retry;
        let limiter = self.rate_limiter.clone();

        tokio::spawn(async move {
            let outcome = tokio::time::timeout(
                timeout,
                run_stream(
                    &client, &endpoint, &api_key, body, retry, limiter, &cancel, &tx,
                ),
            )
            .await;

            let event = match outcome {
                Err(_) => Some(LlmEvent::Error(LlmError::Timeout)),
                Ok(Err(LlmError::Cancelled)) => None,
                Ok(Err(error)) => Some(LlmEvent::Error(error)),
                Ok(Ok(())) => None,
            };
            if let Some(event) = event {
                let _ = tx.send(event).await;
            }
        });

        Ok(rx)
    }
}

/// 拼接 `…/chat/completions`；已带该后缀则原样使用。
fn chat_completions_url(base: &Url) -> Result<Url, LlmError> {
    let trimmed = base.as_str().trim_end_matches('/');
    if trimmed.ends_with("/chat/completions") {
        return Url::parse(trimmed).map_err(|e| LlmError::Validation(e.to_string()));
    }
    Url::parse(&format!("{trimmed}/chat/completions"))
        .map_err(|e| LlmError::Validation(e.to_string()))
}

/// 探测端点的可用模型列表（`GET {base}/models`，OpenAI 兼容）。
///
/// 设置页"输入 API 地址 → 自动检测可用模型"用：不需要已保存的模型，
/// 也不写任何状态。`api_key` 可为空串（本地模型端点通常不需要鉴权）。
/// 返回按字典序排序去重的模型 id（部分网关返回顺序不稳定）。
pub async fn probe_models(base_url: &str, api_key: &str) -> Result<Vec<String>, LlmError> {
    let base = validate_endpoint(base_url).map_err(|e| LlmError::Validation(e.to_string()))?;
    let endpoint = models_url(&base)?;
    let client = reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_secs(30))
        .build()
        .map_err(|e| LlmError::Http(e.to_string()))?;
    let response = client
        .get(endpoint)
        .bearer_auth(api_key)
        .send()
        .await
        .map_err(|e| LlmError::Http(e.to_string()))?;
    let status = response.status();
    if !status.is_success() {
        let text = response.text().await.unwrap_or_default();
        return Err(LlmError::Status {
            status: status.as_u16(),
            body: truncate(&text, 512),
        });
    }
    let value: Value = response
        .json()
        .await
        .map_err(|e| LlmError::InvalidResponse(e.to_string()))?;
    let mut ids: Vec<String> = value
        .get("data")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(|item| item.get("id").and_then(Value::as_str))
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default();
    ids.sort();
    ids.dedup();
    Ok(ids)
}

/// 拼接 `…/models`；已带该后缀则原样使用。
fn models_url(base: &Url) -> Result<Url, LlmError> {
    let trimmed = base.as_str().trim_end_matches('/');
    if trimmed.ends_with("/models") {
        return Url::parse(trimmed).map_err(|e| LlmError::Validation(e.to_string()));
    }
    Url::parse(&format!("{trimmed}/models")).map_err(|e| LlmError::Validation(e.to_string()))
}

#[allow(clippy::too_many_arguments)]
async fn run_stream(
    client: &reqwest::Client,
    endpoint: &Url,
    api_key: &str,
    body: Value,
    retry: RetryPolicy,
    limiter: Arc<RateLimiter>,
    cancel: &CancelToken,
    tx: &mpsc::Sender<LlmEvent>,
) -> Result<(), LlmError> {
    let mut attempt = 0u32;
    loop {
        attempt += 1;
        if cancel.is_cancelled() {
            return Err(LlmError::Cancelled);
        }
        limiter.wait().await;

        let send = client
            .post(endpoint.clone())
            .bearer_auth(api_key)
            .header("accept", "text/event-stream")
            .json(&body)
            .send()
            .await;

        let mut response = match send {
            Ok(response) => response,
            Err(error) => {
                if cancel.is_cancelled() {
                    return Err(LlmError::Cancelled);
                }
                let llm_error = LlmError::Http(error.to_string());
                if retry.should_retry(attempt, &llm_error) {
                    tokio::time::sleep(retry.delay_for(attempt)).await;
                    continue;
                }
                return Err(llm_error);
            }
        };

        let status = response.status();
        if !status.is_success() {
            let retry_after = response
                .headers()
                .get("retry-after")
                .and_then(|v| v.to_str().ok())
                .and_then(|v| v.parse::<u64>().ok())
                .map(|secs| secs.saturating_mul(1000));
            let text = response.text().await.unwrap_or_default();
            let error = if status.as_u16() == 429 {
                LlmError::RateLimited {
                    retry_after_ms: retry_after,
                }
            } else {
                LlmError::Status {
                    status: status.as_u16(),
                    body: truncate(&text, 512),
                }
            };
            if retry.should_retry(attempt, &error) {
                sleep_for_retry(&retry, attempt, retry_after).await;
                continue;
            }
            return Err(error);
        }

        stream_response(&mut response, cancel, tx).await?;
        return Ok(());
    }
}

async fn sleep_for_retry(retry: &RetryPolicy, attempt: u32, retry_after_ms: Option<u64>) {
    let delay = match retry_after_ms {
        Some(ms) => Duration::from_millis(ms).min(Duration::from_secs(10)),
        None => retry.delay_for(attempt),
    };
    tokio::time::sleep(delay).await;
}

async fn stream_response(
    response: &mut reqwest::Response,
    cancel: &CancelToken,
    tx: &mpsc::Sender<LlmEvent>,
) -> Result<(), LlmError> {
    let mut parser = SseParser::default();
    let mut finish_reason: Option<String> = None;

    loop {
        if cancel.is_cancelled() {
            return Err(LlmError::Cancelled);
        }
        let chunk = response
            .chunk()
            .await
            .map_err(|e| LlmError::Http(e.to_string()))?;
        let Some(bytes) = chunk else {
            break;
        };
        if cancel.is_cancelled() {
            return Err(LlmError::Cancelled);
        }
        let text = String::from_utf8_lossy(&bytes);
        for payload in parser.push(&text) {
            if cancel.is_cancelled() {
                return Err(LlmError::Cancelled);
            }
            if payload == "[DONE]" {
                let _ = tx
                    .send(LlmEvent::Done {
                        finish_reason: finish_reason.take(),
                    })
                    .await;
                return Ok(());
            }
            let value: Value = serde_json::from_str(&payload).map_err(|e| {
                LlmError::InvalidResponse(format!("{e}: {}", truncate(&payload, 200)))
            })?;
            if let Some(error) = value.get("error") {
                let message = error
                    .get("message")
                    .and_then(|m| m.as_str())
                    .unwrap_or("provider returned an error")
                    .to_string();
                return Err(LlmError::Provider(message));
            }
            if let Some(usage) = value.get("usage") {
                let prompt_tokens = usage
                    .get("prompt_tokens")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0) as u32;
                let completion_tokens = usage
                    .get("completion_tokens")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0) as u32;
                let _ = tx
                    .send(LlmEvent::Usage {
                        prompt_tokens,
                        completion_tokens,
                    })
                    .await;
            }
            let choice = value
                .get("choices")
                .and_then(|c| c.get(0))
                .cloned()
                .unwrap_or(Value::Null);
            if let Some(reason) = choice.get("finish_reason").and_then(|v| v.as_str()) {
                finish_reason = Some(reason.to_string());
            }
            if let Some(content) = choice
                .get("delta")
                .and_then(|d| d.get("content"))
                .and_then(|c| c.as_str())
            {
                if !content.is_empty()
                    && tx.send(LlmEvent::Delta(content.to_string())).await.is_err()
                {
                    return Ok(());
                }
            }
        }
    }

    let _ = tx
        .send(LlmEvent::Done {
            finish_reason: finish_reason.take(),
        })
        .await;
    Ok(())
}

fn truncate(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_string();
    }
    let truncated: String = text.chars().take(max).collect();
    format!("{truncated}…")
}

/// 事件流 → 最终文本；供一次性调用（脚本/测试）使用。
pub async fn collect(mut stream: LlmStream) -> Result<ChatOutcome, LlmError> {
    let mut text = String::new();
    let mut usage = None;
    let mut finish_reason = None;
    while let Some(event) = stream.recv().await {
        match event {
            LlmEvent::Delta(delta) => text.push_str(&delta),
            LlmEvent::Usage {
                prompt_tokens,
                completion_tokens,
            } => usage = Some((prompt_tokens, completion_tokens)),
            LlmEvent::Done { finish_reason: r } => finish_reason = r,
            LlmEvent::Error(error) => return Err(error),
        }
    }
    Ok(ChatOutcome {
        text,
        finish_reason,
        usage,
    })
}

/// 一次对话的聚合结果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChatOutcome {
    pub text: String,
    pub finish_reason: Option<String>,
    pub usage: Option<(u32, u32)>,
}

/// SSE 增量解析：只处理单行 `data:`（OpenAI 兼容端点的实际形状）。
#[derive(Debug, Default)]
struct SseParser {
    buffer: String,
}

impl SseParser {
    fn push(&mut self, chunk: &str) -> Vec<String> {
        self.buffer.push_str(chunk);
        let mut payloads = Vec::new();
        while let Some(index) = self.buffer.find('\n') {
            let line: String = self.buffer.drain(..=index).collect();
            let line = line.trim_end_matches(['\r', '\n']);
            if let Some(rest) = line.strip_prefix("data:") {
                let payload = rest.trim();
                if !payload.is_empty() {
                    payloads.push(payload.to_string());
                }
            }
        }
        payloads
    }
}

/// 请求启动前的最小间隔限流。
struct RateLimiter {
    min_interval: Duration,
    last: Mutex<Option<tokio::time::Instant>>,
}

impl RateLimiter {
    fn new(min_interval: Duration) -> Self {
        Self {
            min_interval,
            last: Mutex::new(None),
        }
    }

    async fn wait(&self) {
        if self.min_interval.is_zero() {
            return;
        }
        let mut last = self.last.lock().await;
        if let Some(previous) = *last {
            let elapsed = previous.elapsed();
            if elapsed < self.min_interval {
                tokio::time::sleep(self.min_interval - elapsed).await;
            }
        }
        *last = Some(tokio::time::Instant::now());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::net::TcpListener;

    fn credential(endpoint: &str, model: &str) -> ServiceCredential {
        ServiceCredential::new("openai", "sk-test-key")
            .expect("cred")
            .with_endpoint(endpoint)
            .expect("endpoint")
            .with_model(model)
            .expect("model")
    }

    /// 本地 SSE 服务器：按脚本逐连接返回（状态码, 分块, 分块间延迟）。
    struct SseServer {
        base: String,
        hits: Arc<std::sync::atomic::AtomicUsize>,
    }

    impl SseServer {
        fn start(script: Vec<(u16, Vec<&'static str>, u64)>) -> Self {
            let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
            let addr = listener.local_addr().expect("addr");
            let hits = Arc::new(std::sync::atomic::AtomicUsize::new(0));
            let counter = hits.clone();
            std::thread::spawn(move || {
                for (status, chunks, delay_ms) in script {
                    let Ok((mut socket, _)) = listener.accept() else {
                        return;
                    };
                    counter.fetch_add(1, Ordering::SeqCst);
                    let mut buf = [0u8; 8192];
                    let _ = socket.read(&mut buf);
                    let reason = if status == 200 { "OK" } else { "Error" };
                    let headers = format!(
                        "HTTP/1.1 {status} {reason}\r\nContent-Type: text/event-stream\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\n"
                    );
                    let _ = socket.write_all(headers.as_bytes());
                    for chunk in chunks {
                        std::thread::sleep(Duration::from_millis(delay_ms));
                        let _ = write!(socket, "{:X}\r\n", chunk.len());
                        let _ = socket.write_all(chunk.as_bytes());
                        let _ = socket.write_all(b"\r\n");
                        let _ = socket.flush();
                    }
                    let _ = socket.write_all(b"0\r\n\r\n");
                }
            });
            Self {
                base: format!("http://{addr}/v1"),
                hits,
            }
        }

        fn hits(&self) -> usize {
            self.hits.load(Ordering::SeqCst)
        }
    }

    fn sse(content: &str) -> String {
        format!("data: {{\"choices\":[{{\"delta\":{{\"content\":\"{content}\"}}}}]}}\n\n")
    }

    fn done_event() -> &'static str {
        "data: {\"choices\":[{\"delta\":{},\"finish_reason\":\"stop\"}]}\n\ndata: [DONE]\n\n"
    }

    #[test]
    fn sse_parser_handles_split_chunks_and_crlf() {
        let mut parser = SseParser::default();
        assert!(parser.push("data: {\"a\":1}").is_empty());
        let payloads = parser.push("\r\n\r\ndata: [DONE]\n");
        assert_eq!(
            payloads,
            vec!["{\"a\":1}".to_string(), "[DONE]".to_string()]
        );
        assert_eq!(parser.push("\n"), Vec::<String>::new());
    }

    #[test]
    fn retry_policy_backs_off_and_caps() {
        let policy = RetryPolicy {
            max_attempts: 5,
            base_delay: Duration::from_millis(100),
        };
        assert_eq!(policy.delay_for(1), Duration::from_millis(100));
        assert_eq!(policy.delay_for(2), Duration::from_millis(200));
        assert_eq!(policy.delay_for(3), Duration::from_millis(400));
        assert_eq!(policy.delay_for(6), Duration::from_millis(3_200));
        assert_eq!(policy.delay_for(60), Duration::from_millis(3_200));
        assert!(policy.should_retry(1, &LlmError::Http("boom".into())));
        assert!(!policy.should_retry(
            1,
            &LlmError::Status {
                status: 400,
                body: "bad".into()
            }
        ));
        assert!(!policy.should_retry(5, &LlmError::Http("boom".into())));
    }

    #[tokio::test]
    async fn streams_deltas_and_reports_usage() {
        let body1: &'static str = Box::leak(sse("Hel").into_boxed_str());
        let body2: &'static str = Box::leak(sse("lo").into_boxed_str());
        let server = SseServer::start(vec![(
            200,
            vec![
                body1,
                body2,
                "data: {\"choices\":[],\"usage\":{\"prompt_tokens\":7,\"completion_tokens\":2}}\n\n",
                done_event(),
            ],
            0,
        )]);
        let provider = OpenAiCompatibleProvider::new(
            &credential(&server.base, "test-model"),
            LlmOptions::default(),
        )
        .expect("provider");
        assert_eq!(provider.id(), "openai");
        assert!(provider.capabilities().supports_streaming);

        let stream = provider
            .stream_chat(
                ChatRequest::new(vec![ChatMessage::user("hi")]),
                CancelToken::new(),
            )
            .expect("stream");
        let outcome = collect(stream).await.expect("outcome");
        assert_eq!(outcome.text, "Hello");
        assert_eq!(outcome.finish_reason.as_deref(), Some("stop"));
        assert_eq!(outcome.usage, Some((7, 2)));
        assert_eq!(server.hits(), 1);
    }

    #[tokio::test]
    async fn retries_server_errors_then_succeeds() {
        let ok_body: &'static str = Box::leak(sse("ok").into_boxed_str());
        let server = SseServer::start(vec![
            (500, vec!["{\"error\":\"upstream\"}"], 0),
            (200, vec![ok_body, done_event()], 0),
        ]);
        let options = LlmOptions {
            retry: RetryPolicy {
                max_attempts: 3,
                base_delay: Duration::from_millis(1),
            },
            ..LlmOptions::default()
        };
        let provider = OpenAiCompatibleProvider::new(&credential(&server.base, "m"), options)
            .expect("provider");
        let outcome = collect(
            provider
                .stream_chat(
                    ChatRequest::new(vec![ChatMessage::user("hi")]),
                    CancelToken::new(),
                )
                .expect("stream"),
        )
        .await
        .expect("outcome");
        assert_eq!(outcome.text, "ok");
        assert_eq!(server.hits(), 2);
    }

    #[tokio::test]
    async fn does_not_retry_client_errors() {
        let server = SseServer::start(vec![(400, vec!["{\"error\":\"bad request\"}"], 0)]);
        let options = LlmOptions {
            retry: RetryPolicy {
                max_attempts: 3,
                base_delay: Duration::from_millis(1),
            },
            ..LlmOptions::default()
        };
        let provider = OpenAiCompatibleProvider::new(&credential(&server.base, "m"), options)
            .expect("provider");
        let outcome = collect(
            provider
                .stream_chat(
                    ChatRequest::new(vec![ChatMessage::user("hi")]),
                    CancelToken::new(),
                )
                .expect("stream"),
        )
        .await;
        assert!(matches!(outcome, Err(LlmError::Status { status: 400, .. })));
        assert_eq!(server.hits(), 1);
    }

    #[tokio::test]
    async fn cancellation_stops_the_stream() {
        let slow: &'static str = Box::leak(sse("first").into_boxed_str());
        let server = SseServer::start(vec![(200, vec![slow, done_event()], 200)]);
        let provider =
            OpenAiCompatibleProvider::new(&credential(&server.base, "m"), LlmOptions::default())
                .expect("provider");
        let cancel = CancelToken::new();
        let mut stream = provider
            .stream_chat(
                ChatRequest::new(vec![ChatMessage::user("hi")]),
                cancel.clone(),
            )
            .expect("stream");

        assert_eq!(
            stream.recv().await,
            Some(LlmEvent::Delta("first".to_string()))
        );
        cancel.cancel();
        // 之后的读取必须在下一 chunk 到达前观察到取消并结束。
        let next = tokio::time::timeout(Duration::from_secs(2), stream.recv())
            .await
            .expect("cancel observed");
        assert!(next.is_none() || matches!(next, Some(LlmEvent::Error(LlmError::Cancelled))));
    }

    #[tokio::test]
    async fn timeout_is_reported_as_an_error_event() {
        let slow: &'static str = Box::leak(sse("late").into_boxed_str());
        let server = SseServer::start(vec![(200, vec![slow], 500)]);
        let options = LlmOptions {
            timeout: Duration::from_millis(80),
            ..LlmOptions::default()
        };
        let provider = OpenAiCompatibleProvider::new(&credential(&server.base, "m"), options)
            .expect("provider");
        let mut stream = provider
            .stream_chat(
                ChatRequest::new(vec![ChatMessage::user("hi")]),
                CancelToken::new(),
            )
            .expect("stream");
        let mut saw_timeout = false;
        while let Some(event) = stream.recv().await {
            if matches!(event, LlmEvent::Error(LlmError::Timeout)) {
                saw_timeout = true;
            }
        }
        assert!(saw_timeout, "expected a Timeout error event");
    }

    #[tokio::test]
    async fn rate_limiter_spaces_requests() {
        let body: &'static str = Box::leak(sse("x").into_boxed_str());
        let server = SseServer::start(vec![
            (200, vec![body, done_event()], 0),
            (200, vec![body, done_event()], 0),
        ]);
        let options = LlmOptions {
            min_request_interval: Duration::from_millis(60),
            ..LlmOptions::default()
        };
        let provider = OpenAiCompatibleProvider::new(&credential(&server.base, "m"), options)
            .expect("provider");
        let start = std::time::Instant::now();
        for _ in 0..2 {
            let outcome = collect(
                provider
                    .stream_chat(
                        ChatRequest::new(vec![ChatMessage::user("hi")]),
                        CancelToken::new(),
                    )
                    .expect("stream"),
            )
            .await
            .expect("outcome");
            assert_eq!(outcome.text, "x");
        }
        assert!(start.elapsed() >= Duration::from_millis(50));
    }

    #[test]
    fn request_requires_messages_and_model() {
        let server = SseServer::start(vec![]);
        let provider =
            OpenAiCompatibleProvider::new(&credential(&server.base, "m"), LlmOptions::default())
                .expect("provider");
        let error = provider
            .stream_chat(ChatRequest::default(), CancelToken::new())
            .expect_err("empty request");
        assert!(matches!(error, LlmError::Validation(_)));

        let no_model = ServiceCredential::new("openai", "sk-a").expect("cred");
        assert!(OpenAiCompatibleProvider::new(&no_model, LlmOptions::default()).is_err());
    }

    #[tokio::test]
    async fn provider_error_payload_is_surfaced() {
        let server = SseServer::start(vec![(
            200,
            vec!["data: {\"error\":{\"message\":\"quota exceeded\"}}\n\n"],
            0,
        )]);
        let provider =
            OpenAiCompatibleProvider::new(&credential(&server.base, "m"), LlmOptions::default())
                .expect("provider");
        let outcome = collect(
            provider
                .stream_chat(
                    ChatRequest::new(vec![ChatMessage::user("hi")]),
                    CancelToken::new(),
                )
                .expect("stream"),
        )
        .await;
        assert_eq!(outcome, Err(LlmError::Provider("quota exceeded".into())));
    }

    #[test]
    fn chat_completions_url_is_idempotent() {
        let base = Url::parse("https://api.openai.com/v1").expect("url");
        let joined = chat_completions_url(&base).expect("joined");
        assert_eq!(
            joined.as_str(),
            "https://api.openai.com/v1/chat/completions"
        );
        let again = chat_completions_url(&joined).expect("again");
        assert_eq!(again, joined);
    }

    /// 极简一次性 JSON 服务器：读完请求后返回固定响应。
    fn json_server(status: u16, body: &'static str) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
        let addr = listener.local_addr().expect("addr");
        std::thread::spawn(move || {
            let Ok((mut socket, _)) = listener.accept() else {
                return;
            };
            let mut buf = [0u8; 8192];
            let _ = socket.read(&mut buf);
            let reason = if status == 200 { "OK" } else { "Error" };
            let response = format!(
                "HTTP/1.1 {status} {reason}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            let _ = socket.write_all(response.as_bytes());
        });
        format!("http://{addr}/v1")
    }

    #[tokio::test]
    async fn probe_models_lists_sorts_and_dedups() {
        let base = json_server(
            200,
            r#"{"data":[{"id":"zeta"},{"id":"alpha"},{"id":"alpha"}]}"#,
        );
        let models = probe_models(&base, "sk-test").await.expect("models");
        assert_eq!(models, vec!["alpha".to_string(), "zeta".to_string()]);
    }

    #[tokio::test]
    async fn probe_models_surfaces_http_errors() {
        let base = json_server(401, r#"{"error":"bad key"}"#);
        let error = probe_models(&base, "sk-wrong").await.expect_err("401");
        assert!(matches!(error, LlmError::Status { status: 401, .. }));
    }

    #[tokio::test]
    async fn probe_models_rejects_non_https_remote_endpoints() {
        let error = probe_models("http://llm.example.com/v1", "sk-a")
            .await
            .expect_err("endpoint policy");
        assert!(matches!(error, LlmError::Validation(_)));
    }

    #[test]
    fn models_url_is_idempotent() {
        let base = Url::parse("https://api.openai.com/v1").expect("url");
        let joined = models_url(&base).expect("joined");
        assert_eq!(joined.as_str(), "https://api.openai.com/v1/models");
        let again = models_url(&joined).expect("again");
        assert_eq!(again, joined);
    }
}
