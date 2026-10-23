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

    /// 当前 Agent 回合的取消令牌（Phase 6.5）。同一时刻最多一个回合在跑。
    pub agent_cancel: Arc<std::sync::Mutex<Option<rmccc_agent::CancelToken>>>,
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

        Ok(Self {
            core: Arc::new(RwLock::new(core)),
            http_client,
            ui_http_client,
            app_handle,
            running_games: Arc::new(dashmap::DashMap::new()),
            logic: Arc::new(std::sync::Mutex::new(rmccc_logic::LogicRunner::new())),
            agent_cancel: Arc::new(std::sync::Mutex::new(None)),
        })
    }
}
