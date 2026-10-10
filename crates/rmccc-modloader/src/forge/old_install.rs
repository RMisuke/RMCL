//! Forge old install task — for Forge 1.12 and earlier.
//!
//! Extracts the universal JAR from the installer and directly uses
//! the embedded `versionInfo` as the patch metadata source.

use crate::analyzer::LibraryType;
use crate::install_task::InstallTask;
use crate::patch::{GameInstancePatch, PRIORITY_LOADER};
use rmccc_error::{Result, RmcccError};
use std::io::Read;
use std::path::PathBuf;

use super::profile::ForgeInstallProfile;

/// Installs Forge using the old (1.12-) universal JAR method.
#[derive(Debug)]
pub struct ForgeOldInstallTask {
    pub game_version: String,
    pub forge_version: String,
    pub installer_path: PathBuf,
}

#[async_trait::async_trait]
impl InstallTask for ForgeOldInstallTask {
    async fn execute(&mut self) -> Result<GameInstancePatch> {
        tracing::info!(
            game_version = %self.game_version,
            forge_version = %self.forge_version,
            "Executing Forge old install task"
        );

        // Read install_profile.json from the installer JAR
        let file = std::fs::File::open(&self.installer_path)
            .map_err(|e| RmcccError::Other(format!("Cannot open Forge installer: {e}")))?;

        let mut archive = zip::ZipArchive::new(file)
            .map_err(|e| RmcccError::Other(format!("Forge installer is not a valid ZIP: {e}")))?;

        let mut profile_json = String::new();
        {
            let mut entry = archive.by_name("install_profile.json").map_err(|e| {
                RmcccError::Other(format!(
                    "install_profile.json not found in Forge installer: {e}"
                ))
            })?;
            entry.read_to_string(&mut profile_json)?;
        }

        let install_profile: ForgeInstallProfile = serde_json::from_str(&profile_json)
            .map_err(|e| RmcccError::Other(format!("Failed to parse install_profile.json: {e}")))?;

        // Validate Minecraft version match
        if install_profile.install.minecraft != self.game_version {
            return Err(RmcccError::Other(format!(
                "Forge installer targets MC {} but instance is {}",
                install_profile.install.minecraft, self.game_version
            )));
        }

        tracing::info!(
            main_class = %install_profile.version_info.main_class,
            "Forge old install: versionInfo parsed"
        );

        // Build patch from the versionInfo
        let version_info = install_profile.version_info;
        Ok(GameInstancePatch::with_loader(
            LibraryType::Forge.patch_id(),
            &self.forge_version,
            PRIORITY_LOADER,
            Some(version_info.main_class),
            version_info.arguments,
            version_info.libraries,
        ))
    }

    fn description(&self) -> &str {
        "Installing Forge (legacy)"
    }
}
