//! `launch` command handlers — run / status / stop / wait / java / dry-run.

use crate::app::LaunchAction;
use crate::context::{CliContext, LaunchStartOptions};
use crate::error::CliResult;
use crate::output::Output;
use rmccc_core::pipeline::{GameOutputCb, ProgressCb};
use rmccc_launch::ExitType;
use serde_json::{json, Value};
use std::sync::Arc;

pub async fn handle(ctx: &CliContext, out: &Output, action: LaunchAction) -> CliResult<Value> {
    match action {
        LaunchAction::Run {
            profile,
            no_wait,
            no_repair,
        } => run(ctx, out, &profile, no_wait, no_repair).await,
        LaunchAction::Status => status(ctx).await,
        LaunchAction::Stop { profile } => stop(ctx, &profile).await,
        LaunchAction::Wait { profile } => wait(ctx, &profile).await,
        LaunchAction::Java => java(ctx).await,
        LaunchAction::Command { profile } => ctx.launch_command(&profile).await.map_err(Into::into),
    }
}

async fn run(
    ctx: &CliContext,
    out: &Output,
    profile: &str,
    no_wait: bool,
    no_repair: bool,
) -> CliResult<Value> {
    let output_out = out.clone();
    let output: GameOutputCb = Arc::new(move |line| {
        output_out.event("game_output", json!({ "line": line }));
        output_out.note(line);
    });
    let progress_out = out.clone();
    let progress: ProgressCb = Arc::new(move |msg| progress_out.progress(msg));
    let exit_out = out.clone();
    let profile_for_exit = profile.to_string();
    let on_exit = Arc::new(move |exit_code: i32, exit_type: ExitType| {
        exit_out.event(
            "game_exited",
            json!({ "profile_id": profile_for_exit, "exit_code": exit_code, "exit_type": format!("{:?}", exit_type) }),
        );
    });

    let opts = LaunchStartOptions {
        no_repair,
        output: Some(output),
        progress: Some(progress),
        on_exit: Some(on_exit),
    };

    out.note(&format!("Launching profile '{profile}'..."));
    let pid = ctx.launch_start(profile, &opts).await?;
    out.note(&format!("Game started (pid {pid})"));

    let mut result = json!({
        "started": true,
        "profile_id": profile,
        "pid": pid,
    });

    if !no_wait {
        let exit_code = ctx.launch_wait(profile).await?;
        result["exited"] = json!(true);
        result["exit_code"] = json!(exit_code);
    }
    Ok(result)
}

async fn status(ctx: &CliContext) -> CliResult<Value> {
    let running = ctx.launch_status().await?;
    let items: Vec<Value> = running
        .iter()
        .map(|g| json!({ "profile_id": g.profile_id, "pid": g.pid }))
        .collect();
    Ok(json!({ "running": items, "count": items.len() }))
}

async fn stop(ctx: &CliContext, profile: &str) -> CliResult<Value> {
    ctx.launch_stop(profile).await?;
    Ok(json!({ "stopped": profile }))
}

async fn wait(ctx: &CliContext, profile: &str) -> CliResult<Value> {
    match ctx.launch_wait(profile).await? {
        Some(code) => Ok(json!({ "exited": true, "profile_id": profile, "exit_code": code })),
        None => Ok(json!({ "exited": false, "profile_id": profile })),
    }
}

async fn java(ctx: &CliContext) -> CliResult<Value> {
    let installations = ctx.launch_java().await?;
    let items: Vec<Value> = installations
        .iter()
        .map(|j| {
            json!({
                "path": j.path.display().to_string(),
                "major_version": j.major_version,
                "vendor": j.vendor,
                "is_64bit": j.is_64bit,
            })
        })
        .collect();
    Ok(json!({ "java": items, "count": items.len() }))
}
