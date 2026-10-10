//! `version` command handlers — remote manifest + local repository.

use crate::app::{LocalVersionAction, VersionAction};
use crate::context::CliContext;
use crate::error::CliResult;
use crate::output::Output;
use rmccc_core::pipeline::ProgressCb;
use serde_json::{json, Value};
use std::sync::Arc;

pub async fn handle(ctx: &CliContext, out: &Output, action: VersionAction) -> CliResult<Value> {
    match action {
        VersionAction::List { refresh } => list(ctx, refresh).await,
        VersionAction::Get { id } => {
            let v = ctx.version_get(&id).await?;
            Ok(version_to_json(&v))
        }
        VersionAction::Install { profile, version } => {
            let out_cb = out.clone();
            let progress: ProgressCb = Arc::new(move |msg| out_cb.progress(msg));
            ctx.version_install(&profile, &version, Some(&progress))
                .await?;
            Ok(json!({ "installed": version, "profile": profile }))
        }
        VersionAction::Local { action } => handle_local(ctx, action).await,
    }
}

async fn handle_local(ctx: &CliContext, action: LocalVersionAction) -> CliResult<Value> {
    match action {
        LocalVersionAction::List => {
            let items = ctx.version_local_list().await?;
            Ok(json!({ "versions": items, "count": items.len() }))
        }
        LocalVersionAction::Get { id } => {
            let v = ctx.version_local_get(&id).await?;
            Ok(version_to_json(&v))
        }
        LocalVersionAction::Delete { id } => {
            ctx.version_local_delete(&id).await?;
            Ok(json!({ "deleted": id }))
        }
    }
}

async fn list(ctx: &CliContext, refresh: bool) -> CliResult<Value> {
    let versions = ctx.version_list(refresh).await?;
    let items: Vec<Value> = versions
        .iter()
        .map(|v| {
            json!({
                "id": v.id,
                "release_type": format!("{:?}", v.release_type).to_ascii_lowercase(),
                "release_time": v.release_time,
                "url": v.url,
            })
        })
        .collect();
    Ok(json!({ "versions": items, "count": items.len() }))
}

/// Convert a GameVersion into the stable CLI JSON shape (library paths kept
/// minimal to bound payload size).
pub fn version_to_json(v: &rmccc_version::GameVersion) -> Value {
    json!({
        "id": v.id,
        "release_type": format!("{:?}", v.release_type).to_ascii_lowercase(),
        "main_class": v.main_class,
        "inherits_from": v.inherits_from,
        "asset_index": v.asset_index.id,
        "java_version": v.java_version.as_ref().map(|j| j.major_version),
        "library_count": v.libraries.len(),
        "patch_count": v.patches.len(),
        "downloads": v.downloads.keys().cloned().collect::<Vec<_>>(),
        "release_time": v.release_time,
        "time": v.time,
    })
}
