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

/// Raw Fabric Meta data: game-version keys + loader entries.
///
/// Perf audit M5: the old model materialized the full ~500×150 cartesian
/// product up front — ~75 000 allocated objects kept for the whole session —
/// to answer queries that touch a single game version. The per-game-version
/// lists are now built lazily (and cached) on first query.
#[derive(Default, Clone)]
struct FabricMeta {
    game_versions: std::collections::HashSet<String>,
    loaders: Vec<LoaderEntry>,
}

#[derive(Clone)]
struct LoaderEntry {
    version: String,
    stable: bool,
}

/// Version list for Fabric loader.
///
/// Fetches game versions and loader versions from Fabric Meta API,
/// then materializes remote versions per game version on demand.
pub struct FabricVersionList {
    client: reqwest::Client,
    meta: RwLock<Option<FabricMeta>>,
    /// game_version → materialized remote versions (lazy, cached).
    materialized: RwLock<HashMap<String, Vec<Arc<dyn RemoteVersion>>>>,
}

impl FabricVersionList {
    pub fn new(client: reqwest::Client) -> Self {
        Self {
            client,
            meta: RwLock::new(None),
            materialized: RwLock::new(HashMap::new()),
        }
    }

    fn materialize(&self, game_version: &str, meta: &FabricMeta) -> Vec<Arc<dyn RemoteVersion>> {
        meta.loaders
            .iter()
            .map(|lv| {
                Arc::new(FabricRemoteVersion {
                    game_version: game_version.to_string(),
                    loader_version: lv.version.clone(),
                    stable: lv.stable,
                }) as Arc<dyn RemoteVersion>
            })
            .collect()
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

        // Store raw keys + loader entries only; per-game-version lists
        // materialize lazily in `get_versions` (perf audit M5).
        let meta = FabricMeta {
            game_versions: game_versions.iter().map(|g| g.version.clone()).collect(),
            loaders: loader_versions
                .iter()
                .map(|lv| LoaderEntry {
                    version: lv.version.clone(),
                    stable: lv.stable,
                })
                .collect(),
        };

        if let Ok(mut guard) = self.meta.write() {
            *guard = Some(meta);
        }
        if let Ok(mut guard) = self.materialized.write() {
            guard.clear();
        }

        tracing::info!(
            game_count = game_versions.len(),
            loader_count = loader_versions.len(),
            "Fabric version list refreshed"
        );
        Ok(())
    }

    fn is_loaded_any(&self) -> bool {
        self.meta.read().map(|m| m.is_some()).unwrap_or(false)
    }

    fn is_loaded(&self, game_version: &str) -> bool {
        self.meta
            .read()
            .ok()
            .and_then(|m| m.as_ref().map(|m| m.game_versions.contains(game_version)))
            .unwrap_or(false)
    }

    fn get_versions(&self, game_version: &str) -> Vec<Arc<dyn RemoteVersion>> {
        if let Ok(cached) = self.materialized.read() {
            if let Some(rvs) = cached.get(game_version) {
                return rvs.clone();
            }
        }
        let Some(meta) = self
            .meta
            .read()
            .ok()
            .and_then(|m| m.as_ref().map(Clone::clone))
        else {
            return Vec::new();
        };
        if !meta.game_versions.contains(game_version) {
            return Vec::new();
        }
        let rvs = self.materialize(game_version, &meta);
        if let Ok(mut w) = self.materialized.write() {
            w.insert(game_version.to_string(), rvs.clone());
        }
        rvs
    }

    fn get_version(
        &self,
        game_version: &str,
        remote_version: &str,
    ) -> Option<Arc<dyn RemoteVersion>> {
        self.get_versions(game_version)
            .into_iter()
            .find(|rv| rv.self_version() == remote_version)
    }
}
