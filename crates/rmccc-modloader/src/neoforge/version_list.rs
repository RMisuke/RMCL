//! NeoForge version list — BMCLAPI endpoint.

use crate::version_list::VersionList;
use crate::remote_version::RemoteVersion;
use crate::neoforge::remote_version::NeoForgeRemoteVersion;
use rmccc_error::{RmcccError, Result};
use serde::Deserialize;
use std::collections::HashMap;
use std::sync::{Arc, RwLock};

const BMCLAPI_ROOT: &str = "https://bmclapi2.bangbang93.com";

pub struct NeoForgeVersionList {
    client: reqwest::Client,
    versions: RwLock<HashMap<String, Vec<Arc<dyn RemoteVersion>>>>,
}

impl NeoForgeVersionList {
    pub fn new(client: reqwest::Client) -> Self {
        Self { client, versions: RwLock::new(HashMap::new()) }
    }
}

#[derive(Debug, Deserialize)]
struct NeoForgeVersionEntry {
    version: String,
    #[serde(rename = "mcversion")]
    mc_version: String,
}

#[async_trait::async_trait]
impl VersionList for NeoForgeVersionList {
    fn has_type(&self) -> bool { true }

    async fn refresh(&self, game_version: Option<&str>) -> Result<()> {
        let gv = game_version.unwrap_or("1.21");
        let url = format!("{BMCLAPI_ROOT}/neoforge/list/{gv}");
        tracing::info!(url = %url, "Fetching NeoForge versions");

        let resp = self.client.get(&url).send().await
            .map_err(|e| RmcccError::Network(e.to_string()))?;
        if !resp.status().is_success() {
            return Err(RmcccError::HttpStatus { status: resp.status().as_u16(), url });
        }

        let entries: Vec<NeoForgeVersionEntry> = resp.json().await
            .map_err(|e| RmcccError::Network(e.to_string()))?;

        let mut rvs: Vec<Arc<dyn RemoteVersion>> = Vec::new();
        for entry in &entries {
            let installer_url = format!("{BMCLAPI_ROOT}/neoforge/version/{}/download/installer.jar", entry.version);
            rvs.push(Arc::new(NeoForgeRemoteVersion {
                game_version: entry.mc_version.clone(),
                neo_version: entry.version.clone(),
                installer_url,
            }));
        }

        if let Ok(mut guard) = self.versions.write() {
            guard.insert(gv.to_string(), rvs);
        }
        Ok(())
    }

    fn is_loaded_any(&self) -> bool { self.versions.read().map(|g| !g.is_empty()).unwrap_or(false) }
    fn is_loaded(&self, game_version: &str) -> bool { self.versions.read().map(|g| g.contains_key(game_version)).unwrap_or(false) }
    fn get_versions(&self, game_version: &str) -> Vec<Arc<dyn RemoteVersion>> {
        self.versions.read().ok().and_then(|g| g.get(game_version).cloned()).unwrap_or_default()
    }
    fn get_version(&self, game_version: &str, remote_version: &str) -> Option<Arc<dyn RemoteVersion>> {
        self.versions.read().ok().and_then(|g| g.get(game_version).and_then(|rvs| rvs.iter().find(|rv| rv.self_version() == remote_version).cloned()))
    }
}
