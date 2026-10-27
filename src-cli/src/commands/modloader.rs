//! `modloader` command handlers — discovery, install, remove, analyze.

use crate::app::ModloaderAction;
use crate::context::CliContext;
use crate::error::{CliError, CliResult};
use crate::output::Output;
use rmccc_error::ErrorCode;
use rmccc_modloader::ModLoaderType;
use serde_json::{json, Value};

pub async fn handle(ctx: &CliContext, out: &Output, action: ModloaderAction) -> CliResult<Value> {
    match action {
        ModloaderAction::List { ty, version } => list(ctx, &ty, &version).await,
        ModloaderAction::Install {
            profile,
            ty,
            version,
        } => install(ctx, out, &profile, &ty, &version).await,
        ModloaderAction::Remove { profile } => remove(ctx, &profile).await,
        ModloaderAction::Analyze { version } => analyze(ctx, &version).await,
    }
}

fn parse_loader_type(ty: &str) -> CliResult<ModLoaderType> {
    ModLoaderType::from_str(ty).ok_or_else(|| {
        CliError::new(
            ErrorCode::ValidationError,
            format!(
                "Unknown modloader type '{ty}'. Supported: {}",
                ModLoaderType::all()
                    .iter()
                    .map(|t| t.library_id())
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
        )
    })
}

async fn list(ctx: &CliContext, ty: &str, game_version: &str) -> CliResult<Value> {
    let lt = parse_loader_type(ty)?;
    let versions = ctx.modloader_list(&lt, game_version).await?;
    let items: Vec<Value> = versions
        .iter()
        .map(|v| json!({ "version": v.version, "stable": v.stable }))
        .collect();
    Ok(json!({
        "loader": lt.library_id(),
        "game_version": game_version,
        "versions": items,
        "count": items.len(),
    }))
}

async fn install(
    ctx: &CliContext,
    out: &Output,
    profile: &str,
    ty: &str,
    loader_version: &str,
) -> CliResult<Value> {
    let lt = parse_loader_type(ty)?;
    let progress_out = out.clone();
    let progress: rmccc_modloader::ProgressCallback =
        std::sync::Arc::new(move |stage: &str, done: u32, total: u32| {
            progress_out.note(&format!("  {stage} ({done}/{total})"));
        });
    out.note(&format!(
        "Installing {} {} into profile '{}'...",
        lt.display_name(),
        loader_version,
        profile
    ));
    let version_id = ctx
        .modloader_install(profile, &lt, loader_version, Some(progress))
        .await?;
    Ok(json!({
        "installed": true,
        "profile": profile,
        "loader": lt.library_id(),
        "loader_version": loader_version,
        "version_id": version_id,
    }))
}

async fn remove(ctx: &CliContext, profile: &str) -> CliResult<Value> {
    let updated = ctx.modloader_remove(profile).await?;
    Ok(json!({
        "removed": true,
        "profile": profile,
        "game_version": updated.summary.game_version,
    }))
}

async fn analyze(ctx: &CliContext, version: &str) -> CliResult<Value> {
    ctx.modloader_analyze(version).await.map_err(Into::into)
}
