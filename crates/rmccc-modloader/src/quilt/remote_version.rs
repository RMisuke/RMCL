//! Quilt remote version.

use crate::install_task::InstallTask;
use crate::remote_version::{RemoteVersion, VersionType};
use chrono::{DateTime, Utc};
use rmccc_error::Result;

const QUILT_META_BASE: &str = "https://meta.quiltmc.org/v3";

#[derive(Debug, Clone)]
pub struct QuiltRemoteVersion {
    pub game_version: String,
    pub loader_version: String,
}

impl QuiltRemoteVersion {
    pub fn launch_meta_url(&self) -> String {
        format!(
            "{QUILT_META_BASE}/versions/loader/{}/{}",
            self.game_version, self.loader_version
        )
    }
}

impl RemoteVersion for QuiltRemoteVersion {
    fn library_id(&self) -> &str {
        "quilt"
    }

    fn game_version(&self) -> &str {
        &self.game_version
    }

    fn self_version(&self) -> &str {
        &self.loader_version
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
        Ok(Box::new(super::install_task::QuiltInstallTask {
            game_version: self.game_version.clone(),
            loader_version: self.loader_version.clone(),
            launch_meta_url: self.launch_meta_url(),
        }))
    }
}
