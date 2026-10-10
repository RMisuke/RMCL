//! OptiFine version list.

use crate::optifine::remote_version::OptiFineRemoteVersion;
use crate::remote_version::RemoteVersion;
use crate::version_list::VersionList;
use rmccc_error::{Result, RmcccError};
use serde::Deserialize;
use std::collections::HashMap;
use std::sync::{Arc, RwLock};

const BMCLAPI_ROOT: &str = "https://bmclapi2.bangbang93.com";

pub struct OptiFineVersionList {
    client: reqwest::Client,
    versions: RwLock<HashMap<String, Vec<Arc<dyn RemoteVersion>>>>,
}

impl OptiFineVersionList {
    pub fn new(client: reqwest::Client) -> Self {
        Self {
            client,
            versions: RwLock::new(HashMap::new()),
        }
    }
}

#[derive(Debug, Deserialize)]
struct OptiFineVersionEntry {
    #[serde(rename = "mcversion")]
    mc_version: String,
    #[serde(rename = "type")]
    version_type: String,
    patch: String,
    filename: String,
}

#[async_trait::async_trait]
impl VersionList for OptiFineVersionList {
    fn has_type(&self) -> bool {
        true
    }

    async fn refresh(&self, game_version: Option<&str>) -> Result<()> {
        let gv = game_version.unwrap_or("1.21");
        let url = format!("{BMCLAPI_ROOT}/optifine/{gv}");
        tracing::info!(url = %url, "Fetching OptiFine versions");

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

        let entries: Vec<OptiFineVersionEntry> = resp
            .json()
            .await
            .map_err(|e| RmcccError::Network(e.to_string()))?;

        let mut rvs: Vec<Arc<dyn RemoteVersion>> = Vec::new();
        for entry in &entries {
            let dl_url = format!(
                "{BMCLAPI_ROOT}/optifine/{gv}/{}/{}",
                entry.version_type, entry.filename
            );
            rvs.push(Arc::new(OptiFineRemoteVersion {
                game_version: entry.mc_version.clone(),
                optifine_version: entry.patch.clone(),
                download_url: dl_url,
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
