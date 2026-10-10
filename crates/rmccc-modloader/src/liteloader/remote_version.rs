//! LiteLoader remote version.

use crate::install_task::InstallTask;
use crate::remote_version::{RemoteVersion, VersionType};
use chrono::{DateTime, Utc};
use rmccc_error::Result;

#[derive(Debug, Clone)]
pub struct LiteLoaderRemoteVersion {
    pub game_version: String,
    pub loader_version: String,
    pub url: String,
    pub libraries: Vec<String>,
}

impl RemoteVersion for LiteLoaderRemoteVersion {
    fn library_id(&self) -> &str {
        "liteloader"
    }
    fn game_version(&self) -> &str {
        &self.game_version
    }
    fn self_version(&self) -> &str {
        &self.loader_version
    }
    fn urls(&self) -> &[String] {
        std::slice::from_ref(&self.url)
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
    ) -> Result<Box<dyn InstallTask>> {
        Ok(Box::new(super::install_task::LiteLoaderInstallTask {
            game_version: self.game_version.clone(),
            loader_version: self.loader_version.clone(),
            download_url: self.url.clone(),
            libraries: self.libraries.clone(),
        }))
    }
}
