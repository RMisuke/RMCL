mod bindings;
mod commands;
mod events;
mod state;

use rmccc_config::logging::init_logging;
use rmccc_config::paths;
use state::AppState;
use tauri::Manager;
use tauri_plugin_window_state::StateFlags;

fn main() {
    // Ensure data directories exist
    paths::ensure_dirs().expect("Failed to create data directories");

    // Initialize logging
    let log_dir = paths::log_dir();
    init_logging(&log_dir).expect("Failed to initialize logging");

    // P3-13：崩溃上报（本地）——panic hook 将崩溃信息写入日志文件，便于排障。
    // 未引入 Sentry/OTel（体积红线 <10MB）；后续可在此扩展为远程上报。
    std::panic::set_hook(Box::new(|info| {
        let payload = info.payload().downcast_ref::<&str>().map(|s| s.to_string());
        tracing::error!(
            panic_location = ?info.location(),
            panic_payload = payload.unwrap_or_else(|| "unknown".into()),
            "PANIC — crash captured"
        );
        // 双写到 stderr 兜底（日志初始化失败时仍可见）
        eprintln!("PANIC: {:?}", info);
    }));

    tracing::info!("RMCCC starting...");

    // P2-15：启动时生成/刷新 src/ipc/types.ts（ts-rs 自动类型桥接）。
    // 生成失败仅告警不阻塞启动（types.ts 缺失时前端 typecheck 会暴露）。
    if let Err(e) = bindings::write_types_file() {
        tracing::warn!(error = ?e, "Failed to write TS bindings types.ts");
    }

    let app = tauri::Builder::default()
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_opener::init())
        // P2-25：启用自动更新（需 release CI 生成 latest.json + 签名密钥）
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(
            tauri_plugin_window_state::Builder::new()
                .with_state_flags(StateFlags::all() & !StateFlags::VISIBLE)
                .build(),
        )
        .setup(|app| {
            let data_dir = paths::default_data_dir();
            let app_handle = app.handle().clone();
            let app_state = AppState::new(&data_dir, app_handle)?;
            app.manage(app_state);

            // Ready-to-show: make window visible after setup
            if let Some(window) = app.get_webview_window("main") {
                window.show().ok();
                window.set_focus().ok();
            }

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::external::open_external_url,
            commands::auth::auth_microsoft_login,
            commands::auth::auth_offline_login,
            commands::auth::auth_logout,
            commands::auth::auth_get_session,
            commands::auth::auth_refresh_token,
            commands::auth::auth_list_accounts,
            commands::auth::auth_switch_account,
            commands::auth::auth_remove_account,
            commands::download::download_cancel,
            commands::download::download_pause,
            commands::download::download_resume,
            commands::launch::launch_start,
            commands::launch::launch_kill,
            commands::launch::launch_status,
            commands::logs::log_get,
            commands::logs::log_clear,
            commands::modloader::modloader_list,
            commands::modloader::modloader_install,
            commands::mods::mod_search,
            commands::mods::mod_get_versions,
            commands::mods::mod_get_download_url,
            commands::mods::mod_list,
            commands::mods::mod_toggle,
            commands::mods::mod_install,
            commands::mods::mod_remove,
            commands::profile::profile_list,
            commands::profile::profile_create,
            commands::profile::profile_update,
            commands::profile::profile_delete,
            commands::settings::settings_get,
            commands::settings::settings_set,
            commands::settings::java_detect,
            commands::version::version_list,
            commands::version::version_install,
            commands::instance::instance_list,
            commands::instance::instance_refresh,
            commands::instance::instance_rename,
            commands::instance::instance_duplicate,
            commands::instance::instance_remove,
            commands::instance::instance_check_completion,
            commands::instance::instance_repair,
        ])
        .build(tauri::generate_context!())
        .expect("Error while building RMCCC");

    // P4：进程退出前 flush FileSaver 队列，保证防抖写盘不丢最后修改。
    app.run(|app_handle, event| {
        if matches!(event, tauri::RunEvent::Exit) {
            if let Some(state) = app_handle.try_state::<AppState>() {
                if let Ok(core) = state.core.try_read() {
                    core.flush_pending_writes();
                }
            }
        }
    });
}
