//! MCP stdio 桥（ADR-013 S5②）。
//!
//! 把 stdin/stdout 上的 MCP JSON-RPC（行分隔）转发到启动器进程内的
//! Streamable HTTP 端点（`<data>/mcp-endpoint` + `<data>/mcp-token`）。
//! 适合 Claude Desktop / ZCode 等"工具自己拉起 MCP 服务器"的客户端：
//!
//! ```jsonc
//! { "mcpServers": { "rmccc": { "command": "/path/to/rmccc", "args": ["mcp"] } } }
//! ```
//!
//! 启动器未运行时对每个请求返回明确的 JSON-RPC 错误（不写非 JSON 到 stdout）；
//! `--launch` 先拉起 GUI（同目录的 `rmccc` 可执行文件或 `RMCCC_APP_PATH`），
//! 等端点就绪（≤ 10s）再进入转发循环。

use std::io::{BufRead, Write};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

/// 端点就绪等待（`--launch`）。
const LAUNCH_WAIT: Duration = Duration::from_secs(10);
/// 单次工具调用的转发超时（check_completion 级别工具可达数分钟）。
const FORWARD_TIMEOUT: Duration = Duration::from_secs(300);

/// 未运行错误的 JSON-RPC 码（与协议层 `-32000` 语义一致）。
const NOT_RUNNING: i64 = -32000;

pub fn run(data_dir: &Path, launch: bool) -> i32 {
    if launch {
        if let Err(message) = launch_gui_and_wait(data_dir) {
            eprintln!("rmccc mcp: {message}");
            // 不退出：端点可能稍后就绪（用户手动启动了启动器）。
        }
    }

    let client = match reqwest::blocking::Client::builder()
        .timeout(FORWARD_TIMEOUT)
        .build()
    {
        Ok(client) => client,
        Err(e) => {
            eprintln!("rmccc mcp: HTTP client unavailable: {e}");
            return 8;
        }
    };

    let stdin = std::io::stdin();
    let mut stdout = std::io::stdout();
    let mut session_id: Option<String> = None;

    for line in stdin.lock().lines() {
        let Ok(line) = line else { break };
        if line.trim().is_empty() {
            continue;
        }
        let value: serde_json::Value = match serde_json::from_str(&line) {
            Ok(v) => v,
            // 无法提取 id 时只能丢弃（stdout 必须保持纯 JSON-RPC）。
            Err(e) => {
                eprintln!("rmccc mcp: dropping unparseable line: {e}");
                continue;
            }
        };
        let Some(id) = value_id(&value) else {
            // 通知：直接吞掉（当前没有需要转发到端点的客户端通知）。
            continue;
        };
        let is_initialize = value.get("method").and_then(|m| m.as_str()) == Some("initialize");

        let Some((endpoint, token)) = read_endpoint_and_token(data_dir) else {
            let _ = write_error(
                &mut stdout,
                id,
                NOT_RUNNING,
                "RMCCC 未运行，请先启动启动器（或在设置页开启 MCP 服务器）",
            );
            continue;
        };

        let mut request = client
            .post(&endpoint)
            .header("Authorization", format!("Bearer {token}"))
            .header("Content-Type", "application/json");
        if let Some(session) = session_id.as_deref() {
            request = request.header("Mcp-Session-Id", session);
        }
        match request.body(line.clone()).send() {
            Ok(response) => {
                if is_initialize {
                    if let Some(session) = response
                        .headers()
                        .get("mcp-session-id")
                        .and_then(|v| v.to_str().ok())
                    {
                        session_id = Some(session.to_string());
                    }
                }
                match response.text() {
                    Ok(text) => {
                        let _ = writeln!(stdout, "{text}");
                        let _ = stdout.flush();
                    }
                    Err(e) => {
                        let _ = write_error(
                            &mut stdout,
                            id,
                            NOT_RUNNING,
                            &format!("响应读取失败: {e}"),
                        );
                    }
                }
            }
            Err(error) if error.is_connect() || error.is_request() => {
                let _ = write_error(&mut stdout, id, NOT_RUNNING, "RMCCC 未运行，请先启动启动器");
            }
            Err(error) => {
                let _ = write_error(&mut stdout, id, -32603, &format!("转发失败: {error}"));
            }
        }
    }

    0
}

/// 提取请求 id（null/缺失 = 通知）。
fn value_id(value: &serde_json::Value) -> Option<serde_json::Value> {
    let id = value.get("id")?;
    if id.is_null() {
        None
    } else {
        Some(id.clone())
    }
}

fn write_error(
    stdout: &mut std::io::Stdout,
    id: serde_json::Value,
    code: i64,
    message: &str,
) -> std::io::Result<()> {
    let payload = serde_json::json!({
        "jsonrpc": "2.0",
        "id": id,
        "error": { "code": code, "message": message },
    });
    writeln!(stdout, "{payload}")?;
    stdout.flush()
}

/// 端点/令牌文件名——与 `src-tauri/src/mcp/mod.rs` 的
/// `ENDPOINT_FILE` / `TOKEN_FILE` 保持一致（CLI 不依赖应用层 crate）。
const ENDPOINT_FILE: &str = "mcp-endpoint";
const TOKEN_FILE: &str = "mcp-token";

/// 读端点与令牌；二者齐备才算就绪。
fn read_endpoint_and_token(data_dir: &Path) -> Option<(String, String)> {
    let endpoint = std::fs::read_to_string(data_dir.join(ENDPOINT_FILE)).ok()?;
    let token = std::fs::read_to_string(data_dir.join(TOKEN_FILE)).ok()?;
    let endpoint = endpoint.trim().to_string();
    let token = token.trim().to_string();
    if endpoint.is_empty() || token.is_empty() {
        return None;
    }
    Some((endpoint, token))
}

/// `--launch`：拉起 GUI 并等待端点就绪。
fn launch_gui_and_wait(data_dir: &Path) -> Result<(), String> {
    if read_endpoint_and_token(data_dir).is_some() {
        return Ok(());
    }
    let exe = match std::env::var("RMCCC_APP_PATH") {
        Ok(path) => PathBuf::from(path),
        Err(_) => {
            let sibling = std::env::current_exe()
                .ok()
                .and_then(|p| p.parent().map(|d| d.join(gui_binary_name())))
                .ok_or("cannot locate the GUI binary (set RMCCC_APP_PATH)")?;
            sibling
        }
    };
    if !exe.exists() {
        return Err(format!(
            "GUI binary not found at {} (set RMCCC_APP_PATH to the rmccc app binary)",
            exe.display()
        ));
    }
    Command::new(&exe)
        .spawn()
        .map_err(|e| format!("cannot launch {}: {e}", exe.display()))?;

    let deadline = Instant::now() + LAUNCH_WAIT;
    while Instant::now() < deadline {
        if read_endpoint_and_token(data_dir).is_some() {
            return Ok(());
        }
        std::thread::sleep(Duration::from_millis(200));
    }
    Err(format!(
        "MCP endpoint not ready within {}s (is the MCP server enabled in launcher settings?)",
        LAUNCH_WAIT.as_secs()
    ))
}

fn gui_binary_name() -> &'static str {
    if cfg!(windows) {
        "rmccc.exe"
    } else {
        "rmccc"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn value_id_distinguishes_requests_from_notifications() {
        let request = serde_json::json!({"jsonrpc":"2.0","id":7,"method":"ping"});
        assert_eq!(value_id(&request), Some(serde_json::json!(7)));

        let notification =
            serde_json::json!({"jsonrpc":"2.0","method":"notifications/initialized"});
        assert_eq!(value_id(&notification), None);

        let null_id = serde_json::json!({"jsonrpc":"2.0","id":null,"method":"ping"});
        assert_eq!(value_id(&null_id), None);
    }

    #[test]
    fn endpoint_reader_requires_both_files() {
        let dir = std::env::temp_dir().join(format!("rmccc-mcp-test-a-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        assert!(read_endpoint_and_token(&dir).is_none());
        std::fs::write(dir.join(ENDPOINT_FILE), "http://127.0.0.1:1/mcp").unwrap();
        assert!(read_endpoint_and_token(&dir).is_none(), "token missing");
        std::fs::write(dir.join(TOKEN_FILE), "abc\n").unwrap();
        let (endpoint, token) = read_endpoint_and_token(&dir).unwrap();
        assert_eq!(endpoint, "http://127.0.0.1:1/mcp");
        assert_eq!(token, "abc");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn endpoint_reader_rejects_blank_files() {
        let dir = std::env::temp_dir().join(format!("rmccc-mcp-test-b-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join(ENDPOINT_FILE), "  \n").unwrap();
        std::fs::write(dir.join(TOKEN_FILE), "t").unwrap();
        assert!(read_endpoint_and_token(&dir).is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
