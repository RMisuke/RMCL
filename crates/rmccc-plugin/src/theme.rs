//! 主题文档（插件包内 `themes/<id>.json`）的格式与校验（ADR-005 决策 7）。
//!
//! 主题是**纯数据**：宿主只读 `colors` 并映射成 CSS 变量，不执行任何代码。
//! 因此本模块唯一的职责是"把不可信数据收窄成有限语法"：
//!
//! - 颜色只接受 `#rgb` / `#rrggbb` / `#rrggbbaa`（`url()`、`var()`、`;`、`{}` 一律拒绝，
//!   否则主题值会变成 CSS 注入/外链探针）；
//! - 文件 ≤ [`MAX_THEME_BYTES`]，`schemaVersion` 必须为 1，`mode` 只允许 `dark`/`light`；
//! - 宿主安全语义色（`danger`/`success`/`warning`）即使出现也不会进入令牌
//!   （见 `rmccc_dashboard::theme`，宿主拥有这三个颜色）。
//!
//! 校验同时被三条路径复用，保证"能打包 = 能安装 = 能应用"：
//! 包检查（`package::inspect_reader`）、应用令牌（`dashboard_get_theme_tokens`）、
//! 主题选择（`dashboard_set_theme_preference`）。

use rmccc_error::{PluginErrorCode, Result, RmcccError};
use serde::Deserialize;
use std::collections::BTreeMap;

/// 主题文档 schema 版本（v1 严格模式：未知字段拒绝）。
pub const THEME_SCHEMA_VERSION: u32 = 1;

/// 单个主题文件上限。主题只是 8 个颜色，64 KiB 足够容纳说明性元数据。
pub const MAX_THEME_BYTES: u64 = 64 * 1024;

/// 主题名长度上限（字符数）。
pub const MAX_THEME_NAME_CHARS: usize = 80;

/// 必需颜色令牌。
pub const REQUIRED_COLORS: &[&str] = &["background", "surface", "text", "accent", "border"];

/// 可选颜色令牌（camelCase 与 snake_case 都接受，camelCase 优先）。
pub const OPTIONAL_COLORS: &[&str] = &[
    "surfaceElevated",
    "surface_elevated",
    "textMuted",
    "text_muted",
    "accentContrast",
    "accent_contrast",
];

/// 宿主安全语义色：允许出现在文件里，但**不会**被应用。
pub const SAFETY_COLORS: &[&str] = &["danger", "success", "warning"];

/// 主题文档（`themes/<id>.json`）。
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ThemeDocument {
    pub schema_version: u32,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub mode: Option<String>,
    #[serde(default)]
    pub colors: BTreeMap<String, String>,
}

impl ThemeDocument {
    /// 解析并校验一个主题文件（大小上限 + schema + 颜色语法）。
    ///
    /// 这是主题进入系统的**唯一入口**：调用方不得绕开它直接读 `colors`。
    pub fn parse(raw: &str) -> Result<Self> {
        if raw.len() as u64 > MAX_THEME_BYTES {
            return Err(theme_error(format!(
                "主题文件过大（{} 字节，上限 {MAX_THEME_BYTES}）",
                raw.len()
            )));
        }
        let document: Self =
            serde_json::from_str(raw).map_err(|e| theme_error(format!("主题 JSON 非法：{e}")))?;
        document.validate()?;
        Ok(document)
    }

    /// 语义校验：版本、名称、mode、必需令牌与颜色语法。
    pub fn validate(&self) -> Result<()> {
        if self.schema_version != THEME_SCHEMA_VERSION {
            return Err(theme_error(format!(
                "不支持的 schemaVersion {}（宿主支持 {THEME_SCHEMA_VERSION}）",
                self.schema_version
            )));
        }
        if let Some(name) = &self.name {
            if name.trim().is_empty() || name.chars().count() > MAX_THEME_NAME_CHARS {
                return Err(theme_error(format!(
                    "主题 name 非法（必须非空且 ≤ {MAX_THEME_NAME_CHARS} 字符）"
                )));
            }
        }
        if let Some(mode) = self.declared_mode() {
            if mode != "dark" && mode != "light" {
                return Err(theme_error(format!(
                    "主题 mode 只允许 dark / light，收到 {mode:?}"
                )));
            }
        }
        for key in REQUIRED_COLORS {
            if !self.colors.contains_key(*key) {
                return Err(theme_error(format!("主题缺少必需颜色令牌：{key}")));
            }
        }
        for (key, value) in &self.colors {
            if is_known_key(key) {
                validate_color(value)?;
            }
            // 未知键不参与渲染，忽略（不阻断作者添加自己的注释性字段）。
        }
        Ok(())
    }

    /// 作者显式声明的明暗；`None` 时由宿主按背景色亮度推断。
    pub fn declared_mode(&self) -> Option<&str> {
        self.mode.as_deref()
    }
}

/// 颜色是否属于令牌表（必需/可选/安全语义色）。
pub fn is_known_key(key: &str) -> bool {
    REQUIRED_COLORS.contains(&key) || OPTIONAL_COLORS.contains(&key) || SAFETY_COLORS.contains(&key)
}

/// 校验主题颜色值：只接受 `#rgb` / `#rrggbb` / `#rrggbbaa`。
///
/// 拒绝一切非十六进制语法——主题值会写进 CSS 自定义属性，若允许任意字符串，
/// `url(https://…)` 之类的值可能被宿主样式消费成外链/追踪请求，`;`/`}` 则是注入面。
pub fn validate_color(value: &str) -> Result<()> {
    let Some(digits) = value.strip_prefix('#') else {
        return Err(theme_error(format!(
            "颜色必须是 #rgb / #rrggbb / #rrggbbaa，收到 {value:?}"
        )));
    };
    if !matches!(digits.len(), 3 | 6 | 8) || !digits.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(theme_error(format!(
            "颜色必须是 #rgb / #rrggbb / #rrggbbaa，收到 {value:?}"
        )));
    }
    Ok(())
}

/// [`validate_color`] 的布尔形式（主题 JSON 之外的颜色值，如插件 UI 传入的令牌）。
pub fn is_valid_color(value: &str) -> bool {
    validate_color(value).is_ok()
}

fn theme_error(message: String) -> RmcccError {
    RmcccError::Plugin {
        code: PluginErrorCode::ConfigInvalid,
        message,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn document(extra: &str) -> String {
        format!(
            r##"{{
                "schemaVersion": 1,
                "name": "Midnight",
                "mode": "dark",
                "colors": {{
                    "background": "#0b1020",
                    "surface": "#141a2e",
                    "text": "#e6e9f2",
                    "accent": "#7c6cff",
                    "border": "#2a3352"{extra}
                }}
            }}"##
        )
    }

    #[test]
    fn parses_a_minimal_document() {
        let doc = ThemeDocument::parse(&document("")).expect("valid theme");
        assert_eq!(doc.declared_mode(), Some("dark"));
        assert_eq!(doc.colors["background"], "#0b1020");
    }

    #[test]
    fn accepts_optional_tokens_and_safety_colors() {
        let raw = document(
            r##",
                "surfaceElevated": "#1c2440",
                "textMuted": "#8b93a8",
                "accentContrast": "#ffffff",
                "danger": "#ff6b6b""##,
        );
        ThemeDocument::parse(&raw).expect("optional + safety colors are allowed");
    }

    #[test]
    fn accepts_short_and_alpha_hex() {
        validate_color("#fff").unwrap();
        validate_color("#FFF").unwrap();
        validate_color("#0b1020ff").unwrap();
    }

    #[test]
    fn rejects_css_injection_vectors() {
        for bad in [
            "url(https://tracker.example/pixel)",
            "red; background-image: url(https://example/x)",
            "var(--surface)",
            "rgb(0 0 0)",
            "#12",
            "#12345",
            "#gggggg",
            "red",
            "",
            "#0b1020; }",
            "#0b1020\n",
        ] {
            assert!(validate_color(bad).is_err(), "expected reject for {bad:?}");
        }
    }

    #[test]
    fn rejects_missing_required_token() {
        let raw = r##"{ "schemaVersion": 1, "colors": { "background": "#000" } }"##;
        assert!(ThemeDocument::parse(raw).is_err());
    }

    #[test]
    fn rejects_wrong_schema_version_and_unknown_fields() {
        let raw = document("").replace("\"schemaVersion\": 1", "\"schemaVersion\": 2");
        assert!(ThemeDocument::parse(&raw).is_err());
        let raw = document("").replace(
            "\"schemaVersion\": 1,",
            "\"schemaVersion\": 1, \"colours\": {},",
        );
        assert!(ThemeDocument::parse(&raw).is_err());
    }

    #[test]
    fn rejects_bad_mode_and_name() {
        let raw = document("").replace("\"mode\": \"dark\"", "\"mode\": \"neon\"");
        assert!(ThemeDocument::parse(&raw).is_err());
        let raw = document("").replace("\"Midnight\"", "\"\"");
        assert!(ThemeDocument::parse(&raw).is_err());
    }

    #[test]
    fn rejects_oversized_document() {
        let raw = document(&format!(
            ",\n \"pad\": \"{}\"",
            "x".repeat(MAX_THEME_BYTES as usize)
        ));
        assert!(ThemeDocument::parse(&raw).is_err());
        assert!(ThemeDocument::parse(&raw)
            .unwrap_err()
            .to_string()
            .contains("过大"));
    }
}
