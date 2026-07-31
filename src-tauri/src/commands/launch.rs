//! Launch IPC commands.

use crate::state::AppState;
use rmccc_error::IpcError;
use rmccc_launch::{find_java, LaunchArgBuilder};
use rmccc_version::{libraries, MojangVersionProvider, VersionProvider};
use tauri::Emitter;

/// Validate that the game directory and key files exist before launching.
fn validate_launch_readiness(game_dir: &std::path::Path, version_id: &str) -> Result<(), IpcError> {
    // Ensure game directory exists
    if !game_dir.exists() {
        return Err(IpcError {
            code: rmccc_error::ErrorCode::ProfileNotFound,
            message: format!(
                "Game directory does not exist: {}. Install the version first.",
                game_dir.display()
            ),
            retryable: false,
        });
    }

    // Ensure client JAR exists
    let client_jar = game_dir
        .join("versions")
        .join(version_id)
        .join(format!("{version_id}.jar"));
    if !client_jar.exists() {
        return Err(IpcError {
            code: rmccc_error::ErrorCode::DownloadFailed,
            message: format!(
                "Client JAR not found: {}. Install version {} first.",
                client_jar.display(),
                version_id
            ),
            retryable: false,
        });
    }

    Ok(())
}

#[tauri::command]
#[tracing::instrument(skip(state))]
pub async fn launch_start(
    state: tauri::State<'_, AppState>,
    profile_id: String,
) -> Result<(), IpcError> {
    // ── Check not already running FIRST (TOCTOU fix) ──
    if state.running_games.contains_key(&profile_id) {
        return Err(IpcError {
            code: rmccc_error::ErrorCode::AlreadyRunning,
            message: format!("Profile '{profile_id}' is already running"),
            retryable: false,
        });
    }

    // ── Load profile (short lock) ──
    let (profile, game_version, required_java) = {
        let core = state.core.read().await;
        let profile = core
            .profile_store
            .get(&profile_id)?
            .ok_or_else(|| IpcError {
                code: rmccc_error::ErrorCode::ProfileNotFound,
                message: format!("Profile not found: {profile_id}"),
                retryable: false,
            })?;

        // Fetch version metadata
        let provider = MojangVersionProvider::new(state.http_client.clone());
        let game_version = provider.fetch_version(&profile.game_version).await?;

        let required_java = game_version
            .java_version
            .as_ref()
            .map(|j| j.major_version)
            .unwrap_or_else(|| rmccc_version::java_version_for_mc(&profile.game_version));

        (profile, game_version, required_java)
    };

    // ── Validate launch readiness ──
    validate_launch_readiness(&profile.game_dir, &profile.game_version)?;

    // ── Ensure game directories exist ──
    for subdir in &["assets", "libraries", "versions"] {
        std::fs::create_dir_all(profile.game_dir.join(subdir))?;
    }

    // ── Detect Java (no lock needed) ──
    let java_install = match &profile.java_path {
        Some(java_home) => {
            let home = std::path::PathBuf::from(java_home);
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
            rmccc_launch::JavaInstallation {
                path: java_bin,
                major_version: required_java,
                vendor: String::new(),
                is_64bit: true,
            }
        }
        None => find_java(required_java)?,
    };

    tracing::info!(
        profile_id = %profile_id,
        java_version = java_install.major_version,
        java_path = %java_install.path.display(),
        "Launching Minecraft"
    );

    // ── Build classpath ──
    let libs_dir = profile.game_dir.join("libraries");
    let mut classpath = libraries::collect_classpath(&game_version.libraries, &libs_dir);
    let client_jar = profile
        .game_dir
        .join("versions")
        .join(&profile.game_version)
        .join(format!("{}.jar", profile.game_version));
    classpath.push(client_jar);

    // ── Load auth session with proper fallback ──
    let (username, uuid, access_token, user_type) = {
        let data_dir = {
            let core = state.core.read().await;
            core.config.resolved_data_dir()
        };
        let auth_path = data_dir.join("auth.json");
        let vault = state.vault.lock().await;
        match vault.load_session(&auth_path)? {
            Some(session) => {
                let ut = if session.provider == "microsoft" {
                    "msa"
                } else {
                    "legacy"
                };
                (session.username, session.uuid, session.access_token, ut.to_string())
            }
            None => {
                // No session at all — offline fallback
                let name = "Player".to_string();
                let uid = rmccc_auth::OfflineAuthProvider::uuid_from_username(&name);
                // Minecraft expects "0" as the offline access token, not a random UUID
                (name, uid, "0".to_string(), "legacy".to_string())
            }
        }
    };

    tracing::info!(
        username = %username,
        uuid = %uuid,
        user_type = %user_type,
        "Using auth session for launch"
    );

    // Determine version type string
    let version_type = match game_version.release_type {
        rmccc_version::ReleaseType::Release => "release",
        rmccc_version::ReleaseType::Snapshot => "snapshot",
        rmccc_version::ReleaseType::OldBeta => "old_beta",
        rmccc_version::ReleaseType::OldAlpha => "old_alpha",
    };

    let natives_dir = profile.game_dir.join("versions").join(&profile.game_version).join("natives");
    let jvm_args = LaunchArgBuilder::jvm_args(
        &classpath,
        profile.memory_mb,
        &profile.jvm_args,
        &[], // TODO: resolve version JSON JVM args
        &natives_dir,
        &profile.game_dir,
        "RMCL",
        env!("CARGO_PKG_VERSION"),
        java_install.major_version,
        java_install.is_64bit,
    );
    let game_args = LaunchArgBuilder::game_args(
        &profile.game_version,
        &profile.game_dir,
        &username,
        &uuid,
        &access_token,
        &user_type,
        &game_version.asset_index.id,
        version_type,
        &profile.game_args,
    );

    // ── Response file ──
    let response_file = rmccc_launch::build_response_file(
        &jvm_args,
        &game_args,
        &game_version.main_class,
        &profile.game_dir,
    )?;

    // ── Spawn game process ──
    let mut child = rmccc_launch::spawn_game(
        &java_install.path,
        &response_file,
        &profile.game_dir,
        None,
    )
    .await?;

    let pid = child.id().unwrap_or(0);
    let app_handle = state.app_handle.clone();
    let profile_id_clone = profile_id.clone();
    let running_games = state.running_games.clone();

    // Track running game
    running_games.insert(profile_id.clone(), pid);

    // Emit launch started
    let _ = app_handle.emit(
        crate::events::LAUNCH_STARTED,
        serde_json::json!({ "profile_id": profile_id, "pid": pid }),
    );

    // ── Spawn output reader + process monitor (drains BOTH stdout and stderr) ──
    tokio::spawn(async move {
        use tokio::io::AsyncReadExt;

        // Spawn stderr drainer
        let app_handle_stderr = app_handle.clone();
        let pid_stderr = profile_id_clone.clone();
        let stderr_handle = if let Some(mut stderr) = child.stderr.take() {
            Some(tokio::spawn(async move {
                let mut buf = [0u8; 4096];
                loop {
                    match stderr.read(&mut buf).await {
                        Ok(0) => break,
                        Ok(n) => {
                            let _ = app_handle_stderr.emit(
                                crate::events::LAUNCH_OUTPUT,
                                serde_json::json!({
                                    "profile_id": pid_stderr,
                                    "text": String::from_utf8_lossy(&buf[..n]),
                                }),
                            );
                        }
                        Err(_) => break,
                    }
                }
            }))
        } else {
            None
        };

        // Read stdout
        if let Some(mut stdout) = child.stdout.take() {
            let mut buf = [0u8; 4096];
            loop {
                match stdout.read(&mut buf).await {
                    Ok(0) => break,
                    Ok(n) => {
                        let _ = app_handle.emit(
                            crate::events::LAUNCH_OUTPUT,
                            serde_json::json!({
                                "profile_id": profile_id_clone,
                                "text": String::from_utf8_lossy(&buf[..n]),
                            }),
                        );
                    }
                    Err(e) => {
                        tracing::error!("stdout read error: {e}");
                        break;
                    }
                }
            }
        }

        // Wait for stderr drainer
        if let Some(handle) = stderr_handle {
            handle.await.ok();
        }

        // Wait for process exit
        let status = child.wait().await;
        let exit_code = status.ok().and_then(|s| s.code()).unwrap_or(-1);

        // Clean up running_games
        running_games.remove(&profile_id_clone);

        // Clean up response file
        let _ = std::fs::remove_file(&response_file);

        let _ = app_handle.emit(
            crate::events::LAUNCH_EXITED,
            serde_json::json!({
                "profile_id": profile_id_clone,
                "exit_code": exit_code,
            }),
        );

        tracing::info!(
            profile_id = %profile_id_clone,
            exit_code = exit_code,
            "Game process exited"
        );
    });

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
        // SAFETY: kill(pid, SIGKILL) is well-defined for valid PIDs obtained from
        // tokio::process::Child::id(), which returns the OS-allocated process ID.
        unsafe {
            libc::kill(pid as libc::pid_t, libc::SIGKILL);
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
pub async fn launch_status(
    state: tauri::State<'_, AppState>,
) -> Result<Vec<String>, IpcError> {
    let keys: Vec<String> = state.running_games.iter().map(|r| r.key().clone()).collect();
    Ok(keys)
}
