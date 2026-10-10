//! Event name constants and throttled emitter for Rust → Frontend communication.
//!
//! Constants and types here are declared for future use in download progress,
//! auth device code flow, and mod loader installation progress events.
//! P2-24：移除 crate 级 allow(dead_code)，仅对预留常量保留单项 allow。

use serde::Serialize;
use std::sync::Mutex;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter};

// ── Event name constants ──

pub const DOWNLOAD_PROGRESS: &str = "download://progress";
pub const DOWNLOAD_COMPLETE: &str = "download://complete";
pub const DOWNLOAD_ERROR: &str = "download://error";
#[allow(dead_code)] // 预留：前端可据此展示暂停/恢复状态（P2-24 单项豁免）
pub const DOWNLOAD_PAUSED: &str = "download://paused";
#[allow(dead_code)]
pub const DOWNLOAD_RESUMED: &str = "download://resumed";

#[allow(dead_code)] // 预留：多账户/会话状态变更广播
pub const AUTH_STATE_CHANGED: &str = "auth://state-changed";
pub const AUTH_DEVICE_CODE: &str = "auth://device-code";

pub const LAUNCH_STARTED: &str = "launch://started";
pub const LAUNCH_OUTPUT: &str = "launch://output";
pub const LAUNCH_EXITED: &str = "launch://exited";

pub const MODLOADER_INSTALL_PROGRESS: &str = "modloader://progress";

/// Emitted after the game repository re-scans `versions/`
/// (HMCL `RefreshedGameInstancesEvent`).
pub const INSTANCES_REFRESHED: &str = "instances://refreshed";

// ── Download progress payload ──

#[derive(Debug, Clone, Serialize, ts_rs::TS)]
#[ts(export_to = "types.ts")]
pub struct DownloadProgressPayload {
    pub batch_id: String,
    #[ts(type = "number")]
    pub completed_files: u64,
    #[ts(type = "number")]
    pub total_files: u64,
    #[ts(type = "number")]
    pub downloaded_bytes: u64,
    #[ts(type = "number")]
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
        // 容忍 poisoned mutex（P1-07：禁 unwrap_used），退化为立即发射
        let mut last = match self.last_emit.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        };

        // Always emit 100% completion
        let is_complete = payload.completed_files >= payload.total_files && payload.total_files > 0;

        if is_complete || last.elapsed() >= self.interval {
            let _ = self.app.emit(DOWNLOAD_PROGRESS, payload);
            *last = Instant::now();
        }
    }

    /// Emit an event immediately (no throttling).
    #[allow(dead_code)] // 预留：登录完成/关键节点即时广播
    pub fn emit_immediate<T: Serialize + Clone>(&self, event: &str, payload: &T) {
        let _ = self.app.emit(event, payload);
    }
}
