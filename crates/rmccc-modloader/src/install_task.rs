//! Install task trait — async installation lifecycle.

use crate::GameInstancePatch;
use rmccc_error::Result;

#[async_trait::async_trait]
pub trait InstallTask: Send + Sync {
    async fn execute(&mut self) -> Result<GameInstancePatch>;

    async fn cleanup(&self) -> Result<()> {
        Ok(())
    }

    fn description(&self) -> &str {
        "Installing modloader"
    }
}
