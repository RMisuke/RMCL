//! Launch IPC commands.
//!
//! Thin Tauri adapter over [`rmccc_core::pipeline::launch_game`] (P1-3). All
//! orchestration — local manifest resolution, Java detection, auth resolution,
//! command-line generation, native extraction, log4j, spawning and monitoring —
//! lives in the core pipeline so the CLI and the GUI cannot drift apart.
//!
//! What remains here is genuinely Tauri/IPC specific:
//! - duplicate / concurrent launch guards (UI state),
//! - a [`rmccc_launch::ProcessListener`] forwarding game events to the frontend,
//! - process group termination for `launch_kill`.

use crate::state::AppState;
use rmccc_core::pipeline::{ProgressCb, ProgressMsg};
use rmccc_error::IpcError;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use tauri::Emitter;

/// 每实例保留的输出行上限（与进程内存缓冲同量级，崩溃分析只需尾部）。
const GAME_OUTPUT_CAP: usize = 2000;

/// Log-line coalescing for the UI (perf audit 2.1): modded launches emit
/// hundreds of stdout lines per second; one IPC event per line serialized
/// the whole front-end. Lines flush when either the batch fills or the
/// window since the previous flush elapsed — the tail never lags more than
/// one line behind at low rates, and emits are capped at ~10/s at high rates.
const LOG_FLUSH_LINES: usize = 50;
const LOG_FLUSH_WINDOW_MS: u64 = 100;

/// Send one launch progress snapshot. Percentages are clamped to 0..=100 and
/// the frontend additionally enforces monotonicity.
fn emit_launch_progress(
    app_handle: &tauri::AppHandle,
    profile_id: &str,
    percent: u8,
    stage: &str,
    message: impl Into<String>,
    window_opened: bool,
) {
    let _ = app_handle.emit(
        crate::events::LAUNCH_PROGRESS,
        crate::events::LaunchProgressPayload {
            profile_id: profile_id.to_string(),
            percent: percent.min(100),
            stage: stage.to_string(),
            message: message.into(),
            window_opened,
        },
    );
}

/// Markers that the render thread has created the game window. Minecraft
/// logs `Backend library: LWJGL` once the render backend is initialised;
/// `OpenAL initialized` / `Sound engine started` are later fallbacks for
/// versions whose log format differs. Plain process spawn (`LAUNCH_STARTED`)
/// must never count as window-ready.
fn is_window_ready_log(line: &str) -> bool {
    const MARKERS: &[&str] = &[
        "Backend library: LWJGL",
        "OpenAL initialized",
        "Sound engine started",
    ];
    MARKERS.iter().any(|marker| line.contains(marker))
}

/// Forwards game process events to the UI (HMCL `ProcessListener`).
///
/// `on_log` → batched `LAUNCH_OUTPUT` (`{ profile_id, lines }`); `on_exit` →
/// `LAUNCH_EXITED` (with exit type) and removes the profile from
/// `running_games`.
struct TauriProcessListener {
    app_handle: tauri::AppHandle,
    key: String,
    running_games: Arc<dashmap::DashMap<String, u32>>,
    /// 每实例输出环形缓冲（AI 日志分析 / MCP game_logs 的 latest.log 回退源）。
    game_output: Arc<dashmap::DashMap<String, Arc<std::sync::Mutex<Vec<String>>>>>,
    /// Lines buffered for the next `LAUNCH_OUTPUT` emit.
    pending: std::sync::Mutex<Vec<String>>,
    /// Epoch for `last_flush_ms`.
    created: std::time::Instant,
    /// Milliseconds since `created` of the last flush (atomic: sync trait).
    last_flush_ms: AtomicU64,
    /// Set once the game window readiness marker is observed.
    window_opened: AtomicBool,
}

impl TauriProcessListener {
    fn emit_lines(&self, lines: Vec<String>) {
        let _ = self.app_handle.emit(
            crate::events::LAUNCH_OUTPUT,
            serde_json::json!({ "key": self.key, "profile_id": self.key, "lines": lines }),
        );
    }

    fn flush_pending(&self) {
        let lines = {
            let mut pending = self.pending.lock().unwrap_or_else(|p| p.into_inner());
            std::mem::take(&mut *pending)
        };
        if !lines.is_empty() {
            self.last_flush_ms
                .store(self.created.elapsed().as_millis() as u64, Ordering::Relaxed);
            self.emit_lines(lines);
        }
    }
}

impl rmccc_launch::ProcessListener for TauriProcessListener {
    fn on_log(&self, line: String, _is_stderr: bool) {
        if !self.window_opened.load(Ordering::Relaxed) && is_window_ready_log(&line) {
            self.window_opened.store(true, Ordering::Relaxed);
            emit_launch_progress(
                &self.app_handle,
                &self.key,
                100,
                "window-open",
                "游戏窗口已打开",
                true,
            );
            tracing::info!(key = %self.key, "Game window is ready");
        }
        if let Some(buf) = self.game_output.get(&self.key) {
            let mut lines = buf.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
            lines.push(line.clone());
            let excess = lines.len().saturating_sub(GAME_OUTPUT_CAP);
            if excess > 0 {
                lines.drain(..excess);
            }
        }
        let now_ms = self.created.elapsed().as_millis() as u64;
        let flush = {
            let mut pending = self.pending.lock().unwrap_or_else(|p| p.into_inner());
            pending.push(line);
            pending.len() >= LOG_FLUSH_LINES
                || now_ms.saturating_sub(self.last_flush_ms.load(Ordering::Relaxed))
                    >= LOG_FLUSH_WINDOW_MS
        };
        if flush {
            self.flush_pending();
        }
    }

    fn on_exit(&self, exit_code: i32, exit_type: rmccc_launch::ExitType) {
        self.running_games.remove(&self.key);
        // Emit whatever the coalescer still holds so the UI sees the final
        // lines before the exit banner.
        self.flush_pending();
        let _ = self.app_handle.emit(
            crate::events::LAUNCH_EXITED,
            serde_json::json!({
                "key": self.key,
                "profile_id": self.key,
                "exit_code": exit_code,
                "exit_type": format!("{:?}", exit_type),
            }),
        );
        tracing::info!(
            key = %self.key,
            exit_code = exit_code,
            exit_type = ?exit_type,
            "Game process exited"
        );
    }
}

#[tauri::command]
#[tracing::instrument(skip(state), fields(request_id = %uuid::Uuid::new_v4()))]
pub async fn launch_start(state: tauri::State<'_, AppState>, key: String) -> Result<(), IpcError> {
    // ── Guard: prevent duplicate launch ──
    if state.running_games.contains_key(&key) {
        return Err(IpcError {
            code: rmccc_error::ErrorCode::AlreadyRunning,
            message: format!("Instance '{key}' is already running"),
            retryable: false,
        });
    }

    // ── Guard: concurrent launch limit ──
    const MAX_CONCURRENT_GAMES: usize = 3;
    if state.running_games.len() >= MAX_CONCURRENT_GAMES {
        return Err(IpcError {
            code: rmccc_error::ErrorCode::ValidationError,
            message: format!(
                "Already running {} game(s). Maximum {} concurrent launches allowed.",
                state.running_games.len(),
                MAX_CONCURRENT_GAMES
            ),
            retryable: false,
        });
    }

    let listener: Arc<dyn rmccc_launch::ProcessListener> = Arc::new(TauriProcessListener {
        app_handle: state.app_handle.clone(),
        key: key.clone(),
        running_games: state.running_games.clone(),
        game_output: state.game_output.clone(),
        pending: std::sync::Mutex::new(Vec::new()),
        created: std::time::Instant::now(),
        last_flush_ms: AtomicU64::new(0),
        window_opened: AtomicBool::new(false),
    });

    // 启动进度：修复阶段来自真实下载回调，Java/进程阶段来自编排边界；
    // 100% 只由 TauriProcessListener 的窗口就绪日志触发。
    let progress_key = key.clone();
    let progress_app = state.app_handle.clone();
    let progress: ProgressCb = Arc::new(move |msg| match msg {
        ProgressMsg::Stage(message) => {
            let (percent, stage) = if message.contains("🚀") {
                (55, "spawning")
            } else {
                (8, "repairing")
            };
            emit_launch_progress(
                &progress_app,
                &progress_key,
                percent,
                stage,
                message.clone(),
                false,
            );
        }
        ProgressMsg::Download { progress, .. } => {
            let total = if progress.total_bytes > 0 {
                progress.total_bytes
            } else {
                progress.total_files.max(1)
            };
            let done = if progress.total_bytes > 0 {
                progress.downloaded_bytes
            } else {
                progress.completed_files
            };
            let ratio = if total > 0 {
                (done as f64 / total as f64).clamp(0.0, 1.0)
            } else {
                0.0
            };
            let percent = 8 + (ratio * 26.0) as u8;
            emit_launch_progress(
                &progress_app,
                &progress_key,
                percent,
                "repairing",
                format!(
                    "补齐游戏文件 · {}/{} 个文件",
                    progress.completed_files, progress.total_files
                ),
                false,
            );
        }
        ProgressMsg::StageDone(message) => {
            let (percent, stage) = if message.contains("游戏已启动") {
                (84, "spawning")
            } else {
                (34, "checking")
            };
            emit_launch_progress(
                &progress_app,
                &progress_key,
                percent,
                stage,
                message.clone(),
                false,
            );
        }
    });

    // 新一轮启动：清掉上一轮的捕获输出（进程退出后保留供事后分析）。
    // 必须在 spawn 之前建好，on_log 才有地方落行。
    let output_buf = state
        .game_output
        .entry(key.clone())
        .or_insert_with(|| Arc::new(std::sync::Mutex::new(Vec::new())));
    output_buf
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .clear();
    drop(output_buf);

    // ── 解析实例 → 启动前修复 → 启动（编排全在 pipeline）──
    // A2：ensure_launch_ready 可能触发修复下载、launch_game 含 Java 探测与
    // 进程 spawn——handle 快照后立即释放读锁，全程不持 core 锁。
    let core = {
        let core = state.core.read().await;
        core.handle()
    };
    let pid = {
        let instance = {
            let mut service = core.instances.lock().map_err(|_| IpcError {
                code: rmccc_error::ErrorCode::InternalError,
                message: "Instance service lock poisoned".into(),
                retryable: false,
            })?;
            service.get(&key)?
        };

        emit_launch_progress(
            &state.app_handle,
            &key,
            2,
            "checking",
            "检查游戏文件",
            false,
        );
        // 启动前完整性检查与自动补齐（HMCL checkGameCompletionAsync）。
        let repaired =
            rmccc_core::pipeline::ensure_launch_ready(&core, &instance, Some(&progress)).await?;
        if !repaired.is_noop() {
            tracing::info!(
                key = %key,
                files = repaired.total_files,
                "Pre-launch resources repaired"
            );
        }

        emit_launch_progress(
            &state.app_handle,
            &key,
            36,
            "checking-java",
            "检测 Java 与启动参数",
            false,
        );
        rmccc_core::pipeline::launch_game(&core, &instance, listener, Some(&progress))
            .await?
            .pid()
    };

    state.running_games.insert(key.clone(), pid);
    emit_launch_progress(
        &state.app_handle,
        &key,
        88,
        "waiting-window",
        "游戏进程已启动，等待游戏窗口",
        false,
    );
    let _ = state.app_handle.emit(
        crate::events::LAUNCH_STARTED,
        serde_json::json!({ "key": key, "profile_id": key, "pid": pid }),
    );

    Ok(())
}

#[tauri::command]
#[tracing::instrument(skip(state))]
pub async fn launch_kill(state: tauri::State<'_, AppState>, key: String) -> Result<(), IpcError> {
    // 注意：dashmap 的 get() 读守卫绝不能活到 remove() —— 同分片写锁会被
    // 自己的读守卫挡住（自死锁），命令会永久挂起、表项删不掉（曾导致
    // "游戏已退出但界面仍显示运行中"）。先拷贝 pid 让守卫立即释放。
    let pid = state.running_games.get(&key).map(|p| *p);
    if let Some(pid) = pid {
        tracing::info!(key = %key, pid = pid, "Killing game process");

        // 终止方式由平台后端决定（Unix 进程组 SIGKILL；Windows 待 Job Object）。
        // 本层不再出现 `#[cfg(unix)]` / `taskkill`（指南 §5.7，P2-17）。
        let core = state.core.read().await;
        if let Err(e) = core.platform.process.terminate_tree(pid) {
            // 终止失败也不能挂住表项：进程可能已自行退出（ESRCH 已按成功处理），
            // 其余错误记日志后仍移除，交给前端 launch_status 对账兜底。
            tracing::warn!(key = %key, pid = pid, error = ?e, "terminate_tree failed; removing entry anyway");
        }
        state.running_games.remove(&key);
    }
    Ok(())
}

#[tauri::command]
#[tracing::instrument(skip(state))]
pub async fn launch_status(state: tauri::State<'_, AppState>) -> Result<Vec<String>, IpcError> {
    let keys: Vec<String> = state
        .running_games
        .iter()
        .map(|r| r.key().clone())
        .collect();
    Ok(keys)
}

#[cfg(test)]
mod tests {
    use super::is_window_ready_log;

    #[test]
    fn window_ready_markers_are_specific() {
        assert!(is_window_ready_log(
            "[Render thread/INFO]: Backend library: LWJGL version 3.3.3+5"
        ));
        assert!(is_window_ready_log(
            "[Render thread/INFO]: OpenAL initialized on device Built-in Audio"
        ));
        assert!(is_window_ready_log(
            "[Render thread/INFO]: Sound engine started"
        ));
        assert!(!is_window_ready_log("[main/INFO]: Setting user: Steve"));
        assert!(!is_window_ready_log(
            "[main/INFO]: Launching target 'fabricclient'"
        ));
    }
}
