//! Version list trait — remote version discovery for modloaders.
//!
//! Each modloader implements [`VersionList`] to fetch available versions
//! from its remote API. Version lists are keyed by Minecraft version,
//! contain modloader-specific version entries, and support lazy loading
//! with thread-safe caching.

use rmccc_error::Result;
use std::sync::Arc;

use super::remote_version::RemoteVersion;

/// A version list for a specific modloader.
///
/// Implementors fetch version metadata from remote APIs (e.g. Fabric Meta,
/// Forge Maven, BMCLAPI). The list is lazy-loaded: [`load`](VersionList::load)
/// triggers [`refresh`](VersionList::refresh) only when needed.
///
/// # Thread safety
///
/// Implementations must be `Send + Sync` and use interior mutability
/// (e.g. `RwLock<HashMap>`) for the version cache.
#[async_trait::async_trait]
pub trait VersionList: Send + Sync {
    /// Whether this version list supports type classification
    /// (release / snapshot / old).
    fn has_type(&self) -> bool;

    /// Refresh the version list from the remote API.
    ///
    /// If `game_version` is `Some`, only refresh entries for that
    /// specific Minecraft version. If `None`, refresh everything.
    async fn refresh(&self, game_version: Option<&str>) -> Result<()>;

    /// Lazy-load: refresh if the specified game version hasn't been loaded.
    async fn load(&self, game_version: &str) -> Result<()> {
        if !self.is_loaded(game_version) {
            self.refresh(Some(game_version)).await
        } else {
            Ok(())
        }
    }

    /// Check whether any version data has been loaded at all.
    fn is_loaded_any(&self) -> bool;

    /// Check whether the specified game version has been loaded.
    fn is_loaded(&self, game_version: &str) -> bool;

    /// Get all modloader versions for a specific Minecraft version.
    fn get_versions(&self, game_version: &str) -> Vec<Arc<dyn RemoteVersion>>;

    /// Find a specific modloader version.
    ///
    /// Returns `None` if the version is not found.
    fn get_version(
        &self,
        game_version: &str,
        remote_version: &str,
    ) -> Option<Arc<dyn RemoteVersion>>;
}
