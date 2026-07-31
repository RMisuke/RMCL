//! Event name constants and throttled emitter for Rust → Frontend communication.
//!
//! Constants and types here are declared for future use in download progress,
//! auth device code flow, and mod loader installation progress events.
#![allow(dead_code)]

use serde::Serialize;
use std::sync::Mutex;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter};

// ── Event name constants ──

pub const DOWNLOAD_PROGRESS: &str = "download://progress";
pub const DOWNLOAD_COMPLETE: &str = "download://complete";
pub const DOWNLOAD_ERROR: &str = "download://error";
pub const DOWNLOAD_PAUSED: &str = "download://paused";
pub const DOWNLOAD_RESUMED: &str = "download://resumed";

pub const AUTH_STATE_CHANGED: &str = "auth://state-changed";
pub const AUTH_DEVICE_CODE: &str = "auth://device-code";

pub const LAUNCH_STARTED: &str = "launch://started";
pub const LAUNCH_OUTPUT: &str = "launch://output";
pub const LAUNCH_EXITED: &str = "launch://exited";

pub const MODLOADER_INSTALL_PROGRESS: &str = "modloader://progress";

// ── Download progress payload ──

#[derive(Debug, Clone, Serialize)]
pub struct DownloadProgressPayload {
    pub batch_id: String,
    pub completed_files: u64,
    pub total_files: u64,
    pub downloaded_bytes: u64,
    pub total_bytes: u64,
    pub current_file: Option<String>,
}

// ── Throttled emitter ──

/// Emits download progress events at a controlled rate to avoid flooding the frontend.
///
/// Events are throttled to at most one per `interval` (default 100ms).
/// Completion events (100%) are always emitted immediately.
pub struct ThrottledEmitter {
    app: AppHandle,
    last_emit: Mutex<Instant>,
    interval: Duration,
}

impl ThrottledEmitter {
    pub fn new(app: AppHandle, interval_ms: u64) -> Self {
        Self {
            app,
            last_emit: Mutex::new(Instant::now() - Duration::from_millis(interval_ms)),
            interval: Duration::from_millis(interval_ms),
        }
    }

    /// Emit a download progress event (throttled).
    pub fn emit_progress(&self, payload: &DownloadProgressPayload) {
        let mut last = self.last_emit.lock().unwrap();

        // Always emit 100% completion
        let is_complete = payload.completed_files >= payload.total_files
            && payload.total_files > 0;

        if is_complete || last.elapsed() >= self.interval {
            let _ = self.app.emit(DOWNLOAD_PROGRESS, payload);
            *last = Instant::now();
        }
    }

    /// Emit an event immediately (no throttling).
    pub fn emit_immediate<T: Serialize + Clone>(&self, event: &str, payload: &T) {
        let _ = self.app.emit(event, payload);
    }
}
