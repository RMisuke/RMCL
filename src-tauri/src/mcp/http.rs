//! MCP 的 Streamable HTTP 传输（JSON 模式）。
//!
//! - 仅监听 `127.0.0.1`（设计 §5.5：局域网/远程为 v2 议题）；
//! - 所有请求要求 `Authorization: Bearer <token>`（401 一律不区分"令牌错"与"缺失"）；
//! - `Origin` 头存在时必须是回环（DNS rebinding 防御）；
//! - POST `/mcp` 承载 JSON-RPC：请求 → 200 单响应；通知 → 202 无体；
//! - `initialize` 响应带 `Mcp-Session-Id` 头，后续请求须携带；
//! - GET → 405（不提供服务器主动流）；DELETE → 关闭会话 → 204。
//!
//! 基于 hyper 1.x 手写 service（reqwest 已带入 hyper 依赖树；
//! 不引入 axum/tower —— 体积红线 ADR-011）。

use crate::mcp::protocol::Incoming as RpcIncoming;
use crate::mcp::protocol::{self, Ctx};
use crate::mcp::McpHandle;
use http_body_util::{BodyExt, Full};
use hyper::body::Incoming;
use hyper::service::Service;
use hyper::{Request, Response};
use serde_json::json;
use std::future::Future;
use std::net::TcpListener;
use std::pin::Pin;
use std::sync::Arc;
use tokio::sync::watch;

/// 请求体上限（工具参数最大 64 KiB 的 16 倍余量）。
const MAX_BODY_BYTES: usize = 1024 * 1024;

pub(super) async fn serve(
    listener: TcpListener,
    handle: Arc<McpHandle>,
    app: tauri::AppHandle,
    mut shutdown: watch::Receiver<bool>,
) {
    let listener = match tokio::net::TcpListener::from_std(listener) {
        Ok(l) => l,
        Err(e) => {
            tracing::error!(error = %e, "MCP: cannot wrap listener");
            return;
        }
    };
    loop {
        tokio::select! {
            _ = shutdown.changed() => break,
            accepted = listener.accept() => {
                let (stream, _peer) = match accepted {
                    Ok(v) => v,
                    Err(e) => {
                        tracing::warn!(error = %e, "MCP: accept failed");
                        continue;
                    }
                };
                // 仅回环监听；双保险校验对端地址。
                if !_peer.ip().is_loopback() {
                    tracing::warn!(peer = %_peer, "MCP: rejected non-loopback peer");
                    continue;
                }
                let service = McpService {
                    handle: Arc::clone(&handle),
                    app: app.clone(),
                };
                let io = hyper_util::rt::TokioIo::new(stream);
                tokio::spawn(async move {
                    // B9：挂 timer 启用 hyper 的 header 读超时——只连不发的
                    // 慢速连接不再无限占住任务（回环-only 下仅防资源累积）。
                    let _ = hyper::server::conn::http1::Builder::new()
                        .timer(hyper_util::rt::TokioTimer::new())
                        .serve_connection(io, service)
                        .await;
                });
            }
        }
    }
}

struct McpService {
    handle: Arc<McpHandle>,
    app: tauri::AppHandle,
}

impl Service<Request<Incoming>> for McpService {
    type Response = Response<Full<hyper::body::Bytes>>;
    type Error = std::convert::Infallible;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

    fn call(&self, request: Request<Incoming>) -> Self::Future {
        let handle = Arc::clone(&self.handle);
        let app = self.app.clone();
        Box::pin(async move {
            let response = route(&handle, &app, request).await;
            Ok(response.unwrap_or_else(internal_response))
        })
    }
}

async fn route(
    handle: &Arc<McpHandle>,
    app: &tauri::AppHandle,
    request: Request<Incoming>,
) -> Option<Response<Full<hyper::body::Bytes>>> {
    use hyper::Method;
    let path = request.uri().path();
    if path != "/mcp" {
        return Some(text_response(404, "not found"));
    }
    // 周期性清理过期确认（每个请求顺手做，避免引入定时器）。
    handle.expire_confirmations();

    match *request.method() {
        Method::POST => Some(handle_post(handle, app, request).await),
        Method::GET => Some(text_response(
            405,
            "MCP server does not offer server-initiated streams",
        )),
        Method::DELETE => {
            let session = header(&request, "mcp-session-id");
            match session {
                Some(id) if handle.drop_session(&id) => Some(empty_response(204)),
                _ => Some(text_response(404, "unknown session")),
            }
        }
        _ => Some(text_response(405, "method not allowed")),
    }
}

async fn handle_post(
    handle: &Arc<McpHandle>,
    app: &tauri::AppHandle,
    request: Request<Incoming>,
) -> Response<Full<hyper::body::Bytes>> {
    // ── Bearer 认证 ──
    let expected = match handle.token() {
        Ok(t) => t,
        Err(e) => return text_response(500, &format!("token unavailable: {e}")),
    };
    if !bearer_matches(&request, &expected) {
        return json_response(401, &json!({ "error": "unauthorized" }));
    }
    // ── Origin 校验（DNS rebinding 防御）──
    if let Some(origin) = header(&request, "origin") {
        if !origin_is_loopback(&origin) {
            return json_response(403, &json!({ "error": "forbidden origin" }));
        }
    }

    // ── 读请求体 ──
    let (parts, body) = request.into_parts();
    let bytes = match BodyExt::collect(body).await {
        Ok(collected) => collected.to_bytes(),
        Err(e) => return text_response(400, &format!("cannot read body: {e}")),
    };
    if bytes.len() > MAX_BODY_BYTES {
        return json_response(413, &json!({ "error": "body too large" }));
    }

    // ── 协议分发 ──
    let session_id = parts
        .headers
        .get("mcp-session-id")
        .and_then(|v| v.to_str().ok())
        .map(str::to_string);
    let ctx = Ctx {
        handle: Arc::clone(handle),
        app: app.clone(),
        session_id,
    };
    match protocol::parse(&bytes) {
        Ok(RpcIncoming::Notification) => empty_response(202),
        Ok(RpcIncoming::Request { id, method, params }) => {
            let result = protocol::handle(&ctx, &method, &params).await;
            let mut builder = Response::builder().status(200);
            let payload = match result {
                Ok((result, new_session)) => {
                    if let Some(session) = new_session {
                        builder = builder.header("Mcp-Session-Id", session);
                    }
                    json!({ "jsonrpc": "2.0", "id": id, "result": result })
                }
                Err(e) => json!({
                    "jsonrpc": "2.0",
                    "id": id,
                    "error": { "code": e.code, "message": e.message },
                }),
            };
            builder
                .body(Full::from(payload.to_string().into_bytes()))
                .unwrap_or_else(|_| text_response(500, "response error"))
        }
        Err(e) => json_response(
            400,
            &json!({
                "jsonrpc": "2.0",
                "id": Value::Null,
                "error": { "code": e.code, "message": e.message },
            }),
        ),
    }
}

use serde_json::Value;

fn header<T>(request: &Request<T>, name: &str) -> Option<String> {
    request
        .headers()
        .get(name)
        .and_then(|v| v.to_str().ok())
        .map(str::to_string)
}

/// 恒定时间比较不可行（无 subtle 依赖）；长度先行 + 逐字节比较已足够
/// —— 令牌只在回环链路上传输，且重试无放大（限速在会话层）。
fn bearer_matches<T>(request: &Request<T>, expected: &str) -> bool {
    let Some(raw) = header(request, "authorization") else {
        return false;
    };
    let Some(token) = raw
        .strip_prefix("Bearer ")
        .or_else(|| raw.strip_prefix("bearer "))
    else {
        return false;
    };
    token.len() == expected.len()
        && token
            .bytes()
            .zip(expected.bytes())
            .fold(0u8, |acc, (a, b)| acc | (a ^ b))
            == 0
}

fn origin_is_loopback(origin: &str) -> bool {
    let Ok(url) = reqwest::Url::parse(origin) else {
        return false;
    };
    matches!(
        url.host_str(),
        Some("localhost") | Some("127.0.0.1") | Some("[::1]") | Some("::1")
    )
}

fn json_response(status: u16, value: &serde_json::Value) -> Response<Full<hyper::body::Bytes>> {
    Response::builder()
        .status(status)
        .header("Content-Type", "application/json")
        .body(Full::from(value.to_string().into_bytes()))
        .unwrap_or_else(|_| text_response(500, "response error"))
}

fn empty_response(status: u16) -> Response<Full<hyper::body::Bytes>> {
    Response::builder()
        .status(status)
        .body(Full::default())
        .unwrap_or_else(|_| text_response(500, "response error"))
}

fn text_response(status: u16, text: &str) -> Response<Full<hyper::body::Bytes>> {
    Response::builder()
        .status(status)
        .header("Content-Type", "text/plain; charset=utf-8")
        .body(Full::from(format!("{text}\n").into_bytes()))
        .unwrap_or_else(|_| Response::new(Full::default()))
}

fn internal_response() -> Response<Full<hyper::body::Bytes>> {
    text_response(500, "internal error")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bearer_comparison_is_length_safe() {
        let mut request = Request::builder()
            .header("Authorization", "Bearer abc")
            .body(())
            .unwrap();
        assert!(!bearer_matches(&request, "abcd"));
        *request.headers_mut() = {
            let mut map = hyper::HeaderMap::new();
            map.insert("Authorization", "Bearer abcd".parse().unwrap());
            map
        };
        assert!(bearer_matches(&request, "abcd"));
        *request.headers_mut() = {
            let mut map = hyper::HeaderMap::new();
            map.insert("Authorization", "Bearer abce".parse().unwrap());
            map
        };
        assert!(!bearer_matches(&request, "abcd"));
        *request.headers_mut() = {
            let mut map = hyper::HeaderMap::new();
            map.insert("Authorization", "Basic abcd".parse().unwrap());
            map
        };
        assert!(!bearer_matches(&request, "abcd"));
    }

    #[test]
    fn origins_outside_loopback_are_rejected() {
        assert!(origin_is_loopback("http://localhost:1420"));
        assert!(origin_is_loopback("http://127.0.0.1:1420"));
        assert!(!origin_is_loopback("http://evil.example.com"));
        assert!(!origin_is_loopback("http://192.168.1.10:1420"));
    }
}
