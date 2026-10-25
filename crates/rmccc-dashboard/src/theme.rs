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

/// 「林间」：绿意暗色， 苔藓绿点缀。
pub fn builtin_forest() -> ThemeTokens {
    ThemeTokens {
        background: "#0c1512".into(),
        surface: "#122019".into(),
        surface_elevated: "#1a2c22".into(),
        text: "#e3efe7".into(),
        text_muted: "#82a491".into(),
        accent: "#43c783".into(),
        accent_contrast: "#06130c".into(),
        border: "#244234".into(),
    }
}

/// 「深海」：青蓝暗色，海面蓝点缀。
pub fn builtin_ocean() -> ThemeTokens {
    ThemeTokens {
        background: "#081522".into(),
        surface: "#0e2135".into(),
        surface_elevated: "#152c45".into(),
        text: "#ddebf7".into(),
        text_muted: "#7fa0ba".into(),
        accent: "#35a7e0".into(),
        accent_contrast: "#04121f".into(),
        border: "#20425c".into(),
    }
}

/// 「熔岩」：暖色暗调，熔岩橙点缀。
pub fn builtin_ember() -> ThemeTokens {
    ThemeTokens {
        background: "#171009".into(),
        surface: "#221812".into(),
        surface_elevated: "#2e211a".into(),
        text: "#f4e9e1".into(),
        text_muted: "#b09a89".into(),
        accent: "#ef7d3a".into(),
        accent_contrast: "#1c0e04".into(),
        border: "#44322a".into(),
    }
}

/// 「樱吹雪」：粉调亮色，樱粉点缀。
pub fn builtin_sakura() -> ThemeTokens {
    ThemeTokens {
        background: "#faf1f4".into(),
        surface: "#ffffff".into(),
        surface_elevated: "#fdf3f7".into(),
        text: "#42222f".into(),
        text_muted: "#a58493".into(),
        accent: "#e05687".into(),
        accent_contrast: "#ffffff".into(),
        border: "#f3d9e2".into(),
    }
}

/// 「纸墨」：暖纸色亮色（低蓝光护眼），赭金点缀。
pub fn builtin_paper() -> ThemeTokens {
    ThemeTokens {
        background: "#f4efe4".into(),
        surface: "#fffdf7".into(),
        surface_elevated: "#faf5ea".into(),
        text: "#332e20".into(),
        text_muted: "#958c74".into(),
        accent: "#a5721f".into(),
        accent_contrast: "#ffffff".into(),
        border: "#e7dfca".into(),
    }
}

/// 「晴空」：蓝调亮色，天青点缀。
pub fn builtin_sky() -> ThemeTokens {
    ThemeTokens {
        background: "#eff5fb".into(),
        surface: "#ffffff".into(),
        surface_elevated: "#f4f9fe".into(),
        text: "#1c2e40".into(),
        text_muted: "#7e93a7".into(),
        accent: "#1d86c9".into(),
        accent_contrast: "#ffffff".into(),
        border: "#d9e6f2".into(),
    }
}

pub fn builtin_list() -> Vec<ThemeInfo> {
    vec![
        ThemeInfo {
            key: "builtin:dark".into(),
            name: "Dark · 靛夜".into(),
            is_builtin: true,
            mode: "dark".into(),
        },
        ThemeInfo {
            key: "builtin:forest".into(),
            name: "林间".into(),
            is_builtin: true,
            mode: "dark".into(),
        },
        ThemeInfo {
            key: "builtin:ocean".into(),
            name: "深海".into(),
            is_builtin: true,
            mode: "dark".into(),
        },
        ThemeInfo {
            key: "builtin:ember".into(),
            name: "熔岩".into(),
            is_builtin: true,
            mode: "dark".into(),
        },
        ThemeInfo {
            key: "builtin:light".into(),
            name: "Light · 晨白".into(),
            is_builtin: true,
            mode: "light".into(),
        },
        ThemeInfo {
            key: "builtin:sakura".into(),
            name: "樱吹雪".into(),
            is_builtin: true,
            mode: "light".into(),
        },
        ThemeInfo {
            key: "builtin:paper".into(),
            name: "纸墨".into(),
            is_builtin: true,
            mode: "light".into(),
        },
        ThemeInfo {
            key: "builtin:sky".into(),
            name: "晴空".into(),
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

    #[test]
    fn all_builtin_themes_have_unique_tokens_and_keys() {
        let themes = builtin_list();
        assert_eq!(themes.len(), 8);
        let mut keys: Vec<_> = themes.iter().map(|t| t.key.clone()).collect();
        keys.sort();
        keys.dedup();
        assert_eq!(keys.len(), 8, "theme keys must be unique");
        for info in &themes {
            let tokens = match info.key.as_str() {
                "builtin:dark" => builtin_dark(),
                "builtin:light" => builtin_light(),
                "builtin:forest" => builtin_forest(),
                "builtin:ocean" => builtin_ocean(),
                "builtin:ember" => builtin_ember(),
                "builtin:sakura" => builtin_sakura(),
                "builtin:paper" => builtin_paper(),
                "builtin:sky" => builtin_sky(),
                other => panic!("builtin_list contains unknown key: {other}"),
            };
            // 视觉定义令牌必须两两不同（防复制粘贴漏改）；accent_contrast
            // 除外——亮色主题它与 surface 合理地同为纯白。
            let vals = [
                &tokens.background,
                &tokens.surface,
                &tokens.surface_elevated,
                &tokens.text,
                &tokens.text_muted,
                &tokens.accent,
                &tokens.border,
            ];
            let mut sorted = vals.to_vec();
            sorted.sort();
            let uniq = sorted
                .iter()
                .zip(sorted.iter().skip(1))
                .all(|(a, b)| a != b);
            assert!(uniq, "theme {} has duplicate tokens", info.key);
        }
    }
}
