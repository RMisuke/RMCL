//! `profile` command handlers — game instance management.

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
            let profile = ctx
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
            Ok(profile_to_json(&profile))
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
            Some(p) => Ok(profile_to_json(&p)),
            None => Ok(json!({ "active": Value::Null })),
        },
    }
}

async fn list(ctx: &CliContext) -> CliResult<Value> {
    let profiles = ctx.profile_list().await?;
    let active = ctx
        .profile_active()
        .await?
        .map(|p| p.id)
        .unwrap_or_default();
    let items: Vec<Value> = profiles
        .iter()
        .map(|p| {
            let mut v = profile_to_json(p);
            v["active"] = json!(p.id == active);
            v
        })
        .collect();
    Ok(json!({ "profiles": items, "count": items.len() }))
}

async fn get(ctx: &CliContext, id: &str) -> CliResult<Value> {
    let profile = ctx.get_profile(id).await?;
    Ok(profile_to_json(&profile))
}

async fn create(
    ctx: &CliContext,
    out: &Output,
    name: &str,
    version: &str,
    loader: Option<&str>,
    memory: Option<u32>,
) -> CliResult<Value> {
    let profile = ctx.profile_create(name, version, loader, memory).await?;
    out.note(&format!(
        "Created instance '{}' (id: {})",
        profile.name, profile.id
    ));
    Ok(profile_to_json(&profile))
}

/// Convert a profile into the stable CLI JSON shape (the same fields the
/// frontend `Profile` TS type exposes, minus secrets).
pub fn profile_to_json(p: &rmccc_profile::Profile) -> Value {
    json!({
        "id": p.id,
        "name": p.name,
        "game_version": p.game_version,
        "mod_loader": p.mod_loader.as_ref().map(|ml| json!({
            "loader_type": ml.loader_type.library_id(),
            "loader_version": ml.loader_version,
        })),
        "java_path": p.java_path,
        "jvm_args": p.jvm_args,
        "game_args": p.game_args,
        "memory_mb": p.memory_mb,
        "min_memory_mb": p.min_memory_mb,
        "metaspace_mb": p.metaspace_mb,
        "process_priority": p.process_priority,
        "width": p.width,
        "height": p.height,
        "fullscreen": p.fullscreen,
        "use_native_glfw": p.use_native_glfw,
        "use_native_openal": p.use_native_openal,
        "use_custom_natives": p.use_custom_natives,
        "no_generated_jvm_args": p.no_generated_jvm_args,
        "no_generated_optimizing_jvm_args": p.no_generated_optimizing_jvm_args,
        "enable_debug_log_output": p.enable_debug_log_output,
        "wrapper": p.wrapper,
        "java_agents": p.java_agents,
        "override_jvm_args": p.override_jvm_args,
        "pre_launch_command": p.pre_launch_command,
        "post_exit_command": p.post_exit_command,
        "env_vars": p.env_vars,
        "game_dir": p.game_dir.display().to_string(),
        "created_at": p.created_at,
        "last_played": p.last_played,
        "icon": p.icon,
    })
}
