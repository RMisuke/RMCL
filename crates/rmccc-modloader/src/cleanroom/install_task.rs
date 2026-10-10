//! Cleanroom install task (stub).

use crate::install_task::InstallTask;
use crate::patch::GameInstancePatch;
use rmccc_error::Result;

pub struct CleanroomInstallTask;

#[async_trait::async_trait]
impl InstallTask for CleanroomInstallTask {
    async fn execute(&mut self) -> Result<GameInstancePatch> {
        Err(rmccc_error::RmcccError::Other(
            "Cleanroom install not yet implemented".into(),
        ))
    }
    fn description(&self) -> &str {
        "Installing Cleanroom (unimplemented)"
    }
}
