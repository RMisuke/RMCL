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
    /// Path to Java binary (only needed for new format).
    pub java_path: Option<PathBuf>,
    /// Game directory.
    pub game_dir: Option<PathBuf>,
    /// Shared HTTP client (reused from provider).
    pub client: reqwest::Client,
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
            Ok(ForgeInstallState::New(Box::new(ForgeNewInstallTask {
                game_version: self.game_version.clone(),
                forge_version: self.forge_version.clone(),
                installer_path: installer_path.to_path_buf(),
                java_path: self
                    .java_path
                    .clone()
                    .unwrap_or_else(|| PathBuf::from("java")),
                game_dir: self.game_dir.clone().unwrap_or_else(|| PathBuf::from(".")),
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
        // Download the installer if needed
        let installer_path = download_installer(&self.client, &self.installer_url).await?;

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

    fn description(&self) -> &str {
        "Installing Forge"
    }
}

/// Download the Forge installer JAR to a temp file.
async fn download_installer(client: &reqwest::Client, url: &str) -> Result<PathBuf> {
    tracing::info!(url = %url, "Downloading Forge installer");

    let resp = client
        .get(url)
        .send()
        .await
        .map_err(|e| RmcccError::Network(e.to_string()))?;

    if !resp.status().is_success() {
        return Err(RmcccError::DownloadFailed {
            url: url.to_string(),
            reason: format!("HTTP {}", resp.status()),
        });
    }

    let bytes = resp
        .bytes()
        .await
        .map_err(|e| RmcccError::Network(e.to_string()))?;

    let temp = std::env::temp_dir().join(format!("forge-installer-{}.jar", std::process::id()));
    std::fs::write(&temp, &bytes)?;

    // Verify ZIP integrity
    let cursor = std::io::Cursor::new(&bytes);
    zip::ZipArchive::new(cursor)
        .map_err(|e| RmcccError::Other(format!("Forge installer is not a valid ZIP: {e}")))?;

    tracing::info!(path = %temp.display(), "Forge installer downloaded");
    Ok(temp)
}
