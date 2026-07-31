//! 启动命令处理。

use crate::context::CliContext;
use crate::LaunchAction;

pub async fn handle(ctx: CliContext, action: LaunchAction) -> anyhow::Result<()> {
    match action {
        LaunchAction::Run { id } => run(ctx, id).await,
    }
}

async fn run(ctx: CliContext, id: String) -> anyhow::Result<()> {
    // 验证实例是否存在
    let profiles = ctx.instance_list().await?;
    let profile = profiles.iter().find(|p| p.id == id);

    match profile {
        Some(p) => {
            println!("正在启动 '{}' (Minecraft {})...", p.name, p.game_version);
            println!("游戏输出如下:");
            println!("{}", "=".repeat(60));
        }
        None => {
            anyhow::bail!("未找到实例 '{}'，请先用 'rmcli instance create' 创建一个", id);
        }
    }

    ctx.launch_run(&id).await?;

    println!("{}", "=".repeat(60));
    println!("游戏已退出。");

    Ok(())
}
