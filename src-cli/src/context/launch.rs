//! 启动域：启动 / 等待 / 终止 / 状态、Java 探测、命令行 dry-run。

use super::helpers::CliProcessListener;
use super::*;

impl CliContext {
    /// Start a game process, returning its pid.
    pub async fn launch_start(&self, key: &str, opts: &LaunchStartOptions) -> Result<u32> {
        if self.running_games.contains_key(key) {
            return Err(RmcccError::Other(format!(
                "Instance '{key}' is already running"
            )));
        }

        let instance = self.get_instance(key).await?;

        if !opts.no_repair {
            let core = self.core.read().await;
            let report =
                pipeline::ensure_launch_ready(&core.handle(), &instance, opts.progress.as_ref())
                    .await?;
            if !report.is_complete() {
                return Err(RmcccError::DownloadFailed {
                    url: instance.summary.id.clone(),
                    reason: format!(
                        "{} of {} files failed to download",
                        report.failed_files, report.total_files
                    ),
                });
            }
            if !report.is_noop() {
                tracing::info!(
                    key = %key,
                    files = report.total_files,
                    "Pre-launch resources repaired"
                );
            }
        }

        let (exit_tx, exit_rx) = tokio::sync::watch::channel(None);
        self.exit_signals.insert(key.to_string(), exit_rx);
        let listener: Arc<dyn rmccc_launch::ProcessListener> = Arc::new(CliProcessListener {
            key: key.to_string(),
            running_games: self.running_games.clone(),
            exit_tx,
            output: opts.output.clone(),
            on_exit: opts.on_exit.clone(),
        });

        let core = self.core.read().await;
        let managed =
            pipeline::launch_game(&core.handle(), &instance, listener, opts.progress.as_ref())
                .await?;
        let pid = managed.pid();
        self.running_games.insert(key.to_string(), pid);

        tracing::info!(key = %key, pid = pid, "Game launched");
        Ok(pid)
    }

    /// Block until the given instance's game exits; returns its exit code.
    pub async fn launch_wait(&self, key: &str) -> Result<Option<i32>> {
        if !self.running_games.contains_key(key) {
            if let Some(recv) = self.exit_signals.get(key) {
                if let Some(code) = *recv.borrow() {
                    return Ok(Some(code));
                }
            }
            return Ok(None);
        }
        let recv = match self.exit_signals.get(key) {
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
    pub async fn launch_stop(&self, key: &str) -> Result<()> {
        let pid = self
            .running_games
            .get(key)
            .map(|g| *g)
            .ok_or_else(|| RmcccError::Other(format!("Instance '{key}' is not running")))?;
        tracing::info!(key = %key, pid = pid, "Stopping game process");
        {
            let core = self.core.read().await;
            core.platform.process.terminate_tree(pid)?;
        }
        self.running_games.remove(key);
        Ok(())
    }

    /// List running games.
    pub async fn launch_status(&self) -> Result<Vec<RunningGame>> {
        let mut out: Vec<RunningGame> = self
            .running_games
            .iter()
            .map(|g| RunningGame {
                key: g.key().clone(),
                pid: *g.value(),
            })
            .collect();
        out.sort_by(|a, b| a.key.cmp(&b.key));
        Ok(out)
    }

    /// Detect Java installations on the system (经平台适配层注入).
    pub async fn launch_java(&self) -> Result<Vec<rmccc_launch::JavaInstallation>> {
        self.core.read().await.platform.java.detect_all()
    }

    /// Dry-run: generate the exact command line a launch would use (no spawn).
    pub async fn launch_command(&self, key: &str) -> Result<Value> {
        let instance = self.get_instance(key).await?;
        let core = self.core.read().await;
        let (game_version, java_install) = {
            let mut service = core
                .instances
                .lock()
                .map_err(|_| RmcccError::Other("instance service lock poisoned".to_string()))?;
            let (_, resolved) = service.resolved_version(key)?;
            let required_java = resolved
                .launch
                .java_version
                .as_ref()
                .map(|j| j.major_version)
                .unwrap_or_else(|| {
                    rmccc_version::java_version_for_mc(&instance.summary.game_version)
                });
            let java_install = if instance.effective.java_type == rmccc_instance::JavaType::Custom {
                let home = PathBuf::from(&instance.effective.custom_java_path);
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
            } else {
                core.platform.java.find(required_java)?
            };
            (resolved.launch, java_install)
        };
        let (username, uuid, access_token, user_type) = match core.log_in_active_account().await? {
            Some(info) => (info.username, info.uuid, info.access_token, info.user_type),
            None => {
                let info = rmccc_auth::OfflineAccount::new("Player").play_offline();
                (info.username, info.uuid, info.access_token, info.user_type)
            }
        };
        let auth = rmccc_launch::AuthInfo::new(username, uuid, access_token, user_type);
        let options = rmccc_launch::LaunchOptions::from_instance(
            &instance,
            java_install.clone(),
            core.platform.clone(),
        );
        let launcher = rmccc_launch::DefaultLauncher::new(
            instance.summary.id.clone(),
            game_version,
            auth,
            options,
            None,
        );
        let command = launcher
            .generate_command_line()
            .map_err(|e| RmcccError::Other(e.to_string()))?;
        Ok(json!({
            "key": key,
            "java": java_install.path.display().to_string(),
            "process_prefix": command.prefix,
            "main_class": command.main_class,
            "jvm_args": command.jvm_args,
            "game_args": command.game_args,
            "native_dir": command.java_native_folder.display().to_string(),
        }))
    }
}
