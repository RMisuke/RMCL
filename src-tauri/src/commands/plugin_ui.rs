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
use std::net::{IpAddr, SocketAddr};
use std::time::Duration;

use rmccc_core::rmccc_plugin::theme_assets;
use rmccc_core::rmccc_plugin::ui::{self, BridgeError, BridgeErrorCode};
use rmccc_core::server_status::query_server_status;
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
/// 两条路由：
/// - `ui/...`：插件隔离 UI（需要 `dashboard.cards` 或 `ui.pages`）；
/// - `assets/...`：主题皮肤资产（需要 `themes.apply` + `themes.skin`），
///   支持图片/字体/视频，视频带 Range；字体带 CORS（`@font-face` 需要）。
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
    let range_header = request
        .headers()
        .get("range")
        .and_then(|value| value.to_str().ok())
        .map(str::to_string);

    if rmccc_plugin_id_is_invalid(plugin_id) {
        return text_response(403, "forbidden: plugin id");
    }

    enum Route {
        Ui(String),
        Theme(String),
    }

    let route = match ui::validate_asset_path(path) {
        Ok(path) => Route::Ui(path),
        Err(_) => match ui::validate_theme_asset_path(path) {
            Ok(path) => Route::Theme(path),
            Err(_) => return text_response(403, "forbidden: asset path"),
        },
    };

    let state = app.state::<AppState>();
    // 异步协议处理器：await 读锁（绝不阻塞 WKWebView 主线程）。
    let core = state.core.read().await;
    let Some(dir) = core.plugin_manager.installed_dir(plugin_id) else {
        return text_response(404, "plugin not installed");
    };
    let allowed = core.plugin_manager.ensure_enabled(plugin_id).is_ok()
        && match &route {
            Route::Ui(_) => core
                .plugin_manager
                .authorize(plugin_id, "dashboard.cards", None, None)
                .or_else(|_| {
                    core.plugin_manager
                        .authorize(plugin_id, "ui.pages", None, None)
                })
                .is_ok(),
            Route::Theme(_) => {
                core.plugin_manager
                    .authorize(plugin_id, "themes.apply", None, None)
                    .is_ok()
                    && core
                        .plugin_manager
                        .authorize(plugin_id, "themes.skin", None, None)
                        .is_ok()
            }
        };
    if !allowed {
        return text_response(403, "forbidden: plugin is not allowed to serve this asset");
    }
    drop(core);

    let (asset_path, is_theme) = match route {
        Route::Ui(path) => (path, false),
        Route::Theme(path) => (path, true),
    };

    let file = dir.join(&asset_path);
    let Ok(metadata) = std::fs::metadata(&file) else {
        return text_response(404, "asset not found");
    };
    if !metadata.is_file() {
        return text_response(404, "asset not found");
    }
    let max_bytes = if is_theme {
        theme_assets::max_bytes(&asset_path)
    } else {
        ui::MAX_ASSET_BYTES
    };
    if metadata.len() > max_bytes {
        return text_response(413, "asset too large");
    }
    let Ok(bytes) = std::fs::read(&file) else {
        return text_response(500, "asset unreadable");
    };

    let content_type = ui::content_type(&asset_path);
    let total = bytes.len();
    let (status, body, content_range) =
        if is_theme && content_type.starts_with("video/") && total > 0 {
            match range_header
                .as_deref()
                .and_then(|raw| parse_byte_range(raw, total))
            {
                Some((start, end)) => (
                    206,
                    bytes[start..=end].to_vec(),
                    Some(format!("bytes {start}-{end}/{total}")),
                ),
                None => (200, bytes, None),
            }
        } else {
            (200, bytes, None)
        };

    let mut builder = tauri::http::Response::builder()
        .status(status)
        .header("Content-Type", content_type)
        .header("X-Content-Type-Options", "nosniff")
        .header("Cache-Control", "no-store")
        .header("Content-Length", body.len().to_string());
    if let Some(content_range) = content_range {
        builder = builder
            .header("Content-Range", content_range)
            .header("Accept-Ranges", "bytes");
    }
    if is_theme {
        // 主题资产是包内公开资源；字体需要 CORS 才能被 @font-face 使用。
        builder = builder.header("Access-Control-Allow-Origin", "*");
        if content_type.starts_with("image/svg") {
            // 即使有人直接打开 SVG，也不允许脚本/外部资源。
            builder = builder.header(
                "Content-Security-Policy",
                "default-src 'none'; style-src 'unsafe-inline'",
            );
        }
        if content_type.starts_with("video/") {
            builder = builder.header("Accept-Ranges", "bytes");
        }
    }
    if content_type.starts_with("text/html") {
        // 只在文档上带 CSP：子资源不需要，但带上也无害。
        builder = builder.header("Content-Security-Policy", ui::CSP);
    }
    builder
        .body(Cow::<'static, [u8]>::Owned(body))
        .unwrap_or_else(|_| text_response(500, "response error"))
}

/// 解析单段 `Range: bytes=start-end`；不合法时返回 `None`（退回 200 全量）。
fn parse_byte_range(raw: &str, total: usize) -> Option<(usize, usize)> {
    let spec = raw.strip_prefix("bytes=")?;
    if spec.contains(',') || total == 0 {
        return None;
    }
    let (start, end) = spec.split_once('-')?;
    if start.is_empty() {
        let suffix: usize = end.parse().ok()?;
        if suffix == 0 {
            return None;
        }
        let start = total.saturating_sub(suffix);
        return Some((start, total - 1));
    }
    let start: usize = start.parse().ok()?;
    if start >= total {
        return None;
    }
    let end = if end.is_empty() {
        total - 1
    } else {
        end.parse::<usize>().ok()?.min(total - 1)
    };
    (end >= start).then_some((start, end))
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
    let scope_host = match parsed.method.as_str() {
        "network.request" => match network_host(&params) {
            Ok(host) => Some(host),
            Err(e) => return Ok(BridgeReply::from_bridge(e)),
        },
        "server.status" => match server_host(&params) {
            Ok((host, _port)) => Some(host),
            Err(e) => return Ok(BridgeReply::from_bridge(e)),
        },
        _ => None,
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
            let result = {
                let mut service = match core.instances.lock() {
                    Ok(service) => service,
                    Err(_) => {
                        return Ok(BridgeReply::err(
                            BridgeErrorCode::Internal,
                            "instance service lock poisoned",
                        ))
                    }
                };
                service.list(None, false, false)
            };
            match result {
                Ok(instances) => {
                    let items: Vec<Value> = instances
                        .iter()
                        .map(|i| {
                            json!({
                                "key": i.key,
                                "id": i.id,
                                "name": i.display_name,
                                "gameVersion": i.game_version,
                                "loader": i.loader.as_ref().map(|l| json!({
                                    "type": l.kind,
                                    "version": l.version,
                                })),
                                "lastPlayed": i.last_played,
                            })
                        })
                        .collect();
                    BridgeReply::ok(json!({ "instances": items }))
                }
                Err(e) => BridgeReply::from_host(e),
            }
        }
        "tasks.list" => {
            let board = state.tasks.board();
            let active: Vec<Value> = board
                .active
                .iter()
                .take(8)
                .map(public_task_snapshot)
                .collect();
            let recent: Vec<Value> = board
                .recent
                .iter()
                .take(8)
                .map(public_task_snapshot)
                .collect();
            BridgeReply::ok(json!({
                "active": active,
                "recent": recent,
                "generatedAt": board.generated_at,
            }))
        }
        "launch.status" => {
            let running: Vec<String> = state
                .running_games
                .iter()
                .map(|entry| entry.key().clone())
                .collect();
            BridgeReply::ok(json!({ "running": running }))
        }
        "server.status" => match server_status_bridge(&params).await {
            Ok(value) => BridgeReply::ok(value),
            Err(e) => BridgeReply::from_bridge(e),
        },
        "network.request" => match network_request(&params).await {
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

/// 明显的本地/私有域名（域名后缀，无法用 IP 规则覆盖）。
///
/// `.internal` 覆盖云厂商 metadata（如 `metadata.google.internal`），
/// `.local` 覆盖 mDNS，`.home.arpa` 覆盖家庭网络。
fn host_is_blocked(host: &str) -> bool {
    let host = host.trim_end_matches('.').to_ascii_lowercase();
    host == "localhost"
        || host.ends_with(".localhost")
        || host.ends_with(".local")
        || host.ends_with(".internal")
        || host == "home.arpa"
        || host.ends_with(".home.arpa")
}

/// 解析域名并只保留公网地址；全部被过滤时拒绝。
async fn resolve_public_addrs(host: &str, port: u16) -> Result<Vec<SocketAddr>, BridgeError> {
    let resolved = tokio::net::lookup_host((host, port))
        .await
        .map_err(|e| BridgeError::new(BridgeErrorCode::Internal, format!("DNS 解析失败：{e}")))?;
    let addrs: Vec<SocketAddr> = resolved
        .filter(|addr| !is_forbidden_ip(addr.ip()))
        .collect();
    if addrs.is_empty() {
        return Err(BridgeError::new(
            BridgeErrorCode::HostNotAllowed,
            format!("{host} 只解析到本地/私网/保留地址，禁止访问"),
        ));
    }
    Ok(addrs)
}

/// 非公网地址判定：回环、私网、链路本地、未指定、组播、CGNAT、保留/文档段。
fn is_forbidden_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => {
            let octets = v4.octets();
            v4.is_loopback()
                || v4.is_private()
                || v4.is_link_local()
                || v4.is_unspecified()
                || v4.is_broadcast()
                || v4.is_multicast()
                || v4.is_documentation()
                // 0.0.0.0/8（"本网络"）与 100.64.0.0/10（CGNAT）在 std 中没有稳定谓词。
                || octets[0] == 0
                || (octets[0] == 100 && (64..=127).contains(&octets[1]))
        }
        IpAddr::V6(v6) => {
            if let Some(v4) = v6.to_ipv4_mapped() {
                return is_forbidden_ip(IpAddr::V4(v4));
            }
            let segments = v6.segments();
            v6.is_loopback()
                || v6.is_unspecified()
                || v6.is_multicast()
                || (segments[0] & 0xfe00) == 0xfc00 // fc00::/7 unique local
                || (segments[0] & 0xffc0) == 0xfe80 // fe80::/10 link local
        }
    }
}

/// 目标域名（授权作用域）：只接受 http(s) 且带 host 的绝对 URL。
/// Plugin-facing task snapshot: no detail log, no manual directory, and only
/// the current file's basename. Credentials never live in a TaskSnapshot, but
/// paths do, so the projection is explicit rather than "serialize everything".
fn public_task_snapshot(task: &rmccc_tasks::TaskSnapshot) -> Value {
    let mut task = task.clone();
    task.details.clear();
    if let Some(download) = &mut task.download {
        download.current_file = download
            .current_file
            .as_deref()
            .and_then(|path| std::path::Path::new(path).file_name())
            .and_then(|name| name.to_str())
            .map(ToOwned::to_owned);
    }
    if let Some(result) = &mut task.result {
        result.manual_directory = None;
        result.directory_id = None;
    }
    serde_json::to_value(task).unwrap_or(Value::Null)
}

/// `server.status` 的 host/port 参数；host 必须是声明过的域名，不能带协议/路径。
fn server_host(params: &Value) -> Result<(String, u16), BridgeError> {
    let host = params
        .get("host")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|host| !host.is_empty())
        .ok_or_else(|| {
            BridgeError::new(
                BridgeErrorCode::Validation,
                "server.status 需要 params.host",
            )
        })?;
    if host.len() > 253
        || host.contains("://")
        || host.contains('/')
        || host.contains('\\')
        || host.contains(':')
        || host.chars().any(|c| c.is_whitespace())
    {
        return Err(BridgeError::new(
            BridgeErrorCode::Validation,
            "server.status 的 host 只能是域名，端口请用 params.port",
        ));
    }
    if host_is_blocked(host) {
        return Err(BridgeError::new(
            BridgeErrorCode::HostNotAllowed,
            format!("{host} 是本地/私有域名，禁止访问"),
        ));
    }
    let port = match params.get("port").and_then(Value::as_u64) {
        Some(port) if (1..=65535).contains(&port) => port as u16,
        Some(_) => {
            return Err(BridgeError::new(
                BridgeErrorCode::Validation,
                "server.status 的 port 必须在 1..=65535",
            ))
        }
        None => 25565,
    };
    Ok((host.to_ascii_lowercase(), port))
}

/// 受控 SLP 查询：域名必须在 `network.request` 白名单内，且解析到公网地址。
async fn server_status_bridge(params: &Value) -> Result<Value, BridgeError> {
    let (host, port) = server_host(params)?;
    // 与 network.request 使用同一套 SSRF 过滤；查询仍按 host 名发送 handshake。
    let _addrs = resolve_public_addrs(&host, port).await?;
    let address = format!("{host}:{port}");
    let status = tokio::task::spawn_blocking(move || query_server_status(&address))
        .await
        .map_err(|e| BridgeError::new(BridgeErrorCode::Internal, format!("SLP 线程失败：{e}")))?
        .map_err(|e| BridgeError::new(BridgeErrorCode::Internal, e.to_string()))?;
    serde_json::to_value(status)
        .map_err(|e| BridgeError::new(BridgeErrorCode::Internal, format!("序列化状态失败：{e}")))
}

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
async fn network_request(params: &Value) -> Result<Value, BridgeError> {
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

    // ── SSRF 防护（ADR-006 补充，2026-10-27）──
    // 域名白名单只能约束"名字"，不能约束"解析到哪"：攻击者可以让自己的域名解析到
    // 127.0.0.1 / 169.254.169.254 / 私网，借宿主 client 读本地服务。做法：
    // 1) 拒绝明显的本地域名（localhost/.local/.internal/.home.arpa）；
    // 2) 解析后过滤所有非公网地址，全部不合格即拒绝；
    // 3) 用 `resolve_to_addrs` 把连接钉死在这批**已检查**的地址上，封掉二次 DNS
    //    rebinding 的时间窗（代价是每次请求新建 client，插件网络调用频率很低）。
    let host = url
        .host_str()
        .ok_or_else(|| BridgeError::new(BridgeErrorCode::Validation, "URL 缺少 host"))?
        .to_string();
    let port = url.port_or_known_default().unwrap_or(443);
    if host_is_blocked(&host) {
        return Err(BridgeError::new(
            BridgeErrorCode::HostNotAllowed,
            format!("{host} 是本地/私有域名，禁止访问"),
        ));
    }
    let addrs = resolve_public_addrs(&host, port).await?;
    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(Duration::from_secs(30))
        .user_agent(concat!("RMCCC/", env!("CARGO_PKG_VERSION"), " (plugin-ui)"))
        .resolve_to_addrs(&host, &addrs)
        .build()
        .map_err(|e| BridgeError::new(BridgeErrorCode::Internal, format!("HTTP client: {e}")))?;
    let mut request = client
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_single_byte_ranges() {
        assert_eq!(parse_byte_range("bytes=0-99", 1000), Some((0, 99)));
        assert_eq!(parse_byte_range("bytes=900-", 1000), Some((900, 999)));
        assert_eq!(parse_byte_range("bytes=-100", 1000), Some((900, 999)));
        assert_eq!(parse_byte_range("bytes=0-5000", 1000), Some((0, 999)));
        assert_eq!(parse_byte_range("bytes=1000-", 1000), None);
        assert_eq!(parse_byte_range("bytes=5-1", 1000), None);
        assert_eq!(parse_byte_range("bytes=0-1,2-3", 1000), None);
        assert_eq!(parse_byte_range("items=0-1", 1000), None);
    }

    #[test]
    fn blocks_local_and_private_addresses() {
        for ip in [
            "127.0.0.1",
            "10.0.0.1",
            "172.16.5.4",
            "192.168.1.1",
            "169.254.169.254",
            "0.0.0.0",
            "100.64.1.1",
            "224.0.0.1",
            "::1",
            "::",
            "fd00::1",
            "fe80::1",
            "::ffff:127.0.0.1",
        ] {
            let ip: IpAddr = ip.parse().unwrap();
            assert!(is_forbidden_ip(ip), "{ip} must be blocked");
        }
        for ip in [
            "1.1.1.1",
            "8.8.8.8",
            "93.184.216.34",
            "2606:4700:4700::1111",
        ] {
            let ip: IpAddr = ip.parse().unwrap();
            assert!(!is_forbidden_ip(ip), "{ip} must be allowed");
        }
    }

    #[test]
    fn blocks_private_dns_names() {
        for host in [
            "localhost",
            "LOCALHOST.",
            "a.localhost",
            "printer.local",
            "metadata.google.internal",
            "instance-data.ec2.internal",
            "foo.home.arpa",
            "home.arpa",
        ] {
            assert!(host_is_blocked(host), "{host} must be blocked");
        }
        for host in ["example.com", "api.modrinth.com", "api.example.com"] {
            assert!(!host_is_blocked(host), "{host} must be allowed");
        }
    }
}
