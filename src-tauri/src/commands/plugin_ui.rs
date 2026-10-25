//! 隔离 UI 的资源服务与 Host Bridge 入口（Phase 4，ADR-006）。
//!
//! 两个入口：
//!
//! 1. [`serve_asset`]：`rmcccplugin://<plugin-id>/<path>` 方案处理器。只服务该插件
//!    `<install-dir>/ui/` 内的白名单文件，响应带严格 CSP。与 `tauri://localhost`
//!    不同源，因此 iframe 拿不到宿主 DOM，也拿不到注入的 Tauri API。
//! 2. [`plugin_ui_invoke`]：桥的 IPC 入口。**每条**消息重新校验协议形状、
//!    插件启用状态与所需能力；前端只是搬运工，不参与授权。
//!
//! 资源与请求的判定规则都放在 `rmccc_plugin::ui`，与 SDK/schema 共用同一张方法表。

use std::borrow::Cow;
use std::time::Duration;

use rmccc_core::rmccc_plugin::ui::{self, BridgeError, BridgeErrorCode};
use rmccc_error::{IpcError, RmcccError};
use serde::Serialize;
use serde_json::{json, Value};
use ts_rs::TS;

use crate::state::AppState;

/// `network.request` 的响应上限（ADR-006）。
const MAX_NETWORK_RESPONSE: usize = 2 * 1024 * 1024;

/// `network.request` 的请求体上限。
const MAX_NETWORK_REQUEST_BODY: usize = 64 * 1024;

// ────────────────────────── 资源服务 ──────────────────────────

/// `rmcccplugin://` 方案处理器：校验来源/路径/授权后返回文件内容。
///
/// 拒绝路径一律返回 403/404 + 纯文本体，不回显包内目录结构。
pub async fn serve_asset(
    app: &tauri::AppHandle,
    request: tauri::http::Request<Vec<u8>>,
) -> tauri::http::Response<Cow<'static, [u8]>> {
    use tauri::Manager as _;

    let uri = request.uri();
    let plugin_id = uri.host().unwrap_or_default();
    let path = uri.path();

    let Ok(asset_path) = ui::validate_asset_path(path) else {
        return text_response(403, "forbidden: asset path");
    };
    if rmccc_plugin_id_is_invalid(plugin_id) {
        return text_response(403, "forbidden: plugin id");
    }

    let state = app.state::<AppState>();
    // 异步协议处理器：await 读锁（绝不阻塞 WKWebView 主线程）。
    let core = state.core.read().await;
    let Some(dir) = core.plugin_manager.installed_dir(plugin_id) else {
        return text_response(404, "plugin not installed");
    };
    // UI 资源只对"已启用且持有 UI 能力"的插件开放；撤销能力后 iframe 立即失效。
    if core
        .plugin_manager
        .ensure_enabled(plugin_id)
        .and_then(|()| {
            core.plugin_manager
                .authorize(plugin_id, "dashboard.cards", None, None)
                .or_else(|_| {
                    core.plugin_manager
                        .authorize(plugin_id, "ui.pages", None, None)
                })
        })
        .is_err()
    {
        return text_response(403, "forbidden: plugin is not allowed to render UI");
    }
    drop(core);

    let file = dir.join(&asset_path);
    let Ok(metadata) = std::fs::metadata(&file) else {
        return text_response(404, "asset not found");
    };
    if !metadata.is_file() {
        return text_response(404, "asset not found");
    }
    if metadata.len() > ui::MAX_ASSET_BYTES {
        return text_response(413, "asset too large");
    }
    let Ok(bytes) = std::fs::read(&file) else {
        return text_response(500, "asset unreadable");
    };

    let content_type = ui::content_type(&asset_path);
    let mut builder = tauri::http::Response::builder()
        .status(200)
        .header("Content-Type", content_type)
        .header("X-Content-Type-Options", "nosniff")
        .header("Cache-Control", "no-store")
        .header("Content-Length", bytes.len().to_string());
    if content_type.starts_with("text/html") {
        // 只在文档上带 CSP：子资源不需要，但带上也无害。
        builder = builder.header("Content-Security-Policy", ui::CSP);
    }
    builder
        .body(Cow::<'static, [u8]>::Owned(bytes))
        .unwrap_or_else(|_| text_response(500, "response error"))
}

fn rmccc_plugin_id_is_invalid(plugin_id: &str) -> bool {
    rmccc_core::rmccc_plugin::path::validate_plugin_id(plugin_id).is_err()
}

fn text_response(status: u16, body: &str) -> tauri::http::Response<Cow<'static, [u8]>> {
    tauri::http::Response::builder()
        .status(status)
        .header("Content-Type", "text/plain; charset=utf-8")
        .header("Cache-Control", "no-store")
        .body(Cow::Owned(body.as_bytes().to_vec()))
        .unwrap_or_else(|_| tauri::http::Response::new(Cow::Owned(Vec::new())))
}

// ────────────────────────── Host Bridge ──────────────────────────

/// 桥调用的返回信封。
///
/// 命令本身永远返回 `Ok`：失败也要带上**线上错误码**，前端原样转发给 iframe。
/// 如果这里返回 `Err(IpcError)`，`BridgeErrorCode` 就会被压成应用内 `ErrorCode`
/// （例如 `host_not_allowed` 变成 `permission_denied`），插件侧就无法区分。
#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export_to = "types.ts")]
pub struct BridgeReply {
    pub ok: bool,
    #[ts(type = "unknown")]
    pub result: Option<Value>,
    pub error: Option<BridgeApiError>,
}

/// 线上错误（与 `schemas/plugin-ui-bridge.schema.json` 的 `wireError` 一致）。
#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export_to = "types.ts")]
pub struct BridgeApiError {
    pub code: String,
    pub message: String,
    pub retryable: bool,
}

impl BridgeReply {
    fn ok(result: Value) -> Self {
        Self {
            ok: true,
            result: Some(result),
            error: None,
        }
    }

    fn err(code: BridgeErrorCode, message: impl Into<String>) -> Self {
        Self {
            ok: false,
            result: None,
            error: Some(BridgeApiError {
                code: code.as_str().to_string(),
                message: message.into(),
                retryable: code.retryable(),
            }),
        }
    }

    fn from_bridge(err: BridgeError) -> Self {
        Self::err(err.code, err.message)
    }

    fn from_host(err: RmcccError) -> Self {
        let code = ui::wire_code_for(&err);
        Self::err(code, err.to_string())
    }
}

/// 协议层校验（纯函数，可单测）：解析原始消息、匹配方法表、校验插件 ID。
///
/// 授权不在这里做——它必须在 [`plugin_ui_invoke`] 里用当前的注册表/权限状态重新判定。
pub(crate) fn prepare_invocation(
    plugin_id: &str,
    raw: &str,
) -> Result<(ui::BridgeRequest, &'static ui::BridgeMethod), BridgeError> {
    let parsed = ui::parse_request(raw)?;
    let method = ui::method(&parsed.method).ok_or_else(|| {
        BridgeError::new(
            BridgeErrorCode::UnsupportedMethod,
            format!("未知方法：{}", parsed.method),
        )
    })?;
    rmccc_plugin_id_checked(plugin_id)?;
    Ok((parsed, method))
}

fn rmccc_plugin_id_checked(plugin_id: &str) -> Result<(), BridgeError> {
    rmccc_core::rmccc_plugin::path::validate_plugin_id(plugin_id).map_err(|_| {
        BridgeError::new(
            BridgeErrorCode::Validation,
            format!("插件 ID 非法：{plugin_id}"),
        )
    })
}

/// 桥的唯一 IPC 入口。
///
/// `request` 是 iframe 发来的**原始** JSON（≤64 KiB），Rust 重新解析而不是相信前端
/// 传来的结构化参数——否则前端被绕过后协议校验就失效了。
#[tauri::command]
pub async fn plugin_ui_invoke(
    state: tauri::State<'_, AppState>,
    plugin_id: String,
    request: String,
) -> Result<BridgeReply, IpcError> {
    let (parsed, method) = match prepare_invocation(&plugin_id, &request) {
        Ok(v) => v,
        Err(e) => return Ok(BridgeReply::from_bridge(e)),
    };

    let params = parsed.params.clone().unwrap_or(Value::Null);
    // network.request 的作用域是目标域名：授权时一起校验，防止"能力已授予但域名未声明"。
    let scope_host = if parsed.method == "network.request" {
        match network_host(&params) {
            Ok(host) => Some(host),
            Err(e) => return Ok(BridgeReply::from_bridge(e)),
        }
    } else {
        None
    };

    // ── 每次调用重新授权 ──
    {
        let core = state.core.read().await;
        if let Err(e) = core.plugin_manager.ensure_enabled(&plugin_id) {
            return Ok(BridgeReply::from_host(e));
        }
        if let Some(capability) = method.capability {
            if let Err(e) =
                core.plugin_manager
                    .authorize(&plugin_id, capability, scope_host.as_deref(), None)
            {
                return Ok(BridgeReply::from_host(e));
            }
        }
    }

    let reply = match parsed.method.as_str() {
        "host.info" => {
            let core = state.core.read().await;
            BridgeReply::ok(json!({
                "uiApiVersion": ui::UI_API_VERSION,
                "appVersion": env!("CARGO_PKG_VERSION"),
                "pluginId": plugin_id,
                "platform": core.plugin_manager.platform_id(),
            }))
        }
        "theme.get" => {
            let mode = params
                .get("mode")
                .and_then(Value::as_str)
                .unwrap_or("dark")
                .to_string();
            let core = state.core.read().await;
            let preference = core.user_settings.theme.clone().unwrap_or_default();
            match crate::commands::dashboard::theme_tokens_for(&core, &preference, &mode)
                .ok()
                .and_then(|tokens| serde_json::to_value(tokens).ok())
            {
                Some(tokens) => BridgeReply::ok(tokens),
                None => BridgeReply::err(BridgeErrorCode::Internal, "主题令牌不可用"),
            }
        }
        "settings.get" => {
            let core = state.core.read().await;
            // 只暴露公开子集：语言、主题偏好、默认内存。绝不含路径或凭据。
            BridgeReply::ok(json!({
                "locale": core.user_settings.locale,
                "theme": core.user_settings.theme,
                "defaultMinMemoryMb": core.config.default_min_memory_mb,
                "defaultMaxMemoryMb": core.config.default_max_memory_mb,
            }))
        }
        "instances.list" => {
            let core = state.core.read().await;
            match core.profile_store.list() {
                Ok(profiles) => {
                    let items: Vec<Value> = profiles
                        .iter()
                        .map(|p| {
                            json!({
                                "id": p.id,
                                "name": p.name,
                                "gameVersion": p.game_version,
                                "loader": p.mod_loader.as_ref().map(|l| json!({
                                    "type": serde_json::to_value(&l.loader_type).unwrap_or(Value::Null),
                                    "version": l.loader_version,
                                })),
                                "memoryMb": p.memory_mb,
                                "lastPlayed": p.last_played,
                            })
                        })
                        .collect();
                    BridgeReply::ok(json!({ "instances": items }))
                }
                Err(e) => BridgeReply::from_host(e),
            }
        }
        "network.request" => match network_request(&state, &params).await {
            Ok(value) => BridgeReply::ok(value),
            Err(e) => BridgeReply::from_bridge(e),
        },
        "llm.chat" => match llm_chat(&state, &plugin_id, &params).await {
            Ok(value) => BridgeReply::ok(value),
            Err(e) => BridgeReply::from_bridge(e),
        },
        other => BridgeReply::err(
            BridgeErrorCode::UnsupportedMethod,
            format!("未知方法：{other}"),
        ),
    };
    Ok(reply)
}

/// 目标域名（授权作用域）：只接受 http(s) 且带 host 的绝对 URL。
fn network_host(params: &Value) -> Result<String, BridgeError> {
    let url = params.get("url").and_then(Value::as_str).ok_or_else(|| {
        BridgeError::new(
            BridgeErrorCode::Validation,
            "network.request 需要 params.url",
        )
    })?;
    let parsed = reqwest::Url::parse(url)
        .map_err(|e| BridgeError::new(BridgeErrorCode::Validation, format!("URL 非法：{e}")))?;
    if !matches!(parsed.scheme(), "http" | "https") {
        return Err(BridgeError::new(
            BridgeErrorCode::Validation,
            "只允许 http/https",
        ));
    }
    parsed
        .host_str()
        .map(str::to_string)
        .ok_or_else(|| BridgeError::new(BridgeErrorCode::Validation, "URL 缺少 host"))
}

/// 受控 HTTP：GET/POST、无重定向、2 MiB 响应上限、30 s 超时。
async fn network_request(state: &AppState, params: &Value) -> Result<Value, BridgeError> {
    let raw_url = params.get("url").and_then(Value::as_str).ok_or_else(|| {
        BridgeError::new(
            BridgeErrorCode::Validation,
            "network.request 需要 params.url",
        )
    })?;
    let url = reqwest::Url::parse(raw_url)
        .map_err(|e| BridgeError::new(BridgeErrorCode::Validation, format!("URL 非法：{e}")))?;
    if !matches!(url.scheme(), "http" | "https") {
        return Err(BridgeError::new(
            BridgeErrorCode::Validation,
            "只允许 http/https",
        ));
    }

    let method = params
        .get("method")
        .and_then(Value::as_str)
        .unwrap_or("GET")
        .to_ascii_uppercase();
    if method != "GET" && method != "POST" {
        return Err(BridgeError::new(
            BridgeErrorCode::Validation,
            "只允许 GET/POST",
        ));
    }
    let body = params.get("body").and_then(Value::as_str);
    if let Some(body) = body {
        if body.len() > MAX_NETWORK_REQUEST_BODY {
            return Err(BridgeError::new(
                BridgeErrorCode::TooLarge,
                format!("请求体超过 {MAX_NETWORK_REQUEST_BODY} 字节"),
            ));
        }
    }

    let mut request = state
        .ui_http_client
        .request(
            reqwest::Method::from_bytes(method.as_bytes()).map_err(|e| {
                BridgeError::new(BridgeErrorCode::Validation, format!("方法非法：{e}"))
            })?,
            url,
        )
        .timeout(Duration::from_secs(30));
    if let Some(body) = body {
        request = request.body(body.to_string());
    }

    let response = request.send().await.map_err(|e| {
        if e.is_timeout() {
            BridgeError::new(BridgeErrorCode::Timeout, format!("请求超时：{e}"))
        } else {
            BridgeError::new(BridgeErrorCode::Internal, format!("请求失败：{e}"))
        }
    })?;
    let status = response.status().as_u16();

    let mut body = Vec::new();
    let mut response = response;
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|e| BridgeError::new(BridgeErrorCode::Internal, format!("读取响应失败：{e}")))?
    {
        if body.len() + chunk.len() > MAX_NETWORK_RESPONSE {
            return Err(BridgeError::new(
                BridgeErrorCode::TooLarge,
                format!("响应超过 {MAX_NETWORK_RESPONSE} 字节上限"),
            ));
        }
        body.extend_from_slice(&chunk);
    }

    Ok(json!({
        "status": status,
        "body": String::from_utf8_lossy(&body),
    }))
}

// ────────────────────────── llm.chat（ADR-013 S6）──────────────────────────

/// `llm.chat` 每插件限速（次/分钟）。
const LLM_PLUGIN_RPM: usize = 6;
/// 单条消息总量上限（请求体本身已有 64 KiB 桥限制，这里防拼装爆炸）。
const MAX_LLM_MESSAGES: usize = 50;
/// 非流式回复上限（与桥响应上限一致）。
const MAX_LLM_REPLY_BYTES: usize = ui::MAX_RESPONSE_BYTES;

/// 插件消费宿主 AI 服务：密钥经 `host_llm_credential` 代持，永不回传插件。
///
/// - 非流式：等待完整回复（宿主侧超时与方法表 `timeout_ms` 对齐，30s）；
/// - 流式：立即返回 `{streamId}`，随后经 `plugin://push` 中继
///   `{type:"llm.delta"|"llm.done"|"llm.error"}`（前端转发进 iframe）。
async fn llm_chat(state: &AppState, plugin_id: &str, params: &Value) -> Result<Value, BridgeError> {
    // ── 限流（每插件 6 次/分钟）──
    if !acquire_llm_slot(state, plugin_id) {
        return Err(BridgeError::new(
            BridgeErrorCode::RateLimited,
            "llm.chat 限流：每插件每分钟最多 6 次调用",
        ));
    }

    let messages = parse_chat_messages(params)?;
    let stream = params
        .get("stream")
        .and_then(Value::as_bool)
        .unwrap_or(false);

    let credential = crate::commands::ai::host_llm_credential(state)
        .await
        .map_err(|e| BridgeError::new(BridgeErrorCode::Internal, e.to_string()))?;
    let provider = rmccc_llm::OpenAiCompatibleProvider::new(
        &credential,
        rmccc_llm::LlmOptions {
            timeout: Duration::from_secs(30),
            ..Default::default()
        },
    )
    .map_err(|e| BridgeError::new(BridgeErrorCode::Internal, e.to_string()))?;
    let request = rmccc_llm::ChatRequest::new(messages);
    let cancel = rmccc_llm::CancelToken::new();

    if !stream {
        let text = collect_stream(&provider, request, cancel)
            .await
            .map_err(|e| BridgeError::new(BridgeErrorCode::Internal, e))?;
        return Ok(json!({ "text": text }));
    }

    // 流式：spawn 推送任务，立即回 streamId。
    let stream_id = uuid::Uuid::new_v4().simple().to_string();
    let app = state.app_handle.clone();
    let push_plugin_id = plugin_id.to_string();
    let push_stream_id = stream_id.clone();
    tauri::async_runtime::spawn(async move {
        use rmccc_llm::LlmEvent;
        use tauri::Emitter as _;
        let push = |message: Value| {
            let _ = app.emit(
                crate::events::PLUGIN_PUSH,
                &crate::events::PluginPushPayload {
                    plugin_id: push_plugin_id.clone(),
                    message,
                },
            );
        };
        match consume_stream(&provider, request, cancel) {
            Ok(mut rx) => {
                while let Some(event) = rx.recv().await {
                    match event {
                        LlmEvent::Delta(text) => {
                            push(rmccc_core::rmccc_plugin::ui::llm_delta_message(
                                &push_stream_id,
                                &text,
                            ));
                        }
                        LlmEvent::Done { .. } => break,
                        LlmEvent::Error(e) => {
                            push(rmccc_core::rmccc_plugin::ui::llm_error_message(
                                &push_stream_id,
                                &e.to_string(),
                            ));
                            return;
                        }
                        LlmEvent::Usage { .. } => {}
                    }
                }
                push(rmccc_core::rmccc_plugin::ui::llm_done_message(
                    &push_stream_id,
                ));
            }
            Err(e) => {
                push(rmccc_core::rmccc_plugin::ui::llm_error_message(
                    &push_stream_id,
                    &e,
                ));
            }
        }
    });
    Ok(json!({ "streamId": stream_id }))
}

/// 每插件滑动窗口限流；窗口 60s。
fn acquire_llm_slot(state: &AppState, plugin_id: &str) -> bool {
    use std::time::Instant;
    let mut rates = state
        .llm_rate
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let now = Instant::now();
    let window = rates.entry(plugin_id.to_string()).or_default();
    while let Some(front) = window.front() {
        if now.duration_since(*front) > Duration::from_secs(60) {
            window.pop_front();
        } else {
            break;
        }
    }
    if window.len() >= LLM_PLUGIN_RPM {
        return false;
    }
    window.push_back(now);
    true
}

/// `{messages: [{role, content}]}` 校验：role ∈ system/user/assistant，内容非空、总量受限。
fn parse_chat_messages(params: &Value) -> Result<Vec<rmccc_llm::ChatMessage>, BridgeError> {
    let raw = params
        .get("messages")
        .and_then(Value::as_array)
        .ok_or_else(|| {
            BridgeError::new(BridgeErrorCode::Validation, "llm.chat 需要 messages 数组")
        })?;
    if raw.is_empty() {
        return Err(BridgeError::new(
            BridgeErrorCode::Validation,
            "messages 不能为空",
        ));
    }
    if raw.len() > MAX_LLM_MESSAGES {
        return Err(BridgeError::new(
            BridgeErrorCode::TooLarge,
            format!("messages 超过 {MAX_LLM_MESSAGES} 条上限"),
        ));
    }
    let mut messages = Vec::with_capacity(raw.len());
    let mut total_bytes = 0usize;
    for (index, item) in raw.iter().enumerate() {
        let role = item.get("role").and_then(Value::as_str).ok_or_else(|| {
            BridgeError::new(
                BridgeErrorCode::Validation,
                format!("messages[{index}].role 缺失"),
            )
        })?;
        let content = item.get("content").and_then(Value::as_str).ok_or_else(|| {
            BridgeError::new(
                BridgeErrorCode::Validation,
                format!("messages[{index}].content 缺失"),
            )
        })?;
        if content.is_empty() {
            return Err(BridgeError::new(
                BridgeErrorCode::Validation,
                format!("messages[{index}].content 不能为空"),
            ));
        }
        total_bytes += content.len();
        if total_bytes > MAX_LLM_REPLY_BYTES {
            return Err(BridgeError::new(
                BridgeErrorCode::TooLarge,
                "messages 总量超过上限",
            ));
        }
        let message = match role {
            "system" => rmccc_llm::ChatMessage::system(content),
            "user" => rmccc_llm::ChatMessage::user(content),
            "assistant" => rmccc_llm::ChatMessage {
                role: rmccc_llm::ChatRole::Assistant,
                content: content.to_string(),
            },
            other => {
                return Err(BridgeError::new(
                    BridgeErrorCode::Validation,
                    format!("messages[{index}].role 非法: {other}"),
                ))
            }
        };
        messages.push(message);
    }
    Ok(messages)
}

/// 非流式：消费流直到 Done，拼接全部增量。
async fn collect_stream(
    provider: &rmccc_llm::OpenAiCompatibleProvider,
    request: rmccc_llm::ChatRequest,
    cancel: rmccc_llm::CancelToken,
) -> Result<String, String> {
    let mut rx = consume_stream(provider, request, cancel)?;
    use rmccc_llm::LlmEvent;
    let mut text = String::new();
    while let Some(event) = rx.recv().await {
        match event {
            LlmEvent::Delta(delta) => {
                if text.len() + delta.len() > MAX_LLM_REPLY_BYTES {
                    return Err("回复超过上限".into());
                }
                text.push_str(&delta);
            }
            LlmEvent::Done { .. } => break,
            LlmEvent::Error(e) => return Err(e.to_string()),
            LlmEvent::Usage { .. } => {}
        }
    }
    Ok(text)
}

/// 启动一次流式请求，返回事件接收端。
fn consume_stream(
    provider: &rmccc_llm::OpenAiCompatibleProvider,
    request: rmccc_llm::ChatRequest,
    cancel: rmccc_llm::CancelToken,
) -> Result<rmccc_llm::LlmStream, String> {
    use rmccc_llm::LlmProvider as _;
    provider
        .stream_chat(request, cancel)
        .map_err(|e| e.to_string())
}
