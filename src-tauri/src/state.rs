//! Application state shared across all Tauri IPC handlers.
//!
//! Uses the same `CoreContext` DI container as the CLI, ensuring both
//! interfaces go through identical backend logic. Credentials (vault) and the
//! account repository are owned by `CoreContext`; IPC handlers never touch
//! encryption or storage paths directly.

use std::sync::Arc;
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

/// Backend context — wraps CoreContext + Tauri-specific handles.
pub struct AppState {
    /// Core orchestrator (DI container with all providers and the account repo).
    pub core: Arc<RwLock<rmccc_core::CoreContext>>,

    /// Shared HTTP client — cloned from CoreContext for convenient access
    /// in IPC commands without holding a read lock.
    pub http_client: reqwest::Client,

    /// HTTP client for plugin `network.request` (ADR-006): redirects disabled so a
    /// 30x cannot escape the declared host allowlist, short timeout, no cookies.
    pub ui_http_client: reqwest::Client,

    /// Tauri app handle for event emission.
    pub app_handle: tauri::AppHandle,

    /// Running game processes (profile_id → pid).
    pub running_games: Arc<dashmap::DashMap<String, u32>>,

    /// Logic plugin helpers (ADR-007), one slot per plugin (see [`LogicSlot`]).
    pub logic: dashmap::DashMap<String, Arc<LogicSlot>>,

    /// 平台适配层句柄（从 CoreContext 克隆）：launch_kill 与逻辑插件取消
    /// 都要按 PID 终止进程，短锁克隆后不再依赖 core RwLock。
    pub platform: Arc<rmccc_core::rmccc_platform::Platform>,

    /// 进行中的 AI 日志分析（analysis_id → 取消令牌，ADR-013 S2）。
    /// 并发上限见 `commands/ai.rs`；任务结束自摘除。
    pub ai_cancels:
        Arc<std::sync::Mutex<std::collections::HashMap<String, rmccc_llm::CancelToken>>>,

    /// MCP 服务器句柄（ADR-013 S4/S5）。默认关闭，`mcp_set_enabled` 启停。
    pub mcp: Arc<crate::mcp::McpHandle>,

    /// 插件桥 `llm.chat` 的限流状态（S6：每插件 6 次/分钟）。
    pub llm_rate: Arc<
        std::sync::Mutex<
            std::collections::HashMap<String, std::collections::VecDeque<std::time::Instant>>,
        >,
    >,
}

impl AppState {
    pub fn new(
        data_dir: &std::path::Path,
        app_handle: tauri::AppHandle,
    ) -> rmccc_error::Result<Self> {
        let core = rmccc_core::CoreContext::new(data_dir)?;
        let http_client = core.http_client.clone();
        let ui_http_client = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .timeout(std::time::Duration::from_secs(30))
            .user_agent(concat!("RMCCC/", env!("CARGO_PKG_VERSION"), " (plugin-ui)"))
            // B6：构建失败必须报错——`Client::new()` 兜底会**跟随重定向**，
            // `network.request` 的"30x 不能逃出域名白名单"保证随之失效。
            .build()
            .map_err(|e| rmccc_error::RmcccError::Other(format!("ui http client: {e}")))?;

        let mcp = Arc::new(crate::mcp::McpHandle::new(
            data_dir,
            core.platform.fs.clone(),
        )?);
        mcp.set_app(app_handle.clone());

        // 平台句柄在 core 移入 RwLock 前克隆（kill 路径免等 core 锁）。
        let platform = core.platform.clone();

        Ok(Self {
            core: Arc::new(RwLock::new(core)),
            http_client,
            ui_http_client,
            app_handle,
            running_games: Arc::new(dashmap::DashMap::new()),
            logic: dashmap::DashMap::new(),
            platform,
            ai_cancels: Arc::new(std::sync::Mutex::new(std::collections::HashMap::new())),
            mcp,
            llm_rate: Arc::new(std::sync::Mutex::new(std::collections::HashMap::new())),
        })
    }
}
