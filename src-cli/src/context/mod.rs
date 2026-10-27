//! CLI context — the single backend facade used by every command handler.
//!
//! Wraps [`rmccc_core::CoreContext`] (the DI container) and exposes one method
//! per core capability so that:
//! - CLI commands map 1:1 to backend capabilities (no business logic in the
//!   presentation layer),
//! - the Tauri command layer and the CLI share the exact same orchestration
//!   (both delegate to `rmccc_core::pipeline` / the DI providers),
//! - launch lifecycle (running games, exit signals) is observable and
//!   controllable for state-sync (`launch status` / `stop` / `wait`).
//!
//! All methods return `rmccc_error::Result<T>`; the dispatcher converts to
//! [`crate::error::CliError`] with a stable error code + exit code.

use rmccc_auth::Account;
use rmccc_core::pipeline::{self, GameOutputCb, ProgressCb};
use rmccc_core::CoreContext;
use rmccc_error::{Result, RmcccError};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// A currently running game process (for state observation).
#[derive(Debug, Clone)]
pub struct RunningGame {
    pub key: String,
    pub pid: u32,
}

/// Launch entry callback fired when the game process exits.
pub type CliExitCb = Arc<dyn Fn(i32, rmccc_launch::ExitType) + Send + Sync>;

/// Options accepted by [`CliContext::launch_start`].
#[derive(Default, Clone)]
pub struct LaunchStartOptions {
    /// Skip the pre-launch completeness check & auto-repair.
    pub no_repair: bool,
    /// Game stdout/stderr line callback.
    pub output: Option<GameOutputCb>,
    /// Pipeline progress callback.
    pub progress: Option<ProgressCb>,
    /// Callback invoked on process exit (event stream / status sync).
    pub on_exit: Option<CliExitCb>,
}

/// Unified backend API facade used by CLI commands.
#[derive(Clone)]
pub struct CliContext {
    pub core: Arc<tokio::sync::RwLock<CoreContext>>,
    pub http_client: reqwest::Client,
    pub data_dir: PathBuf,
    /// instance key → OS pid of a running game (state sync).
    pub running_games: Arc<dashmap::DashMap<String, u32>>,
    /// instance key → exit-code watch channel (allows `launch wait`).
    pub exit_signals: Arc<dashmap::DashMap<String, tokio::sync::watch::Receiver<Option<i32>>>>,
}

mod auth;
mod config;
mod helpers;
mod launch;
mod modloader;
mod mods;
mod plugin;
mod profile;
mod resource;
mod system;
mod version;

use helpers::*;

impl CliContext {
    pub fn new(data_dir: &Path) -> Result<Self> {
        std::fs::create_dir_all(data_dir)?;
        let core = CoreContext::new(data_dir)?;
        let http_client = core.http_client.clone();
        Ok(Self {
            core: Arc::new(tokio::sync::RwLock::new(core)),
            http_client,
            data_dir: data_dir.to_path_buf(),
            running_games: Arc::new(dashmap::DashMap::new()),
            exit_signals: Arc::new(dashmap::DashMap::new()),
        })
    }

    /// Load an instance detail or fail with `ProfileNotFound`.
    pub async fn get_instance(&self, key: &str) -> Result<rmccc_instance::InstanceDetail> {
        let core = self.core.read().await;
        let mut service = core
            .instances
            .lock()
            .map_err(|_| RmcccError::Other("instance service lock poisoned".to_string()))?;
        service.get(key)
    }
}
