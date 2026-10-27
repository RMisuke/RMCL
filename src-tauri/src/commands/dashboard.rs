//! Dashboard theme/widget IPC commands (Phase 3, ADR-005).
//!
//! The layout CRUD commands (get/save/reset) were retired with the
//! HomeDashboard grid (frontend audit B3-1): the hero home persists its
//! layout locally, and the registry guard requires every registered command
//! to have a frontend caller.
//!
//! Theme resolution is **host-authoritative**: the backend re-reads and
//! re-validates the plugin theme file every time (size + schema + color
//! grammar), and the frontend only maps the returned tokens to CSS variables.

use crate::state::AppState;
use rmccc_core::rmccc_dashboard::{
    builtin_dark, builtin_ember, builtin_forest, builtin_light, builtin_list, builtin_ocean,
    builtin_paper, builtin_sakura, builtin_sky, extract_tokens, theme_mode, ThemeInfo, ThemeJson,
    ThemeSkin, ThemeTokens, WidgetDescriptor, WidgetRegistry, MAX_THEME_BYTES,
};
use rmccc_core::rmccc_plugin::theme::ThemeDocument;
use rmccc_core::rmccc_plugin::theme_assets::{validate_theme_assets, FsAssetReader};
use rmccc_error::{ErrorCode, IpcError};
use serde::Serialize;
use serde_json::{json, Value};
use std::path::Path;
use ts_rs::TS;

fn registry(core: &rmccc_core::CoreContext) -> WidgetRegistry {
    let cards = core.plugin_manager.contributions().dashboard_cards;
    WidgetRegistry::from_contributions(&cards)
}

#[tauri::command]
#[tracing::instrument(skip(state))]
pub async fn dashboard_list_widgets(
    state: tauri::State<'_, AppState>,
) -> Result<Vec<WidgetDescriptor>, IpcError> {
    let core = state.core.read().await;
    Ok(registry(&core).list().to_vec())
}

#[tauri::command]
#[tracing::instrument(skip(state))]
pub async fn dashboard_list_themes(
    state: tauri::State<'_, AppState>,
) -> Result<Vec<ThemeInfo>, IpcError> {
    let core = state.core.read().await;
    let mut themes = builtin_list();
    for view in core.plugin_manager.contributions().themes {
        // 明暗用于主题卡片/默认可读性；读不到（文件被删/损坏）时保持 unknown，
        // 不因为一个坏主题让整个主题列表失败。
        let mode = core
            .plugin_manager
            .installed_dir(&view.plugin_id)
            .and_then(|dir| read_theme_text(&dir, &view.theme.path).ok())
            .and_then(|raw| ThemeJson::parse(&raw).ok())
            .map(|doc| theme_mode(&doc).to_string())
            .unwrap_or_else(|| "custom".into());
        themes.push(ThemeInfo {
            key: view.key,
            name: view.theme.name,
            is_builtin: false,
            mode,
        });
    }
    Ok(themes)
}

#[tauri::command]
#[tracing::instrument(skip(state))]
pub async fn dashboard_get_theme_tokens(
    state: tauri::State<'_, AppState>,
    key: String,
) -> Result<ThemeTokens, IpcError> {
    let core = state.core.read().await;
    theme_tokens_for(&core, &key, "dark")
}

/// 内置主题 key → 令牌；插件主题返回 `None`。
fn builtin_tokens(key: &str) -> Option<ThemeTokens> {
    match key {
        // `light`/`dark` 是旧版偏好（ThemePreference 的序列化值）；继续接受，
        // 避免升级后旧设置无法再被"重新选择"。
        "builtin:dark" | "dark" => Some(builtin_dark()),
        "builtin:light" | "light" => Some(builtin_light()),
        "builtin:forest" => Some(builtin_forest()),
        "builtin:ocean" => Some(builtin_ocean()),
        "builtin:ember" => Some(builtin_ember()),
        "builtin:sakura" => Some(builtin_sakura()),
        "builtin:paper" => Some(builtin_paper()),
        "builtin:sky" => Some(builtin_sky()),
        _ => None,
    }
}

/// 读取主题文件（大小上限与包检查一致）。
fn read_theme_text(dir: &Path, relative: &str) -> Result<String, IpcError> {
    let path = dir.join(relative);
    let metadata = std::fs::metadata(&path)?;
    if metadata.len() > MAX_THEME_BYTES {
        return Err(IpcError {
            code: ErrorCode::ValidationError,
            message: format!(
                "主题文件过大：{} 字节（上限 {MAX_THEME_BYTES}）",
                metadata.len()
            ),
            retryable: false,
        });
    }
    let raw = std::fs::read_to_string(&path)?;
    if raw.len() as u64 > MAX_THEME_BYTES {
        return Err(IpcError {
            code: ErrorCode::ValidationError,
            message: format!("主题文件过大：{} 字节", raw.len()),
            retryable: false,
        });
    }
    Ok(raw)
}

/// 把主题 key 解析为令牌（宿主唯一真相，Phase 3 命令与隔离 UI 桥共用）。
///
/// `key` 为空、`system` 或未知值时，按 `fallback_mode`（`light`/`dark`）回退到内置主题；
/// `fallback_mode` 来自 WebView 的 `prefers-color-scheme`，因为"跟随系统"只有渲染侧知道。
///
/// 插件主题先解析再校验：`url()`/分号之类的值在写进 CSS 变量之前就被拒绝。
pub(crate) fn theme_tokens_for(
    core: &rmccc_core::CoreContext,
    key: &str,
    fallback_mode: &str,
) -> Result<ThemeTokens, IpcError> {
    if let Some(tokens) = builtin_tokens(key) {
        return Ok(tokens);
    }
    if key.is_empty() || key == "system" {
        return Ok(if fallback_mode == "light" {
            builtin_light()
        } else {
            builtin_dark()
        });
    }
    let contributions = core.plugin_manager.contributions();
    let Some(view) = contributions.themes.iter().find(|t| t.key == key) else {
        // 未知主题回退到安全内置主题（ADR-005 决策 7）。
        tracing::warn!(%key, "unknown theme key, falling back to the built-in theme");
        return Ok(if fallback_mode == "light" {
            builtin_light()
        } else {
            builtin_dark()
        });
    };
    let Some(dir) = core.plugin_manager.installed_dir(&view.plugin_id) else {
        return Ok(builtin_dark());
    };
    let raw = read_theme_text(&dir, &view.theme.path)?;
    let theme = ThemeJson::parse(&raw)?;
    Ok(extract_tokens(&theme)?)
}

#[tauri::command]
#[tracing::instrument(skip(state))]
pub async fn dashboard_set_theme_preference(
    state: tauri::State<'_, AppState>,
    theme: String,
) -> Result<rmccc_config::UserSettings, IpcError> {
    let theme = theme.trim();
    let normalized = if theme.is_empty() || theme == "system" || theme == "follow_system" {
        None
    } else {
        Some(theme.to_string())
    };

    // 先验证再落盘：选择一个不存在的/损坏的主题应当立即报错，
    // 而不是"保存成功 + 界面回退成内置主题"（用户会以为主题坏了）。
    if let Some(key) = normalized.as_deref() {
        validate_theme_choice(&state, key).await?;
    }

    let mut core = state.core.write().await;
    core.user_settings.theme = normalized;

    let data_dir = core.config.resolved_data_dir();
    let path = rmccc_config::paths::user_settings_path(&data_dir);
    let content = serde_json::to_string_pretty(&core.user_settings)?;
    core.file_saver.save(path, content);
    Ok(core.user_settings.clone())
}

/// 主题选择校验：内置 key 直接通过；插件 key 必须属于某个已启用插件且文件可解析。
async fn validate_theme_choice(
    state: &tauri::State<'_, AppState>,
    key: &str,
) -> Result<(), IpcError> {
    if builtin_tokens(key).is_some() {
        return Ok(());
    }
    let core = state.core.read().await;
    let contributions = core.plugin_manager.contributions();
    let Some(view) = contributions.themes.iter().find(|t| t.key == key) else {
        return Err(IpcError {
            code: ErrorCode::ValidationError,
            message: format!("主题 {key} 不存在，或所属插件未启用"),
            retryable: false,
        });
    };
    let Some(dir) = core.plugin_manager.installed_dir(&view.plugin_id) else {
        return Err(IpcError {
            code: ErrorCode::ValidationError,
            message: format!("主题 {key} 所属插件未安装"),
            retryable: false,
        });
    };
    let raw = read_theme_text(&dir, &view.theme.path)?;
    let theme = ThemeJson::parse(&raw)?;
    extract_tokens(&theme)?;
    Ok(())
}

/// v2 皮肤的已解析结果（资产已变成 `rmcccplugin://` 只读 URL）。
#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export_to = "types.ts")]
pub struct ThemeSkinPayload {
    pub key: String,
    pub plugin_id: String,
    /// 主题文档是否包含 v2 皮肤段。
    pub requires_skin: bool,
    /// 插件是否持有 `themes.skin`（false 时只应用配色）。
    pub granted: bool,
    pub skin: Option<ThemeSkin>,
}

impl ThemeSkinPayload {
    fn empty(key: &str) -> Self {
        Self {
            key: key.to_string(),
            plugin_id: String::new(),
            requires_skin: false,
            granted: false,
            skin: None,
        }
    }
}

/// 读取主题 v2 皮肤（字体/形状/间距/背景/组件/特效/导航）。
///
/// 纯配色主题（v1）或未授权 `themes.skin` 时返回 `skin: None`，前端只应用颜色。
/// 资产在运行期再校验一次（存在性/大小/魔数/SVG 内容），防止安装后被手改。
#[tauri::command]
#[tracing::instrument(skip(state))]
pub async fn dashboard_get_theme_skin(
    state: tauri::State<'_, AppState>,
    key: String,
) -> Result<ThemeSkinPayload, IpcError> {
    if builtin_tokens(&key).is_some() {
        return Ok(ThemeSkinPayload::empty(&key));
    }
    let core = state.core.read().await;
    let contributions = core.plugin_manager.contributions();
    let Some(view) = contributions.themes.iter().find(|t| t.key == key) else {
        return Ok(ThemeSkinPayload::empty(&key));
    };
    let Some(dir) = core.plugin_manager.installed_dir(&view.plugin_id) else {
        return Ok(ThemeSkinPayload::empty(&key));
    };
    let raw = read_theme_text(&dir, &view.theme.path)?;
    let document = ThemeDocument::parse(&raw)?;
    let requires_skin = document.requires_skin();
    let granted = core
        .plugin_manager
        .authorize(&view.plugin_id, "themes.skin", None, None)
        .is_ok();

    let mut payload = ThemeSkinPayload {
        key: key.clone(),
        plugin_id: view.plugin_id.clone(),
        requires_skin,
        granted,
        skin: None,
    };
    if !requires_skin || !granted {
        return Ok(payload);
    }

    validate_theme_assets(&document, &mut FsAssetReader::new(&dir))?;
    payload.skin = Some(ThemeSkin::from_document(&document, &view.plugin_id));
    Ok(payload)
}

#[tauri::command]
#[tracing::instrument(skip(state))]
pub async fn dashboard_get_theme_preference(
    state: tauri::State<'_, AppState>,
) -> Result<Option<String>, IpcError> {
    let core = state.core.read().await;
    Ok(core.user_settings.theme.clone())
}

/// 主题 JSON 原样读取（调试/编辑器用）。
///
/// 内置主题没有文件，这里按同一 schema 合成一份，避免"查看 JSON"对内置主题返回 null。
#[tauri::command]
#[tracing::instrument(skip(state))]
pub async fn dashboard_get_theme_json(
    state: tauri::State<'_, AppState>,
    key: String,
) -> Result<Value, IpcError> {
    if let Some(value) = builtin_theme_json(&key) {
        return Ok(value);
    }
    let core = state.core.read().await;
    let contributions = core.plugin_manager.contributions();
    let Some(view) = contributions.themes.iter().find(|t| t.key == key) else {
        return Ok(Value::Null);
    };
    let Some(dir) = core.plugin_manager.installed_dir(&view.plugin_id) else {
        return Ok(Value::Null);
    };
    let raw = read_theme_text(&dir, &view.theme.path)?;
    ThemeJson::parse(&raw)?;
    Ok(serde_json::from_str(&raw)?)
}

/// 合成内置主题的 JSON 表示（与插件主题同 schema，便于对照/二次创作）。
fn builtin_theme_json(key: &str) -> Option<Value> {
    let tokens = builtin_tokens(key)?;
    let info = builtin_list().into_iter().find(|t| t.key == key)?;
    Some(json!({
        "schemaVersion": 1,
        "name": info.name,
        "mode": info.mode,
        "colors": {
            "background": tokens.background,
            "surface": tokens.surface,
            "surfaceElevated": tokens.surface_elevated,
            "text": tokens.text,
            "textMuted": tokens.text_muted,
            "accent": tokens.accent,
            "accentContrast": tokens.accent_contrast,
            "border": tokens.border,
        }
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(tag: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "rmccc-dashboard-theme-{tag}-{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn reads_theme_text_with_size_limit() {
        let dir = temp_dir("read");
        std::fs::write(dir.join("ok.json"), "{}").unwrap();
        assert_eq!(read_theme_text(&dir, "ok.json").unwrap(), "{}");

        let big = "x".repeat(MAX_THEME_BYTES as usize + 1);
        std::fs::write(dir.join("big.json"), &big).unwrap();
        let err = read_theme_text(&dir, "big.json").unwrap_err();
        assert_eq!(err.code, ErrorCode::ValidationError);
        assert!(err.message.contains("过大"));

        assert!(read_theme_text(&dir, "missing.json").is_err());
    }

    #[test]
    fn builtin_theme_json_matches_schema() {
        for info in builtin_list() {
            let json = builtin_theme_json(&info.key).expect("builtin key must synthesize");
            assert_eq!(json["schemaVersion"], 1);
            assert_eq!(json["mode"], info.mode);
            for key in ["background", "surface", "text", "accent", "border"] {
                assert!(
                    json["colors"][key].is_string(),
                    "{} missing {key}",
                    info.key
                );
            }
            assert!(builtin_theme_json("builtin:nope").is_none());
        }
    }

    #[test]
    fn legacy_light_dark_aliases_still_resolve() {
        assert_eq!(
            builtin_tokens("dark").unwrap().background,
            builtin_dark().background
        );
        assert_eq!(
            builtin_tokens("light").unwrap().background,
            builtin_light().background
        );
        assert!(builtin_tokens("system").is_none());
        assert!(builtin_tokens("builtin:nope").is_none());
    }

    #[test]
    fn builtin_theme_json_roundtrips_through_validator() {
        // 合成出来的内置 JSON 必须能通过插件主题的同一套校验，否则文档会说谎。
        let json = builtin_theme_json("builtin:dark").unwrap();
        let raw = serde_json::to_string(&json).unwrap();
        let doc = ThemeJson::parse(&raw).unwrap();
        let tokens = extract_tokens(&doc).unwrap();
        assert_eq!(tokens.background, builtin_dark().background);
    }
}
