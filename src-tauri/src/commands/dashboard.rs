//! Dashboard layout/theme IPC commands (Phase 3, ADR-005).
//!
//! The backend owns layout normalization and theme-token validation; the
//! frontend only renders the normalized grid and applies CSS variables.

use crate::state::AppState;
use rmccc_core::rmccc_dashboard::{
    builtin_dark, builtin_ember, builtin_forest, builtin_light, builtin_list, builtin_ocean,
    builtin_paper, builtin_sakura, builtin_sky, extract_tokens, DashboardLayout,
    NormalizationResult, ThemeInfo, ThemeJson, ThemeTokens, WidgetDescriptor, WidgetRegistry,
};
use rmccc_error::IpcError;
use serde_json::Value;

fn registry(core: &rmccc_core::CoreContext) -> WidgetRegistry {
    let cards = core.plugin_manager.contributions().dashboard_cards;
    WidgetRegistry::from_contributions(&cards)
}

#[tauri::command]
#[tracing::instrument(skip(state))]
pub async fn dashboard_get_layout(
    state: tauri::State<'_, AppState>,
    layout_id: Option<String>,
) -> Result<DashboardLayout, IpcError> {
    let core = state.core.read().await;
    let id = layout_id.unwrap_or_else(|| "default".to_string());
    let reg = registry(&core);
    let (layout, _) = core
        .dashboard
        .load_or_init_with(&id, &reg.constraint_map())?;
    Ok(layout)
}

#[tauri::command]
#[tracing::instrument(skip(state, layout))]
pub async fn dashboard_save_layout(
    state: tauri::State<'_, AppState>,
    layout: DashboardLayout,
) -> Result<NormalizationResult, IpcError> {
    let core = state.core.read().await;
    let reg = registry(&core);
    let mut layout = layout;
    Ok(core
        .dashboard
        .save_with(&mut layout, &reg.constraint_map())?)
}

#[tauri::command]
#[tracing::instrument(skip(state))]
pub async fn dashboard_reset_layout(
    state: tauri::State<'_, AppState>,
    layout_id: Option<String>,
) -> Result<DashboardLayout, IpcError> {
    let core = state.core.read().await;
    let id = layout_id.unwrap_or_else(|| "default".to_string());
    Ok(core.dashboard.reset(&id)?)
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
        themes.push(ThemeInfo {
            key: view.key,
            name: view.theme.name,
            is_builtin: false,
            mode: "custom".into(),
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

/// 把主题 key 解析为令牌（宿主唯一真相，Phase 3 命令与隔离 UI 桥共用）。
///
/// `key` 为空、`system` 或未知值时，按 `fallback_mode`（`light`/`dark`）回退到内置主题；
/// `fallback_mode` 来自 WebView 的 `prefers-color-scheme`，因为"跟随系统"只有渲染侧知道。
pub(crate) fn theme_tokens_for(
    core: &rmccc_core::CoreContext,
    key: &str,
    fallback_mode: &str,
) -> Result<ThemeTokens, IpcError> {
    match key {
        "builtin:dark" => Ok(builtin_dark()),
        "builtin:light" => Ok(builtin_light()),
        "builtin:forest" => Ok(builtin_forest()),
        "builtin:ocean" => Ok(builtin_ocean()),
        "builtin:ember" => Ok(builtin_ember()),
        "builtin:sakura" => Ok(builtin_sakura()),
        "builtin:paper" => Ok(builtin_paper()),
        "builtin:sky" => Ok(builtin_sky()),
        "" | "system" => Ok(if fallback_mode == "light" {
            builtin_light()
        } else {
            builtin_dark()
        }),
        _ => {
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
            let path = dir.join(&view.theme.path);
            let json = std::fs::read_to_string(&path)?;
            let theme: ThemeJson = serde_json::from_str(&json)?;
            Ok(extract_tokens(&theme)?)
        }
    }
}

#[tauri::command]
#[tracing::instrument(skip(state))]
pub async fn dashboard_set_theme_preference(
    state: tauri::State<'_, AppState>,
    theme: String,
) -> Result<rmccc_config::UserSettings, IpcError> {
    let mut core = state.core.write().await;
    let theme = theme.trim();
    core.user_settings.theme = if theme.is_empty() || theme == "system" || theme == "follow_system"
    {
        None
    } else {
        // `light` / `dark` / `builtin:...` / 插件主题 key 都按原样保存；
        // 解析失败时 `dashboard_get_theme_tokens` 会回退到安全主题。
        Some(theme.to_string())
    };

    let data_dir = core.config.resolved_data_dir();
    let path = rmccc_config::paths::user_settings_path(&data_dir);
    let content = serde_json::to_string_pretty(&core.user_settings)?;
    core.file_saver.save(path, content);
    Ok(core.user_settings.clone())
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
#[tauri::command]
#[tracing::instrument(skip(state))]
pub async fn dashboard_get_theme_json(
    state: tauri::State<'_, AppState>,
    key: String,
) -> Result<Value, IpcError> {
    let core = state.core.read().await;
    let contributions = core.plugin_manager.contributions();
    let Some(view) = contributions.themes.iter().find(|t| t.key == key) else {
        return Ok(Value::Null);
    };
    let Some(dir) = core.plugin_manager.installed_dir(&view.plugin_id) else {
        return Ok(Value::Null);
    };
    let path = dir.join(&view.theme.path);
    let json = std::fs::read_to_string(&path)?;
    Ok(serde_json::from_str(&json)?)
}
