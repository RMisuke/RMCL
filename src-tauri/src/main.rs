mod bindings;
mod commands;
mod events;
mod handlers;
#[cfg(test)]
mod ipc_tests;
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
    // Logging must not take the whole app down: an unavailable log directory
    // (sandbox / read-only mount / permission issue) degrades to stderr only.
    if let Err(e) = init_logging(&log_dir) {
        eprintln!("Warning: failed to initialize logging: {e}");
    }

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
        .plugin(tauri_plugin_dialog::init())
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
        .invoke_handler(crate::handlers::invoke_handlers())
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
