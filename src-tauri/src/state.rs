//! Application state shared across all Tauri IPC handlers.
//!
//! Uses the same `CoreContext` DI container as the CLI, ensuring both
//! interfaces go through identical backend logic. Credentials (vault) and the
//! account repository are owned by `CoreContext`; IPC handlers never touch
//! encryption or storage paths directly.

use std::sync::Arc;
use tokio::sync::RwLock;

/// Backend context — wraps CoreContext + Tauri-specific handles.
pub struct AppState {
    /// Core orchestrator (DI container with all providers and the account repo).
    pub core: Arc<RwLock<rmccc_core::CoreContext>>,

    /// Shared HTTP client — cloned from CoreContext for convenient access
    /// in IPC commands without holding a read lock.
    pub http_client: reqwest::Client,

    /// HTTP client for plugin `network.request` (ADR-006): redirects disabled so a
    /// 30x cannot escape the declared host allowlist, short timeout, no cookies.
    pub ui_http_client: reqwest::Client,

    /// Tauri app handle for event emission.
    pub app_handle: tauri::AppHandle,

    /// Running game processes (profile_id → pid).
    pub running_games: Arc<dashmap::DashMap<String, u32>>,

    /// Logic plugin helpers (ADR-007). Blocking handles live behind a std mutex
    /// because they own child processes and pipes, and every call runs on a
    /// blocking thread.
    pub logic: Arc<std::sync::Mutex<rmccc_logic::LogicRunner>>,

    /// 进行中的 AI 日志分析（analysis_id → 取消令牌，ADR-013 S2）。
    /// 并发上限见 `commands/ai.rs`；任务结束自摘除。
    pub ai_cancels:
        Arc<std::sync::Mutex<std::collections::HashMap<String, rmccc_llm::CancelToken>>>,

    /// MCP 服务器句柄（ADR-013 S4/S5）。默认关闭，`mcp_set_enabled` 启停。
    pub mcp: Arc<crate::mcp::McpHandle>,

    /// 插件桥 `llm.chat` 的限流状态（S6：每插件 6 次/分钟）。
    pub llm_rate: Arc<
        std::sync::Mutex<
            std::collections::HashMap<String, std::collections::VecDeque<std::time::Instant>>,
        >,
    >,
}

impl AppState {
    pub fn new(
        data_dir: &std::path::Path,
        app_handle: tauri::AppHandle,
    ) -> rmccc_error::Result<Self> {
        let core = rmccc_core::CoreContext::new(data_dir)?;
        let http_client = core.http_client.clone();
        let ui_http_client = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .timeout(std::time::Duration::from_secs(30))
            .user_agent(concat!("RMCCC/", env!("CARGO_PKG_VERSION"), " (plugin-ui)"))
            .build()
            .unwrap_or_else(|_| reqwest::Client::new());

        let mcp = Arc::new(crate::mcp::McpHandle::new(
            data_dir,
            core.platform.fs.clone(),
        )?);
        mcp.set_app(app_handle.clone());

        Ok(Self {
            core: Arc::new(RwLock::new(core)),
            http_client,
            ui_http_client,
            app_handle,
            running_games: Arc::new(dashmap::DashMap::new()),
            logic: Arc::new(std::sync::Mutex::new(rmccc_logic::LogicRunner::new())),
            ai_cancels: Arc::new(std::sync::Mutex::new(std::collections::HashMap::new())),
            mcp,
            llm_rate: Arc::new(std::sync::Mutex::new(std::collections::HashMap::new())),
        })
    }
}
