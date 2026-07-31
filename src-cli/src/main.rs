//! RMCCC CLI — Minecraft 启动器命令行工具。
//!
//! 子命令:
//! - `auth login/logout/refresh/status`   认证管理
//! - `instance create/list/delete`         游戏实例管理
//! - `resource check/download/verify`      资源与库文件校验
//! - `launch run`                          启动游戏实例
//! - `config get/set`                      配置管理
//!
//! 不带子命令启动时自动进入交互式 REPL 模式。

mod commands;
mod context;
mod repl;

use clap::{Parser, Subcommand};
use context::CliContext;
use std::io::IsTerminal;

/// RMCCC — Rust Minecraft 核心客户端启动器 (CLI)
#[derive(Parser)]
#[command(name = "rmcli")]
#[command(version = "0.1.0")]
#[command(about = "Minecraft 启动器命令行工具", long_about = None)]
pub(crate) struct Cli {
    /// 覆盖数据目录（默认使用平台相关 RMCL 目录）
    #[arg(short, long)]
    data_dir: Option<String>,

    /// 强制进入交互模式
    #[arg(short = 'i', long)]
    interactive: bool,

    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
pub(crate) enum Commands {
    /// 认证：Microsoft OAuth2 或离线登录
    Auth {
        #[command(subcommand)]
        action: AuthAction,
    },
    /// 游戏实例管理
    Instance {
        #[command(subcommand)]
        action: InstanceAction,
    },
    /// 资源：检查、下载、校验游戏文件
    Resource {
        #[command(subcommand)]
        action: ResourceAction,
    },
    /// 启动游戏实例
    Launch {
        #[command(subcommand)]
        action: LaunchAction,
    },
    /// 配置管理
    Config {
        #[command(subcommand)]
        action: ConfigAction,
    },
    /// 模组加载器：查看版本、安装 Fabric/Forge/NeoForge
    Modloader {
        #[command(subcommand)]
        action: ModloaderAction,
    },
}

#[derive(Subcommand)]
pub(crate) enum AuthAction {
    /// 使用 Microsoft OAuth2 登录（设备码流程）
    Login,
    /// 登出并清除会话
    Logout,
    /// 刷新过期令牌
    Refresh,
    /// 查看当前认证状态
    Status,
}

#[derive(Subcommand)]
pub(crate) enum InstanceAction {
    /// 创建新的游戏实例
    Create {
        /// 实例名称
        #[arg(short, long)]
        name: String,
        /// Minecraft 版本（例如 "1.20.4"）
        #[arg(short, long)]
        version: Option<String>,
        /// 模组加载器：fabric, forge, neoforge
        #[arg(short, long)]
        loader: Option<String>,
    },
    /// 列出所有实例
    List,
    /// 删除一个实例
    Delete {
        /// 实例 ID
        #[arg(short, long)]
        id: String,
    },
}

#[derive(Subcommand)]
pub(crate) enum ResourceAction {
    /// 检查游戏文件完整性
    Check {
        /// 实例 ID
        #[arg(short, long)]
        id: String,
    },
    /// 下载缺失或损坏的游戏文件
    Download {
        /// 实例 ID
        #[arg(short, long)]
        id: String,
        /// 要安装的 Minecraft 版本
        #[arg(short, long)]
        version: String,
    },
    /// 校验已下载文件的 SHA1 哈希
    Verify {
        /// 实例 ID
        #[arg(short, long)]
        id: String,
    },
}

#[derive(Subcommand)]
pub(crate) enum LaunchAction {
    /// 启动游戏实例
    Run {
        /// 实例 ID
        #[arg(short, long)]
        id: String,
    },
}

#[derive(Subcommand)]
pub(crate) enum ConfigAction {
    /// 查看当前配置
    Get {
        /// 配置键（省略则显示全部）
        key: Option<String>,
    },
    /// 设置配置值
    Set {
        /// 配置键
        key: String,
        /// 配置值
        value: String,
    },
}

#[derive(Subcommand)]
pub(crate) enum ModloaderAction {
    /// 列出可用的模组加载器版本
    List {
        /// 加载器类型: fabric, forge, neoforge, quilt, liteloader, optifine, cleanroom, legacyfabric
        #[arg(short, long)]
        loader_type: String,
        /// Minecraft 版本（例如 "1.20.4"）
        #[arg(short, long)]
        game_version: String,
    },
    /// 安装模组加载器到指定实例
    Install {
        /// 实例 ID
        #[arg(short = 'p', long)]
        profile_id: String,
        /// 加载器类型: fabric, forge, neoforge, quilt, liteloader, optifine, cleanroom, legacyfabric
        #[arg(short, long)]
        loader_type: String,
        /// 加载器版本号
        #[arg(short, long)]
        loader_version: String,
    },
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // 初始化日志
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "rmccc=info,rmcli=info".into()),
        )
        .init();

    let cli = Cli::parse();

    // 初始化后端上下文
    let data_dir = cli
        .data_dir
        .map(std::path::PathBuf::from)
        .unwrap_or_else(rmccc_config::paths::default_data_dir);

    std::fs::create_dir_all(&data_dir)?;
    let ctx = CliContext::new(&data_dir)?;

    // 决定运行模式：
    // 1. 如果指定了 -i/--interactive，强制进入 REPL
    // 2. 如果没有子命令且 stdin 是终端，自动进入 REPL
    // 3. 否则执行单命令后退出
    let should_repl = cli.interactive || (cli.command.is_none() && std::io::stdin().is_terminal());

    if should_repl {
        let mut repl = repl::Repl::new(ctx, data_dir)?;
        repl.run().await?;
        return Ok(());
    }

    // 单命令模式
    match cli.command {
        Some(cmd) => dispatch(ctx, cmd).await,
        None => {
            // 非 TTY 且无子命令：打印帮助
            Cli::parse_from(["rmcli", "--help"]);
            Ok(())
        }
    }
}

/// 单命令模式的分发逻辑。
async fn dispatch(ctx: CliContext, command: Commands) -> anyhow::Result<()> {
    match command {
        Commands::Auth { action } => commands::auth::handle(ctx, action).await,
        Commands::Instance { action } => commands::instance::handle(ctx, action).await,
        Commands::Resource { action } => commands::resource::handle(ctx, action).await,
        Commands::Launch { action } => commands::launch::handle(ctx, action).await,
        Commands::Config { action } => commands::config::handle(ctx, action).await,
        Commands::Modloader { action } => commands::modloader::handle(ctx, action).await,
    }
}
