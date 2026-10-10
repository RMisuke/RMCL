//! E2E 冒烟测试（P2-23）。
//!
//! 使用 `tauri-driver`（WebDriver server）驱动构建后的应用，覆盖
//! "应用启动 → 前端可访问"的 happy path 冒烟验证。
//!
//! # 运行前提
//! - 应用已构建（`pnpm tauri build --debug` 或 dev 模式）
//! - `tauri-driver` 在 `:4444` 运行（WebDriver server）
//!
//! CI 中建议在 macos/ubuntu job 后台启动 `tauri-driver` 后运行本测试。
//! 本地若无 GUI/WebDriver 环境，此测试会被跳过（不会 FAIL）。

use thirtyfour::prelude::*;

/// 从环境变量读取 tauri-driver WebDriver 地址（默认 localhost:4444）。
fn webdriver_url() -> String {
    std::env::var("TAURI_DRIVER_URL").unwrap_or_else(|_| "http://localhost:4444".into())
}

/// 从环境变量读取应用地址（dev: http://localhost:1420；release 用 tauri://localhost）。
fn app_url() -> String {
    std::env::var("RMCCC_APP_URL").unwrap_or_else(|_| "http://localhost:1420".into())
}

/// 应用启动后，前端 Dashboard 应渲染出登录入口或主界面。
///
/// 仅当 `RMCCC_RUN_E2E=1`（由专用 CI e2e job 设置）时实际运行；
/// 本地无 GUI/WebDriver 环境默认跳过，避免 `cargo test --workspace` 误失败。
#[tokio::test]
// thirtyfour 的 WebDriverError 较大，符合 clippy::result_large_err 误报场景
#[allow(clippy::result_large_err)]
async fn app_boots_and_serves_frontend() -> WebDriverResult<()> {
    // 未显式开启 E2E 则跳过（本地/普通 test job 兜底）。
    if std::env::var("RMCCC_RUN_E2E")
        .map(|v| v != "1")
        .unwrap_or(true)
    {
        eprintln!("RMCCC_RUN_E2E not set — skipping E2E");
        return Ok(());
    }

    let caps = DesiredCapabilities::chrome();
    let driver = WebDriver::new(webdriver_url(), caps).await?;
    driver.goto(app_url()).await?;

    // 首页应能渲染（body 非空）。前端路由为 / → Dashboard。
    let body_text = driver.find(By::Tag("body")).await?.text().await?;
    assert!(
        !body_text.trim().is_empty(),
        "Frontend body is empty — app did not render"
    );

    driver.quit().await?;
    Ok(())
}
