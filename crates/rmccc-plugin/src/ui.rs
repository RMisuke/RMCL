//! 隔离 UI：资源白名单、CSP 与版本化 Host Bridge（Phase 4，ADR-006）。
//!
//! 第三方 UI 只在 `rmcccplugin://<plugin-id>/` 这个**独立源**里执行（主窗口是
//! `tauri://localhost`），拿不到宿主 DOM，也拿不到注入的 Tauri API。数据只能经本模块定义的
//! 消息桥获取，宿主对**每条**消息重新做权限判定。
//!
//! 本模块只做协议与策略判定，不含 IO、不读全局状态：
//!
//! | 子域 | 入口 |
//! |---|---|
//! | 请求解析/限额 | [`parse_request`]、[`MAX_REQUEST_BYTES`]、[`MAX_RESPONSE_BYTES`] |
//! | 方法 → 能力映射 | [`method`]、[`BRIDGE_METHODS`] |
//! | 线上错误码 | [`BridgeErrorCode`]、[`wire_code_for`] |
//! | 静态资源白名单 | [`validate_asset_path`]、[`content_type`]、[`CSP`] |
//! | 容器 URL | [`ui_url`]、[`validate_ui_entry`] |

use rmccc_error::{ErrorCode, PluginErrorCode, Result, RmcccError};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use ts_rs::TS;

use crate::path::{plugin_error, validate_local_id, validate_plugin_id, validate_relative_path};

/// UI API 主版本。破坏性变更才提升，新增方法走 minor（记录在 SDK 兼容矩阵）。
pub const UI_API_VERSION: u32 = 1;

/// 插件 UI 的独立 URI 方案。与 `tauri://` 不同 scheme + host，因此不同源。
pub const UI_SCHEME: &str = "rmcccplugin";

/// 包内 UI 资源根目录。
pub const UI_ROOT_DIR: &str = "ui";

/// 单条请求上限（字节，UTF-8 JSON）。
pub const MAX_REQUEST_BYTES: usize = 64 * 1024;

/// 单条响应上限（字节）。超过即拒绝，不截断。
pub const MAX_RESPONSE_BYTES: usize = 512 * 1024;

/// 默认请求超时；方法表可各自指定，上限 [`MAX_TIMEOUT_MS`]。
pub const DEFAULT_TIMEOUT_MS: u64 = 10_000;

/// 请求超时上限。
pub const MAX_TIMEOUT_MS: u64 = 30_000;

/// 同一插件允许的并发 in-flight 请求数。
pub const MAX_IN_FLIGHT: usize = 8;

/// 单个静态资源的上限（与包内单条目上限一致）。
pub const MAX_ASSET_BYTES: u64 = 4 * 1024 * 1024;

/// 请求 ID 上限。
pub const MAX_REQUEST_ID_LEN: usize = 64;

/// 插件 UI 的 CSP（响应头，`connect-src 'none'` 禁止插件自己发网络请求）。
pub const CSP: &str = "default-src 'none'; script-src 'self'; style-src 'self' 'unsafe-inline'; \
img-src 'self' data:; font-src 'self'; connect-src 'none'; form-action 'none'; \
base-uri 'none'; object-src 'none'";

/// `ui/` 下允许的资源扩展名（小写比较）。
const ASSET_EXTENSIONS: &[&str] = &[
    "html", "htm", "js", "mjs", "css", "json", "md", "txt", "svg", "png", "jpg", "jpeg", "gif",
    "webp", "ttf", "otf", "woff", "woff2", "map",
];

// ────────────────────────── 方法表 ──────────────────────────

/// 桥方法：名字、所需能力（`None` = 任何启用且带 UI 入口的插件都可调）、超时。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BridgeMethod {
    pub name: &'static str,
    pub capability: Option<&'static str>,
    pub timeout_ms: u64,
}

/// v1 方法表。**新增方法必须同时更新**：本文档、SDK 类型、桥 schema、契约测试。
pub const BRIDGE_METHODS: &[BridgeMethod] = &[
    BridgeMethod {
        name: "host.info",
        capability: None,
        timeout_ms: 5_000,
    },
    BridgeMethod {
        name: "theme.get",
        capability: None,
        timeout_ms: 5_000,
    },
    BridgeMethod {
        name: "settings.get",
        capability: Some("settings.read"),
        timeout_ms: DEFAULT_TIMEOUT_MS,
    },
    BridgeMethod {
        name: "instances.list",
        capability: Some("instance.read"),
        timeout_ms: DEFAULT_TIMEOUT_MS,
    },
    BridgeMethod {
        name: "tasks.list",
        capability: Some("tasks.read"),
        timeout_ms: DEFAULT_TIMEOUT_MS,
    },
    BridgeMethod {
        name: "launch.status",
        capability: Some("launch.read"),
        timeout_ms: 5_000,
    },
    BridgeMethod {
        name: "server.status",
        capability: Some("network.request"),
        timeout_ms: MAX_TIMEOUT_MS,
    },
    BridgeMethod {
        name: "network.request",
        capability: Some("network.request"),
        timeout_ms: MAX_TIMEOUT_MS,
    },
    // ADR-013 S6：宿主 AI 服务。流式回复以 `llm.delta/done/error` 推送
    // （见下方构造器，经前端中继进 iframe）；非流式等待完整回复，
    // 宿主侧超时与 [`MAX_TIMEOUT_MS`] 对齐。
    BridgeMethod {
        name: "llm.chat",
        capability: Some("ai.chat"),
        timeout_ms: MAX_TIMEOUT_MS,
    },
];

/// 按名字查方法表；未知方法返回 `None`（调用方映射为 [`BridgeErrorCode::UnsupportedMethod`]）。
pub fn method(name: &str) -> Option<&'static BridgeMethod> {
    BRIDGE_METHODS.iter().find(|m| m.name == name)
}

/// 方法名列表（SDK 与契约测试共用）。
pub fn method_names() -> Vec<&'static str> {
    BRIDGE_METHODS.iter().map(|m| m.name).collect()
}

// ────────────────────────── 线上错误码 ──────────────────────────

/// 桥的线上错误码：稳定字符串契约（SOC 见 `schemas/plugin-ui-bridge.schema.json`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export_to = "types.ts")]
pub enum BridgeErrorCode {
    /// 消息形状/参数非法（含请求 ID 非法）。
    Validation,
    /// 方法不在 v1 方法表内。
    UnsupportedMethod,
    /// `v` 与宿主 [`UI_API_VERSION`] 不一致。
    ProtocolVersion,
    /// 请求或响应超过大小上限。
    TooLarge,
    /// 宿主在超时内没有完成请求。
    Timeout,
    /// 插件侧取消（或卡片已卸载）。
    Cancelled,
    /// 插件未安装。
    NotInstalled,
    /// 插件存在但未启用。
    NotEnabled,
    /// 能力未授予或作用域（host/instance）不符。
    PermissionDenied,
    /// 能力已声明但尚未审查授予。
    PermissionPending,
    /// `network.request` 的目标域名不在声明白名单内。
    HostNotAllowed,
    /// 宿主内部错误（拿到的是非插件错误）。
    Internal,
    /// 调用频率超限（`llm.chat` 每插件 6 次/分钟，ADR-013 S6）。
    RateLimited,
}

impl BridgeErrorCode {
    /// 线上字符串（写进 JSON 的值）。
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Validation => "validation",
            Self::UnsupportedMethod => "unsupported_method",
            Self::ProtocolVersion => "protocol_version",
            Self::TooLarge => "too_large",
            Self::Timeout => "timeout",
            Self::Cancelled => "cancelled",
            Self::NotInstalled => "not_installed",
            Self::NotEnabled => "not_enabled",
            Self::PermissionDenied => "permission_denied",
            Self::PermissionPending => "permission_pending",
            Self::HostNotAllowed => "host_not_allowed",
            Self::Internal => "internal",
            Self::RateLimited => "rate_limited",
        }
    }

    /// 是否值得重试（写进响应的 `retryable`）。
    pub fn retryable(self) -> bool {
        matches!(self, Self::Timeout | Self::Internal | Self::RateLimited)
    }

    /// 映射到 IPC 错误码，让宿主 UI 用同一套错误展示。
    pub fn to_error_code(self) -> ErrorCode {
        match self {
            Self::PermissionDenied | Self::HostNotAllowed => ErrorCode::PluginPermissionDenied,
            Self::PermissionPending => ErrorCode::PluginPermissionPending,
            Self::NotInstalled => ErrorCode::PluginNotFound,
            Self::NotEnabled => ErrorCode::PluginStateConflict,
            Self::Validation | Self::ProtocolVersion | Self::TooLarge => ErrorCode::ValidationError,
            Self::UnsupportedMethod => ErrorCode::ValidationError,
            Self::Timeout => ErrorCode::NetworkError,
            Self::Cancelled | Self::Internal => ErrorCode::InternalError,
            Self::RateLimited => ErrorCode::NetworkError,
        }
    }
}

/// 把 `PluginManager::authorize` 等宿主错误映射成线上错误码。
pub fn wire_code_for(err: &RmcccError) -> BridgeErrorCode {
    match err {
        RmcccError::Plugin { code, .. } => match code {
            PluginErrorCode::PermissionDenied => BridgeErrorCode::PermissionDenied,
            PluginErrorCode::PermissionPending => BridgeErrorCode::PermissionPending,
            PluginErrorCode::NotFound => BridgeErrorCode::NotInstalled,
            PluginErrorCode::StateConflict => BridgeErrorCode::NotEnabled,
            PluginErrorCode::ManifestInvalid
            | PluginErrorCode::PackageInvalid
            | PluginErrorCode::ConfigInvalid => BridgeErrorCode::Validation,
            _ => BridgeErrorCode::Internal,
        },
        RmcccError::Timeout(_) => BridgeErrorCode::Timeout,
        _ => BridgeErrorCode::Internal,
    }
}

// ────────────────────────── 请求 ──────────────────────────

/// 插件 → 宿主的请求（`postMessage` 的 payload）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export_to = "types.ts")]
pub struct BridgeRequest {
    /// 协议版本，必须等于 [`UI_API_VERSION`]。
    pub v: u32,
    /// 请求 ID（`[A-Za-z0-9_-]{1,64}`），宿主用它配对响应。
    pub id: String,
    /// 方法名，必须在 [`BRIDGE_METHODS`] 内。
    pub method: String,
    /// 方法参数（大小已受 [`MAX_REQUEST_BYTES`] 约束）。
    #[serde(default)]
    #[ts(type = "unknown")]
    pub params: Option<Value>,
}

/// 解析并校验一条插件请求。
///
/// 校验顺序即拒绝优先级：大小 → 形状 → 版本 → 方法 → ID。
pub fn parse_request(raw: &str) -> std::result::Result<BridgeRequest, BridgeError> {
    if raw.len() > MAX_REQUEST_BYTES {
        return Err(BridgeError::new(
            BridgeErrorCode::TooLarge,
            format!(
                "请求超过 {MAX_REQUEST_BYTES} 字节上限（实际 {}）",
                raw.len()
            ),
        ));
    }
    let request: BridgeRequest = serde_json::from_str(raw).map_err(|e| {
        BridgeError::new(BridgeErrorCode::Validation, format!("请求 JSON 非法：{e}"))
    })?;

    if request.v != UI_API_VERSION {
        return Err(BridgeError::new(
            BridgeErrorCode::ProtocolVersion,
            format!(
                "不支持的 UI API 版本 {}（宿主支持 {UI_API_VERSION}）",
                request.v
            ),
        ));
    }
    if method(&request.method).is_none() {
        return Err(BridgeError::new(
            BridgeErrorCode::UnsupportedMethod,
            format!("未知方法：{}", request.method),
        ));
    }
    if !is_valid_request_id(&request.id) {
        return Err(BridgeError::new(
            BridgeErrorCode::Validation,
            "请求 ID 必须是 1..=64 个 [A-Za-z0-9_-] 字符".to_string(),
        ));
    }
    Ok(request)
}

/// 请求 ID 形状：`[A-Za-z0-9_-]{1,64}`。
pub fn is_valid_request_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= MAX_REQUEST_ID_LEN
        && id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}

/// 桥错误：`code` 直接写进响应，`message` 会在宿主侧也记录一次。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BridgeError {
    pub code: BridgeErrorCode,
    pub message: String,
}

impl BridgeError {
    pub fn new(code: BridgeErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }
}

impl std::fmt::Display for BridgeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.code.as_str(), self.message)
    }
}

impl std::error::Error for BridgeError {}

// ────────────────────────── 静态资源 ──────────────────────────

/// 校验 `entry.ui`：必须位于 `ui/` 下、以 `.html` 结尾。
pub fn validate_ui_entry(entry: &str) -> Result<()> {
    validate_relative_path(entry)?;
    let Some(rest) = entry.strip_prefix("ui/") else {
        return Err(plugin_error(
            PluginErrorCode::ManifestInvalid,
            format!("entry.ui 必须位于 ui/ 下：{entry}"),
        ));
    };
    if rest.is_empty() {
        return Err(plugin_error(
            PluginErrorCode::ManifestInvalid,
            "entry.ui 必须指向 ui/ 下的具体文件".to_string(),
        ));
    }
    let lowered = entry.to_ascii_lowercase();
    if !(lowered.ends_with(".html") || lowered.ends_with(".htm")) {
        return Err(plugin_error(
            PluginErrorCode::ManifestInvalid,
            format!("entry.ui 必须是 .html：{entry}"),
        ));
    }
    Ok(())
}

/// 校验 URI 方案处理器收到的资源路径。
///
/// 输入形如 `/ui/index.html`（`request.uri().path()`），返回去掉前导 `/` 的规范路径。
/// 规则：**只**允许 `ui/` 前缀、白名单扩展名、无 `..`/反斜杠/百分号编码（避免二次解码绕过）、
/// 无可执行类型。
pub fn validate_asset_path(raw: &str) -> Result<String> {
    if raw.is_empty() {
        return Err(asset_error(raw, "路径为空"));
    }
    if raw.contains('%') {
        return Err(asset_error(
            raw,
            "不允许百分号编码（请使用 URL 安全文件名）",
        ));
    }
    if raw.contains('\\') || raw.contains('?') || raw.contains('#') {
        return Err(asset_error(raw, "路径包含非法字符"));
    }
    let trimmed = raw.strip_prefix('/').unwrap_or(raw);
    validate_relative_path(trimmed)?;

    let Some(rest) = trimmed.strip_prefix("ui/") else {
        return Err(asset_error(raw, "只允许 ui/ 目录下的资源"));
    };
    if rest.is_empty() {
        return Err(asset_error(raw, "必须是具体文件，不支持目录列举"));
    }
    if rest.starts_with('.') || rest.split('/').any(|s| s.starts_with('.')) {
        return Err(asset_error(raw, "不允许隐藏文件"));
    }
    let Some(ext) = rest.rsplit('/').next().and_then(|f| f.rsplit_once('.')) else {
        return Err(asset_error(raw, "缺少扩展名"));
    };
    let ext = ext.1.to_ascii_lowercase();
    if !ASSET_EXTENSIONS.contains(&ext.as_str()) {
        return Err(asset_error(raw, &format!("不允许的资源类型 .{ext}")));
    }
    Ok(trimmed.to_string())
}

/// 校验主题资产路径：包内 `assets/` 下的图片/字体/视频（ADR-014）。
///
/// 资源处理器用它区分 `ui/`（插件 UI）与 `assets/`（主题皮肤）两条路由；
/// 主题路由要求 `themes.apply` + `themes.skin` 两个能力。
pub fn validate_theme_asset_path(raw: &str) -> Result<String> {
    if raw.is_empty() {
        return Err(asset_error(raw, "路径为空"));
    }
    if raw.contains('%') {
        return Err(asset_error(raw, "不允许百分号编码（请使用 ASCII 文件名）"));
    }
    if raw.contains('\\') || raw.contains('?') || raw.contains('#') {
        return Err(asset_error(raw, "路径包含非法字符"));
    }
    let trimmed = raw.strip_prefix('/').unwrap_or(raw);
    crate::theme::validate_asset_path(trimmed, "主题资产")?;
    Ok(trimmed.to_string())
}

/// 按扩展名给出 Content-Type（未知类型在校验阶段已被拒绝）。
pub fn content_type(path: &str) -> &'static str {
    let lowered = path.to_ascii_lowercase();
    let ext = lowered.rsplit_once('.').map(|(_, e)| e).unwrap_or("");
    match ext {
        "html" | "htm" => "text/html; charset=utf-8",
        "js" | "mjs" => "text/javascript; charset=utf-8",
        "css" => "text/css; charset=utf-8",
        "json" | "map" => "application/json; charset=utf-8",
        "md" | "txt" => "text/plain; charset=utf-8",
        "svg" => "image/svg+xml",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "ttf" => "font/ttf",
        "otf" => "font/otf",
        "woff" => "font/woff",
        "woff2" => "font/woff2",
        "mp4" => "video/mp4",
        "webm" => "video/webm",
        _ => "application/octet-stream",
    }
}

/// 生成 iframe 的 `src`：`rmcccplugin://<id>/<entry>[?card=<cardId>]`。
pub fn ui_url(plugin_id: &str, entry: &str, card_id: Option<&str>) -> Result<String> {
    validate_plugin_id(plugin_id)?;
    validate_ui_entry(entry)?;
    let mut url = format!("{UI_SCHEME}://{plugin_id}/{entry}");
    if let Some(card) = card_id {
        validate_local_id(card)?;
        url.push_str("?card=");
        url.push_str(card);
    }
    Ok(url)
}

/// 宿主 → 插件的 `init` 消息（容器就绪后一次）。
pub fn init_message(plugin_id: &str, card_id: Option<&str>, theme: Value) -> Value {
    let mut msg = serde_json::json!({
        "v": UI_API_VERSION,
        "type": "init",
        "pluginId": plugin_id,
        "theme": theme,
        "limits": {
            "maxRequestBytes": MAX_REQUEST_BYTES,
            "maxResponseBytes": MAX_RESPONSE_BYTES,
            "timeoutMs": DEFAULT_TIMEOUT_MS,
        },
    });
    if let Some(card) = card_id {
        if let Some(object) = msg.as_object_mut() {
            object.insert("cardId".to_string(), Value::String(card.to_string()));
        }
    }
    msg
}

/// 宿主 → 插件的主题更新消息。
pub fn theme_message(theme: Value) -> Value {
    serde_json::json!({ "v": UI_API_VERSION, "type": "theme", "theme": theme })
}

/// 宿主 → 插件的权限撤销通知。
pub fn revoke_message(capability: &str) -> Value {
    serde_json::json!({ "v": UI_API_VERSION, "type": "revoke", "capability": capability })
}

/// 宿主 → 插件的 AI 流式增量（ADR-013 S6 `llm.chat {stream:true}`）。
pub fn llm_delta_message(stream_id: &str, text: &str) -> Value {
    serde_json::json!({
        "v": UI_API_VERSION, "type": "llm.delta",
        "streamId": stream_id, "text": text,
    })
}

/// 宿主 → 插件的 AI 流式完成通知。
pub fn llm_done_message(stream_id: &str) -> Value {
    serde_json::json!({ "v": UI_API_VERSION, "type": "llm.done", "streamId": stream_id })
}

/// 宿主 → 插件的 AI 流式失败通知。
pub fn llm_error_message(stream_id: &str, message: &str) -> Value {
    serde_json::json!({
        "v": UI_API_VERSION, "type": "llm.error",
        "streamId": stream_id, "message": message,
    })
}

/// 成功的响应。
pub fn ok_response(id: &str, result: Value) -> Value {
    serde_json::json!({ "v": UI_API_VERSION, "id": id, "ok": true, "result": result })
}

/// 失败的响应。
pub fn error_response(id: &str, error: &BridgeError) -> Value {
    serde_json::json!({
        "v": UI_API_VERSION,
        "id": id,
        "ok": false,
        "error": {
            "code": error.code.as_str(),
            "message": error.message,
            "retryable": error.code.retryable(),
        },
    })
}

fn asset_error(raw: &str, reason: &str) -> RmcccError {
    plugin_error(
        PluginErrorCode::PackageInvalid,
        format!("插件 UI 资源路径非法（{reason}）：{raw}"),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request(json: &str) -> std::result::Result<BridgeRequest, BridgeError> {
        parse_request(json)
    }

    #[test]
    fn parses_a_valid_request() {
        let r = request(r#"{"v":1,"id":"req-1","method":"host.info"}"#).unwrap();
        assert_eq!(r.id, "req-1");
        assert_eq!(r.method, "host.info");
        assert!(r.params.is_none());
    }

    #[test]
    fn rejects_unknown_fields() {
        let err = request(r#"{"v":1,"id":"a","method":"host.info","evil":1}"#).unwrap_err();
        assert_eq!(err.code, BridgeErrorCode::Validation);
    }

    #[test]
    fn rejects_wrong_protocol_version() {
        let err = request(r#"{"v":2,"id":"a","method":"host.info"}"#).unwrap_err();
        assert_eq!(err.code, BridgeErrorCode::ProtocolVersion);
    }

    #[test]
    fn rejects_unknown_method() {
        let err = request(r#"{"v":1,"id":"a","method":"fs.read"}"#).unwrap_err();
        assert_eq!(err.code, BridgeErrorCode::UnsupportedMethod);
    }

    #[test]
    fn rejects_bad_request_ids() {
        for id in ["", "has space", "semi;colon", "../x"] {
            let raw = format!(r#"{{"v":1,"id":"{id}","method":"host.info"}}"#);
            let err = request(&raw).unwrap_err();
            assert_eq!(err.code, BridgeErrorCode::Validation, "id {id:?}");
        }
        let too_long = "a".repeat(MAX_REQUEST_ID_LEN + 1);
        let raw = format!(r#"{{"v":1,"id":"{too_long}","method":"host.info"}}"#);
        assert_eq!(request(&raw).unwrap_err().code, BridgeErrorCode::Validation);
    }

    #[test]
    fn rejects_oversized_requests_before_parsing() {
        let padding = "x".repeat(MAX_REQUEST_BYTES);
        let raw =
            format!(r#"{{"v":1,"id":"a","method":"host.info","params":{{"p":"{padding}"}}}}"#);
        assert_eq!(request(&raw).unwrap_err().code, BridgeErrorCode::TooLarge);
    }

    #[test]
    fn every_method_maps_to_a_known_capability() {
        for m in BRIDGE_METHODS {
            if let Some(cap) = m.capability {
                assert!(
                    crate::capability::Capability::parse(cap).is_ok(),
                    "method {} requires unknown capability {cap}",
                    m.name
                );
            }
            assert!(m.timeout_ms > 0 && m.timeout_ms <= MAX_TIMEOUT_MS);
        }
    }

    #[test]
    fn method_table_is_stable_and_unique() {
        let names = method_names();
        assert_eq!(
            names,
            vec![
                "host.info",
                "theme.get",
                "settings.get",
                "instances.list",
                "tasks.list",
                "launch.status",
                "server.status",
                "network.request",
                "llm.chat"
            ]
        );
        let mut sorted = names.clone();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(sorted.len(), names.len(), "duplicate bridge method");
    }

    #[test]
    fn wire_codes_serialize_to_the_documented_strings() {
        assert_eq!(BridgeErrorCode::Validation.as_str(), "validation");
        assert_eq!(
            BridgeErrorCode::PermissionDenied.as_str(),
            "permission_denied"
        );
        assert_eq!(BridgeErrorCode::HostNotAllowed.as_str(), "host_not_allowed");
        let json = serde_json::to_string(&BridgeErrorCode::TooLarge).unwrap();
        assert_eq!(json, "\"too_large\"");
    }

    #[test]
    fn host_errors_map_to_wire_codes() {
        let denied = RmcccError::Plugin {
            code: PluginErrorCode::PermissionDenied,
            message: "no".into(),
        };
        assert_eq!(wire_code_for(&denied), BridgeErrorCode::PermissionDenied);
        let pending = RmcccError::Plugin {
            code: PluginErrorCode::PermissionPending,
            message: "pending".into(),
        };
        assert_eq!(wire_code_for(&pending), BridgeErrorCode::PermissionPending);
        let missing = RmcccError::Plugin {
            code: PluginErrorCode::NotFound,
            message: "missing".into(),
        };
        assert_eq!(wire_code_for(&missing), BridgeErrorCode::NotInstalled);
        assert_eq!(
            wire_code_for(&RmcccError::Timeout(30)),
            BridgeErrorCode::Timeout
        );
        assert_eq!(
            wire_code_for(&RmcccError::Other("x".into())),
            BridgeErrorCode::Internal
        );
    }

    #[test]
    fn ui_entry_must_live_under_ui_and_be_html() {
        assert!(validate_ui_entry("ui/index.html").is_ok());
        assert!(validate_ui_entry("ui/card/index.htm").is_ok());
        for bad in [
            "index.html",
            "ui/",
            "ui/app.js",
            "ui/../app/index.html",
            "/ui/index.html",
            "ui\\index.html",
        ] {
            assert!(validate_ui_entry(bad).is_err(), "expected reject: {bad}");
        }
    }

    #[test]
    fn asset_paths_are_confined_to_ui() {
        assert_eq!(
            validate_asset_path("/ui/index.html").unwrap(),
            "ui/index.html"
        );
        assert_eq!(
            validate_asset_path("ui/assets/app.js").unwrap(),
            "ui/assets/app.js"
        );
        for bad in [
            "/manifest.json",
            "/ui/../manifest.json",
            "/ui/",
            "/ui//app.js",
            "/ui/app%2ejs",
            "/ui/app.js?x=1",
            "/ui/.env",
            "/ui/run.sh",
            "/ui/plugin.dylib",
            "/ui/app",
            "",
        ] {
            assert!(validate_asset_path(bad).is_err(), "expected reject: {bad}");
        }
    }

    #[test]
    fn content_types_cover_the_whitelist() {
        assert_eq!(content_type("ui/index.html"), "text/html; charset=utf-8");
        assert_eq!(content_type("ui/app.mjs"), "text/javascript; charset=utf-8");
        assert_eq!(content_type("ui/a.css"), "text/css; charset=utf-8");
        assert_eq!(content_type("ui/logo.svg"), "image/svg+xml");
        assert_eq!(content_type("ui/font.woff2"), "font/woff2");
    }

    #[test]
    fn ui_url_is_built_from_validated_ids() {
        let url = ui_url("dev.rmccc.card", "ui/index.html", Some("server-status")).unwrap();
        assert_eq!(
            url,
            "rmcccplugin://dev.rmccc.card/ui/index.html?card=server-status"
        );
        assert!(ui_url("Bad_ID", "ui/index.html", None).is_err());
        assert!(ui_url("dev.rmccc.card", "ui/app.js", None).is_err());
    }

    #[test]
    fn responses_and_messages_carry_the_api_version() {
        let ok = ok_response("a", serde_json::json!({"x": 1}));
        assert_eq!(ok["v"], UI_API_VERSION);
        assert_eq!(ok["ok"], true);
        let err = error_response("b", &BridgeError::new(BridgeErrorCode::Timeout, "slow"));
        assert_eq!(err["v"], UI_API_VERSION);
        assert_eq!(err["ok"], false);
        assert_eq!(err["error"]["code"], "timeout");
        assert_eq!(err["error"]["retryable"], true);
        let init = init_message("dev.rmccc.card", Some("card-a"), serde_json::json!({}));
        assert_eq!(init["type"], "init");
        assert_eq!(init["cardId"], "card-a");
        assert_eq!(init["limits"]["maxRequestBytes"], MAX_REQUEST_BYTES);
        assert_eq!(theme_message(serde_json::json!({}))["type"], "theme");
        assert_eq!(
            revoke_message("instance.read")["capability"],
            "instance.read"
        );
    }

    #[test]
    fn csp_blocks_network_and_remote_code() {
        assert!(CSP.contains("connect-src 'none'"));
        assert!(CSP.contains("script-src 'self'"));
        assert!(CSP.contains("object-src 'none'"));
        assert!(!CSP.contains("unsafe-eval"));
    }
}
