//! Launch IPC commands.
//!
//! Orchestration only — the actual launch pipeline (command-line generation,
//! native extraction, log4j, process spawning and monitoring) lives in
//! [`rmccc_launch::DefaultLauncher`] (HMCL `DefaultLauncher`). This module:
//! - resolves the profile + local launch manifest,
//! - runs the pre-launch completeness check & auto-repair (HMCL
//!   `checkGameCompletionAsync`),
//! - builds a [`rmccc_launch::ProcessListener`] forwarding game events to the UI,
//! - guards against duplicate / excessive concurrent launches.

use crate::state::AppState;
use rmccc_auth::Account;
use rmccc_error::IpcError;
use rmccc_launch::{find_java, LaunchOptions};
use rmccc_version::GameRepository as _;
use rmccc_version::GameVersion;
use std::path::PathBuf;
use std::sync::Arc;
use tauri::Emitter;

/// Result of the profile + version resolution phase.
struct ResolvedLaunch {
    profile: rmccc_profile::Profile,
    game_version: GameVersion,
    required_java: u32,
}

/// Auth credentials extracted for the game launch.
struct LaunchAuth {
    username: String,
    uuid: String,
    access_token: String,
    user_type: String,
}

/// Phase 1: Load profile from store and resolve game version metadata.
///
/// Resolves **locally** from the [`GameRepository`] (offline-capable) instead of
/// the network `fetch_version` — the patched / inherited manifest written to
/// disk is the source of truth for launching.
async fn resolve_launch_profile(
    state: &AppState,
    profile_id: &str,
) -> Result<ResolvedLaunch, IpcError> {
    let core = state.core.read().await;
    let profile = core
        .profile_store
        .get(profile_id)?
        .ok_or_else(|| IpcError {
            code: rmccc_error::ErrorCode::ProfileNotFound,
            message: format!("Profile not found: {profile_id}"),
            retryable: false,
        })?;

    // 从本地仓库解析（启动数据源 = 磁盘，而非网络）
    let resolved = {
        let repo = core
            .game_repository
            .read()
            .map_err(|_| rmccc_error::RmcccError::Other("game repository lock poisoned".into()))?;
        repo.get_resolved(&profile.game_version)
            .map_err(|e| rmccc_error::RmcccError::Other(e.to_string()))?
    };
    let game_version = resolved.launch;

    let required_java = game_version
        .java_version
        .as_ref()
        .map(|j| j.major_version)
        .unwrap_or_else(|| rmccc_version::java_version_for_mc(&profile.game_version));

    Ok(ResolvedLaunch {
        profile,
        game_version,
        required_java,
    })
}

/// Phase 3: Detect Java installation (custom path or auto-detect).
fn detect_java(
    profile: &rmccc_profile::Profile,
    required_java: u32,
) -> Result<rmccc_launch::JavaInstallation, IpcError> {
    match &profile.java_path {
        Some(java_home) => {
            let home = PathBuf::from(java_home);
            let java_bin = if home.is_file() {
                home.clone()
            } else {
                home.join("bin").join("java")
            };
            if !java_bin.exists() {
                return Err(IpcError {
                    code: rmccc_error::ErrorCode::JavaNotFound,
                    message: format!("Java not found at: {}", java_bin.display()),
                    retryable: false,
                });
            }
            Ok(rmccc_launch::JavaInstallation {
                path: java_bin,
                major_version: required_java,
                vendor: String::new(),
                is_64bit: true,
            })
        }
        None => find_java(required_java).map_err(IpcError::from),
    }
}

/// Phase 5: Load auth session with offline fallback.
async fn resolve_auth_session(state: &AppState) -> Result<LaunchAuth, IpcError> {
    let core = state.core.read().await;
    // 统一经 Account::log_in() 获取 AuthInfo（提前窗口 + 刷新 + uuid 一致性校验）。
    match core.log_in_active_account().await? {
        Some(info) => Ok(LaunchAuth {
            username: info.username,
            uuid: info.uuid,
            access_token: info.access_token,
            user_type: info.user_type,
        }),
        // 无账户回退到默认离线账户（对齐 HMCL：msa + 随机 token）。
        None => {
            let info = rmccc_auth::OfflineAccount::new("Player").play_offline();
            Ok(LaunchAuth {
                username: info.username,
                uuid: info.uuid,
                access_token: info.access_token,
                user_type: info.user_type,
            })
        }
    }
}

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

    // ── Phase 1: Resolve profile & version (offline local) ──
    let resolved = resolve_launch_profile(&state, &profile_id).await?;

    // ── Phase 2: Pre-launch completeness check & auto-repair (L5) ──
    {
        let core = state.core.read().await;
        let _repaired =
            rmccc_core::pipeline::ensure_launch_ready(&core, &resolved.profile, None).await?;
    }

    // ── Phase 3: Detect Java ──
    let java_install = detect_java(&resolved.profile, resolved.required_java)?;
    tracing::info!(
        profile_id = %profile_id,
        java_version = java_install.major_version,
        java_path = %java_install.path.display(),
        "Launching Minecraft"
    );

    // ── Phase 4: Resolve auth ──
    let auth = resolve_auth_session(&state).await?;
    let auth_info =
        rmccc_launch::AuthInfo::new(auth.username, auth.uuid, auth.access_token, auth.user_type);
    tracing::info!(
        username = %auth_info.username,
        uuid = %auth_info.uuid,
        user_type = %auth_info.user_type,
        "Using auth session for launch"
    );

    // ── Phase 5: Build options + launcher (HMCL DefaultLauncher) ──
    // `from_profile` derives both the shared game root (versions/libraries/
    // assets, where the resolved manifest lives) and the instance's run
    // directory (which honours its isolation setting), so the launcher can no
    // longer be handed a per-profile directory holding none of those files.
    let options = {
        let core = state.core.read().await;
        LaunchOptions::from_profile(&resolved.profile, &core.config, java_install.clone())
    };
    let listener: Arc<dyn rmccc_launch::ProcessListener> = Arc::new(TauriProcessListener {
        app_handle: state.app_handle.clone(),
        profile_id: profile_id.clone(),
        running_games: state.running_games.clone(),
    });
    let launcher = rmccc_launch::DefaultLauncher::new(
        resolved.profile.game_version.clone(),
        resolved.game_version,
        auth_info,
        options,
        Some(listener),
    );

    // ── Phase 6: Launch (natives + log4j + spawn + monitors) ──
    let managed = launcher
        .launch()
        .await
        .map_err(|e| IpcError::from(rmccc_error::RmcccError::Other(e.to_string())))?;

    let pid = managed.pid();
    state.running_games.insert(profile_id.clone(), pid);
    let _ = state.app_handle.emit(
        crate::events::LAUNCH_STARTED,
        serde_json::json!({ "profile_id": profile_id, "pid": pid }),
    );

    // ── Update last_played ──
    let core = state.core.read().await;
    core.profile_store.touch(&profile_id)?;

    Ok(())
}

#[tauri::command]
#[tracing::instrument(skip(state))]
pub async fn launch_kill(
    state: tauri::State<'_, AppState>,
    profile_id: String,
) -> Result<(), IpcError> {
    if let Some(pid) = state.running_games.get(&profile_id) {
        let pid = *pid;
        tracing::info!(profile_id = %profile_id, pid = pid, "Killing game process");

        #[cfg(unix)]
        // SAFETY: kill(-pid, SIGKILL) 向 pid 所在进程组发信号。进程组由
        // spawn_game 的 process_group(0) 建立，可同时终止 Java 及其子进程树（P2-17）。
        unsafe {
            libc::kill(-(pid as libc::pid_t), libc::SIGKILL);
        }
        #[cfg(windows)]
        {
            let _ = std::process::Command::new("taskkill")
                .args(["/PID", &pid.to_string(), "/F"])
                .output();
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
