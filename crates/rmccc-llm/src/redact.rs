//! 脱敏规则（ADR-010 决策 4）——**写盘前**执行，不是展示时才处理。
//!
//! 规则有意保持保守且可解释：
//!
//! | 目标 | 处理 |
//! |------|------|
//! | 敏感字段名（`api_key` / `token` / `secret` / `password` / `authorization` …） | 值替换为 `«redacted»` |
//! | 凭据形状的字符串（`sk-…` / `gsk_…` / `ghp_…` / `AKIA…` / `xoxb-…`） | token 替换为 `«redacted»` |
//! | URL 查询参数里的敏感名（`?token=…` / `&api_key=…`） | 值替换为 `«redacted»` |
//! | 用户 home 目录前缀 | 替换为 `~` |
//! | 邮箱 | `«email»` |
//!
//! 不做"猜测式"脱敏（例如任意长数字），避免把正常内容改坏。

use serde_json::{Map, Value};
use std::path::{Path, PathBuf};

/// 当前用户的 home 目录（ADR-010 决策 4：落盘前把 home 前缀替换为 `~`）。
///
/// 取不到（环境变量缺失等）返回 `None`，此时调用方跳过该规则——
/// 与 `redact_text(text, None)` 的既有语义一致。
pub fn user_home() -> Option<PathBuf> {
    std::env::home_dir().filter(|p| !p.as_os_str().is_empty())
}

/// 字段名命中即整体打码。
const SENSITIVE_KEYS: &[&str] = &[
    "api_key",
    "apikey",
    "api-key",
    "token",
    "access_token",
    "refresh_token",
    "secret",
    "client_secret",
    "password",
    "authorization",
    "bearer",
    "private_key",
];

/// 凭据形状前缀。太短的不处理，避免误伤普通单词。
const SECRET_PREFIXES: &[&str] = &["sk-", "gsk_", "ghp_", "gho_", "AKIA", "xoxb-", "xoxp-"];

/// 递归脱敏 JSON（对象键、数组元素、字符串）。
pub fn redact_json(value: &Value, home: Option<&Path>) -> Value {
    match value {
        Value::Object(map) => {
            let mut out = Map::new();
            for (key, entry) in map {
                if is_sensitive_key(key) {
                    out.insert(key.clone(), Value::String("«redacted»".into()));
                } else {
                    out.insert(key.clone(), redact_json(entry, home));
                }
            }
            Value::Object(out)
        }
        Value::Array(items) => Value::Array(items.iter().map(|v| redact_json(v, home)).collect()),
        Value::String(text) => Value::String(redact_text(text, home)),
        other => other.clone(),
    }
}

/// 文本脱敏：home 前缀、邮箱、凭据形状 token。
pub fn redact_text(text: &str, home: Option<&Path>) -> String {
    let without_home = match home {
        Some(home) if !home.as_os_str().is_empty() => {
            let home_str = home.to_string_lossy();
            text.replace(home_str.as_ref(), "~")
        }
        _ => text.to_string(),
    };

    // `Bearer <token>`：按词判断时需要看前一个词，否则 token 本身不像凭据
    // （例如随机十六进制串），就会漏掉。
    let mut previous_was_bearer = false;
    let words = without_home
        .split_whitespace()
        .map(|word| {
            let redacted = if previous_was_bearer {
                redact_after_bearer(word)
            } else {
                redact_word(word)
            };
            previous_was_bearer = word.eq_ignore_ascii_case("bearer");
            redacted
        })
        .collect::<Vec<_>>()
        .join(" ");
    redact_sensitive_params(&words)
}

/// 屏蔽 URL 查询串/`key=value` 中的敏感值（下载链接常带 `?token=…`）。
fn redact_sensitive_params(text: &str) -> String {
    // `to_ascii_lowercase` 只映射 ASCII，字节长度与原文一致，索引可直接复用。
    let lower = text.to_ascii_lowercase();
    let mut out = String::with_capacity(text.len());
    let mut cursor = 0usize;
    while let Some((_, value_start)) = find_sensitive_param(&lower, cursor) {
        let value_end = text[value_start..]
            .find(|c: char| c == '&' || c == '#' || c.is_whitespace())
            .map(|offset| value_start + offset)
            .unwrap_or(text.len());
        out.push_str(&text[cursor..value_start]);
        out.push_str("«redacted»");
        cursor = value_end;
    }
    out.push_str(&text[cursor..]);
    out
}

/// 从 `from` 起找最早的 `敏感名=`，返回值起点与值起点。
fn find_sensitive_param(lower: &str, from: usize) -> Option<(usize, usize)> {
    let mut best: Option<(usize, usize)> = None;
    for key in SENSITIVE_KEYS {
        let needle = format!("{key}=");
        let mut search = from;
        while let Some(offset) = lower[search..].find(&needle) {
            let start = search + offset;
            let boundary_ok = match lower[..start].chars().next_back() {
                None => true,
                Some(prev) => matches!(prev, '?' | '&' | ' ' | '\t' | '\n' | '"' | '\''),
            };
            if boundary_ok {
                let value_start = start + needle.len();
                if best.is_none_or(|(best_start, _)| start < best_start) {
                    best = Some((start, value_start));
                }
                break;
            }
            // needle 是 ASCII，`start + needle.len()` 必然落在字符边界上。
            search = start + needle.len();
        }
    }
    best
}

/// `Bearer` 之后的任意非空 token 一律视为凭据。
fn redact_after_bearer(word: &str) -> String {
    let (prefix, core, suffix) = split_punctuation(word);
    if core.is_empty() {
        return word.to_string();
    }
    format!("{prefix}«redacted»{suffix}")
}

fn redact_word(word: &str) -> String {
    let (prefix, core, suffix) = split_punctuation(word);
    if core.contains('@') && core.len() >= 3 && !core.starts_with('@') {
        return format!("{prefix}«email»{suffix}");
    }
    let looks_secret = SECRET_PREFIXES
        .iter()
        .any(|p| core.starts_with(p) && core.len() >= 12);
    if looks_secret {
        format!("{prefix}«redacted»{suffix}")
    } else {
        word.to_string()
    }
}

fn split_punctuation(word: &str) -> (&str, &str, &str) {
    let start = word
        .find(|c: char| c.is_ascii_alphanumeric())
        .unwrap_or(word.len());
    let end = word
        .rfind(|c: char| c.is_ascii_alphanumeric())
        .map(|i| i + 1)
        .unwrap_or(start);
    if start >= end {
        return ("", word, "");
    }
    (&word[..start], &word[start..end], &word[end..])
}

fn is_sensitive_key(key: &str) -> bool {
    let lower = key.to_ascii_lowercase();
    SENSITIVE_KEYS.iter().any(|needle| lower.contains(needle))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn sensitive_object_fields_are_masked() {
        let value = json!({
            "api_key": "anything",
            "nested": { "authorization": "Bearer abc", "ok": "keep" },
            "list": [{ "token": "t" }]
        });
        let redacted = redact_json(&value, None);
        assert_eq!(redacted["api_key"], "«redacted»");
        assert_eq!(redacted["nested"]["authorization"], "«redacted»");
        assert_eq!(redacted["nested"]["ok"], "keep");
        assert_eq!(redacted["list"][0]["token"], "«redacted»");
    }

    #[test]
    fn secret_shaped_tokens_are_masked() {
        let text = "using sk-abcdefghijklmnop and ghp_1234567890abcdef plus normal words";
        let redacted = redact_text(text, None);
        assert!(!redacted.contains("sk-abcdefghijklmnop"));
        assert!(!redacted.contains("ghp_1234567890abcdef"));
        assert!(redacted.contains("normal words"));
        assert_eq!(redacted.matches("«redacted»").count(), 2);
    }

    #[test]
    fn short_prefix_words_survive() {
        let redacted = redact_text("token risk- small", None);
        assert_eq!(redacted, "token risk- small");
    }

    #[test]
    fn home_prefix_is_shortened() {
        let home = Path::new("/Users/alice");
        let text = "log at /Users/alice/Library/Logs/latest.log";
        let redacted = redact_text(text, Some(home));
        assert_eq!(redacted, "log at ~/Library/Logs/latest.log");
    }

    #[test]
    fn emails_are_masked_but_prose_is_kept() {
        let redacted = redact_text("contact alice@example.com or @handle", None);
        assert_eq!(redacted, "contact «email» or @handle");
    }

    #[test]
    fn non_string_values_are_preserved() {
        let value = json!({ "count": 3, "flag": true, "nil": null, "items": [1, 2] });
        assert_eq!(redact_json(&value, None), value);
    }

    #[test]
    fn query_parameters_are_masked() {
        let text = "GET https://cdn.example.com/a.jar?file=x&token=abc123&sig=keep";
        let redacted = redact_text(text, None);
        assert_eq!(
            redacted,
            "GET https://cdn.example.com/a.jar?file=x&token=«redacted»&sig=keep"
        );
        assert!(!redacted.contains("abc123"));
    }

    #[test]
    fn parameter_names_need_a_boundary() {
        // `mytoken=` 不是 `token=`：不能把正常字段名切开。
        let redacted = redact_text("set mytoken=abc123 now", None);
        assert_eq!(redacted, "set mytoken=abc123 now");
    }
}
