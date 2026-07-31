//! Mod management: Modrinth, CurseForge, local file management.

use rmccc_error::Result;
use serde::{Deserialize, Serialize};

/// Mod search query.
#[derive(Debug, Clone)]
pub struct ModSearchQuery {
    pub query: String,
    pub game_version: Option<String>,
    pub loader: Option<String>,
    pub page: u32,
    pub limit: u32,
}

/// Mod search result page.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModSearchResult {
    pub hits: Vec<ModInfo>,
    pub total: u64,
    pub page: u32,
}

/// Mod metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModInfo {
    pub id: String,
    pub source: String,
    pub name: String,
    pub slug: String,
    pub authors: Vec<String>,
    pub description: String,
    pub download_count: u64,
    pub icon_url: Option<String>,
    pub categories: Vec<String>,
}

/// A specific version of a mod.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModVersion {
    pub id: String,
    pub name: String,
    pub version_number: String,
    pub game_versions: Vec<String>,
    pub loaders: Vec<String>,
    pub download_url: Option<String>,
}

/// Mod dependency.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModDependency {
    pub mod_id: String,
    pub version_constraint: Option<String>,
    pub dependency_type: DependencyType,
    pub source: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DependencyType {
    Required,
    Optional,
    Incompatible,
}

/// Trait for mod sources.
#[async_trait::async_trait]
pub trait ModSource: Send + Sync {
    fn source_id(&self) -> &str;
    async fn search(&self, query: &ModSearchQuery) -> Result<ModSearchResult>;
    async fn list_versions(&self, mod_id: &str) -> Result<Vec<ModVersion>>;
    async fn get_download_url(&self, version_id: &str) -> Result<String>;
}
