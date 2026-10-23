//! 启动域：启动 / 等待 / 终止 / 状态、Java 探测、命令行 dry-run。
//!
//! `impl CliContext` 的一个分片（P2-1：把 1,450 行的 God class 按域拆开）。
//! 方法名 / 签名与拆分前完全一致，命令层无需改动。

use super::helpers::CliProcessListener;
use super::*;

impl CliContext {
    /// Start a game process (HMCL-aligned orchestration), returning its pid.
    ///
    /// Guards against duplicate launches, runs the pre-launch completeness
    /// check (unless `no_repair`), detects Java, resolves auth (offline
    /// fallback) and builds [`rmccc_launch::DefaultLauncher`] with a
    /// state-syncing [`ProcessListener`]. The process is registered in
    /// [`Self::running_games`] and its exit code is published to the exit
    /// signal channel so `launch status` / `launch wait` can observe it.
    pub async fn launch_start(&self, profile_id: &str, opts: &LaunchStartOptions) -> Result<u32> {
        // Guard: duplicate launch.
        if self.running_games.contains_key(profile_id) {
            return Err(RmcccError::Other(format!(
                "Profile '{profile_id}' is already running"
            )));
        }

        let profile = self.get_profile(profile_id).await?;

        // 启动前完整性检查与自动补齐（HMCL checkGameCompletionAsync）。
        if !opts.no_repair {
            let core = self.core.read().await;
            let report =
                pipeline::ensure_launch_ready(&core, &profile, opts.progress.as_ref()).await?;
            if !report.is_complete() {
                return Err(RmcccError::DownloadFailed {
                    url: profile.game_version.clone(),
                    reason: format!(
                        "{} of {} files failed to download",
                        report.failed_files, report.total_files
                    ),
                });
            }
            if !report.is_noop() {
                tracing::info!(
                    profile_id = %profile_id,
                    files = report.total_files,
                    "Pre-launch resources repaired"
                );
            }
        }

        // 状态同步监听器（CLI 侧唯一的启动适配）。
        let (exit_tx, exit_rx) = tokio::sync::watch::channel(None);
        self.exit_signals.insert(profile_id.to_string(), exit_rx);
        let listener: Arc<dyn rmccc_launch::ProcessListener> = Arc::new(CliProcessListener {
            profile_id: profile_id.to_string(),
            running_games: self.running_games.clone(),
            exit_tx,
            output: opts.output.clone(),
            on_exit: opts.on_exit.clone(),
        });

        // 启动编排（Java 探测 / 鉴权 / 命令行 / natives / spawn）全在 pipeline。
        let core = self.core.read().await;
        let managed =
            pipeline::launch_game(&core, &profile, listener, opts.progress.as_ref()).await?;
        let pid = managed.pid();
        self.running_games.insert(profile_id.to_string(), pid);

        tracing::info!(profile_id = %profile_id, pid = pid, "Game launched");
        Ok(pid)
    }

    /// Block until the given profile's game exits; returns its exit code.
    /// Returns `None` when the profile is not running and has no recorded exit.
    pub async fn launch_wait(&self, profile_id: &str) -> Result<Option<i32>> {
        // Already exited and recorded?
        if !self.running_games.contains_key(profile_id) {
            if let Some(recv) = self.exit_signals.get(profile_id) {
                if let Some(code) = *recv.borrow() {
                    return Ok(Some(code));
                }
            }
            return Ok(None);
        }
        let recv = match self.exit_signals.get(profile_id) {
            Some(r) => r.clone(),
            None => return Ok(None),
        };
        let mut recv = recv;
        loop {
            if let Some(code) = *recv.borrow() {
                return Ok(Some(code));
            }
            if recv.changed().await.is_err() {
                return Ok(None);
            }
        }
    }

    /// Terminate a running game (whole process tree).
    ///
    /// 终止方式由平台后端决定（Unix 进程组 `SIGKILL`；Windows 待 Job Object），
    /// 本层不再出现 `#[cfg(unix)]` / `taskkill`（指南 §5.7）。
    pub async fn launch_stop(&self, profile_id: &str) -> Result<()> {
        let pid = self
            .running_games
            .get(profile_id)
            .map(|g| *g)
            .ok_or_else(|| RmcccError::Other(format!("Profile '{profile_id}' is not running")))?;
        tracing::info!(profile_id = %profile_id, pid = pid, "Stopping game process");
        {
            let core = self.core.read().await;
            core.platform.process.terminate_tree(pid)?;
        }
        self.running_games.remove(profile_id);
        Ok(())
    }

    /// List running games.
    pub async fn launch_status(&self) -> Result<Vec<RunningGame>> {
        let mut out: Vec<RunningGame> = self
            .running_games
            .iter()
            .map(|g| RunningGame {
                profile_id: g.key().clone(),
                pid: *g.value(),
            })
            .collect();
        out.sort_by(|a, b| a.profile_id.cmp(&b.profile_id));
        Ok(out)
    }

    /// Detect Java installations on the system (经平台适配层注入)。
    pub async fn launch_java(&self) -> Result<Vec<rmccc_launch::JavaInstallation>> {
        self.core.read().await.platform.java.detect_all()
    }

    /// Dry-run: generate the exact command line a launch would use (no spawn).
    pub async fn launch_command(&self, profile_id: &str) -> Result<Value> {
        let profile = self.get_profile(profile_id).await?;
        let game_version = {
            let core = self.core.read().await;
            let repo = core
                .game_repository
                .read()
                .map_err(|_| RmcccError::Other("game repository lock poisoned".to_string()))?;
            repo.get_resolved(&profile.game_version)
                .map_err(|e| RmcccError::Other(e.to_string()))?
                .launch
        };
        let required_java = game_version
            .java_version
            .as_ref()
            .map(|j| j.major_version)
            .unwrap_or_else(|| rmccc_version::java_version_for_mc(&profile.game_version));
        let java_install = match &profile.java_path {
            Some(jh) => {
                let home = PathBuf::from(jh);
                let bin = if home.is_file() {
                    home
                } else {
                    home.join("bin").join("java")
                };
                rmccc_launch::JavaInstallation {
                    path: bin,
                    major_version: required_java,
                    vendor: String::new(),
                    is_64bit: true,
                }
            }
            None => rmccc_launch::find_java(required_java)?,
        };
        let (username, uuid, access_token, user_type) = {
            let core = self.core.read().await;
            match core.log_in_active_account().await? {
                Some(info) => (info.username, info.uuid, info.access_token, info.user_type),
                None => {
                    let info = rmccc_auth::OfflineAccount::new("Player").play_offline();
                    (info.username, info.uuid, info.access_token, info.user_type)
                }
            }
        };
        let auth = rmccc_launch::AuthInfo::new(username, uuid, access_token, user_type);
        let core = self.core.read().await;
        let options = rmccc_launch::LaunchOptions::from_profile(
            &profile,
            &core.config,
            java_install.clone(),
            core.platform.clone(),
        );
        drop(core);
        let launcher = rmccc_launch::DefaultLauncher::new(
            profile.game_version.clone(),
            game_version,
            auth,
            options,
            None,
        );
        let command = launcher
            .generate_command_line()
            .map_err(|e| RmcccError::Other(e.to_string()))?;
        Ok(json!({
            "profile_id": profile_id,
            "java": java_install.path.display().to_string(),
            "process_prefix": command.prefix,
            "main_class": command.main_class,
            "jvm_args": command.jvm_args,
            "game_args": command.game_args,
            "native_dir": command.java_native_folder.display().to_string(),
        }))
    }
}
