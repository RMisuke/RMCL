//! `system` command handlers — launcher-wide diagnostics.

use crate::app::SystemAction;
use crate::context::CliContext;
use crate::error::CliResult;
use crate::output::Output;
use serde_json::Value;

pub async fn handle(ctx: &CliContext, _out: &Output, action: SystemAction) -> CliResult<Value> {
    match action {
        SystemAction::Info => ctx.system_info().await.map_err(Into::into),
        SystemAction::Paths => ctx.config_paths().await.map_err(Into::into),
    }
}
