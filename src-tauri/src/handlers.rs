//! 唯一的 IPC 命令注册表。
//!
//! `main.rs` 与契约测试（`ipc_tests`）共用同一份列表：注册表少一条、名字拼错、
//! 命令签名与前端不匹配，都会在测试里立刻暴露（审计 P2-2）。

use crate::commands;

/// IPC 命令名清单 —— 与下方 `generate_handler!` 一一对应。
///
/// 契约测试用它检查"清单 ↔ 宏注册"不漂移（`generate_handler!` 无法反射，
/// 所以用 `include_str!` 交叉校验源码里确实出现了每个名字）。
/// 仅在契约测试中使用（release 构建不需要这份清单）。
#[cfg_attr(not(test), allow(dead_code))]
pub const IPC_COMMANDS: &[&str] = &[
    "open_external_url",
    "auth_microsoft_login",
    "auth_offline_login",
    "auth_logout",
    "auth_get_session",
    "auth_refresh_token",
    "auth_list_accounts",
    "auth_switch_account",
    "auth_remove_account",
    "download_cancel",
    "download_pause",
    "download_resume",
    "launch_start",
    "launch_kill",
    "launch_status",
    "log_get",
    "log_clear",
    "frontend_log",
    "modloader_list",
    "modloader_install",
    "mod_search",
    "mod_get_versions",
    "mod_get_download_url",
    "mod_list",
    "mod_toggle",
    "mod_install",
    "mod_remove",
    "mod_install_local",
    "mod_pick_and_install",
    // ── mods 依赖诊断（ADR-013 S3，纯逻辑）──
    "mods_dependency_scan",
    "mods_apply_plan",
    "profile_list",
    "profile_create",
    "profile_update",
    "profile_delete",
    "settings_get",
    "settings_set",
    "settings_get_user",
    "settings_set_user",
    "settings_supported_locales",
    "java_detect",
    "version_list",
    "version_install",
    "instance_list",
    "instance_refresh",
    "instance_rename",
    "instance_duplicate",
    "instance_remove",
    "instance_check_completion",
    "instance_repair",
    "instance_paths",
    "instance_resolved_settings",
    "plugin_list",
    "plugin_inspect",
    "plugin_install",
    "plugin_pick_and_install",
    "plugin_enable",
    "plugin_disable",
    "plugin_remove",
    "plugin_rollback",
    "plugin_grant",
    "plugin_revoke",
    "plugin_config_get",
    "plugin_config_set",
    "plugin_config_reset",
    "plugin_capabilities",
    "plugin_contributions",
    "plugin_ui_invoke",
    "plugin_logic_invoke",
    "plugin_logic_cancel",
    "dashboard_get_layout",
    "dashboard_save_layout",
    "dashboard_reset_layout",
    "dashboard_list_widgets",
    "dashboard_list_themes",
    "dashboard_get_theme_tokens",
    "dashboard_set_theme_preference",
    "dashboard_get_theme_preference",
    "dashboard_get_theme_json",
    // ── AI 服务（ADR-013 S1/S2）──
    "ai_config_get",
    "ai_config_set",
    "ai_models_probe",
    "ai_analyze_log",
    "ai_analyze_cancel",
    // ── MCP 服务器管理（ADR-013 S4/S5）──
    "mcp_status",
    "mcp_set_enabled",
    "mcp_regenerate_token",
    "mcp_tools_list",
    "mcp_tools_set",
    "mcp_sessions_disconnect",
    "mcp_confirmations_list",
    "mcp_confirmation_resolve",
    "mcp_recent_calls",
];

/// 全部 IPC 命令（顺序无关，但请与 `commands/` 的模块顺序保持可读）。
///
/// 新增命令时必须同时：
/// 1. 在这里注册；
/// 2. 在 `bindings.rs` 的 `types` 数组里登记其 DTO（若跨 IPC 传值）；
/// 3. 在 `ipc_tests.rs` 补一条契约测试。
pub fn invoke_handlers() -> impl Fn(tauri::ipc::Invoke<tauri::Wry>) -> bool + Send + Sync + 'static
{
    tauri::generate_handler![
        // ── external ──
        commands::external::open_external_url,
        // ── auth ──
        commands::auth::auth_microsoft_login,
        commands::auth::auth_offline_login,
        commands::auth::auth_logout,
        commands::auth::auth_get_session,
        commands::auth::auth_refresh_token,
        commands::auth::auth_list_accounts,
        commands::auth::auth_switch_account,
        commands::auth::auth_remove_account,
        // ── download ──
        commands::download::download_cancel,
        commands::download::download_pause,
        commands::download::download_resume,
        // ── launch ──
        commands::launch::launch_start,
        commands::launch::launch_kill,
        commands::launch::launch_status,
        // ── logs ──
        commands::logs::log_get,
        commands::logs::log_clear,
        // ── webview ──
        commands::ui::frontend_log,
        // ── AI 服务（ADR-013）──
        commands::ai::ai_config_get,
        commands::ai::ai_config_set,
        commands::ai::ai_models_probe,
        commands::ai::ai_analyze_log,
        commands::ai::ai_analyze_cancel,
        // ── modloader ──
        commands::modloader::modloader_list,
        commands::modloader::modloader_install,
        // ── mods ──
        commands::mods::mod_search,
        commands::mods::mod_get_versions,
        commands::mods::mod_get_download_url,
        commands::mods::mod_list,
        commands::mods::mod_toggle,
        commands::mods::mod_install,
        commands::mods::mod_remove,
        commands::mods::mod_install_local,
        commands::mods::mod_pick_and_install,
        // ── mods 依赖诊断（ADR-013 S3，纯逻辑）──
        commands::mods::mods_dependency_scan,
        commands::mods::mods_apply_plan,
        // ── profile ──
        commands::profile::profile_list,
        commands::profile::profile_create,
        commands::profile::profile_update,
        commands::profile::profile_delete,
        // ── settings ──
        commands::settings::settings_get,
        commands::settings::settings_set,
        commands::settings::settings_get_user,
        commands::settings::settings_set_user,
        commands::settings::settings_supported_locales,
        commands::settings::java_detect,
        // ── version ──
        commands::version::version_list,
        commands::version::version_install,
        // ── instance ──
        commands::instance::instance_list,
        commands::instance::instance_refresh,
        commands::instance::instance_rename,
        commands::instance::instance_duplicate,
        commands::instance::instance_remove,
        commands::instance::instance_check_completion,
        commands::instance::instance_repair,
        commands::instance::instance_paths,
        commands::instance::instance_resolved_settings,
        // ── plugin ──
        commands::plugin::plugin_list,
        commands::plugin::plugin_inspect,
        commands::plugin::plugin_install,
        commands::plugin::plugin_pick_and_install,
        commands::plugin::plugin_enable,
        commands::plugin::plugin_disable,
        commands::plugin::plugin_remove,
        commands::plugin::plugin_rollback,
        commands::plugin::plugin_grant,
        commands::plugin::plugin_revoke,
        commands::plugin::plugin_config_get,
        commands::plugin::plugin_config_set,
        commands::plugin::plugin_config_reset,
        commands::plugin::plugin_capabilities,
        commands::plugin::plugin_contributions,
        // ── isolated plugin UI (Phase 4, ADR-006) ──
        commands::plugin_ui::plugin_ui_invoke,
        // ── logic plugins (Phase 5, ADR-007) ──
        commands::plugin_logic::plugin_logic_invoke,
        commands::plugin_logic::plugin_logic_cancel,
        // ── dashboard (Phase 3) ──
        commands::dashboard::dashboard_get_layout,
        commands::dashboard::dashboard_save_layout,
        commands::dashboard::dashboard_reset_layout,
        commands::dashboard::dashboard_list_widgets,
        commands::dashboard::dashboard_list_themes,
        commands::dashboard::dashboard_get_theme_tokens,
        commands::dashboard::dashboard_set_theme_preference,
        commands::dashboard::dashboard_get_theme_preference,
        commands::dashboard::dashboard_get_theme_json,
        // ── MCP server management (ADR-013 S4/S5) ──
        commands::mcp::mcp_status,
        commands::mcp::mcp_set_enabled,
        commands::mcp::mcp_regenerate_token,
        commands::mcp::mcp_tools_list,
        commands::mcp::mcp_tools_set,
        commands::mcp::mcp_sessions_disconnect,
        commands::mcp::mcp_confirmations_list,
        commands::mcp::mcp_confirmation_resolve,
        commands::mcp::mcp_recent_calls,
    ]
}
