//! 配置命令处理。

use crate::context::CliContext;
use crate::ConfigAction;

pub async fn handle(ctx: CliContext, action: ConfigAction) -> anyhow::Result<()> {
    match action {
        ConfigAction::Get { key } => get(ctx, key).await,
        ConfigAction::Set { key, value } => set(ctx, key, value).await,
    }
}

async fn get(ctx: CliContext, key: Option<String>) -> anyhow::Result<()> {
    let config = ctx.config_get(key.as_deref()).await?;

    match key {
        Some(k) => {
            println!("{} = {}", k, serde_json::to_string_pretty(&config)?);
        }
        None => {
            println!("{}", serde_json::to_string_pretty(&config)?);
        }
    }

    Ok(())
}

async fn set(ctx: CliContext, key: String, value: String) -> anyhow::Result<()> {
    ctx.config_set(&key, &value).await?;
    println!("✓ 已设置 {} = {}", key, value);
    Ok(())
}
