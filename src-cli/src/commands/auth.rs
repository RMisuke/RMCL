//! 认证命令处理。

use crate::context::CliContext;
use crate::AuthAction;

pub async fn handle(ctx: CliContext, action: AuthAction) -> anyhow::Result<()> {
    match action {
        AuthAction::Login => login(ctx).await,
        AuthAction::Logout => logout(ctx).await,
        AuthAction::Refresh => refresh(ctx).await,
        AuthAction::Status => status(ctx).await,
    }
}

async fn login(ctx: CliContext) -> anyhow::Result<()> {
    use std::io::{self, Write};

    println!("登录方式:");
    println!("  1. Microsoft 正版登录（推荐）");
    println!("  2. 离线登录");
    print!("请选择 [1]: ");
    io::stdout().flush()?;

    let mut input = String::new();
    io::stdin().read_line(&mut input)?;
    let choice = input.trim();

    match choice {
        "2" => {
            print!("用户名: ");
            io::stdout().flush()?;
            let mut username = String::new();
            io::stdin().read_line(&mut username)?;
            let username = username.trim();
            if username.is_empty() {
                anyhow::bail!("用户名不能为空");
            }
            let session = ctx.auth_login_offline(username).await?;
            println!("✓ 已以离线模式登录: '{}'", session.username);
            println!("  UUID: {}", session.uuid);
        }
        _ => {
            println!("正在启动 Microsoft OAuth2 设备码登录流程...");
            println!("浏览器将自动打开，请按照页面提示操作。");
            let session = ctx.auth_login_microsoft().await?;
            println!("✓ 已以 Microsoft 账户登录: '{}'", session.username);
            println!("  UUID: {}", session.uuid);
        }
    }

    Ok(())
}

async fn logout(ctx: CliContext) -> anyhow::Result<()> {
    ctx.auth_logout().await?;
    println!("✓ 已成功登出");
    Ok(())
}

async fn refresh(ctx: CliContext) -> anyhow::Result<()> {
    match ctx.auth_refresh().await? {
        Some(session) => {
            println!("✓ 令牌已刷新: '{}'", session.username);
        }
        None => {
            println!("✗ 没有可刷新的活跃会话");
        }
    }
    Ok(())
}

async fn status(ctx: CliContext) -> anyhow::Result<()> {
    match ctx.auth_get_session().await? {
        Some(session) => {
            println!("登录方式:  {}", session.provider);
            println!("用户名:    {}", session.username);
            println!("UUID:      {}", session.uuid);
            if let Some(exp) = session.expires_at {
                let now = chrono::Utc::now();
                if exp > now {
                    let remaining = exp - now;
                    println!(
                        "过期时间:  {} (剩余 {} 小时 {} 分钟)",
                        exp,
                        remaining.num_hours(),
                        remaining.num_minutes() % 60
                    );
                } else {
                    println!("过期时间:  {} (已过期)", exp);
                }
            } else {
                println!("过期时间:  永不过期（离线模式）");
            }
        }
        None => {
            println!("未登录");
        }
    }
    Ok(())
}
