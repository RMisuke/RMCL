//! `auth` command handlers — Microsoft OAuth2 / offline account management.

use crate::app::AuthAction;
use crate::context::CliContext;
use crate::error::{CliError, CliResult};
use crate::output::Output;
use rmccc_error::ErrorCode;
use serde_json::{json, Value};

pub async fn handle(ctx: &CliContext, out: &Output, action: AuthAction) -> CliResult<Value> {
    match action {
        AuthAction::Login { ty, username } => login(ctx, out, &ty, username.as_deref()).await,
        AuthAction::Logout { account } => logout(ctx, account.as_deref()).await,
        AuthAction::Refresh { account } => refresh(ctx, account.as_deref()).await,
        AuthAction::Status => status(ctx).await,
        AuthAction::List => list(ctx).await,
        AuthAction::Select { account } => select(ctx, &account).await,
    }
}

async fn login(
    ctx: &CliContext,
    out: &Output,
    ty: &str,
    username: Option<&str>,
) -> CliResult<Value> {
    match ty.to_ascii_lowercase().as_str() {
        "offline" => {
            let name = username.filter(|n| !n.trim().is_empty()).ok_or_else(|| {
                CliError::new(
                    ErrorCode::ValidationError,
                    "offline login requires --username <name>",
                )
            })?;
            let session = ctx.auth_login_offline(name).await?;
            out.note(&format!("Logged in as offline player '{name}'"));
            Ok(json!({
                "provider": session.provider,
                "username": session.username,
                "uuid": session.uuid,
                "account_id": session.account_id,
            }))
        }
        "microsoft" => {
            out.note("Starting Microsoft OAuth2 device-code flow...");
            let progress_out = out.clone();
            let progress: rmccc_auth::ProgressCallback = Box::new(move |p| {
                if let rmccc_auth::AuthProgress::DeviceCode {
                    user_code,
                    verification_uri,
                } = p
                {
                    progress_out.note(&format!(
                        "Open {} and enter the code {}",
                        verification_uri, user_code
                    ));
                }
            });
            let session = ctx.auth_login_microsoft(Some(progress)).await?;
            out.note(&format!(
                "Logged in as Microsoft account '{}'",
                session.username
            ));
            Ok(json!({
                "provider": session.provider,
                "username": session.username,
                "uuid": session.uuid,
                "account_id": session.account_id,
                "expires_at": session.expires_at,
            }))
        }
        other => Err(CliError::new(
            ErrorCode::ValidationError,
            format!("Unknown login type '{other}' (expected 'microsoft' or 'offline')"),
        )),
    }
}

async fn logout(ctx: &CliContext, account: Option<&str>) -> CliResult<Value> {
    match account {
        Some(id) => {
            ctx.auth_logout_account(id).await?;
            Ok(json!({ "logged_out": id }))
        }
        None => {
            let active = ctx.auth_status().await?;
            match active {
                Some(s) => {
                    ctx.auth_logout().await?;
                    Ok(json!({ "logged_out": s.account_id, "username": s.username }))
                }
                None => Ok(json!({ "logged_out": Value::Null, "note": "no active account" })),
            }
        }
    }
}

async fn refresh(ctx: &CliContext, account: Option<&str>) -> CliResult<Value> {
    let session = match account {
        Some(id) => ctx.auth_refresh_account(id).await?,
        None => ctx.auth_refresh().await?,
    };
    match session {
        Some(s) => Ok(json!({
            "refreshed": true,
            "provider": s.provider,
            "username": s.username,
            "account_id": s.account_id,
            "expires_at": s.expires_at,
        })),
        None => Ok(json!({ "refreshed": false, "reason": "no_active_session" })),
    }
}

async fn status(ctx: &CliContext) -> CliResult<Value> {
    match ctx.auth_status().await? {
        Some(s) => Ok(json!({
            "logged_in": true,
            "provider": s.provider,
            "username": s.username,
            "uuid": s.uuid,
            "account_id": s.account_id,
            "expires_at": s.expires_at,
        })),
        None => Ok(json!({ "logged_in": false })),
    }
}

async fn list(ctx: &CliContext) -> CliResult<Value> {
    let accounts = ctx.auth_list().await?;
    let active = ctx
        .auth_status()
        .await?
        .map(|s| s.account_id)
        .unwrap_or_default();
    let items: Vec<Value> = accounts
        .iter()
        .map(|m| {
            json!({
                "account_id": m.account_id,
                "provider": m.r#type,
                "username": m.profile_name,
                "uuid": m.profile_id,
                "active": m.account_id == active,
            })
        })
        .collect();
    Ok(json!({ "accounts": items, "count": items.len() }))
}

async fn select(ctx: &CliContext, account: &str) -> CliResult<Value> {
    ctx.auth_select(account).await?;
    Ok(json!({ "selected": account }))
}
