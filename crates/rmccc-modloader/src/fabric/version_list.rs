//! Fabric version list — fetches available loader versions from the Fabric Meta API.
//!
//! API: `GET https://meta.fabricmc.net/v2/versions/loader` + `/versions/game`

use crate::fabric::remote_version::FabricRemoteVersion;
use crate::remote_version::RemoteVersion;
use crate::version_list::VersionList;
use rmccc_error::{Result, RmcccError};
use std::collections::HashMap;
use std::sync::{Arc, RwLock};

/// Base URL for the Fabric Meta API.
const FABRIC_META_BASE: &str = "https://meta.fabricmc.net/v2";

/// Version list for Fabric loader.
///
/// Fetches game versions and loader versions from Fabric Meta API,
/// then generates the cartesian product for all version combinations.
pub struct FabricVersionList {
    client: reqwest::Client,
    /// game_version → sorted set of remote versions
    versions: RwLock<HashMap<String, Vec<Arc<dyn RemoteVersion>>>>,
}

impl FabricVersionList {
    pub fn new(client: reqwest::Client) -> Self {
        Self {
            client,
            versions: RwLock::new(HashMap::new()),
        }
    }
}

#[async_trait::async_trait]
impl VersionList for FabricVersionList {
    fn has_type(&self) -> bool {
        false
    }

    async fn refresh(&self, _game_version: Option<&str>) -> Result<()> {
        tracing::info!("Fetching Fabric version list from meta.fabricmc.net");

        // Fetch game versions
        let game_url = format!("{FABRIC_META_BASE}/versions/game");
        let game_resp = self
            .client
            .get(&game_url)
            .send()
            .await
            .map_err(|e| RmcccError::Network(e.to_string()))?;

        if !game_resp.status().is_success() {
            return Err(RmcccError::HttpStatus {
                status: game_resp.status().as_u16(),
                url: game_url,
            });
        }

        #[derive(serde::Deserialize)]
        struct GameVersionEntry {
            version: String,
            #[allow(dead_code)]
            stable: bool,
        }

        let game_versions: Vec<GameVersionEntry> = game_resp
            .json()
            .await
            .map_err(|e| RmcccError::Network(e.to_string()))?;

        // Fetch loader versions
        let loader_url = format!("{FABRIC_META_BASE}/versions/loader");
        let loader_resp = self
            .client
            .get(&loader_url)
            .send()
            .await
            .map_err(|e| RmcccError::Network(e.to_string()))?;

        if !loader_resp.status().is_success() {
            return Err(RmcccError::HttpStatus {
                status: loader_resp.status().as_u16(),
                url: loader_url,
            });
        }

        #[derive(serde::Deserialize)]
        struct LoaderVersionEntry {
            version: String,
            stable: bool,
        }

        let loader_versions: Vec<LoaderVersionEntry> = loader_resp
            .json()
            .await
            .map_err(|e| RmcccError::Network(e.to_string()))?;

        // Build cartesian product
        let mut map: HashMap<String, Vec<Arc<dyn RemoteVersion>>> = HashMap::new();
        for gv in &game_versions {
            let mut rvs: Vec<Arc<dyn RemoteVersion>> = Vec::new();
            for lv in &loader_versions {
                rvs.push(Arc::new(FabricRemoteVersion {
                    game_version: gv.version.clone(),
                    loader_version: lv.version.clone(),
                    stable: lv.stable,
                }));
            }
            map.insert(gv.version.clone(), rvs);
        }

        if let Ok(mut guard) = self.versions.write() {
            *guard = map;
        }

        tracing::info!(
            game_count = game_versions.len(),
            loader_count = loader_versions.len(),
            "Fabric version list refreshed"
        );
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
