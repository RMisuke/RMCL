//! Cleanroom remote version (stub).

use crate::install_task::InstallTask;
use crate::remote_version::{RemoteVersion, VersionType};
use chrono::{DateTime, Utc};
use rmccc_error::Result;

#[derive(Debug, Clone)]
pub struct CleanroomRemoteVersion;

impl RemoteVersion for CleanroomRemoteVersion {
    fn library_id(&self) -> &str {
        "cleanroom"
    }
    fn game_version(&self) -> &str {
        ""
    }
    fn self_version(&self) -> &str {
        ""
    }
    fn urls(&self) -> &[String] {
        &[]
    }
    fn release_date(&self) -> Option<DateTime<Utc>> {
        None
    }
    fn version_type(&self) -> VersionType {
        VersionType::Uncategorized
    }

    fn create_install_task(
        &self,
        _dm: &super::super::DependencyManager,
        _manifest: &rmccc_version::GameVersion,
        _ctx: &super::super::remote_version::InstallTaskContext<'_>,
    ) -> Result<Box<dyn InstallTask>> {
        Err(rmccc_error::RmcccError::Other(
            "Cleanroom install not yet implemented".into(),
        ))
    }
}
