//! Core orchestrator and dependency injection container.
//!
//! Re-exports all sub-crate public APIs and provides the `CoreContext`
//! that wires everything together for CLI and Tauri layers.

// Re-export error types
pub use rmccc_error::{ErrorCode, IpcError, RmcccError, Result};

// Re-export sub-crate APIs
pub use rmccc_auth;
pub use rmccc_config;
pub use rmccc_download;
pub use rmccc_launch;
pub use rmccc_modloader;
pub use rmccc_mods;
pub use rmccc_profile;
pub use rmccc_version;

use std::path::{Path, PathBuf};
use std::sync::Arc;

/// Core context — the DI container that holds all providers and managers.
///
/// Default providers are injected in [`CoreContext::new`]. For testing or
/// custom configurations, use [`CoreContext::builder`] to swap implementations.
pub struct CoreContext {
    /// Application configuration (loaded from config.json).
    pub config: rmccc_config::AppConfig,

    /// The actual data directory in use (respects CLI `-d` override).
    /// All file I/O (auth, config, profiles) uses this path, not
    /// `config.resolved_data_dir()` which ignores CLI overrides.
    pub data_dir: PathBuf,

    /// Shared HTTP client with connection pooling. All providers that make
    /// network requests should use this client for consistent TLS and proxy config.
    pub http_client: reqwest::Client,

    /// Download engine with concurrent workers, retry, and SHA1 verification.
    pub download_manager: Arc<rmccc_download::ConcurrentDownloadManager>,

    /// Profile / game instance store (JSON file-based).
    pub profile_store: rmccc_profile::ProfileStore,

    /// Version provider — resolves Mojang version manifests and fetches version JSON.
    /// Defaults to [`rmccc_version::MojangVersionProvider`].
    pub version_provider: Box<dyn rmccc_version::VersionProvider>,

    /// Download provider — translates URLs for mirror support.
    /// Defaults to Mojang official source.
    pub download_provider: Box<dyn rmccc_download::provider::DownloadProvider>,
}

impl CoreContext {
    /// Create a new CoreContext with default providers.
    ///
    /// Uses [`rmccc_version::MojangVersionProvider`] as the version provider.
    pub fn new(data_dir: &Path) -> Result<Self> {
        let mut config = rmccc_config::AppConfig::load(data_dir)?;

        // Ensure config.data_dir reflects the actual directory in use
        // so that resolved_data_dir() and resolved_game_root() respect
        // CLI `-d` overrides (Bug #4).
        config.data_dir = Some(data_dir.to_path_buf());

        let http_client = reqwest::Client::builder()
            .pool_max_idle_per_host(32)
            .build()
            .map_err(|e| RmcccError::Other(e.to_string()))?;

        // SHA1 cache repository — deduplicates downloads across instances.
        let cache_dir = data_dir.join("cache");
        let cache_repo = Arc::new(rmccc_download::CacheRepository::new(cache_dir));

        let download_manager = Arc::new(
            rmccc_download::ConcurrentDownloadManager::new(
                http_client.clone(),
                config.download_concurrency,
            )
            .with_cache(cache_repo.clone()),
        );

        let profiles_dir = data_dir.join("profiles");
        std::fs::create_dir_all(&profiles_dir)?;
        let profile_store = rmccc_profile::ProfileStore::new(profiles_dir);

        let version_provider = Box::new(rmccc_version::MojangVersionProvider::new(
            http_client.clone(),
        ));

        let download_provider: Box<dyn rmccc_download::provider::DownloadProvider> =
            if let Some(ref mirror) = config.mirror_url {
                match mirror.as_str() {
                    "bmclapi" | "https://bmclapi2.bangbang93.com" => Box::new(
                        rmccc_download::provider::BmclApiDownloadProvider::new(
                            "https://bmclapi2.bangbang93.com",
                        ),
                    ),
                    _ => Box::new(rmccc_download::provider::MojangDownloadProvider),
                }
            } else {
                Box::new(rmccc_download::provider::MojangDownloadProvider)
            };

        Ok(Self {
            config,
            data_dir: data_dir.to_path_buf(),
            http_client,
            download_manager,
            profile_store,
            version_provider,
            download_provider,
        })
    }

    /// Create a mod loader provider for the given type.
    ///
    /// Uses the shared `http_client` for network access.
    pub fn make_modloader_provider(
        &self,
        loader_type: &rmccc_modloader::ModLoaderType,
    ) -> Box<dyn rmccc_modloader::ModLoaderProvider> {
        rmccc_modloader::create_provider(loader_type, self.http_client.clone())
    }

    /// Create a Microsoft auth provider using the shared HTTP client.
    pub fn make_microsoft_auth_provider(&self) -> rmccc_auth::MicrosoftAuthProvider {
        rmccc_auth::MicrosoftAuthProvider::new(self.http_client.clone())
    }
}
