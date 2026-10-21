//! Dashboard layout/theme IPC commands (Phase 3, ADR-005).
//!
//! The backend owns layout normalization and theme-token validation; the
//! frontend only renders the normalized grid and applies CSS variables.

use crate::state::AppState;
use rmccc_core::rmccc_dashboard::{
    builtin_dark, builtin_light, builtin_list, extract_tokens, DashboardLayout,
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
    match key.as_str() {
        "builtin:dark" | "" => Ok(builtin_dark()),
        "builtin:light" => Ok(builtin_light()),
        _ => {
            let contributions = core.plugin_manager.contributions();
            let Some(view) = contributions.themes.iter().find(|t| t.key == key) else {
                // 未知主题回退到安全内置主题（ADR-005 决策 7）。
                tracing::warn!(%key, "unknown theme key, falling back to builtin dark");
                return Ok(builtin_dark());
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
