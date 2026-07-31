//! Configuration and path management for RMCCC.
//!
//! Leaf crate — no business logic dependencies.

pub mod logging;
pub mod paths;

use rmccc_error::Result;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Global application configuration (persisted as config.json).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    /// Override for the launcher data directory (profiles, auth, logs, cache).
    #[serde(default)]
    pub data_dir: Option<PathBuf>,
    /// Custom Minecraft root directory (.minecraft equivalent).
    /// All versions, assets, libraries, and mods live under this directory.
    /// If None, defaults to `{data_dir}/instances`.
    #[serde(default)]
    pub game_root_dir: Option<PathBuf>,
    /// Default Java path (None = auto-detect).
    #[serde(default)]
    pub default_java_path: Option<String>,
    /// Default minimum heap size in MB (-Xms).
    #[serde(default = "default_min_memory")]
    pub default_min_memory_mb: u32,
    /// Default maximum heap size in MB (-Xmx).
    #[serde(default = "default_max_memory")]
    pub default_max_memory_mb: u32,
    /// Download concurrency (number of parallel workers).
    #[serde(default = "default_concurrency")]
    pub download_concurrency: u32,
    /// Optional download mirror URL (e.g. BMCLAPI).
    #[serde(default)]
    pub mirror_url: Option<String>,
    /// Event throttle interval in milliseconds.
    #[serde(default = "default_throttle")]
    pub event_throttle_ms: u64,
}

fn default_min_memory() -> u32 { 256 }
fn default_max_memory() -> u32 { 4096 }
fn default_concurrency() -> u32 { 16 }
fn default_throttle() -> u64 { 100 }

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            data_dir: None,
            game_root_dir: None,
            default_java_path: None,
            default_min_memory_mb: 256,
            default_max_memory_mb: 4096,
            download_concurrency: 16,
            mirror_url: None,
            event_throttle_ms: 100,
        }
    }
}

impl AppConfig {
    /// Load config from disk, or create default.
    pub fn load(data_dir: &std::path::Path) -> Result<Self> {
        let path = data_dir.join("config.json");
        if path.exists() {
            let content = std::fs::read_to_string(&path)?;
            Ok(serde_json::from_str(&content)?)
        } else {
            let config = Self::default();
            config.save(data_dir)?;
            Ok(config)
        }
    }

    /// Save config to disk.
    pub fn save(&self, data_dir: &std::path::Path) -> Result<()> {
        let path = data_dir.join("config.json");
        let content = serde_json::to_string_pretty(self)?;
        std::fs::write(&path, content)?;
        Ok(())
    }

    /// Resolve the effective data directory (for profiles, auth, logs, cache).
    pub fn resolved_data_dir(&self) -> PathBuf {
        self.data_dir
            .clone()
            .unwrap_or_else(|| paths::default_data_dir())
    }

    /// Resolve the effective game root directory.
    ///
    /// This is the equivalent of the `.minecraft` directory.
    /// All version JARs, assets, libraries, and mods live under this root.
    pub fn resolved_game_root(&self) -> PathBuf {
        self.game_root_dir
            .clone()
            .unwrap_or_else(|| self.resolved_data_dir().join("instances"))
    }

    /// Resolve the game directory for a specific profile.
    ///
    /// Follows Mojang's isolated directory layout:
    /// `{game_root}/{profile_id}/` containing:
    ///   - versions/{version_id}/{version_id}.jar + {version_id}.json
    ///   - libraries/ (shared)
    ///   - assets/ (shared)
    ///   - mods/
    ///   - resourcepacks/
    pub fn profile_game_dir(&self, profile_id: &str) -> PathBuf {
        self.resolved_game_root().join(profile_id)
    }
}
