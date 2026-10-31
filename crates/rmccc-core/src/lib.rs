//! Core orchestrator and dependency injection container.
//!
//! Re-exports all sub-crate public APIs and provides the `CoreContext`
//! that wires everything together for CLI and Tauri layers.

// Re-export error types
pub use rmccc_error::{ErrorCode, IpcError, Result, RmcccError};

// Re-export sub-crate APIs
pub use rmccc_auth;
pub use rmccc_config;
/// 内核版本（关于页展示；与启动器版本独立语义）。
pub const CORE_VERSION: &str = env!("CARGO_PKG_VERSION");

pub use rmccc_dashboard;
pub use rmccc_download;
pub use rmccc_instance;
pub use rmccc_launch;
pub use rmccc_modloader;
pub use rmccc_modpack;
pub use rmccc_mods;
pub use rmccc_platform;
pub use rmccc_plugin;
pub use rmccc_version;

// 游戏安装/修复/启动编排（P1-01 抽取自 CLI，进度经回调传递）。
pub mod pipeline;

// Mod loader 安装编排（P1-4 收敛：Tauri/CLI 共用这一份）。
pub mod modloader;

// 整合包安装编排（HMCL ModpackProvider + install task 对应层）。
pub mod modpack;

// 主界面服务器信息卡：真实 Minecraft Server List Ping。
pub mod server_status;

use std::path::{Path, PathBuf};
use std::sync::Arc;

/// Core context — the DI container that holds all providers and managers.
///
/// Default providers are injected in [`CoreContext::new`]. For testing or
/// custom configurations, use [`CoreContext::builder`] to swap implementations.
pub struct CoreContext {
    /// Application configuration (loaded from `config/launcher-settings.json`).
    pub config: rmccc_config::AppConfig,

    /// Loss-tolerant runtime state (loaded from `state/launcher-state.json`).
    /// P3: separates settings from runtime state.
    pub launcher_state: rmccc_config::LauncherState,

    /// User-level preferences (loaded from `user-settings.json`).
    ///
    /// Holds the UI language (`locale`: `None` = follow the system language)
    /// and the update-check preference. Owned here so the Tauri layer and the
    /// CLI read the same value.
    pub user_settings: rmccc_config::UserSettings,

    /// The actual data directory in use (respects CLI `-d` override).
    /// All file I/O (auth, config, profiles) uses this path, not
    /// `config.resolved_data_dir()` which ignores CLI overrides.
    pub data_dir: PathBuf,

    /// Shared HTTP client with connection pooling. All providers that make
    /// network requests should use this client for consistent TLS and proxy config.
    pub http_client: reqwest::Client,

    /// Download engine with concurrent workers, retry, and SHA1 verification.
    ///
    /// 以 `dyn` trait 持有（P2-05），便于 mock 与替换实现。
    pub download_manager: Arc<dyn rmccc_download::DownloadManager>,

    /// Game directories + version-backed instances + per-instance settings.
    ///
    /// This is the **only** owner of instance state (ADR-016): the old
    /// `ProfileStore` and single `GameRepository` are gone. `std::sync::Mutex`
    /// is deliberate — every method is synchronous and callers must never hold
    /// the guard across an `.await`.
    pub instances: Arc<std::sync::Mutex<rmccc_instance::InstanceService>>,

    /// Version provider — resolves Mojang version manifests and fetches version JSON.
    /// Defaults to [`rmccc_version::MojangVersionProvider`].
    pub version_provider: std::sync::Arc<dyn rmccc_version::VersionProvider>,

    /// Download provider — translates URLs for mirror support.
    /// Defaults to Mojang official source.
    pub download_provider: std::sync::Arc<dyn rmccc_download::provider::DownloadProvider>,

    /// Auth provider — handles Microsoft OAuth2 / offline authentication.
    /// Defaults to [`rmccc_auth::MicrosoftAuthProvider`] using the shared HTTP
    /// client. Held as `Arc` so the account factory / accounts can share it.
    pub auth_provider: Arc<dyn rmccc_auth::AuthProvider>,

    /// Account repository — plaintext metadata + encrypted private data storage
    /// (aligned with HMCL `Accounts`). The single owner of persisted accounts.
    pub account_repo: Arc<rmccc_auth::AccountRepository>,

    /// Debounced file writer (P4): batches config writes with a 200ms debounce
    /// and coalesces repeated saves to the same path. Flushed on process exit.
    pub file_saver: rmccc_config::FileSaver,

    /// Mod sources (Modrinth, CurseForge, etc.) for searching and downloading mods.
    /// Empty until Phase 4 mod API implementation is complete.
    /// Arc'd so IPC commands can snapshot a source under a short read lock
    /// and run network calls lock-free (perf audit 1.1).
    pub mod_sources: Vec<std::sync::Arc<dyn rmccc_mods::ModSource>>,

    /// 平台适配层（指南 §5.7）。**在组装层注入一次**，下游全链路依赖 trait：
    /// 进程终止、文件权限、符号链接、打开 URL、平台描述符都从这里取，
    /// 领域 crate 不得再内联 `#[cfg(target_os)]` 执行分支。
    pub platform: Arc<rmccc_platform::Platform>,

    /// 插件管理器（Phase 2）。加载失败只产生诊断，不阻断启动
    /// （`PluginManager::load` 永不返回 Err）。
    pub plugin_manager: rmccc_plugin::PluginManager,

    /// Dashboard 布局存储（Phase 3，ADR-005）。布局归一化/持久化/默认恢复。
    pub dashboard: rmccc_dashboard::LayoutStore,
}

/// Cheap, lock-free snapshot of the Arc-backed pieces long-running pipelines
/// need (audit 2026-10-25 A2).
///
/// IPC 命令必须在进入分钟级安装/启动前释放 `AppState.core` 的读锁——
/// tokio RwLock 是公平锁：只要有一个写者（`settings_set` 等）排队，后续
/// 读请求也会排在写者后面，整包下载期间几乎所有 IPC 都会卡死。命令层的
/// 正确姿势是 `let handle = { let core = state.core.read().await; core.handle() };`
/// 然后全程使用 handle。
///
/// `config` 是取快照时的克隆：安装期间改设置不会影响已开始的安装
/// （原先长读锁的语义是"安装期间 settings_set 阻塞"，快照语义是"立即
/// 生效于下一次安装"——后者才是可用的行为）。
#[derive(Clone)]
pub struct CoreHandle {
    /// 配置快照（游戏根、并发数、事件节流等）。
    pub config: rmccc_config::AppConfig,
    /// 实际使用的数据目录（同 [`CoreContext::data_dir`]）。
    pub data_dir: PathBuf,
    /// 共享 HTTP 客户端（连接池复用）。
    pub http_client: reqwest::Client,
    /// 下载引擎。
    pub download_manager: Arc<dyn rmccc_download::DownloadManager>,
    /// 实例服务（目录注册表 + 版本仓库 + 设置）。
    pub instances: Arc<std::sync::Mutex<rmccc_instance::InstanceService>>,
    /// 版本元数据 provider。
    pub version_provider: Arc<dyn rmccc_version::VersionProvider>,
    /// 镜像 provider。
    pub download_provider: Arc<dyn rmccc_download::provider::DownloadProvider>,
    /// 认证 provider（Microsoft OAuth）。
    pub auth_provider: Arc<dyn rmccc_auth::AuthProvider>,
    /// 账户仓库（凭据唯一持有者）。
    pub account_repo: Arc<rmccc_auth::AccountRepository>,
    /// 平台适配层（进程终止 / Java 探测 / 描述符）。
    pub platform: Arc<rmccc_platform::Platform>,
}

impl CoreContext {
    /// Take a lock-free [`CoreHandle`] snapshot for long-running work.
    ///
    /// 代价是一次 `AppConfig` 克隆 + 若干 Arc 引用计数，微秒级；换来的
    /// 是调用方可以立即释放 IPC 层的 core 读锁。
    pub fn handle(&self) -> CoreHandle {
        CoreHandle {
            config: self.config.clone(),
            data_dir: self.data_dir.clone(),
            http_client: self.http_client.clone(),
            download_manager: self.download_manager.clone(),
            version_provider: self.version_provider.clone(),
            download_provider: self.download_provider.clone(),
            auth_provider: self.auth_provider.clone(),
            account_repo: self.account_repo.clone(),
            instances: self.instances.clone(),
            platform: self.platform.clone(),
        }
    }
}

impl CoreContext {
    /// Create a new CoreContext with default providers.
    ///
    /// Uses [`rmccc_version::MojangVersionProvider`] as the version provider.
    pub fn new(data_dir: &Path) -> Result<Self> {
        // P2-14：环境区分（RMCCC_ENV=dev|prod，默认 prod 保持安全默认）。
        // 后续日志过滤 / 行为开关可依据该值调整。
        let environment = std::env::var("RMCCC_ENV").unwrap_or_else(|_| "prod".to_string());
        tracing::info!(environment = %environment, data_dir = %data_dir.display(), "CoreContext initializing");

        let mut config = rmccc_config::AppConfig::load(data_dir)?;
        // P3：运行状态（可丢失）与设置分离。
        let launcher_state = rmccc_config::LauncherState::load(data_dir)?;
        // 用户级偏好（UI 语言等）；locale = None 表示跟随系统。
        let user_settings = rmccc_config::UserSettings::load(data_dir)?;

        // Ensure config.data_dir reflects the actual directory in use
        // so that resolved_data_dir() and resolved_game_root() respect
        // CLI `-d` overrides (Bug #4).
        config.data_dir = Some(data_dir.to_path_buf());

        // 平台适配层在组装层构造一次，实例仓库/启动/账户全部复用。
        let platform = Arc::new(rmccc_platform::Platform::current());

        // Game-directory registry + instance service + legacy migration.
        let instances = Arc::new(std::sync::Mutex::new(rmccc_instance::InstanceService::new(
            data_dir,
            &config,
            platform.clone(),
            launcher_state.selected_profile_id.as_deref(),
        )?));

        // P2-20：显式设置 connect/timeout，弱网下避免长时间挂起。
        // 不再设置"总超时"（`.timeout`）：慢速 CDN 上的大 jar 会在 body 读到一半时被
        // `TimedOut` 误杀（sodium 安装即此症状）。改用 `read_timeout`（单次读间隔），
        // 只要仍在收数据就继续，真正卡死才失败。
        let http_client = reqwest::Client::builder()
            .pool_max_idle_per_host(32)
            .connect_timeout(std::time::Duration::from_secs(15))
            .read_timeout(std::time::Duration::from_secs(120))
            // BMCLAPI 的重定向镜像池（cernet/ustc 等）对**无 User-Agent 的请求
            // 直接 403**（2026-10-25 NeoForge 安装器下载失败实测：reqwest 默认
            // 不发 UA → 302 链终点的镜像拒绝；curl 有 UA 则全程 200）。reqwest
            // 默认 UA 为空，必须显式设置。
            .user_agent(concat!("RMCCC/", env!("CARGO_PKG_VERSION")))
            .build()
            .map_err(|e| RmcccError::Other(e.to_string()))?;

        // SHA1 cache repository — deduplicates downloads across instances.
        let cache_dir = data_dir.join("cache");
        let cache_repo = Arc::new(rmccc_download::CacheRepository::new(cache_dir));

        // 自动选源（2026-10-25，`mirror_url = "auto"`）：TCP 连接延迟探测两大
        // 下载源，选更快的（资源更新可能滞后——见设置页提示）。TCP 连接时长是
        // 到 CDN 边缘 RTT 的良好代理，且同步、有界（单次 ≤ 2s），不拖慢启动。
        let chosen_mirror: Option<String> = match config.mirror_url.as_deref() {
            Some("auto") => {
                let (official_ms, bmclapi_ms) = probe_source_latencies();
                let pick_bmclapi = match (official_ms, bmclapi_ms) {
                    // 并发限流注定的镜像池 + 官方更权威：持平或官方不可测时选官方。
                    (_, Some(b)) => official_ms.is_some_and(|o| b < o),
                    _ => false,
                };
                tracing::info!(
                    official_ms = ?official_ms,
                    bmclapi_ms = ?bmclapi_ms,
                    chosen = if pick_bmclapi { "bmclapi" } else { "official" },
                    "Auto download source selected"
                );
                pick_bmclapi.then(|| "https://bmclapi2.bangbang93.com".to_string())
            }
            other => other.map(str::to_string),
        };
        let is_bmclapi_mirror = matches!(
            chosen_mirror.as_deref(),
            Some("bmclapi") | Some("https://bmclapi2.bangbang93.com")
        );
        let effective_concurrency = if is_bmclapi_mirror {
            config.download_concurrency.min(16)
        } else {
            config.download_concurrency
        };
        let download_manager: Arc<dyn rmccc_download::DownloadManager> = Arc::new(
            rmccc_download::ConcurrentDownloadManager::new(
                http_client.clone(),
                effective_concurrency,
            )
            .with_cache(cache_repo.clone()),
        );

        /// TCP connection latency probe for auto source selection.
        /// Returns `None` on failure or after the 2s `connect_timeout` —
        /// without the timeout a SYN-blackholed host would stall startup for
        /// the OS-level SYN retry window (~75s on macOS).
        fn tcp_connect_ms(host: &str) -> Option<u128> {
            use std::net::{TcpStream, ToSocketAddrs};
            use std::time::{Duration, Instant};
            let start = Instant::now();
            let addr = (host, 443).to_socket_addrs().ok()?.next()?;
            TcpStream::connect_timeout(&addr, Duration::from_secs(2))
                .ok()
                .map(|_| start.elapsed().as_millis())
        }

        /// Latency snapshot of the two download sources (official launcher
        /// metadata host vs BMCLAPI). Probes run in parallel so the total
        /// wait is bounded by one 2s connect timeout, not two.
        fn probe_source_latencies() -> (Option<u128>, Option<u128>) {
            std::thread::scope(|s| {
                let official = s.spawn(|| tcp_connect_ms("launchermeta.mojang.com"));
                let bmclapi = s.spawn(|| tcp_connect_ms("bmclapi2.bangbang93.com"));
                (
                    official.join().ok().flatten(),
                    bmclapi.join().ok().flatten(),
                )
            })
        }

        let version_provider = std::sync::Arc::new(
            rmccc_version::MojangVersionProvider::new(http_client.clone())
                // Perf audit V2: ETag + manifest survive restarts, so a cold
                // start issues a conditional request (304 → no download)
                // instead of re-fetching the ~500 KB manifest.
                .with_disk_cache(data_dir.join("cache").join("version-manifest.json")),
        );

        let download_provider: std::sync::Arc<dyn rmccc_download::provider::DownloadProvider> =
            if let Some(ref mirror) = chosen_mirror {
                match mirror.as_str() {
                    "bmclapi" | "https://bmclapi2.bangbang93.com" => {
                        std::sync::Arc::new(rmccc_download::provider::BmclApiDownloadProvider::new(
                            "https://bmclapi2.bangbang93.com",
                        ))
                    }
                    _ => std::sync::Arc::new(rmccc_download::provider::MojangDownloadProvider),
                }
            } else {
                std::sync::Arc::new(rmccc_download::provider::MojangDownloadProvider)
            };

        // P4：延迟写入 FileSaver —— 命令层配置/状态写盘统一走它（防抖合并）。
        let file_saver = rmccc_config::FileSaver::new();

        // Auth provider — Microsoft OAuth2 via the shared HTTP client.
        let auth_provider: Arc<dyn rmccc_auth::AuthProvider> =
            Arc::new(rmccc_auth::MicrosoftAuthProvider::new(http_client.clone()));

        // Account repository — plaintext metadata + encrypted private data.
        // 平台适配层在上方构造一次，下游（vault / launcher / repository）都复用它。
        let auth_vault = Arc::new(rmccc_auth::TokenVault::init(
            &data_dir.join(".key"),
            platform.fs.as_ref(),
        )?);
        let mut account_repo = rmccc_auth::AccountRepository::new(data_dir, auth_vault.clone())?;
        // 注册登录方式工厂（对齐 HMCL Accounts）：离线 + Microsoft。
        account_repo.register_factory(Box::new(rmccc_auth::OfflineAccountFactory));
        account_repo.register_factory(Box::new(rmccc_auth::MicrosoftAccountFactory::new(
            auth_provider.clone(),
        )));
        let account_repo = Arc::new(account_repo);
        // 迁移旧版账户存储（accounts/ 目录与 auth.json），失败不阻断启动。
        if let Err(e) = account_repo.migrate_legacy() {
            tracing::warn!(error = %e, "Legacy account migration failed");
        }

        // 插件系统：损坏的注册表/插件只记诊断，不影响主链路（ADR-003）。
        let plugin_manager = rmccc_plugin::PluginManager::load(
            data_dir,
            env!("CARGO_PKG_VERSION"),
            platform.descriptor.plugin_platform_id(),
            platform.fs.clone(),
        );
        for diagnostic in plugin_manager.diagnostics() {
            match diagnostic.severity {
                rmccc_plugin::manager::DiagnosticSeverity::Error => {
                    tracing::warn!(
                        plugin = diagnostic.plugin_id.as_deref().unwrap_or("-"),
                        "插件诊断：{}",
                        diagnostic.message
                    );
                }
                rmccc_plugin::manager::DiagnosticSeverity::Warning => {
                    tracing::info!(
                        plugin = diagnostic.plugin_id.as_deref().unwrap_or("-"),
                        "插件诊断：{}",
                        diagnostic.message
                    );
                }
            }
        }

        // Dashboard 布局存储（Phase 3）：路径 + 平台文件系统适配器。
        let dashboard = rmccc_dashboard::LayoutStore::new(data_dir, platform.fs.clone());

        // Mod loader 安装不再预创建 provider：统一走
        // `modloader::install_loader` + `DependencyManager`（审计 P1-4）。
        // Mod sources — Modrinth API client (CurseForge to follow).
        let mod_sources: Vec<std::sync::Arc<dyn rmccc_mods::ModSource>> =
            vec![std::sync::Arc::new(
                rmccc_mods::modrinth::ModrinthSource::new(http_client.clone()),
            )];

        Ok(Self {
            config,
            launcher_state,
            user_settings,
            data_dir: data_dir.to_path_buf(),
            http_client,
            download_manager,
            instances,
            version_provider,
            download_provider,
            auth_provider,
            account_repo,
            file_saver,
            mod_sources,
            platform,
            plugin_manager,
            dashboard,
        })
    }

    /// Get the Microsoft auth provider from the DI container.
    ///
    /// This is a convenience accessor; callers that need the trait object
    /// should use `core.auth_provider` directly.
    pub fn microsoft_auth(&self) -> &dyn rmccc_auth::AuthProvider {
        self.auth_provider.as_ref()
    }

    /// 静默登录/刷新账户（P2：token 生命周期由 `Account` 内部承担，不再吞错）。
    ///
    /// 从仓库恢复账户 → `Account::log_in()`（提前刷新窗口 + 刷新 + profile UUID
    /// 一致性校验）→ 刷新成功后经仓库回写并返回最新会话。
    ///
    /// # Errors
    ///
    /// 凭证彻底失效返回 [`RmcccError::CredentialExpired`]（调用方应引导重新登录），
    /// 刷新后换号返回 [`RmcccError::WrongAccount`]，其余网络错误原样传播。
    pub async fn refresh_session_if_needed(
        &self,
        session: &rmccc_auth::AuthSession,
    ) -> rmccc_error::Result<rmccc_auth::AuthSession> {
        refresh_session(self.account_repo.as_ref(), session).await
    }

    /// 静默登录当前 active 账户，返回启动用 [`rmccc_auth::AuthInfo`]。
    ///
    /// 无 active 账户时返回 `None`（调用方按需走离线回退）。凭证失效 /
    /// 刷新换号 / 网络错误会原样传播，调用方据此引导用户。
    pub async fn log_in_active_account(&self) -> rmccc_error::Result<Option<rmccc_auth::AuthInfo>> {
        log_in_active_account(self.account_repo.as_ref()).await
    }

    /// Flushes all pending debounced writes to disk (process exit hook).
    ///
    /// P4：保证 FileSaver 队列中未落盘的配置修改在进程退出前写入，避免丢数据。
    pub fn flush_pending_writes(&self) {
        self.file_saver.wait_for_all_saves();
    }
}

impl CoreHandle {
    /// 静默登录/刷新账户（同 [`CoreContext::refresh_session_if_needed`]，
    /// 供已释放 core 读锁的 IPC 调用方使用，审计 A1）。
    pub async fn refresh_session(
        &self,
        session: &rmccc_auth::AuthSession,
    ) -> rmccc_error::Result<rmccc_auth::AuthSession> {
        refresh_session(self.account_repo.as_ref(), session).await
    }

    /// 静默登录当前 active 账户（同 [`CoreContext::log_in_active_account`]，
    /// 供启动链路经 [`CoreHandle`] 调用）。
    pub async fn log_in_active_account(&self) -> rmccc_error::Result<Option<rmccc_auth::AuthInfo>> {
        log_in_active_account(self.account_repo.as_ref()).await
    }
}

/// `refresh_session` 的共享实现：只依赖 `account_repo`，因此 IPC 层可以在
/// 短锁克隆 `Arc<AccountRepository>` 后立即释放 core 读锁再调用
/// （审计 A1：嵌套读锁死锁的根治）。
pub async fn refresh_session(
    account_repo: &rmccc_auth::AccountRepository,
    session: &rmccc_auth::AuthSession,
) -> rmccc_error::Result<rmccc_auth::AuthSession> {
    let account = account_repo
        .load_account(&session.account_id)?
        .ok_or_else(|| RmcccError::ProfileNotFound(session.account_id.clone()))?;
    account.log_in().await?;
    // Perf audit C5: persist only when the session actually changed.
    if account.needs_save() {
        account_repo.save_account(account.as_ref())?;
    }
    Ok(account.to_session())
}

/// `log_in_active_account` 的共享实现（同上，仅依赖 `account_repo`）。
async fn log_in_active_account(
    account_repo: &rmccc_auth::AccountRepository,
) -> rmccc_error::Result<Option<rmccc_auth::AuthInfo>> {
    let account = match account_repo.load_active_account()? {
        Some(a) => a,
        None => return Ok(None),
    };
    let info = account.log_in().await?;
    // Perf audit C5: persist only when the session actually changed — an
    // in-window token no longer costs 4 file IOs + 2 AES passes per launch.
    if account.needs_save() {
        account_repo.save_account(account.as_ref())?;
    }
    Ok(Some(info))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// P1-06：CoreContext DI 容器默认初始化完整（无网络）。
    #[test]
    fn test_core_context_new_builds_di() {
        let dir = std::env::temp_dir().join(format!(
            "rmccc-core-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        // CoreContext::new 内部经 AppConfig::load 写 config.json，data_dir 必须已存在
        std::fs::create_dir_all(&dir).unwrap();

        let core = CoreContext::new(&dir).unwrap();

        // 关键 Provider 均注入；实例服务保证至少一个游戏目录。
        {
            let instances = core.instances.lock().unwrap();
            assert_eq!(instances.directories().len(), 1);
            assert!(instances.selected_directory().path.is_dir());
        }
        // Mod loader 安装入口已收敛为 `modloader::install_loader`（无 provider 表）
        assert_eq!(core.mod_sources.len(), 1); // Modrinth
        assert_eq!(core.auth_provider.provider_id(), "microsoft");

        let _ = std::fs::remove_dir_all(&dir);
    }
}
