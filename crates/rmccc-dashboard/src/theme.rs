//! 主题令牌系统（ADR-005 决策 7）。
//!
//! 主题包 = Phase 2 的 `contributes.themes`（JSON + 静态资源，无 JS）。
//! 安全语义色（danger/success/warning）由宿主拥有，主题不得覆盖。

use rmccc_error::{PluginErrorCode, Result, RmcccError};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use ts_rs::TS;

/// 主题令牌（映射到 `--rmccc-*` CSS variables）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export_to = "types.ts")]
pub struct ThemeTokens {
    pub background: String,
    pub surface: String,
    pub surface_elevated: String,
    pub text: String,
    pub text_muted: String,
    pub accent: String,
    pub accent_contrast: String,
    pub border: String,
}

/// 宿主拥有的安全语义色（主题不能覆盖）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export_to = "types.ts")]
pub struct SafetyColors {
    pub danger: String,
    pub success: String,
    pub warning: String,
}

impl SafetyColors {
    pub fn dark() -> Self {
        Self {
            danger: "#ff6b6b".into(),
            success: "#4ade80".into(),
            warning: "#f5a524".into(),
        }
    }

    pub fn light() -> Self {
        Self {
            danger: "#dc2626".into(),
            success: "#16a34a".into(),
            warning: "#d97706".into(),
        }
    }
}

/// 主题 JSON（插件 `themes/<id>.json` 格式）。
#[derive(Debug, Clone, Deserialize)]
pub struct ThemeJson {
    #[serde(default)]
    pub schema_version: Option<u32>,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub colors: BTreeMap<String, String>,
}

/// 解析后的主题信息。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export_to = "types.ts")]
pub struct ThemeInfo {
    /// `builtin:dark` / `builtin:light` / `<pluginId>/<themeId>`。
    pub key: String,
    pub name: String,
    pub is_builtin: bool,
    /// `dark` / `light`（内置）；插件主题由 JSON 内容决定。
    pub mode: String,
}

/// 主题偏好（用户设置）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export_to = "types.ts")]
pub enum ThemePreference {
    Light,
    Dark,
    #[default]
    FollowSystem,
}

/// 从插件主题 JSON 提取令牌；忽略安全语义色。
pub fn extract_tokens(theme: &ThemeJson) -> Result<ThemeTokens> {
    let c = &theme.colors;
    let required = |key: &str| -> Result<String> {
        c.get(key).cloned().ok_or_else(|| RmcccError::Plugin {
            code: PluginErrorCode::ConfigInvalid,
            message: format!("主题缺少必需颜色令牌：{key}"),
        })
    };
    let optional_or = |primary: &str, fallback: &str| -> Option<String> {
        c.get(primary).or_else(|| c.get(fallback)).cloned()
    };

    // 安全语义色出现在主题 JSON 中时忽略并记录（不报错）。
    for forbidden in ["danger", "success", "warning"] {
        if c.contains_key(forbidden) {
            tracing::warn!(key = forbidden, "主题 JSON 包含宿主安全语义色，已忽略");
        }
    }

    Ok(ThemeTokens {
        background: required("background")?,
        surface: required("surface")?,
        surface_elevated: optional_or("surfaceElevated", "surface_elevated")
            .unwrap_or_else(|| required("surface").unwrap_or_default()),
        text: required("text")?,
        text_muted: optional_or("textMuted", "text_muted")
            .unwrap_or_else(|| required("text").unwrap_or_default()),
        accent: required("accent")?,
        accent_contrast: optional_or("accentContrast", "accent_contrast")
            .unwrap_or_else(|| "#ffffff".into()),
        border: required("border")?,
    })
}

/// 内置安全主题令牌。
pub fn builtin_dark() -> ThemeTokens {
    ThemeTokens {
        background: "#0b1020".into(),
        surface: "#141a2e".into(),
        surface_elevated: "#1c2440".into(),
        text: "#e6e9f2".into(),
        text_muted: "#8b93a8".into(),
        accent: "#7c6cff".into(),
        accent_contrast: "#ffffff".into(),
        border: "#2a3352".into(),
    }
}

pub fn builtin_light() -> ThemeTokens {
    ThemeTokens {
        background: "#f2f2f4".into(),
        surface: "#ffffff".into(),
        surface_elevated: "#fafafa".into(),
        text: "#1a1a2e".into(),
        text_muted: "#888888".into(),
        accent: "#1a56db".into(),
        accent_contrast: "#ffffff".into(),
        border: "#e0e0e4".into(),
    }
}

pub fn builtin_list() -> Vec<ThemeInfo> {
    vec![
        ThemeInfo {
            key: "builtin:dark".into(),
            name: "Dark".into(),
            is_builtin: true,
            mode: "dark".into(),
        },
        ThemeInfo {
            key: "builtin:light".into(),
            name: "Light".into(),
            is_builtin: true,
            mode: "light".into(),
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_tokens_from_theme_json() {
        let json = ThemeJson {
            schema_version: Some(1),
            name: Some("Midnight".into()),
            colors: serde_json::from_str(
                r##"{
                    "background": "#0b1020",
                    "surface": "#141a2e",
                    "surfaceElevated": "#1c2440",
                    "text": "#e6e9f2",
                    "textMuted": "#8b93a8",
                    "accent": "#7c6cff",
                    "accentContrast": "#ffffff",
                    "border": "#2a3352"
                }"##,
            )
            .unwrap(),
        };
        let tokens = extract_tokens(&json).unwrap();
        assert_eq!(tokens.background, "#0b1020");
        assert_eq!(tokens.accent, "#7c6cff");
    }

    #[test]
    fn ignores_safety_colors() {
        let json = ThemeJson {
            schema_version: None,
            name: None,
            colors: serde_json::from_str(
                r##"{
                    "background": "#000",
                    "surface": "#111",
                    "text": "#fff",
                    "accent": "#fff",
                    "border": "#222",
                    "danger": "#ff0000",
                    "success": "#00ff00",
                    "warning": "#ffff00"
                }"##,
            )
            .unwrap(),
        };
        let tokens = extract_tokens(&json).unwrap();
        // 不 panic，安全色被忽略。
        assert_eq!(tokens.background, "#000");
    }

    #[test]
    fn rejects_missing_required_token() {
        let json = ThemeJson {
            schema_version: None,
            name: None,
            colors: BTreeMap::new(),
        };
        assert!(extract_tokens(&json).is_err());
    }

    #[test]
    fn builtin_tokens_differ() {
        assert_ne!(builtin_dark(), builtin_light());
    }
}
