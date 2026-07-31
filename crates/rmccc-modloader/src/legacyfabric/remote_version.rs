//! Legacy Fabric remote version.

use crate::remote_version::{RemoteVersion, VersionType};
use crate::install_task::InstallTask;
use chrono::{DateTime, Utc};
use rmccc_error::Result;

#[derive(Debug, Clone)]
pub struct LegacyFabricRemoteVersion {
    pub game_version: String,
    pub loader_version: String,
    pub meta_url: String,
    pub stable: bool,
}

impl RemoteVersion for LegacyFabricRemoteVersion {
    fn library_id(&self) -> &str { "legacyfabric" }
    fn game_version(&self) -> &str { &self.game_version }
    fn self_version(&self) -> &str { &self.loader_version }
    fn urls(&self) -> &[String] { std::slice::from_ref(&self.meta_url) }
    fn release_date(&self) -> Option<DateTime<Utc>> { None }
    fn version_type(&self) -> VersionType { if self.stable { VersionType::Release } else { VersionType::Snapshot } }

    fn create_install_task(&self, _dm: &super::super::DependencyManager, _manifest: &rmccc_version::GameVersion) -> Result<Box<dyn InstallTask>> {
        Ok(Box::new(super::install_task::LegacyFabricInstallTask {
            game_version: self.game_version.clone(),
            loader_version: self.loader_version.clone(),
            meta_url: self.meta_url.clone(),
        }))
    }
}
