//! Forge install task — routes between new (1.13+) and old (1.12-) install methods.
//!
//! Detects the installer type by reading `install_profile.json` from the
//! installer JAR: presence of `spec` key → new format, otherwise old.

use crate::install_task::InstallTask;
use crate::patch::GameInstancePatch;
use rmccc_error::{Result, RmcccError};
use std::io::Read;
use std::path::PathBuf;

use super::new_install::ForgeNewInstallTask;
use super::old_install::ForgeOldInstallTask;

/// The state of the Forge install task.
#[derive(Debug)]
pub enum ForgeInstallState {
    NotStarted,
    /// Detected as new-format installer.
    New(Box<ForgeNewInstallTask>),
    /// Detected as old-format installer.
    Old(Box<ForgeOldInstallTask>),
}

/// Routes between new and old Forge installer formats.
pub struct ForgeInstallTask {
    pub game_version: String,
    pub forge_version: String,
    pub installer_url: String,
    pub state: ForgeInstallState,
    /// Path to Java binary (required for new format).
    pub java_path: Option<PathBuf>,
    /// Game directory (required for new format).
    pub game_dir: Option<PathBuf>,
    /// Shared HTTP client (reused from provider).
    pub client: reqwest::Client,
    /// 平台描述符（批次 4）：new-format processors 的 `java -cp` 分隔符。
    pub platform: rmccc_platform::PlatformDescriptor,
}

impl ForgeInstallTask {
    /// Detect the installer type by reading `install_profile.json`.
    fn detect_type(&self, installer_path: &std::path::Path) -> Result<ForgeInstallState> {
        let file = std::fs::File::open(installer_path)
            .map_err(|e| RmcccError::Other(format!("Cannot open Forge installer: {e}")))?;

        let mut archive = zip::ZipArchive::new(file)
            .map_err(|e| RmcccError::Other(format!("Forge installer is not a valid ZIP: {e}")))?;

        let mut profile_json = String::new();
        {
            let mut entry = archive
                .by_name("install_profile.json")
                .map_err(|e| RmcccError::Other(format!("install_profile.json not found: {e}")))?;
            entry.read_to_string(&mut profile_json)?;
        }

        // Check for new-format marker ("spec" field)
        let parsed: serde_json::Value = serde_json::from_str(&profile_json)
            .map_err(|e| RmcccError::Other(format!("Invalid install_profile.json: {e}")))?;

        if parsed.get("spec").is_some() {
            // New format
            tracing::info!("Detected new-format Forge installer");
            // 批次 4：java/game_dir 必须来自 InstallTaskContext（此前静默退到
            // PATH "java" 与 "."，processors 必然在错误目录里跑）。
            let java_path = self.java_path.clone().ok_or_else(|| {
                RmcccError::Other("new-format Forge install requires java_path".into())
            })?;
            let game_dir = self.game_dir.clone().ok_or_else(|| {
                RmcccError::Other("new-format Forge install requires game_dir".into())
            })?;
            Ok(ForgeInstallState::New(Box::new(ForgeNewInstallTask {
                game_version: self.game_version.clone(),
                forge_version: self.forge_version.clone(),
                installer_path: installer_path.to_path_buf(),
                java_path,
                game_dir,
                client: self.client.clone(),
                platform: self.platform,
            })))
        } else if parsed.get("install").is_some() && parsed.get("versionInfo").is_some() {
            // Old format
            tracing::info!("Detected old-format Forge installer");
            Ok(ForgeInstallState::Old(Box::new(ForgeOldInstallTask {
                game_version: self.game_version.clone(),
                forge_version: self.forge_version.clone(),
                installer_path: installer_path.to_path_buf(),
            })))
        } else {
            Err(RmcccError::Other("Unknown Forge installer format".into()))
        }
    }
}

#[async_trait::async_trait]
impl InstallTask for ForgeInstallTask {
    async fn execute(&mut self) -> Result<GameInstancePatch> {
        // 批次 4：下载走共享实现（唯一临时文件名 + ZIP 校验 + .sha1 校验；
        // 此前固定名 `/tmp/forge-installer-{pid}.jar` 且从不删除——每次
        // 安装泄漏 ~180MB）。
        let (installer_path, _used_url) = crate::installer::download_installer_file(
            &self.client,
            &[self.installer_url.clone()],
            None,
        )
        .await?;

        let result = async {
            // Detect type if not already done
            if matches!(self.state, ForgeInstallState::NotStarted) {
                self.state = self.detect_type(&installer_path)?;
            }

            // Execute the appropriate sub-task
            match &mut self.state {
                ForgeInstallState::NotStarted => {
                    unreachable!("State must be set after detection")
                }
                ForgeInstallState::New(task) => task.execute().await,
                ForgeInstallState::Old(task) => task.execute().await,
            }
        }
        .await;

        let _ = std::fs::remove_file(&installer_path);
        result
    }

    fn description(&self) -> &str {
        "Installing Forge"
    }
}
