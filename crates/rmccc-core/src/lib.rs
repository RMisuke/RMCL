//! Core orchestrator and dependency injection container.
//!
//! Re-exports all sub-crate public APIs and provides the `CoreContext`
//! that wires everything together for CLI and Tauri layers.

// Re-export error types
pub use rmccc_error::{ErrorCode, IpcError, Result, RmcccError};

// Re-export sub-crate APIs
pub use rmccc_auth;
pub use rmccc_config;
pub use rmccc_download;
pub use rmccc_launch;
pub use rmccc_modloader;
pub use rmccc_mods;
pub use rmccc_profile;
pub use rmccc_version;

// 游戏安装/修复/启动编排（P1-01 抽取自 CLI，进度经回调传递）。
pub mod pipeline;

use rmccc_version::GameRepository as _;
use std::collections::HashMap;
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

    /// Profile / game instance store (JSON file-based).
    pub profile_store: rmccc_profile::ProfileStore,

    /// Local game repository (HMCL `GameRepository`).
    ///
    /// Indexes `versions/` under the resolved game root and resolves manifests
    /// **offline** (dual-view: launch / standalone). Launch, modloader install,
    /// and completeness checks all read from here — the network
    /// `version_provider` is only the metadata source during installation.
    pub game_repository: std::sync::Arc<std::sync::RwLock<rmccc_version::DefaultGameRepository>>,

    /// Version provider — resolves Mojang version manifests and fetches version JSON.
    /// Defaults to [`rmccc_version::MojangVersionProvider`].
    pub version_provider: Box<dyn rmccc_version::VersionProvider>,

    /// Download provider — translates URLs for mirror support.
    /// Defaults to Mojang official source.
    pub download_provider: Box<dyn rmccc_download::provider::DownloadProvider>,

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

    /// Mod loader providers keyed by loader type.
    /// Pre-created for all currently supported loaders (Fabric, Forge, NeoForge).
    /// Legacy loaders (Quilt, LiteLoader, etc.) are not yet available and will
    /// return an error when requested via [`Self::modloader_provider`].
    pub modloader_providers:
        HashMap<rmccc_modloader::ModLoaderType, Box<dyn rmccc_modloader::ModLoaderProvider>>,

    /// Mod sources (Modrinth, CurseForge, etc.) for searching and downloading mods.
    /// Empty until Phase 4 mod API implementation is complete.
    pub mod_sources: Vec<Box<dyn rmccc_mods::ModSource>>,
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

        // P2-20：显式设置 connect/timeout，弱网下避免长时间挂起（架构文档 10s connect）。
        let http_client = reqwest::Client::builder()
            .pool_max_idle_per_host(32)
            .connect_timeout(std::time::Duration::from_secs(10))
            .timeout(std::time::Duration::from_secs(120))
            .build()
            .map_err(|e| RmcccError::Other(e.to_string()))?;

        // SHA1 cache repository — deduplicates downloads across instances.
        let cache_dir = data_dir.join("cache");
        let cache_repo = Arc::new(rmccc_download::CacheRepository::new(cache_dir));

        let download_manager: Arc<dyn rmccc_download::DownloadManager> = Arc::new(
            rmccc_download::ConcurrentDownloadManager::new(
                http_client.clone(),
                config.download_concurrency,
            )
            .with_cache(cache_repo.clone()),
        );

        let profiles_dir = data_dir.join("profiles");
        std::fs::create_dir_all(&profiles_dir)?;
        let profile_store = rmccc_profile::ProfileStore::new(profiles_dir);

        // Local game repository rooted at the resolved game root (the `.minecraft`
        // equivalent). A failed initial scan must not block startup — it is
        // re-run after every install via `refresh()`.
        let game_root = config.resolved_game_root();
        std::fs::create_dir_all(&game_root)?;

        // Adopt instance data written by the pre-shared-root layout *before*
        // indexing, so the repository sees the migrated versions immediately.
        if let Err(e) = migrate_instance_layout(&config, &profile_store) {
            tracing::warn!(error = %e, "Instance layout migration failed");
        }

        let mut game_repository = rmccc_version::DefaultGameRepository::new(game_root.clone());
        if let Err(e) = game_repository.refresh() {
            tracing::warn!(error = %e, game_root = %game_root.display(), "Initial game repository refresh failed");
        }
        let game_repository = std::sync::Arc::new(std::sync::RwLock::new(game_repository));

        let version_provider = Box::new(rmccc_version::MojangVersionProvider::new(
            http_client.clone(),
        ));

        let download_provider: Box<dyn rmccc_download::provider::DownloadProvider> =
            if let Some(ref mirror) = config.mirror_url {
                match mirror.as_str() {
                    "bmclapi" | "https://bmclapi2.bangbang93.com" => {
                        Box::new(rmccc_download::provider::BmclApiDownloadProvider::new(
                            "https://bmclapi2.bangbang93.com",
                        ))
                    }
                    _ => Box::new(rmccc_download::provider::MojangDownloadProvider),
                }
            } else {
                Box::new(rmccc_download::provider::MojangDownloadProvider)
            };

        // P4：延迟写入 FileSaver —— 命令层配置/状态写盘统一走它（防抖合并）。
        let file_saver = rmccc_config::FileSaver::new();

        // Auth provider — Microsoft OAuth2 via the shared HTTP client.
        let auth_provider: Arc<dyn rmccc_auth::AuthProvider> =
            Arc::new(rmccc_auth::MicrosoftAuthProvider::new(http_client.clone()));

        // Account repository — plaintext metadata + encrypted private data.
        let auth_vault = Arc::new(rmccc_auth::TokenVault::init(&data_dir.join(".key"))?);
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

        // Pre-create modloader providers for currently supported loaders.
        let mut modloader_providers: HashMap<
            rmccc_modloader::ModLoaderType,
            Box<dyn rmccc_modloader::ModLoaderProvider>,
        > = HashMap::new();
        for lt in [
            rmccc_modloader::ModLoaderType::Fabric,
            rmccc_modloader::ModLoaderType::Forge,
            rmccc_modloader::ModLoaderType::NeoForge,
        ] {
            modloader_providers.insert(
                lt.clone(),
                rmccc_modloader::create_provider(&lt, http_client.clone())?,
            );
        }

        // Mod sources — Modrinth API client (CurseForge to follow).
        let mod_sources: Vec<Box<dyn rmccc_mods::ModSource>> = vec![Box::new(
            rmccc_mods::modrinth::ModrinthSource::new(http_client.clone()),
        )];

        Ok(Self {
            config,
            launcher_state,
            user_settings,
            data_dir: data_dir.to_path_buf(),
            http_client,
            download_manager,
            profile_store,
            game_repository,
            version_provider,
            download_provider,
            auth_provider,
            account_repo,
            file_saver,
            modloader_providers,
            mod_sources,
        })
    }

    /// Get a mod loader provider for the given type from the DI container.
    ///
    /// Returns an error for legacy loaders (Quilt, LiteLoader, etc.) that
    /// have not yet been migrated to the [`ModLoaderProvider`] trait.
    pub fn modloader_provider(
        &self,
        loader_type: &rmccc_modloader::ModLoaderType,
    ) -> Result<&dyn rmccc_modloader::ModLoaderProvider> {
        self.modloader_providers
            .get(loader_type)
            .map(|b| b.as_ref())
            .ok_or_else(|| {
                RmcccError::Other(format!(
                    "ModLoaderProvider not available for {loader_type:?}"
                ))
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
        let account = self
            .account_repo
            .load_account(&session.account_id)?
            .ok_or_else(|| RmcccError::ProfileNotFound(session.account_id.clone()))?;
        account.log_in().await?;
        self.account_repo.save_account(account.as_ref())?;
        Ok(account.to_session())
    }

    /// 静默登录当前 active 账户，返回启动用 [`rmccc_auth::AuthInfo`]。
    ///
    /// 无 active 账户时返回 `None`（调用方按需走离线回退）。凭证失效 /
    /// 刷新换号 / 网络错误会原样传播，调用方据此引导用户。
    pub async fn log_in_active_account(&self) -> rmccc_error::Result<Option<rmccc_auth::AuthInfo>> {
        let account = match self.account_repo.load_active_account()? {
            Some(a) => a,
            None => return Ok(None),
        };
        let info = account.log_in().await?;
        self.account_repo.save_account(account.as_ref())?;
        Ok(Some(info))
    }

    /// Flushes all pending debounced writes to disk (process exit hook).
    ///
    /// P4：保证 FileSaver 队列中未落盘的配置修改在进程退出前写入，避免丢数据。
    pub fn flush_pending_writes(&self) {
        self.file_saver.wait_for_all_saves();
    }
}

/// Moves instance data written by the pre-shared-root layout into the shared
/// game root, and repoints the owning profile at it.
///
/// Earlier builds created a nested directory per profile
/// (`{game_root}/{profile_id}/versions|assets|libraries`) while
/// `GameRepository` indexes `{game_root}`. Those installs were therefore
/// invisible to the launch path, which resolves versions through the repository
/// (`get_resolved()`), so launching failed even though the download succeeded.
///
/// Only a direct child of the game root that actually contains `versions/` is
/// touched, so a user-chosen custom `game_dir` (possibly on another volume) is
/// never moved. Trees are moved with same-filesystem renames where possible,
/// which keeps this cheap even for several hundred MB.
fn migrate_instance_layout(
    config: &rmccc_config::AppConfig,
    profile_store: &rmccc_profile::ProfileStore,
) -> rmccc_error::Result<usize> {
    let game_root = config.resolved_game_root();
    if !game_root.is_dir() {
        return Ok(0);
    }

    let profiles = profile_store.list()?;
    let mut migrated = 0usize;

    for mut profile in profiles.iter().cloned() {
        if profile.game_dir == game_root {
            continue;
        }
        // Only handle the legacy shape; anything else is a deliberate choice.
        if profile.game_dir.parent() != Some(game_root.as_path()) {
            continue;
        }

        if profile.game_dir.is_dir() {
            adopt_instance_tree(&game_root, &profile.game_dir)?;
        }

        let legacy = profile.game_dir.clone();
        profile.game_dir = game_root.clone();
        profile_store.save(&profile)?;
        migrated += 1;
        tracing::info!(
            profile_id = %profile.id,
            from = %legacy.display(),
            to = %game_root.display(),
            "Migrated instance to the shared game root"
        );
    }

    // Leftover legacy directories no longer referenced by any profile are only
    // reported: silently adopting them would keep disk usage the user may have
    // intended to reclaim by deleting a profile.
    let referenced: Vec<&std::path::Path> = profiles
        .iter()
        .map(|p| p.game_dir.as_path())
        .chain(std::iter::once(game_root.as_path()))
        .collect();
    if let Ok(entries) = std::fs::read_dir(&game_root) {
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_dir()
                || !path.join("versions").is_dir()
                || referenced.contains(&path.as_path())
            {
                continue;
            }
            tracing::warn!(
                path = %path.display(),
                "Orphaned pre-migration instance directory; delete it to reclaim space"
            );
        }
    }

    Ok(migrated)
}

/// Merges the game trees of a legacy instance directory into `game_root`.
fn adopt_instance_tree(
    game_root: &std::path::Path,
    legacy: &std::path::Path,
) -> rmccc_error::Result<()> {
    for sub in ["versions", "libraries", "assets"] {
        let from = legacy.join(sub);
        if from.is_dir() {
            merge_tree(&from, &game_root.join(sub))?;
        }
    }

    // Anything left over is run-directory data (saves, mods, options.txt, ...)
    // that we deliberately do not move; the non-empty removal simply reports it.
    match std::fs::remove_dir(legacy) {
        Ok(()) => {}
        Err(_) => tracing::warn!(
            path = %legacy.display(),
            "Pre-migration instance directory still holds non-game files; left in place"
        ),
    }
    Ok(())
}

/// Moves the contents of `from` into `to`, keeping files that already exist
/// (identical versions produce identical, SHA1-verified content).
fn merge_tree(from: &std::path::Path, to: &std::path::Path) -> rmccc_error::Result<()> {
    std::fs::create_dir_all(to)?;
    for entry in std::fs::read_dir(from)? {
        let entry = entry?;
        let source = entry.path();
        let target = to.join(entry.file_name());

        if entry.file_type()?.is_dir() {
            merge_tree(&source, &target)?;
            continue;
        }

        if target.exists() {
            let _ = std::fs::remove_file(&source);
            continue;
        }

        if std::fs::rename(&source, &target).is_err() {
            // Different filesystem: fall back to a copy.
            std::fs::copy(&source, &target)?;
            let _ = std::fs::remove_file(&source);
        }
    }

    // Best effort: drop the source once it is empty.
    let _ = std::fs::remove_dir(from);
    Ok(())
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

        // 关键 Provider 均注入
        assert!(core.profile_store.list().unwrap().is_empty());
        assert_eq!(core.modloader_providers.len(), 3); // fabric / forge / neoforge
        assert_eq!(core.mod_sources.len(), 1); // Modrinth
        assert_eq!(core.auth_provider.provider_id(), "microsoft");

        let _ = std::fs::remove_dir_all(&dir);
    }
    fn temp_dir(tag: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "rmccc-core-{tag}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// 旧布局（`{game_root}/{profile_id}/versions`）必须被并入共享根，否则
    /// `GameRepository` 索引不到版本，启动必定失败。
    #[test]
    fn test_migrate_instance_layout_adopts_nested_instances() {
        let data_dir = temp_dir("migrate");
        let config = rmccc_config::AppConfig {
            data_dir: Some(data_dir.clone()),
            ..Default::default()
        };
        let game_root = config.resolved_game_root();
        let profile_store = rmccc_profile::ProfileStore::new(data_dir.join("profiles"));

        let profile_id = "11111111-1111-1111-1111-111111111111";
        let legacy_dir = game_root.join(profile_id);
        let version_dir = legacy_dir.join("versions").join("1.20.4");
        std::fs::create_dir_all(&version_dir).unwrap();
        std::fs::write(version_dir.join("1.20.4.json"), b"{}").unwrap();
        std::fs::create_dir_all(legacy_dir.join("libraries")).unwrap();
        std::fs::write(legacy_dir.join("libraries/lib.jar"), b"x").unwrap();
        // 运行目录残留：不应被搬走
        std::fs::write(legacy_dir.join("options.txt"), b"fov:70").unwrap();

        profile_store
            .save(&rmccc_profile::Profile {
                id: profile_id.to_string(),
                name: "Old".into(),
                game_version: "1.20.4".into(),
                game_dir: legacy_dir.clone(),
                ..Default::default()
            })
            .unwrap();

        assert_eq!(migrate_instance_layout(&config, &profile_store).unwrap(), 1);

        assert!(game_root.join("versions/1.20.4/1.20.4.json").exists());
        assert!(game_root.join("libraries/lib.jar").exists());
        assert_eq!(
            profile_store.get(profile_id).unwrap().unwrap().game_dir,
            game_root
        );
        // 非游戏文件留在原处，且目录未被误删
        assert!(legacy_dir.join("options.txt").exists());

        let _ = std::fs::remove_dir_all(&data_dir);
    }

    /// 用户自定义的 `game_dir`（不在共享根之下）绝不能被迁移。
    #[test]
    fn test_migrate_leaves_custom_game_dir_alone() {
        let data_dir = temp_dir("migrate-custom");
        let config = rmccc_config::AppConfig {
            data_dir: Some(data_dir.clone()),
            ..Default::default()
        };
        let custom = data_dir.join("elsewhere");
        std::fs::create_dir_all(custom.join("versions")).unwrap();

        let store = rmccc_profile::ProfileStore::new(data_dir.join("profiles"));
        store
            .save(&rmccc_profile::Profile {
                id: "p1".into(),
                name: "Custom".into(),
                game_version: "1.20.4".into(),
                game_dir: custom.clone(),
                ..Default::default()
            })
            .unwrap();

        assert_eq!(migrate_instance_layout(&config, &store).unwrap(), 0);
        assert_eq!(store.get("p1").unwrap().unwrap().game_dir, custom);

        let _ = std::fs::remove_dir_all(&data_dir);
    }
}
