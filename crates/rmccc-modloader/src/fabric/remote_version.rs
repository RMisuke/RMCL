//! Fabric remote version — metadata for a specific Fabric loader version.
//!
//! The URLs point to the launch metadata endpoint (`/v2/versions/loader/{game}/{loader}`)
//! which returns a JSON with loader Maven coordinates, intermediary mappings, and launcher metadata.

use crate::remote_version::{RemoteVersion, VersionType};
use crate::install_task::InstallTask;
use chrono::{DateTime, Utc};
use rmccc_error::Result;

/// Base URL for the Fabric Meta API.
const FABRIC_META_BASE: &str = "https://meta.fabricmc.net/v2";

/// A specific Fabric loader version for a Minecraft version.
#[derive(Debug, Clone)]
pub struct FabricRemoteVersion {
    pub game_version: String,
    pub loader_version: String,
    pub stable: bool,
}

impl FabricRemoteVersion {
    /// Build the launch metadata URL for this version combination.
    pub fn launch_meta_url(&self) -> String {
        format!(
            "{FABRIC_META_BASE}/versions/loader/{}/{}",
            self.game_version, self.loader_version
        )
    }

    /// Build the profile JSON URL (alternative simpler endpoint).
    pub fn profile_json_url(&self) -> String {
        format!(
            "{FABRIC_META_BASE}/versions/loader/{}/{}/profile/json",
            self.game_version, self.loader_version
        )
    }
}

impl RemoteVersion for FabricRemoteVersion {
    fn library_id(&self) -> &str {
        "fabric"
    }

    fn game_version(&self) -> &str {
        &self.game_version
    }

    fn self_version(&self) -> &str {
        &self.loader_version
    }

    fn urls(&self) -> &[String] {
        // Return as a slice from a static; but since this is dynamic we need a different approach.
        // For now we return an empty slice — the install task knows how to build URLs.
        &[]
    }

    fn release_date(&self) -> Option<DateTime<Utc>> {
        None
    }

    fn version_type(&self) -> VersionType {
        if self.stable {
            VersionType::Release
        } else {
            VersionType::Snapshot
        }
    }

    fn create_install_task(
        &self,
        _dm: &super::super::DependencyManager,
        _manifest: &rmccc_version::GameVersion,
    ) -> Result<Box<dyn InstallTask>> {
        Ok(Box::new(super::install_task::FabricInstallTask {
            game_version: self.game_version.clone(),
            loader_version: self.loader_version.clone(),
            launch_meta_url: self.launch_meta_url(),
        }))
    }
}
