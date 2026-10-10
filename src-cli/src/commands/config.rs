//! `config` command handlers — launcher settings, runtime state, user prefs.

use crate::app::{ConfigAction, StateAction, UserAction};
use crate::context::CliContext;
use crate::error::CliResult;
use crate::output::Output;
use serde_json::Value;

pub async fn handle(ctx: &CliContext, _out: &Output, action: ConfigAction) -> CliResult<Value> {
    match action {
        ConfigAction::Get { key } => ctx.config_get(key.as_deref()).await.map_err(Into::into),
        ConfigAction::Set { field, value } => {
            ctx.config_set(&field, &value).await.map_err(Into::into)
        }
        ConfigAction::Path => ctx.config_paths().await.map_err(Into::into),
        ConfigAction::State { action } => handle_state(ctx, action).await,
        ConfigAction::User { action } => handle_user(ctx, action).await,
    }
}

async fn handle_state(ctx: &CliContext, action: StateAction) -> CliResult<Value> {
    match action {
        StateAction::Get { key } => ctx.state_get(key.as_deref()).await.map_err(Into::into),
        StateAction::Set { field, value } => {
            ctx.state_set(&field, &value).await.map_err(Into::into)
        }
    }
}

async fn handle_user(ctx: &CliContext, action: UserAction) -> CliResult<Value> {
    match action {
        UserAction::Get { key } => ctx.user_get(key.as_deref()).await.map_err(Into::into),
        UserAction::Set { field, value } => ctx.user_set(&field, &value).await.map_err(Into::into),
    }
}
