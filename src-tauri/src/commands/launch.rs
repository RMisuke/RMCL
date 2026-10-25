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
use rmccc_error::IpcError;
use std::sync::Arc;
use tauri::Emitter;

/// Forwards game process events to the UI (HMCL `ProcessListener`).
///
/// `on_log` → `LAUNCH_OUTPUT`; `on_exit` → `LAUNCH_EXITED` (with exit type)
/// and removes the profile from `running_games`.
struct TauriProcessListener {
    app_handle: tauri::AppHandle,
    profile_id: String,
    running_games: Arc<dashmap::DashMap<String, u32>>,
}

impl rmccc_launch::ProcessListener for TauriProcessListener {
    fn on_log(&self, line: String, _is_stderr: bool) {
        let _ = self.app_handle.emit(
            crate::events::LAUNCH_OUTPUT,
            serde_json::json!({ "profile_id": self.profile_id, "text": line }),
        );
    }

    fn on_exit(&self, exit_code: i32, exit_type: rmccc_launch::ExitType) {
        self.running_games.remove(&self.profile_id);
        let _ = self.app_handle.emit(
            crate::events::LAUNCH_EXITED,
            serde_json::json!({
                "profile_id": self.profile_id,
                "exit_code": exit_code,
                "exit_type": format!("{:?}", exit_type),
            }),
        );
        tracing::info!(
            profile_id = %self.profile_id,
            exit_code = exit_code,
            exit_type = ?exit_type,
            "Game process exited"
        );
    }
}

#[tauri::command]
#[tracing::instrument(skip(state), fields(request_id = %uuid::Uuid::new_v4()))]
pub async fn launch_start(
    state: tauri::State<'_, AppState>,
    profile_id: String,
) -> Result<(), IpcError> {
    // ── Guard: prevent duplicate launch ──
    if state.running_games.contains_key(&profile_id) {
        return Err(IpcError {
            code: rmccc_error::ErrorCode::AlreadyRunning,
            message: format!("Profile '{profile_id}' is already running"),
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
        profile_id: profile_id.clone(),
        running_games: state.running_games.clone(),
    });

    // ── 解析 profile → 启动前修复 → 启动（编排全在 pipeline）──
    // A2：ensure_launch_ready 可能触发修复下载、launch_game 含 Java 探测与
    // 进程 spawn——handle 快照后立即释放读锁，全程不持 core 锁。
    let core = {
        let core = state.core.read().await;
        core.handle()
    };
    let pid = {
        let profile = core
            .profile_store
            .get(&profile_id)?
            .ok_or_else(|| IpcError {
                code: rmccc_error::ErrorCode::ProfileNotFound,
                message: format!("Profile not found: {profile_id}"),
                retryable: false,
            })?;

        // 启动前完整性检查与自动补齐（HMCL checkGameCompletionAsync）。
        let repaired = rmccc_core::pipeline::ensure_launch_ready(&core, &profile, None).await?;
        if !repaired.is_noop() {
            tracing::info!(
                profile_id = %profile_id,
                files = repaired.total_files,
                "Pre-launch resources repaired"
            );
        }

        rmccc_core::pipeline::launch_game(&core, &profile, listener, None)
            .await?
            .pid()
    };

    state.running_games.insert(profile_id.clone(), pid);
    let _ = state.app_handle.emit(
        crate::events::LAUNCH_STARTED,
        serde_json::json!({ "profile_id": profile_id, "pid": pid }),
    );

    Ok(())
}

#[tauri::command]
#[tracing::instrument(skip(state))]
pub async fn launch_kill(
    state: tauri::State<'_, AppState>,
    profile_id: String,
) -> Result<(), IpcError> {
    // 注意：dashmap 的 get() 读守卫绝不能活到 remove() —— 同分片写锁会被
    // 自己的读守卫挡住（自死锁），命令会永久挂起、表项删不掉（曾导致
    // "游戏已退出但界面仍显示运行中"）。先拷贝 pid 让守卫立即释放。
    let pid = state.running_games.get(&profile_id).map(|p| *p);
    if let Some(pid) = pid {
        tracing::info!(profile_id = %profile_id, pid = pid, "Killing game process");

        // 终止方式由平台后端决定（Unix 进程组 SIGKILL；Windows 待 Job Object）。
        // 本层不再出现 `#[cfg(unix)]` / `taskkill`（指南 §5.7，P2-17）。
        let core = state.core.read().await;
        if let Err(e) = core.platform.process.terminate_tree(pid) {
            // 终止失败也不能挂住表项：进程可能已自行退出（ESRCH 已按成功处理），
            // 其余错误记日志后仍移除，交给前端 launch_status 对账兜底。
            tracing::warn!(profile_id = %profile_id, pid = pid, error = ?e, "terminate_tree failed; removing entry anyway");
        }
        state.running_games.remove(&profile_id);
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
