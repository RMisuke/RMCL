#![allow(dead_code, unused_imports)]

mod commands;
mod events;
mod state;

use rmccc_config::logging::init_logging;
use rmccc_config::paths;
use state::AppState;
use std::sync::Arc;
use tauri::Manager;
use tauri_plugin_window_state::StateFlags;

fn main() {
    // Ensure data directories exist
    paths::ensure_dirs().expect("Failed to create data directories");

    // Initialize logging
    let log_dir = paths::log_dir();
    init_logging(&log_dir).expect("Failed to initialize logging");

    tracing::info!("RMCCC starting...");

    tauri::Builder::default()
        .plugin(tauri_plugin_process::init())
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
            commands::auth::auth_microsoft_login,
            commands::auth::auth_offline_login,
            commands::auth::auth_logout,
            commands::auth::auth_get_session,
            commands::auth::auth_refresh_token,
            commands::profile::profile_list,
            commands::profile::profile_create,
            commands::profile::profile_update,
            commands::profile::profile_delete,
            commands::version::version_list,
            commands::version::version_install,
            commands::launch::launch_start,
            commands::launch::launch_kill,
            commands::launch::launch_status,
            commands::settings::settings_get,
            commands::settings::settings_set,
            commands::settings::java_detect,
            commands::modloader::modloader_list,
            commands::modloader::modloader_install,
        ])
        .run(tauri::generate_context!())
        .expect("Error while running RMCCC");
}
