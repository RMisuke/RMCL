//! 实例（Profile）命令处理。

use crate::context::CliContext;
use crate::InstanceAction;

pub async fn handle(ctx: CliContext, action: InstanceAction) -> anyhow::Result<()> {
    match action {
        InstanceAction::Create {
            name,
            version,
            loader,
        } => create(ctx, name, version, loader).await,
        InstanceAction::List => list(ctx).await,
        InstanceAction::Delete { id } => delete(ctx, id).await,
    }
}

async fn create(
    ctx: CliContext,
    name: String,
    version: Option<String>,
    loader: Option<String>,
) -> anyhow::Result<()> {
    let profile = ctx
        .instance_create(&name, version.as_deref(), loader.as_deref())
        .await?;

    println!("✓ 实例创建成功:");
    println!("  ID:             {}", profile.id);
    println!("  名称:           {}", profile.name);
    println!("  游戏版本:       {}", profile.game_version);
    if let Some(ref ml) = profile.mod_loader {
        println!(
            "  模组加载器:     {} ({})",
            format!("{:?}", ml.loader_type).to_lowercase(),
            if ml.loader_version.is_empty() {
                "未安装"
            } else {
                &ml.loader_version
            }
        );
    }
    println!("  内存:           {} MB", profile.memory_mb);
    println!("  游戏目录:       {}", profile.game_dir.display());
    println!();
    println!("提示: 运行 'rmcli resource download -i {} -v {}' 来下载游戏文件",
             profile.id, profile.game_version);

    Ok(())
}

async fn list(ctx: CliContext) -> anyhow::Result<()> {
    let profiles = ctx.instance_list().await?;

    if profiles.is_empty() {
        println!("暂无游戏实例。");
        println!("创建一个: rmcli instance create -n <名称>");
        return Ok(());
    }

    println!("游戏实例列表:");
    println!("{:<38} {:<20} {:<15} {:<12}", "ID", "名称", "版本", "最后游玩");
    println!("{}", "-".repeat(90));

    for p in &profiles {
        let last_played = p
            .last_played
            .map(|t| {
                let d = t.format("%Y-%m-%d").to_string();
                d
            })
            .unwrap_or_else(|| "从未".to_string());

        let loader_info = match &p.mod_loader {
            Some(ml) => ml.loader_type.clone(),
            None => "原版".to_string(),
        };

        println!(
            "{:<38} {:<20} {:<15} {:<12}",
            p.id, p.name, format!("{} ({})", p.game_version, loader_info), last_played
        );
    }

    Ok(())
}

async fn delete(ctx: CliContext, id: String) -> anyhow::Result<()> {
    // 确认删除
    use std::io::{self, Write};

    let profiles = ctx.instance_list().await?;
    let profile = profiles.iter().find(|p| p.id == id);

    match profile {
        Some(p) => {
            print!(
                "确认删除实例 '{}' ({} v{})? [y/N]: ",
                p.name, p.id, p.game_version
            );
            io::stdout().flush()?;

            let mut answer = String::new();
            io::stdin().read_line(&mut answer)?;
            let answer = answer.trim().to_lowercase();

            if answer == "y" || answer == "yes" {
                ctx.instance_delete(&id, true).await?;
                println!("✓ 实例 '{}' 已删除", p.name);
            } else {
                println!("已取消");
            }
        }
        None => {
            anyhow::bail!("未找到实例 '{}'", id);
        }
    }

    Ok(())
}
