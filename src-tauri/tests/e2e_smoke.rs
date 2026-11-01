//! E2E 冒烟测试（P2-23）。
//!
//! 使用 `tauri-driver`（WebDriver server）驱动**构建后的应用二进制**，覆盖
//! "应用启动 → 前端可访问"的 happy path 冒烟验证。
//!
//! # 运行前提
//! - 应用已构建（`pnpm tauri build --debug --no-bundle`），二进制路径由
//!   `RMCCC_E2E_BINARY` 给出
//! - `tauri-driver` 在 `:4444` 运行（WebDriver server）
//!
//! # 平台
//! `tauri-driver` 只支持 Windows（自带 Edge WebDriver）与 Linux
//! （WebKitWebDriver）。macOS **不被支持**，因此 CI 的门禁跑在
//! `windows-latest`（2026-11-01 Windows 审计后从 macos-14 迁走）。
//!
//! 本地没有 WebDriver 环境时测试自动跳过（不会 FAIL）。

use std::time::Duration;
use thirtyfour::prelude::*;

/// 从环境变量读取 tauri-driver WebDriver 地址（默认 localhost:4444）。
fn webdriver_url() -> String {
    std::env::var("TAURI_DRIVER_URL").unwrap_or_else(|_| "http://localhost:4444".into())
}

/// 待测应用的绝对路径（tauri-driver 需要它来启动应用）。
fn app_binary() -> Option<String> {
    std::env::var("RMCCC_E2E_BINARY")
        .ok()
        .map(|path| path.trim().to_string())
        .filter(|path| !path.is_empty())
}

/// 应用启动后，前端主界面应渲染出内容（body 非空）。
///
/// 仅当 `RMCCC_RUN_E2E=1`（由专用 CI e2e job 设置）时实际运行；本地无
/// GUI/WebDriver 环境默认跳过，避免 `cargo test --workspace` 误失败。
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

    let application = app_binary().expect(
        "RMCCC_E2E_BINARY must point at the built app binary \
         (e.g. target/debug/rmccc.exe)",
    );

    // tauri-driver 通过 `tauri:options.application` 启动被测应用；会话建立后
    // 就已经挂在应用窗口上，不需要（也不能）再 `goto` 一个外部 URL。
    let mut caps = DesiredCapabilities::chrome();
    // thirtyfour 0.32 没有 `insert`：自定义（vendor）能力走
    // `CapabilitiesHelper::set_base_capability`。
    caps.set_base_capability(
        "tauri:options",
        serde_json::json!({ "application": application }),
    )?;

    let driver = WebDriver::new(webdriver_url(), caps).await?;

    // 窗口先隐藏、等前端初始化后才 show，所以给渲染留出有界重试窗口。
    let mut body = String::new();
    for _ in 0..40 {
        if let Ok(element) = driver.find(By::Tag("body")).await {
            if let Ok(text) = element.text().await {
                if !text.trim().is_empty() {
                    body = text;
                    break;
                }
            }
        }
        tokio::time::sleep(Duration::from_millis(250)).await;
    }

    assert!(
        !body.trim().is_empty(),
        "Frontend body is empty — app did not render"
    );

    driver.quit().await?;
    Ok(())
}
