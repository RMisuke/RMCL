//! Application state shared across all Tauri IPC handlers.
//!
//! Uses the same `CoreContext` DI container as the CLI, ensuring both
//! interfaces go through identical backend logic. Credentials (vault) and the
//! account repository are owned by `CoreContext`; IPC handlers never touch
//! encryption or storage paths directly.

use std::sync::Arc;
use tauri::Emitter;
use tokio::sync::RwLock;

/// One plugin's logic runtime (ADR-007).
///
/// 审计批次 2：此前全部插件共享一把 `Mutex<LogicRunner>`——任何插件的
/// 30s 慢调用都会串行化其它插件，且 `plugin_logic_cancel` 要等同一把锁，
/// "中止在途调用"实际失效。现在每个插件一个槽位：runner 互斥锁只属于
/// 该插件；`pid` 是调用方在 load 之后、call 之前发布的 helper PID，
/// cancel 在拿不到 runner 锁（调用进行中）时直接按 PID 杀进程，
/// 阻塞中的 `request()` 会因管道关闭立即返回。
pub struct LogicSlot {
    /// 该插件独占的 runner（内部 hosts 映射只会有这一个插件）。
    pub runner: std::sync::Mutex<rmccc_logic::LogicRunner>,
    /// 当前在途调用的 helper PID（无在途调用时为 None）。
    pub pid: std::sync::Mutex<Option<u32>>,
}

/// 一次游戏运行的标识与时间戳。
///
/// 用于把"日志分析"绑定到具体运行：日志新鲜度以 `started_at_ms` 为界，
/// 分析结果按 `run_id` 隔离。退出后保留到同实例下次启动被覆盖，
/// 以便崩溃后仍可分析本次运行。
#[derive(Debug, Clone)]
pub struct RunRecord {
    pub run_id: String,
    pub pid: u32,
    /// `SystemTime::now()` 的 unix 毫秒。
    pub started_at_ms: u64,
    pub exited: bool,
}

/// 一次在途 AI 日志分析的登记：取消令牌 + 归属（用于按实例取消）。
pub struct AnalysisRecord {
    pub cancel: rmccc_llm::CancelToken,
    pub profile_id: Option<String>,
}

/// Backend context — wraps CoreContext + Tauri-specific handles.
pub struct AppState {
    /// Core orchestrator (DI container with all providers and the account repo).
    pub core: Arc<RwLock<rmccc_core::CoreContext>>,

    /// Shared HTTP client — cloned from CoreContext for convenient access
    /// in IPC commands without holding a read lock.
    pub http_client: reqwest::Client,

    /// Tauri app handle for event emission.
    pub app_handle: tauri::AppHandle,

    /// 运行中游戏（profile_id → pid）。
    pub running_games: Arc<dashmap::DashMap<String, u32>>,

    /// 最近一次运行记录（profile_id → RunRecord）。启动时铸造 `run_id`，
    /// 退出后保留（`exited=true`）供事后日志分析，下次启动覆盖。
    pub runs: Arc<dashmap::DashMap<String, RunRecord>>,

    /// 游戏进程输出环形缓冲（profile_id → 最近 GAME_OUTPUT_CAP 行 stdout/stderr）。
    /// 用途：latest.log 未生成（log4j 初始化前崩溃，如 Forge BootstrapLauncher
    /// 的 ExceptionInInitializerError）时，AI 日志分析与 MCP game_logs 仍能
    /// 拿到输出。进程退出后保留供事后分析，下次启动同实例时清空。
    pub game_output: Arc<dashmap::DashMap<String, Arc<std::sync::Mutex<Vec<String>>>>>,

    /// Logic plugin helpers (ADR-007), one slot per plugin (see [`LogicSlot`]).
    pub logic: dashmap::DashMap<String, Arc<LogicSlot>>,

    /// 平台适配层句柄（从 CoreContext 克隆）：launch_kill 与逻辑插件取消
    /// 都要按 PID 终止进程，短锁克隆后不再依赖 core RwLock。
    pub platform: Arc<rmccc_core::rmccc_platform::Platform>,

    /// 统一任务注册表：所有下载 / 安装事务的唯一事实源。
    pub tasks: Arc<rmccc_tasks::TaskRegistry>,

    /// 进行中的 AI 日志分析（analysis_id → 取消令牌 + 归属，ADR-013 S2）。
    /// 并发上限见 `commands/ai.rs`；任务结束自摘除。归属用于"新运行取消旧运行分析"。
    pub ai_analyses: Arc<std::sync::Mutex<std::collections::HashMap<String, AnalysisRecord>>>,

    /// MCP 服务器句柄（ADR-013 S4/S5）。默认关闭，`mcp_set_enabled` 启停。
    pub mcp: Arc<crate::mcp::McpHandle>,

    /// 插件桥 `llm.chat` 的限流状态（S6：每插件 6 次/分钟）。
    pub llm_rate: Arc<
        std::sync::Mutex<
            std::collections::HashMap<String, std::collections::VecDeque<std::time::Instant>>,
        >,
    >,

    /// 初次启动检测快照：必须在 core 写盘之前取样，否则 core 创建的
    /// `.minecraft` / 目录注册表会把首次运行误判成已配置。
    pub onboarding: crate::commands::onboarding::OnboardingSnapshot,
}

impl AppState {
    pub fn new(
        data_dir: &std::path::Path,
        app_handle: tauri::AppHandle,
    ) -> rmccc_error::Result<Self> {
        let onboarding = crate::commands::onboarding::OnboardingSnapshot::detect();
        let core = rmccc_core::CoreContext::new(data_dir)?;
        let http_client = core.http_client.clone();
        let mcp = Arc::new(crate::mcp::McpHandle::new(
            data_dir,
            core.platform.fs.clone(),
        )?);
        mcp.set_app(app_handle.clone());

        // 平台句柄在 core 移入 RwLock 前克隆（kill 路径免等 core 锁）。
        let platform = core.platform.clone();

        // 任务快照桥：注册表只负责状态，Tauri 层负责把它变成事件。
        // 事件通道容量有限；一旦前端落后（Lagged），补发整张任务板，
        // 而不是让某些进度永久丢失。
        let tasks = Arc::new(rmccc_tasks::TaskRegistry::new());
        {
            let bridge = tasks.clone();
            let emitter = app_handle.clone();
            tauri::async_runtime::spawn(async move {
                let mut receiver = bridge.subscribe();
                loop {
                    match receiver.recv().await {
                        Ok(snapshot) => {
                            let _ = emitter.emit(crate::events::TASK_UPDATED, snapshot);
                        }
                        Err(tokio::sync::broadcast::error::RecvError::Lagged(skipped)) => {
                            tracing::warn!(skipped, "task event bridge lagged; resyncing board");
                            let _ = emitter.emit(crate::events::TASK_SYNC, bridge.board());
                        }
                        Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                    }
                }
            });
        }

        Ok(Self {
            core: Arc::new(RwLock::new(core)),
            http_client,
            app_handle,
            running_games: Arc::new(dashmap::DashMap::new()),
            runs: Arc::new(dashmap::DashMap::new()),
            game_output: Arc::new(dashmap::DashMap::new()),
            logic: dashmap::DashMap::new(),
            platform,
            tasks,
            ai_analyses: Arc::new(std::sync::Mutex::new(std::collections::HashMap::new())),
            mcp,
            llm_rate: Arc::new(std::sync::Mutex::new(std::collections::HashMap::new())),
            onboarding,
        })
    }
}
