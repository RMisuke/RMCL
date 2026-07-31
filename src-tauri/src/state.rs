//! Application state shared across all Tauri IPC handlers.
//!
//! Uses the same `CoreContext` DI container as the CLI, ensuring both
//! interfaces go through identical backend logic.

use std::sync::Arc;
use tokio::sync::RwLock;

/// Backend context — wraps CoreContext + Tauri-specific handles.
pub struct AppState {
    /// Core orchestrator (DI container with all providers).
    pub core: Arc<RwLock<rmccc_core::CoreContext>>,

    /// Shared HTTP client — cloned from CoreContext for convenient access
    /// in IPC commands without holding a read lock.
    pub http_client: reqwest::Client,

    /// Tauri app handle for event emission.
    pub app_handle: tauri::AppHandle,

    /// Token vault for auth session encryption/decryption.
    pub vault: Arc<tokio::sync::Mutex<rmccc_auth::TokenVault>>,

    /// Running game processes (profile_id → pid).
    pub running_games: Arc<dashmap::DashMap<String, u32>>,
}

impl AppState {
    pub fn new(
        data_dir: &std::path::Path,
        app_handle: tauri::AppHandle,
    ) -> rmccc_error::Result<Self> {
        let core = rmccc_core::CoreContext::new(data_dir)?;
        let http_client = core.http_client.clone();
        let vault = rmccc_auth::TokenVault::init(&data_dir.join(".key"))?;

        Ok(Self {
            core: Arc::new(RwLock::new(core)),
            http_client,
            app_handle,
            vault: Arc::new(tokio::sync::Mutex::new(vault)),
            running_games: Arc::new(dashmap::DashMap::new()),
        })
    }
}
