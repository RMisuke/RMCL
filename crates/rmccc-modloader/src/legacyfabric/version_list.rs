//! Legacy Fabric version list — Legacy Fabric Meta API.

use crate::legacyfabric::remote_version::LegacyFabricRemoteVersion;
use crate::remote_version::RemoteVersion;
use crate::version_list::VersionList;
use rmccc_error::{Result, RmcccError};
use serde::Deserialize;
use std::collections::HashMap;
use std::sync::{Arc, RwLock};

const LEGACY_FABRIC_META: &str = "https://meta.legacyfabric.net/v2";

pub struct LegacyFabricVersionList {
    client: reqwest::Client,
    versions: RwLock<HashMap<String, Vec<Arc<dyn RemoteVersion>>>>,
}

impl LegacyFabricVersionList {
    pub fn new(client: reqwest::Client) -> Self {
        Self {
            client,
            versions: RwLock::new(HashMap::new()),
        }
    }
}

#[derive(Debug, Deserialize)]
struct LegacyLoaderEntry {
    loader: LegacyLoaderInfo,
}

#[derive(Debug, Deserialize)]
struct LegacyLoaderInfo {
    version: String,
    stable: bool,
}

#[async_trait::async_trait]
impl VersionList for LegacyFabricVersionList {
    fn has_type(&self) -> bool {
        false
    }

    async fn refresh(&self, game_version: Option<&str>) -> Result<()> {
        let gv = game_version.unwrap_or("1.12.2");
        let url = format!("{LEGACY_FABRIC_META}/versions/loader/{gv}");
        tracing::info!(url = %url, "Fetching Legacy Fabric versions");

        let resp = self
            .client
            .get(&url)
            .send()
            .await
            .map_err(|e| RmcccError::Network(e.to_string()))?;
        if !resp.status().is_success() {
            return Err(RmcccError::HttpStatus {
                status: resp.status().as_u16(),
                url,
            });
        }

        let entries: Vec<LegacyLoaderEntry> = resp
            .json()
            .await
            .map_err(|e| RmcccError::Network(e.to_string()))?;

        let mut rvs: Vec<Arc<dyn RemoteVersion>> = Vec::new();
        for entry in &entries {
            let meta_url = format!(
                "{LEGACY_FABRIC_META}/versions/loader/{gv}/{}",
                entry.loader.version
            );
            rvs.push(Arc::new(LegacyFabricRemoteVersion {
                game_version: gv.to_string(),
                loader_version: entry.loader.version.clone(),
                meta_url,
                stable: entry.loader.stable,
            }));
        }

        if let Ok(mut guard) = self.versions.write() {
            guard.insert(gv.to_string(), rvs);
        }
        Ok(())
    }

    fn is_loaded_any(&self) -> bool {
        self.versions.read().map(|g| !g.is_empty()).unwrap_or(false)
    }
    fn is_loaded(&self, game_version: &str) -> bool {
        self.versions
            .read()
            .map(|g| g.contains_key(game_version))
            .unwrap_or(false)
    }
    fn get_versions(&self, game_version: &str) -> Vec<Arc<dyn RemoteVersion>> {
        self.versions
            .read()
            .ok()
            .and_then(|g| g.get(game_version).cloned())
            .unwrap_or_default()
    }
    fn get_version(
        &self,
        game_version: &str,
        remote_version: &str,
    ) -> Option<Arc<dyn RemoteVersion>> {
        self.versions.read().ok().and_then(|g| {
            g.get(game_version).and_then(|rvs| {
                rvs.iter()
                    .find(|rv| rv.self_version() == remote_version)
                    .cloned()
            })
        })
    }
}
