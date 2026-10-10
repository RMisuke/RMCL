//! `mod` command handlers — Modrinth search / install / local file management.

use crate::app::ModAction;
use crate::context::CliContext;
use crate::error::{CliError, CliResult};
use crate::output::Output;
use rmccc_error::ErrorCode;
use serde_json::{json, Value};

pub async fn handle(ctx: &CliContext, out: &Output, action: ModAction) -> CliResult<Value> {
    match action {
        ModAction::Search {
            query,
            game_version,
            loader,
            page,
            limit,
        } => {
            let result = ctx
                .mod_search(
                    &query,
                    game_version.as_deref(),
                    loader.as_deref(),
                    page.unwrap_or(1),
                    limit.unwrap_or(20),
                )
                .await?;
            let hits: Vec<Value> = result
                .hits
                .iter()
                .map(|m| {
                    json!({
                        "id": m.id,
                        "source": m.source,
                        "name": m.name,
                        "slug": m.slug,
                        "authors": m.authors,
                        "description": m.description,
                        "download_count": m.download_count,
                        "icon_url": m.icon_url,
                        "categories": m.categories,
                    })
                })
                .collect();
            Ok(json!({
                "total": result.total,
                "page": result.page,
                "hits": hits,
                "count": hits.len(),
            }))
        }
        ModAction::Versions { id } => {
            let versions = ctx.mod_versions(&id).await?;
            let items: Vec<Value> = versions
                .iter()
                .map(|v| {
                    json!({
                        "id": v.id,
                        "name": v.name,
                        "version_number": v.version_number,
                        "game_versions": v.game_versions,
                        "loaders": v.loaders,
                        "download_url": v.download_url,
                        "filename": v.filename,
                    })
                })
                .collect();
            Ok(json!({ "mod_id": id, "versions": items, "count": items.len() }))
        }
        ModAction::Install {
            profile,
            mod_id,
            version_id,
            file,
        } => {
            install(
                ctx,
                out,
                &profile,
                mod_id.as_deref(),
                version_id.as_deref(),
                file.as_deref(),
            )
            .await
        }
        ModAction::List { profile } => ctx.mod_list(&profile).await.map_err(Into::into),
        ModAction::Remove { profile, file } => {
            ctx.mod_remove(&profile, &file).await?;
            Ok(json!({ "removed": file, "profile": profile }))
        }
        ModAction::Toggle {
            profile,
            file,
            enabled,
        } => {
            ctx.mod_toggle(&profile, &file, enabled).await?;
            Ok(json!({
                "toggled": file,
                "profile": profile,
                "enabled": enabled,
            }))
        }
        ModAction::DownloadUrl { version_id } => {
            let url = ctx.mod_download_url(&version_id).await?;
            Ok(json!({ "version_id": version_id, "url": url }))
        }
    }
}

async fn install(
    ctx: &CliContext,
    out: &Output,
    profile: &str,
    mod_id: Option<&str>,
    version_id: Option<&str>,
    file: Option<&str>,
) -> CliResult<Value> {
    match (mod_id, version_id, file) {
        (Some(mid), Some(vid), None) => {
            // Resolve the version metadata from the mod, find the matching id.
            let versions = ctx.mod_versions(mid).await?;
            let version = versions.into_iter().find(|v| v.id == vid).ok_or_else(|| {
                CliError::new(
                    ErrorCode::NotFound,
                    format!("No version '{vid}' found for mod '{mid}'"),
                )
            })?;
            let filename = ctx.mod_install(profile, version).await?;
            out.note(&format!(
                "Installed '{}' into profile '{}'",
                filename, profile
            ));
            Ok(json!({ "installed": filename, "profile": profile, "source": "modrinth" }))
        }
        (None, None, Some(path)) => {
            let filename = ctx.mod_install_local(profile, path).await?;
            out.note(&format!("Installed local mod '{}'", filename));
            Ok(json!({ "installed": filename, "profile": profile, "source": "local" }))
        }
        _ => Err(CliError::new(
            ErrorCode::ValidationError,
            "mod install requires BOTH --mod-id and --version-id, or --file <path>",
        )),
    }
}
