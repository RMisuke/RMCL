//! Minecraft Server List Ping (SLP) 查询。
//!
//! 这是主页「服务器信息」卡片的真实数据源：只做一次 TCP status 查询，
//! 不下载资源、不执行服务器返回的内容。协议实现保持无第三方依赖，便于在
//! Tauri IPC 的 blocking 线程中运行，也便于用本地 mock 服务器做端到端测试。
//!
//! 当前**不支持 SRV 记录**（`_minecraft._tcp`）；需要 SRV 的地址请用户填写
//! 实际转发后的主机名和端口。这个限制在 UI/文档中明确展示，不做静默猜测。

use rmccc_error::{Result, RmcccError};
use serde::{Deserialize, Serialize};
use std::io::{Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use ts_rs::TS;

/// 连接 + 读写超时。
const TIMEOUT: Duration = Duration::from_secs(4);
/// 单个状态包上限（防止恶意服务器用超大 VarInt 撑爆内存）。
const MAX_PACKET: usize = 2 * 1024 * 1024;
/// favicon 上限（SLP 规范是 64x64 PNG，128 KiB 足够）。
const MAX_FAVICON: usize = 128 * 1024;
/// 样本玩家最多展示数量。
const MAX_SAMPLE: usize = 12;

/// 一次 SLP 查询结果（IPC / 插件桥共用的稳定 DTO）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export_to = "types.ts")]
pub struct ServerStatus {
    pub host: String,
    pub port: u16,
    /// 聊天组件递归展平后的纯文本 MOTD（可能包含换行）。
    pub motd: String,
    pub online: u32,
    pub max: u32,
    pub sample: Vec<String>,
    pub version: String,
    pub protocol: u32,
    #[ts(type = "number")]
    pub latency_ms: u64,
    /// 只接受 PNG data URL；其他格式/超限内容被丢弃。
    pub favicon: Option<String>,
    #[ts(type = "number")]
    pub queried_at: u64,
}

/// 解析 `host`、`host:port`、`[IPv6]:port`；缺省端口 25565。
pub fn parse_address(input: &str) -> Result<(String, u16)> {
    let raw = input.trim();
    if raw.is_empty() {
        return Err(RmcccError::Validation("服务器地址不能为空".into()));
    }
    if raw.len() > 300 {
        return Err(RmcccError::Validation("服务器地址过长".into()));
    }
    if raw.contains("://") {
        return Err(RmcccError::Validation(
            "请输入主机名或 IP，不要包含 http:// / minecraft:// 前缀".into(),
        ));
    }
    if raw.chars().any(|c| c.is_whitespace() || c.is_control()) {
        return Err(RmcccError::Validation("服务器地址不能包含空白字符".into()));
    }

    if raw.matches(':').count() > 1 && !raw.starts_with('[') {
        return Err(RmcccError::Validation(
            "IPv6 地址必须写成 [地址]:端口".into(),
        ));
    }

    let (host, port) = if let Some(rest) = raw.strip_prefix('[') {
        let end = rest
            .find(']')
            .ok_or_else(|| RmcccError::Validation("IPv6 地址缺少 ]".into()))?;
        let host = &rest[..end];
        let tail = &rest[end + 1..];
        let port = if tail.is_empty() {
            25565
        } else if let Some(p) = tail.strip_prefix(':') {
            parse_port(p)?
        } else {
            return Err(RmcccError::Validation("IPv6 地址后的内容非法".into()));
        };
        (host, port)
    } else if let Some((host, p)) = raw.rsplit_once(':') {
        if p.is_empty() {
            return Err(RmcccError::Validation("端口不能为空".into()));
        }
        if !p.chars().all(|c| c.is_ascii_digit()) {
            return Err(RmcccError::Validation("端口必须是数字".into()));
        }
        (host, parse_port(p)?)
    } else {
        (raw, 25565)
    };

    let host = host.trim().trim_end_matches('.');
    if host.is_empty() {
        return Err(RmcccError::Validation("服务器主机名为空".into()));
    }
    if host.len() > 253 {
        return Err(RmcccError::Validation("服务器主机名过长".into()));
    }
    if host.contains('/') || host.contains('\\') {
        return Err(RmcccError::Validation(
            "服务器主机名不能包含路径分隔符".into(),
        ));
    }
    Ok((host.to_string(), port))
}

fn parse_port(raw: &str) -> Result<u16> {
    let port: u32 = raw
        .parse()
        .map_err(|_| RmcccError::Validation("端口必须是数字".into()))?;
    if !(1..=65535).contains(&port) {
        return Err(RmcccError::Validation("端口必须在 1..=65535".into()));
    }
    Ok(port as u16)
}

/// 查询一个 Minecraft 服务器。**阻塞**调用；Tauri 层应在 blocking 线程中执行。
pub fn query_server_status(address: &str) -> Result<ServerStatus> {
    let (host, port) = parse_address(address)?;
    let started = Instant::now();
    let stream = connect(&host, port)?;
    let mut status = query_stream(stream, &host, port)?;
    status.latency_ms = started.elapsed().as_millis() as u64;
    Ok(status)
}

fn connect(host: &str, port: u16) -> Result<TcpStream> {
    let addrs: Vec<_> = (host, port)
        .to_socket_addrs()
        .map_err(|e| RmcccError::Network(format!("无法解析服务器地址 {host}:{port}：{e}")))?
        .take(8)
        .collect();
    if addrs.is_empty() {
        return Err(RmcccError::Network(format!(
            "无法解析服务器地址 {host}:{port}"
        )));
    }
    let deadline = Instant::now() + TIMEOUT;
    let mut last_error = None;
    for addr in addrs {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            break;
        }
        match TcpStream::connect_timeout(&addr, remaining.min(TIMEOUT)) {
            Ok(stream) => {
                stream.set_read_timeout(Some(TIMEOUT)).ok();
                stream.set_write_timeout(Some(TIMEOUT)).ok();
                return Ok(stream);
            }
            Err(e) => last_error = Some(e),
        }
    }
    let detail = last_error
        .map(|e| e.to_string())
        .unwrap_or_else(|| "连接超时".into());
    Err(RmcccError::Network(format!(
        "无法连接服务器 {host}:{port}（{detail}）"
    )))
}

fn query_stream(mut stream: TcpStream, host: &str, port: u16) -> Result<ServerStatus> {
    // handshake: packetId=0, protocol=47, address, port, nextState=1
    let mut handshake = Vec::with_capacity(host.len() + 16);
    handshake.push(0x00);
    write_varint(&mut handshake, 47);
    write_string(&mut handshake, host);
    handshake.extend_from_slice(&port.to_be_bytes());
    write_varint(&mut handshake, 1);
    write_packet(&mut stream, &handshake)?;

    // status request: packetId=0, 空 body
    write_packet(&mut stream, &[0x00])?;

    let packet = read_packet(&mut stream)?;
    let mut cursor = 0usize;
    let packet_id = read_varint_slice(&packet, &mut cursor)?;
    if packet_id != 0 {
        return Err(RmcccError::Network(format!(
            "服务器返回了非预期的 packet id {packet_id}"
        )));
    }
    let json = read_string_slice(&packet, &mut cursor)?;
    let raw: serde_json::Value = serde_json::from_str(&json)
        .map_err(|e| RmcccError::Network(format!("服务器状态 JSON 解析失败：{e}")))?;

    let motd = raw
        .get("description")
        .map(flatten_description)
        .unwrap_or_default();
    let motd = sanitize_motd(&motd);

    let online = raw
        .get("players")
        .and_then(|p| p.get("online"))
        .and_then(|v| v.as_u64())
        .unwrap_or(0)
        .min(u32::MAX as u64) as u32;
    let max = raw
        .get("players")
        .and_then(|p| p.get("max"))
        .and_then(|v| v.as_u64())
        .unwrap_or(0)
        .min(u32::MAX as u64) as u32;
    let sample = raw
        .get("players")
        .and_then(|p| p.get("sample"))
        .and_then(|v| v.as_array())
        .map(|list| {
            list.iter()
                .filter_map(|p| p.get("name").and_then(|n| n.as_str()))
                .filter(|n| !n.trim().is_empty())
                .take(MAX_SAMPLE)
                .map(ToOwned::to_owned)
                .collect()
        })
        .unwrap_or_default();
    let version = raw
        .get("version")
        .and_then(|v| v.get("name"))
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .unwrap_or("Unknown")
        .to_string();
    let protocol = raw
        .get("version")
        .and_then(|v| v.get("protocol"))
        .and_then(|v| v.as_u64())
        .unwrap_or(0)
        .min(u32::MAX as u64) as u32;
    let favicon = raw
        .get("favicon")
        .and_then(|v| v.as_str())
        .filter(|v| v.starts_with("data:image/png;base64,") && v.len() <= MAX_FAVICON)
        .map(ToOwned::to_owned);

    Ok(ServerStatus {
        host: host.to_string(),
        port,
        motd,
        online,
        max,
        sample,
        version,
        protocol,
        latency_ms: 0,
        favicon,
        queried_at: now_ms(),
    })
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// 把聊天组件递归展平成文本。兼容 string / `{text,extra}` / `{translate}`。
fn flatten_description(value: &serde_json::Value) -> String {
    fn walk(value: &serde_json::Value, out: &mut String) {
        match value {
            serde_json::Value::String(text) => out.push_str(text),
            serde_json::Value::Array(items) => {
                for item in items {
                    walk(item, out);
                }
            }
            serde_json::Value::Object(map) => {
                if let Some(text) = map.get("text").and_then(|v| v.as_str()) {
                    out.push_str(text);
                } else if let Some(translate) = map.get("translate").and_then(|v| v.as_str()) {
                    out.push_str(translate);
                    if let Some(args) = map.get("with").and_then(|v| v.as_array()) {
                        for arg in args {
                            walk(arg, out);
                        }
                    }
                }
                if let Some(extra) = map.get("extra") {
                    walk(extra, out);
                }
            }
            _ => {}
        }
    }
    let mut out = String::new();
    walk(value, &mut out);
    out
}

fn sanitize_motd(raw: &str) -> String {
    let mut text = String::with_capacity(raw.len());
    let mut chars = raw.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '§' {
            // `§` 后跟一个格式码；丢弃两者。
            chars.next();
            continue;
        }
        text.push(ch);
    }
    let trimmed = text.trim();
    if trimmed.is_empty() {
        "Minecraft Server".into()
    } else {
        trimmed.to_string()
    }
}

fn write_packet(stream: &mut TcpStream, payload: &[u8]) -> Result<()> {
    let mut framed = Vec::with_capacity(payload.len() + 5);
    write_varint(&mut framed, payload.len() as i32);
    framed.extend_from_slice(payload);
    stream
        .write_all(&framed)
        .map_err(|e| RmcccError::Network(format!("发送状态查询失败：{e}")))
}

fn read_packet(stream: &mut TcpStream) -> Result<Vec<u8>> {
    let length = read_varint_reader(stream)?;
    if length <= 0 || length as usize > MAX_PACKET {
        return Err(RmcccError::Network(format!(
            "服务器状态包长度非法：{length}"
        )));
    }
    let mut buf = vec![0u8; length as usize];
    stream
        .read_exact(&mut buf)
        .map_err(|e| RmcccError::Network(format!("读取服务器状态失败：{e}")))?;
    Ok(buf)
}

fn write_varint(out: &mut Vec<u8>, value: i32) {
    let mut value = value as u32;
    loop {
        let mut byte = (value & 0x7f) as u8;
        value >>= 7;
        if value != 0 {
            byte |= 0x80;
        }
        out.push(byte);
        if value == 0 {
            break;
        }
    }
}

fn read_varint_reader(stream: &mut TcpStream) -> Result<i32> {
    let mut result = 0i32;
    for shift in (0..35).step_by(7) {
        let mut byte = [0u8; 1];
        stream
            .read_exact(&mut byte)
            .map_err(|e| RmcccError::Network(format!("读取 VarInt 失败：{e}")))?;
        result |= ((byte[0] & 0x7f) as i32) << shift;
        if byte[0] & 0x80 == 0 {
            return Ok(result);
        }
    }
    Err(RmcccError::Network("VarInt 过长".into()))
}

fn read_varint_slice(bytes: &[u8], cursor: &mut usize) -> Result<i32> {
    let mut result = 0i32;
    for shift in (0..35).step_by(7) {
        let byte = *bytes
            .get(*cursor)
            .ok_or_else(|| RmcccError::Network("状态包在 VarInt 处截断".into()))?;
        *cursor += 1;
        result |= ((byte & 0x7f) as i32) << shift;
        if byte & 0x80 == 0 {
            return Ok(result);
        }
    }
    Err(RmcccError::Network("VarInt 过长".into()))
}

fn write_string(out: &mut Vec<u8>, value: &str) {
    write_varint(out, value.len() as i32);
    out.extend_from_slice(value.as_bytes());
}

fn read_string_slice(bytes: &[u8], cursor: &mut usize) -> Result<String> {
    let len = read_varint_slice(bytes, cursor)?;
    if len < 0 || len as usize > MAX_PACKET {
        return Err(RmcccError::Network("字符串长度非法".into()));
    }
    let end = *cursor + len as usize;
    let slice = bytes
        .get(*cursor..end)
        .ok_or_else(|| RmcccError::Network("状态包在字符串处截断".into()))?;
    *cursor = end;
    String::from_utf8(slice.to_vec())
        .map_err(|e| RmcccError::Network(format!("状态包不是合法 UTF-8：{e}")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::TcpListener;
    use std::thread;

    fn frame_response(json: &str) -> Vec<u8> {
        let mut payload = Vec::new();
        write_varint(&mut payload, 0);
        write_string(&mut payload, json);
        let mut framed = Vec::new();
        write_varint(&mut framed, payload.len() as i32);
        framed.extend(payload);
        framed
    }

    #[test]
    fn parses_addresses() {
        assert_eq!(
            parse_address("play.example.net").unwrap(),
            ("play.example.net".into(), 25565)
        );
        assert_eq!(
            parse_address("play.example.net:25566").unwrap(),
            ("play.example.net".into(), 25566)
        );
        assert_eq!(parse_address("[::1]:25567").unwrap(), ("::1".into(), 25567));
        assert_eq!(
            parse_address("  example.com.  ").unwrap(),
            ("example.com".into(), 25565)
        );
        assert!(parse_address("").is_err());
        assert!(parse_address("https://example.com").is_err());
        assert!(parse_address("example.com:0").is_err());
        assert!(parse_address("a b").is_err());
    }

    #[test]
    fn flattens_chat_components() {
        let value = serde_json::json!({
            "text": "§aHello ",
            "extra": [
                { "text": "world", "color": "red" },
                { "translate": "chat.type.text", "with": ["Steve", "hi"] }
            ]
        });
        assert_eq!(
            flatten_description(&value),
            "§aHello worldchat.type.textStevehi"
        );
        assert_eq!(
            sanitize_motd(&flatten_description(&value)),
            "Hello worldchat.type.textStevehi"
        );
    }

    #[test]
    fn queries_local_mock_server() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let response = frame_response(
            r#"{"version":{"name":"1.21.4","protocol":769},"players":{"online":3,"max":20,"sample":[{"name":"Alex"},{"name":"Steve"}]},"description":{"text":"§aHello ","extra":[{"text":"world"}]},"favicon":"data:image/png;base64,AAAA"}"#,
        );
        let handle = thread::spawn(move || {
            let (mut socket, _) = listener.accept().unwrap();
            // handshake + status request：读到两条长度前缀包即可（不再解析 body）。
            let mut header = [0u8; 1];
            socket.read_exact(&mut header).unwrap();
            let len = header[0] as usize;
            let mut body = vec![0u8; len];
            socket.read_exact(&mut body).unwrap();
            socket.read_exact(&mut header).unwrap();
            let len = header[0] as usize;
            let mut body = vec![0u8; len];
            socket.read_exact(&mut body).unwrap();
            socket.write_all(&response).unwrap();
        });
        let status = query_server_status(&format!("127.0.0.1:{port}")).unwrap();
        handle.join().unwrap();
        assert_eq!(status.host, "127.0.0.1");
        assert_eq!(status.port, port);
        assert_eq!(status.motd, "Hello world");
        assert_eq!(status.online, 3);
        assert_eq!(status.max, 20);
        assert_eq!(status.sample, vec!["Alex", "Steve"]);
        assert_eq!(status.version, "1.21.4");
        assert_eq!(status.protocol, 769);
        assert_eq!(
            status.favicon.as_deref(),
            Some("data:image/png;base64,AAAA")
        );
    }

    #[test]
    fn rejects_http_prefix() {
        assert!(parse_address("http://example.com").is_err());
    }
}
