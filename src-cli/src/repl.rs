//! 交互式 REPL (Read-Eval-Print Loop) 模式。
//!
//! 提供命令行历史、Tab 自动补全、彩色提示符等功能。
//! 用户输入的命令通过 clap 解析后复用同一套 handler 逻辑。

use crate::commands;
use crate::context::CliContext;
use crate::Commands;

use clap::Parser;
use rustyline::completion::{Completer, Pair};
use rustyline::highlight::Highlighter;
use rustyline::hint::Hinter;
use rustyline::validate::Validator;
use rustyline::Helper;
use std::path::PathBuf;

// ── 补全辅助结构 ──

const TOP_COMMANDS: &[&str] = &[
    "auth", "instance", "resource", "launch", "config", "help", "exit", "quit",
];

const AUTH_SUB: &[&str] = &["login", "logout", "refresh", "status"];
const INSTANCE_SUB: &[&str] = &["create", "list", "delete"];
const RESOURCE_SUB: &[&str] = &["check", "download", "verify"];
const LAUNCH_SUB: &[&str] = &["run"];
const CONFIG_SUB: &[&str] = &["get", "set"];

fn subcommands_for(cmd: &str) -> Option<&[&str]> {
    match cmd {
        "auth" => Some(AUTH_SUB),
        "instance" => Some(INSTANCE_SUB),
        "resource" => Some(RESOURCE_SUB),
        "launch" => Some(LAUNCH_SUB),
        "config" => Some(CONFIG_SUB),
        _ => None,
    }
}

struct ReplHelper;

// ── 手动实现 rustyline 辅助 trait（默认无操作） ──

impl Highlighter for ReplHelper {}
impl Hinter for ReplHelper {
    type Hint = String;
}
impl Validator for ReplHelper {}

impl Completer for ReplHelper {
    type Candidate = Pair;

    fn complete(
        &self,
        line: &str,
        pos: usize,
        _ctx: &rustyline::Context<'_>,
    ) -> rustyline::Result<(usize, Vec<Pair>)> {
        let partial = &line[..pos];
        let words: Vec<&str> = partial.split_whitespace().collect();

        let candidates: Vec<Pair> = match words.len() {
            0 | 1 => {
                // 补全顶层命令
                let prefix = words.first().copied().unwrap_or("");
                TOP_COMMANDS
                    .iter()
                    .filter(|c| c.starts_with(prefix))
                    .map(|c| Pair {
                        display: c.to_string(),
                        replacement: c.to_string(),
                    })
                    .collect()
            }
            _ => {
                // 补全子命令
                let cmd = words[0];
                if let Some(subs) = subcommands_for(cmd) {
                    let prefix = words.last().copied().unwrap_or("");
                    subs.iter()
                        .filter(|s| s.starts_with(prefix))
                        .map(|s| Pair {
                            display: s.to_string(),
                            replacement: s.to_string(),
                        })
                        .collect()
                } else {
                    vec![]
                }
            }
        };

        let start = partial
            .rfind(|c: char| c.is_whitespace())
            .map(|i| i + 1)
            .unwrap_or(0);

        Ok((start, candidates))
    }
}

impl Helper for ReplHelper {}

// ── REPL 主结构 ──

pub struct Repl {
    ctx: CliContext,
    rl: rustyline::Editor<ReplHelper, rustyline::history::FileHistory>,
    history_path: PathBuf,
}

impl Repl {
    /// 创建 REPL 实例。
    ///
    /// 从 `data_dir` 加载命令历史（`.rmcli_history`）。
    pub fn new(ctx: CliContext, data_dir: PathBuf) -> rustyline::Result<Self> {
        let mut rl = rustyline::Editor::new()?;
        rl.set_helper(Some(ReplHelper));

        let history_path = data_dir.join(".rmcli_history");

        // 尝试加载历史文件，文件不存在时忽略
        if history_path.exists() {
            let _ = rl.load_history(&history_path);
        }

        Ok(Self {
            ctx,
            rl,
            history_path,
        })
    }

    /// 运行 REPL 主循环。
    ///
    /// 打印欢迎信息后进入 read-eval-print 循环，
    /// 直到用户输入 `exit`/`quit`/`q` 或按下 Ctrl+D。
    pub async fn run(&mut self) -> anyhow::Result<()> {
        self.print_welcome();

        loop {
            let prompt = "\x1b[1;36mrmcli>\x1b[0m ";
            let readline = self.rl.readline(prompt);

            match readline {
                Ok(line) => {
                    let line = line.trim().to_string();

                    // 空行跳过
                    if line.is_empty() {
                        continue;
                    }

                    // 记录到历史
                    let _ = self.rl.add_history_entry(&line);

                    // 退出命令
                    if matches!(line.as_str(), "exit" | "quit" | "q") {
                        println!("👋 感谢使用 RMCLI，再见！");
                        break;
                    }

                    // 处理输入
                    self.handle_input(&line).await;
                }
                Err(rustyline::error::ReadlineError::Interrupted) => {
                    // Ctrl+C — 取消当前输入，不退出
                    println!("^C");
                    continue;
                }
                Err(rustyline::error::ReadlineError::Eof) => {
                    // Ctrl+D — 优雅退出
                    println!();
                    println!("👋 感谢使用 RMCLI，再见！");
                    break;
                }
                Err(e) => {
                    eprintln!("读取错误: {}", e);
                    break;
                }
            }
        }

        // 持久化历史
        if let Some(parent) = self.history_path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let _ = self.rl.save_history(&self.history_path);

        Ok(())
    }

    /// 打印欢迎横幅和命令简介。
    fn print_welcome(&self) {
        println!();
        println!("╔══════════════════════════════════════════════╗");
        println!("║      \x1b[1;36mRMCLI\x1b[0m — Minecraft 启动器命令行        ║");
        println!("║          v0.1.0  |  交互模式                 ║");
        println!("╚══════════════════════════════════════════════╝");
        println!();
        println!("可用命令:");
        println!("  \x1b[1;33mauth\x1b[0m      login/logout/refresh/status  — 认证管理");
        println!("  \x1b[1;33minstance\x1b[0m  create/list/delete           — 实例管理");
        println!("  \x1b[1;33mresource\x1b[0m  check/download/verify        — 资源管理");
        println!("  \x1b[1;33mlaunch\x1b[0m    run                           — 启动游戏");
        println!("  \x1b[1;33mconfig\x1b[0m    get/set                       — 配置管理");
        println!();
        println!("  \x1b[1;32mhelp\x1b[0m / \x1b[1;32m?\x1b[0m          — 显示此帮助");
        println!("  \x1b[1;31mexit\x1b[0m / \x1b[1;31mquit\x1b[0m / \x1b[1;31mq\x1b[0m  — 退出");
        println!();
        println!("输入命令开始操作...");
        println!();
    }

    /// 打印帮助信息。
    fn print_help(&self) {
        println!();
        println!("══════════ 命令帮助 ══════════");
        println!();
        println!("  \x1b[1;33mauth login\x1b[0m              使用 Microsoft OAuth2 或离线登录");
        println!("  \x1b[1;33mauth logout\x1b[0m             登出并清除会话");
        println!("  \x1b[1;33mauth refresh\x1b[0m            刷新过期令牌");
        println!("  \x1b[1;33mauth status\x1b[0m             查看当前认证状态");
        println!();
        println!("  \x1b[1;33minstance create -n <名称>\x1b[0m  创建新游戏实例");
        println!("      可选: -v <版本>  -l <fabric|forge|neoforge>");
        println!("  \x1b[1;33minstance list\x1b[0m            列出所有实例");
        println!("  \x1b[1;33minstance delete -i <ID>\x1b[0m   删除实例");
        println!();
        println!("  \x1b[1;33mresource check -i <ID>\x1b[0m     检查文件完整性");
        println!("  \x1b[1;33mresource download -i <ID> -v <版本>\x1b[0m  下载游戏文件");
        println!("  \x1b[1;33mresource verify -i <ID>\x1b[0m    校验 SHA1 哈希");
        println!();
        println!("  \x1b[1;33mlaunch run -i <ID>\x1b[0m         启动游戏实例");
        println!();
        println!("  \x1b[1;33mconfig get [键]\x1b[0m            查看配置");
        println!("  \x1b[1;33mconfig set <键> <值>\x1b[0m       设置配置");
        println!();
        println!("  \x1b[1;32mhelp\x1b[0m / \x1b[1;32m?\x1b[0m                   显示此帮助");
        println!("  \x1b[1;31mexit\x1b[0m / \x1b[1;31mquit\x1b[0m / \x1b[1;31mq\x1b[0m           退出");
        println!();
    }

    /// 解析用户输入并调度到对应的命令处理器。
    async fn handle_input(&self, line: &str) {
        // 特殊命令：help / ?
        if matches!(line, "help" | "?" | "h" | "？" | "帮助") {
            self.print_help();
            return;
        }

        // 使用 shlex 进行 shell 风格分词（支持引号）
        let args = match shlex::split(line) {
            Some(args) if !args.is_empty() => args,
            Some(_) => return,
            None => {
                eprintln!("⚠️ 输入解析失败，请检查引号是否匹配");
                return;
            }
        };

        // 再次检查特殊命令（可能带有额外空格）
        if args.len() == 1 && matches!(args[0].as_str(), "help" | "?" | "h" | "？" | "帮助") {
            self.print_help();
            return;
        }

        // 构造完整参数列表交给 clap 解析
        let full_args: Vec<String> = std::iter::once("rmcli".to_string())
            .chain(args.into_iter())
            .collect();

        match crate::Cli::try_parse_from(&full_args) {
            Ok(cli) => {
                if let Some(cmd) = cli.command {
                    if let Err(e) = self.dispatch(cmd).await {
                        eprintln!("❌ 错误: {:#}", e);
                    }
                }
            }
            Err(e) => {
                // clap 解析错误 — 可能是未知命令等
                let err_msg = e.to_string();
                // 提取第一行关键信息（去掉颜色码）
                let first_line = err_msg.lines().next().unwrap_or(&err_msg);
                eprintln!("⚠️ {}", first_line);
                eprintln!("💡 输入 'help' 查看可用命令");
            }
        }
    }

    /// 将 clap 解析出的命令枚举分发到对应的 handler。
    async fn dispatch(&self, command: Commands) -> anyhow::Result<()> {
        // 注意：这里不能直接 move ctx，因为 REPL 需要复用 ctx。
        // 当前 handlers 接收 CliContext by value，我们需要克隆。
        // CliContext 内部是 Arc，所以 clone 很轻量。
        let ctx = self.ctx.clone();
        match command {
            Commands::Auth { action } => commands::auth::handle(ctx, action).await,
            Commands::Instance { action } => commands::instance::handle(ctx, action).await,
            Commands::Resource { action } => commands::resource::handle(ctx, action).await,
            Commands::Launch { action } => commands::launch::handle(ctx, action).await,
            Commands::Config { action } => commands::config::handle(ctx, action).await,
            Commands::Modloader { action } => commands::modloader::handle(ctx, action).await,
        }
    }
}
