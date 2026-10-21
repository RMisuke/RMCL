mod bindings;
mod bundle;
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

/// WebView self-check used to diagnose "window opens but stays blank" reports.
///
/// The release build has no Web Inspector and WKWebView's console is invisible
/// from the outside, so a blank window is indistinguishable from a WebView that
/// never loaded the bundle. Running the binary with `RMCCC_WEBVIEW_PROBE=1`
/// logs the URL the WebView actually loaded plus what the bundle mounted, and
/// paints the same verdict into the page so a screenshot is enough.
///
/// Gated by an environment variable: a normal launch does nothing.
fn spawn_webview_probe(window: tauri::WebviewWindow) {
    use std::time::Duration;

    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(Duration::from_secs(4)).await;

        // Runs after the bundle had every chance to mount. Reports the loaded
        // document, not the configured one: a failed asset protocol shows up as
        // an empty/again-loaded page with no `#root` children.
        let js = r##"
            (function () {
              function info() {
                var root = document.getElementById("root");
                return [
                  "href=" + location.href,
                  "readyState=" + document.readyState,
                  "rootChildren=" + (root ? root.childElementCount : "no-root"),
                  "appRoot=" + !!document.querySelector(".app-root"),
                  "scripts=" + [].map.call(document.scripts, function (s) {
                    return s.src || "inline";
                  }).join(" , "),
                  "sheets=" + document.styleSheets.length
                ].join("\n");
              }
              var pre = document.createElement("pre");
              pre.textContent = "WEBVIEW PROBE\n" + info();
              pre.style.cssText =
                "position:fixed;inset:0;z-index:2147483647;margin:0;padding:12px;" +
                "background:#fff;color:#000;font:12px/1.4 ui-monospace,Menlo,monospace;" +
                "white-space:pre-wrap;overflow:auto;";
              (document.body || document.documentElement).appendChild(pre);
              document.title = "PROBE " + pre.textContent.replace(/\n/g, " | ");
            })();
        "##;

        if let Err(e) = window.eval(js) {
            tracing::warn!(error = %e, "WebView probe: eval failed (webview not alive)");
            return;
        }

        tokio::time::sleep(Duration::from_millis(1200)).await;
        tracing::info!(
            url = %window.url().map(|u| u.to_string()).unwrap_or_default(),
            title = %window.title().unwrap_or_default(),
            "WebView probe"
        );
    });
}

/// Show a bundle-failure page instead of leaving a transparent window behind.
///
/// Navigates to a self-contained `data:` URL: `eval` is not an option here,
/// because when `index.html` itself was never embedded the WebView has no
/// script context to evaluate in.
fn report_broken_bundle(window: tauri::WebviewWindow, report: &bundle::BundleReport) {
    let url = bundle::error_page_url(report);
    match tauri::Url::parse(&url) {
        Ok(url) => {
            if let Err(e) = window.navigate(url) {
                tracing::error!(error = %e, "Could not show the broken-bundle page");
            }
        }
        Err(e) => tracing::error!(error = %e, "Could not encode the broken-bundle page"),
    }
}

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

                // A bundle embedded from a stale `dist/` renders nothing at all,
                // so say so instead of leaving a transparent window behind.
                if let Some(report) = bundle::inspect(app) {
                    if report.is_ok() {
                        tracing::debug!("{}", report.describe());
                    } else {
                        tracing::error!(
                            missing = ?report.missing,
                            index_present = report.index_present,
                            "Embedded frontend bundle is incomplete; rebuilding the app is required"
                        );
                        report_broken_bundle(window.clone(), &report);
                    }
                }

                if std::env::var("RMCCC_WEBVIEW_PROBE").is_ok() {
                    spawn_webview_probe(window);
                }
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
