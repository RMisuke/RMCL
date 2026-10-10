//! Forge remote version — metadata for a specific Forge version.
//!
//! URLs point to the Forge installer JAR (Maven or HMCL mirror).

use crate::install_task::InstallTask;
use crate::remote_version::{RemoteVersion, VersionType};
use chrono::{DateTime, Utc};
use rmccc_error::Result;

/// A specific Forge version for a Minecraft version.
#[derive(Debug, Clone)]
pub struct ForgeRemoteVersion {
    pub game_version: String,
    pub forge_version: String,
    pub installer_url: String,
    pub modified: i64,
}

impl RemoteVersion for ForgeRemoteVersion {
    fn library_id(&self) -> &str {
        "forge"
    }

    fn game_version(&self) -> &str {
        &self.game_version
    }

    fn self_version(&self) -> &str {
        &self.forge_version
    }

    fn urls(&self) -> &[String] {
        // URLs are dynamic — return empty, install task uses installer_url directly
        &[]
    }

    fn release_date(&self) -> Option<DateTime<Utc>> {
        if self.modified > 0 {
            DateTime::from_timestamp(self.modified, 0)
        } else {
            None
        }
    }

    fn version_type(&self) -> VersionType {
        if self.modified > 0 {
            VersionType::Release
        } else {
            VersionType::Uncategorized
        }
    }

    fn create_install_task(
        &self,
        _dm: &super::super::DependencyManager,
        _manifest: &rmccc_version::GameVersion,
    ) -> Result<Box<dyn InstallTask>> {
        Ok(Box::new(super::install_task::ForgeInstallTask {
            game_version: self.game_version.clone(),
            forge_version: self.forge_version.clone(),
            installer_url: self.installer_url.clone(),
            state: super::install_task::ForgeInstallState::NotStarted,
            java_path: None,
            game_dir: None,
            client: reqwest::Client::new(),
        }))
    }
}
