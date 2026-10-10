//! Quilt version list — fetches available loader versions from the Quilt Meta API.
//!
//! API: `GET https://meta.quiltmc.org/v3/versions/loader` + `/versions/game`

use crate::quilt::remote_version::QuiltRemoteVersion;
use crate::remote_version::RemoteVersion;
use crate::version_list::VersionList;
use rmccc_error::{Result, RmcccError};
use std::collections::HashMap;
use std::sync::{Arc, RwLock};

/// Base URL for the Quilt Meta API.
const QUILT_META_BASE: &str = "https://meta.quiltmc.org/v3";

/// Version list for Quilt loader.
pub struct QuiltVersionList {
    client: reqwest::Client,
    versions: RwLock<HashMap<String, Vec<Arc<dyn RemoteVersion>>>>,
}

impl QuiltVersionList {
    pub fn new(client: reqwest::Client) -> Self {
        Self {
            client,
            versions: RwLock::new(HashMap::new()),
        }
    }
}

#[async_trait::async_trait]
impl VersionList for QuiltVersionList {
    fn has_type(&self) -> bool {
        false
    }

    async fn refresh(&self, _game_version: Option<&str>) -> Result<()> {
        tracing::info!("Fetching Quilt version list from meta.quiltmc.org");

        let game_url = format!("{QUILT_META_BASE}/versions/game");
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
        }

        let game_versions: Vec<GameVersionEntry> = game_resp
            .json()
            .await
            .map_err(|e| RmcccError::Network(e.to_string()))?;

        let loader_url = format!("{QUILT_META_BASE}/versions/loader");
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
        }

        let loader_versions: Vec<LoaderVersionEntry> = loader_resp
            .json()
            .await
            .map_err(|e| RmcccError::Network(e.to_string()))?;

        let mut map: HashMap<String, Vec<Arc<dyn RemoteVersion>>> = HashMap::new();
        for gv in &game_versions {
            let mut rvs: Vec<Arc<dyn RemoteVersion>> = Vec::new();
            for lv in &loader_versions {
                rvs.push(Arc::new(QuiltRemoteVersion {
                    game_version: gv.version.clone(),
                    loader_version: lv.version.clone(),
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
            "Quilt version list refreshed"
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
