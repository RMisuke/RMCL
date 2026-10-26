//! OptiFine remote version.

use crate::install_task::InstallTask;
use crate::remote_version::{RemoteVersion, VersionType};
use chrono::{DateTime, Utc};
use rmccc_error::Result;

#[derive(Debug, Clone)]
pub struct OptiFineRemoteVersion {
    pub game_version: String,
    pub optifine_version: String,
    pub download_url: String,
}

impl RemoteVersion for OptiFineRemoteVersion {
    fn library_id(&self) -> &str {
        "optifine"
    }
    fn game_version(&self) -> &str {
        &self.game_version
    }
    fn self_version(&self) -> &str {
        &self.optifine_version
    }
    fn urls(&self) -> &[String] {
        &[]
    }
    fn release_date(&self) -> Option<DateTime<Utc>> {
        None
    }
    fn version_type(&self) -> VersionType {
        VersionType::Release
    }

    fn create_install_task(
        &self,
        _dm: &super::super::DependencyManager,
        _manifest: &rmccc_version::GameVersion,
        _ctx: &super::super::remote_version::InstallTaskContext<'_>,
    ) -> Result<Box<dyn InstallTask>> {
        Ok(Box::new(super::install_task::OptiFineInstallTask {
            client: _dm.client_for_tasks(),
            game_version: self.game_version.clone(),
            optifine_version: self.optifine_version.clone(),
            download_url: self.download_url.clone(),
        }))
    }
}
