//! `resource` command handlers — install / check / repair / verify game files.

use crate::app::ResourceAction;
use crate::context::CliContext;
use crate::error::CliResult;
use crate::output::Output;
use rmccc_core::pipeline::ProgressCb;
use serde_json::{json, Value};
use std::sync::Arc;

pub async fn handle(ctx: &CliContext, out: &Output, action: ResourceAction) -> CliResult<Value> {
    match action {
        ResourceAction::Install { profile, version } => {
            let out_cb = out.clone();
            let progress: ProgressCb = Arc::new(move |msg| out_cb.progress(msg));
            ctx.resource_install(&profile, &version, Some(&progress)).await?;
            Ok(json!({ "installed": true, "profile": profile, "version": version }))
        }
        ResourceAction::Check { profile } => ctx.resource_check(&profile).await.map_err(Into::into),
        ResourceAction::Repair { profile } => {
            let out_cb = out.clone();
            let progress: ProgressCb = Arc::new(move |msg| out_cb.progress(msg));
            let repaired = ctx.resource_repair(&profile, Some(&progress)).await?;
            Ok(json!({ "repaired": repaired, "profile": profile }))
        }
        ResourceAction::Verify { profile } => {
            ctx.resource_verify(&profile).await.map_err(Into::into)
        }
        ResourceAction::Status => download_status(ctx).await,
    }
}

async fn download_status(ctx: &CliContext) -> CliResult<Value> {
    let core = ctx.core.read().await;
    let concurrency = core.config.download_concurrency;
    let mirror = core.config.mirror_url.clone();
    Ok(json!({
        "download_concurrency": concurrency,
        "mirror_url": mirror,
        "provider": if mirror.as_deref() == Some("bmclapi") { "bmclapi" } else { "mojang" },
        "cache_dir": ctx.data_dir.join("cache").display().to_string(),
    }))
}
