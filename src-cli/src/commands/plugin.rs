//! `plugin` command handlers — declarative plugin management (Phase 2).

use crate::app::{PluginAction, PluginConfigAction};
use crate::context::CliContext;
use crate::error::CliResult;
use crate::output::Output;
use serde_json::Value;

pub async fn handle(ctx: &CliContext, out: &Output, action: PluginAction) -> CliResult<Value> {
    match action {
        PluginAction::List { all } => ctx.plugin_list(all).await.map_err(Into::into),
        PluginAction::Inspect { id } => ctx.plugin_inspect(&id).await.map_err(Into::into),
        PluginAction::Install { path, enable } => {
            let value = ctx.plugin_install(&path, enable).await?;
            if value
                .get("requires_review")
                .and_then(|v| v.as_bool())
                .unwrap_or(false)
            {
                out.note(
                    "该插件请求敏感权限：安装后保持 disabled，请用 `plugin grant` 审查后再 `plugin enable`。",
                );
            }
            Ok(value)
        }
        PluginAction::Enable { id } => ctx.plugin_enable(&id).await.map_err(Into::into),
        PluginAction::Disable { id } => ctx.plugin_disable(&id).await.map_err(Into::into),
        PluginAction::Remove { id, delete_data } => {
            let value = ctx.plugin_remove(&id, delete_data).await?;
            if !delete_data {
                out.note("已保留 plugin-data/<id>/ 下的用户配置（删除请加 --delete-data）。");
            }
            Ok(value)
        }
        PluginAction::Rollback { id } => ctx.plugin_rollback(&id).await.map_err(Into::into),
        PluginAction::Grant { id, capability } => {
            ctx.plugin_grant(&id, &capability).await.map_err(Into::into)
        }
        PluginAction::Revoke { id, capability } => ctx
            .plugin_revoke(&id, &capability)
            .await
            .map_err(Into::into),
        PluginAction::Config { action } => match action {
            PluginConfigAction::Get { id } => ctx.plugin_config_get(&id).await.map_err(Into::into),
            PluginConfigAction::Set { id, values } => ctx
                .plugin_config_set(&id, &values)
                .await
                .map_err(Into::into),
            PluginConfigAction::Reset { id } => {
                ctx.plugin_config_reset(&id).await.map_err(Into::into)
            }
            PluginConfigAction::Path { id } => {
                ctx.plugin_config_path(&id).await.map_err(Into::into)
            }
        },
        PluginAction::Capabilities => ctx.plugin_capabilities().await.map_err(Into::into),
        PluginAction::Pack { dir, out: out_path } => {
            let value = ctx.plugin_pack(&dir, &out_path).await?;
            out.note(&format!("已生成插件包：{out_path}"));
            Ok(value)
        }
    }
}
