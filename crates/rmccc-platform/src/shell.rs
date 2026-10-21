//! shell / 打开外部资源（指南 §5.7.2「shell / 打开 URL / 打开路径」）。
//!
//! 统一收口"用系统默认程序打开 URL / 文件夹"，避免各层各自 `cfg` 拼
//! `open` / `explorer` / `xdg-open`。

use rmccc_error::{Result, RmcccError};
use std::path::Path;

/// 平台 shell 能力。
pub trait PlatformShell: Send + Sync {
    /// 用系统默认浏览器打开 **http(s)** URL。
    ///
    /// 非 `http`/`https` scheme 一律拒绝：外部命令拼串是命令注入的经典入口
    /// （`file://`、`javascript:`、`-x` 参数注入）。
    fn open_url(&self, url: &str) -> Result<()>;

    /// 用系统文件管理器打开一个目录或选中一个文件。
    fn open_path(&self, path: &Path) -> Result<()>;
}

/// URL scheme 白名单校验（纯函数，便于单测）。
pub fn validate_openable_url(url: &str) -> Result<()> {
    let trimmed = url.trim();
    if trimmed.is_empty() {
        return Err(RmcccError::Validation("URL 不能为空".into()));
    }
    // 以 `-` 开头的参数会被 `open` / `xdg-open` 当成选项，必须拒绝。
    if trimmed.starts_with('-') {
        return Err(RmcccError::Validation(
            "URL 不能以 '-' 开头（会被当作命令行选项）".into(),
        ));
    }
    let lower = trimmed.to_ascii_lowercase();
    if lower.starts_with("https://") || lower.starts_with("http://") {
        return Ok(());
    }
    Err(RmcccError::Validation(format!(
        "只允许打开 http(s) URL，收到: {trimmed}"
    )))
}

/// 后端标识（日志 / 诊断）。
pub fn shell_backend_name() -> &'static str {
    if cfg!(target_os = "macos") {
        "macos"
    } else if cfg!(target_os = "windows") {
        "windows"
    } else if cfg!(target_os = "linux") {
        "linux"
    } else {
        "unsupported"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_http_and_https() {
        validate_openable_url("https://modrinth.com/mod/sodium").expect("https");
        validate_openable_url("HTTP://example.com/x").expect("http uppercase");
        validate_openable_url("  https://example.com  ").expect("trimmed");
    }

    #[test]
    fn rejects_non_http_schemes() {
        for url in [
            "file:///etc/passwd",
            "javascript:alert(1)",
            "ftp://example.com/a",
            "data:text/html,<script>",
            "mailto:a@b.c",
        ] {
            assert!(validate_openable_url(url).is_err(), "{url} 应被拒绝");
        }
    }

    #[test]
    fn rejects_empty_and_option_like_urls() {
        assert!(validate_openable_url("").is_err());
        assert!(validate_openable_url("   ").is_err());
        // 命令注入/选项注入
        assert!(validate_openable_url("-a /Applications").is_err());
        assert!(validate_openable_url("--args").is_err());
    }
}
