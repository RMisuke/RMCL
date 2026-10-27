//! `profile` command handlers — instance management over version-backed keys.

use crate::app::ProfileAction;
use crate::context::CliContext;
use crate::error::CliResult;
use crate::output::Output;
use serde_json::{json, Value};

pub async fn handle(ctx: &CliContext, out: &Output, action: ProfileAction) -> CliResult<Value> {
    match action {
        ProfileAction::List => list(ctx).await,
        ProfileAction::Get { id } => get(ctx, &id).await,
        ProfileAction::Create {
            name,
            version,
            loader,
            memory,
        } => create(ctx, out, &name, &version, loader.as_deref(), memory).await,
        ProfileAction::Update {
            id,
            name,
            version,
            java_path,
            memory,
            icon,
            jvm_args,
            game_args,
        } => {
            let detail = ctx
                .profile_update(
                    &id,
                    name.as_deref(),
                    version.as_deref(),
                    java_path.as_deref(),
                    jvm_args,
                    game_args,
                    memory,
                    icon.as_deref(),
                )
                .await?;
            Ok(detail_to_json(&detail))
        }
        ProfileAction::Delete { id, cleanup } => {
            ctx.profile_delete(&id, cleanup).await?;
            Ok(json!({ "deleted": id, "cleanup": cleanup }))
        }
        ProfileAction::Select { id } => {
            ctx.profile_select(&id).await?;
            Ok(json!({ "selected": id }))
        }
        ProfileAction::Active => match ctx.profile_active().await? {
            Some(detail) => Ok(detail_to_json(&detail)),
            None => Ok(json!({ "active": Value::Null })),
        },
    }
}

async fn list(ctx: &CliContext) -> CliResult<Value> {
    let instances = ctx.profile_list().await?;
    let active = ctx
        .profile_active()
        .await?
        .map(|detail| detail.summary.key)
        .unwrap_or_default();
    let items: Vec<Value> = instances
        .iter()
        .map(|item| {
            let mut value = json!({
                "key": item.key,
                "id": item.id,
                "directory_id": item.directory_id,
                "name": item.display_name,
                "game_version": item.game_version,
                "loader": item.loader,
                "game_dir": item.path.parent().and_then(|p| p.parent()).map(|p| p.display().to_string()),
                "run_dir": item.run_dir.display().to_string(),
                "last_played": item.last_played,
                "play_time_seconds": item.play_time_seconds,
            });
            value["active"] = json!(item.key == active);
            value
        })
        .collect();
    Ok(json!({ "instances": items, "count": items.len() }))
}

async fn get(ctx: &CliContext, key: &str) -> CliResult<Value> {
    let detail = ctx.get_instance(key).await?;
    Ok(detail_to_json(&detail))
}

async fn create(
    ctx: &CliContext,
    out: &Output,
    name: &str,
    version: &str,
    _loader: Option<&str>,
    memory: Option<u32>,
) -> CliResult<Value> {
    let detail = ctx.profile_create(name, version, None, memory).await?;
    out.note(&format!(
        "Created instance '{}' from installed version '{}'",
        detail.summary.display_name, detail.summary.id
    ));
    Ok(detail_to_json(&detail))
}

fn detail_to_json(detail: &rmccc_instance::InstanceDetail) -> Value {
    let summary = &detail.summary;
    json!({
        "key": summary.key,
        "id": summary.id,
        "directory_id": summary.directory_id,
        "name": summary.display_name,
        "game_version": summary.game_version,
        "loader": summary.loader,
        "game_dir": detail.paths.game_dir.display().to_string(),
        "instance_dir": detail.paths.instance_dir.display().to_string(),
        "run_dir": detail.paths.run_dir.display().to_string(),
        "mods_dir": detail.paths.mods_dir.display().to_string(),
        "created_at": summary.created_at,
        "last_played": summary.last_played,
        "play_time_seconds": summary.play_time_seconds,
        "settings": detail.settings,
        "effective": detail.effective,
        "manifest": detail.manifest,
    })
}
