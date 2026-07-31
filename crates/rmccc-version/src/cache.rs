//! Offline version cache.
//!
//! Caches the Mojang version manifest locally. Serves stale data
//! when offline instead of failing.

use rmccc_error::Result;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

use super::types::VersionInfo;

/// Wrapper for the cached version manifest.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct CachedManifest {
    fetched_at: chrono::DateTime<chrono::Utc>,
    versions: Vec<VersionInfo>,
}

/// Cache manager for the version list.
pub struct VersionCache {
    cache_path: PathBuf,
    ttl_seconds: i64, // cache freshness window
}

impl VersionCache {
    pub fn new(cache_dir: &Path, ttl_seconds: i64) -> Self {
        Self {
            cache_path: cache_dir.join("versions.json"),
            ttl_seconds,
        }
    }

    /// Read the cached version list.
    pub fn read(&self) -> Result<Option<Vec<VersionInfo>>> {
        if !self.cache_path.exists() {
            return Ok(None);
        }

        let data = std::fs::read_to_string(&self.cache_path)?;
        let cached: CachedManifest = serde_json::from_str(&data)?;

        let age = (chrono::Utc::now() - cached.fetched_at).num_seconds();
        tracing::debug!(cache_age_seconds = age, ttl = self.ttl_seconds, "Version cache check");

        if age > self.ttl_seconds {
            tracing::info!("Version cache expired (age={age}s > TTL={}s)", self.ttl_seconds);
            return Ok(None);
        }

        Ok(Some(cached.versions))
    }

    /// Write the version list to the cache.
    pub fn write(&self, versions: &[VersionInfo]) -> Result<()> {
        if let Some(parent) = self.cache_path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let cached = CachedManifest {
            fetched_at: chrono::Utc::now(),
            versions: versions.to_vec(),
        };

        let data = serde_json::to_string_pretty(&cached)?;
        std::fs::write(&self.cache_path, data)?;

        tracing::info!("Version cache updated: {} versions", versions.len());
        Ok(())
    }
}
