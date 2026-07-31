//! LiteLoader version list.

use crate::version_list::VersionList;
use crate::remote_version::RemoteVersion;
use crate::liteloader::remote_version::LiteLoaderRemoteVersion;
use rmccc_error::{RmcccError, Result};
use serde::Deserialize;
use std::collections::HashMap;
use std::sync::{Arc, RwLock};

const LITELOADER_VERSIONS_URL: &str = "http://dl.liteloader.com/versions/versions.json";

pub struct LiteLoaderVersionList {
    client: reqwest::Client,
    versions: RwLock<HashMap<String, Vec<Arc<dyn RemoteVersion>>>>,
}

impl LiteLoaderVersionList {
    pub fn new(client: reqwest::Client) -> Self {
        Self { client, versions: RwLock::new(HashMap::new()) }
    }
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct LiteLoaderRoot {
    versions: std::collections::HashMap<String, LiteLoaderGameEntry>,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct LiteLoaderGameEntry {
    #[serde(default)]
    repo: Vec<LiteLoaderRepoEntry>,
    #[serde(default)]
    artefacts: std::collections::HashMap<String, LiteLoaderArtefact>,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct LiteLoaderRepoEntry {
    stream: String,
    #[serde(default)]
    versions: Vec<LiteLoaderVersionEntry>,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct LiteLoaderVersionEntry {
    version: String,
    #[serde(default)]
    file: String,
    #[serde(default)]
    md5: String,
    #[serde(default)]
    url: Option<String>,
    #[serde(default)]
    type_field: Option<String>,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct LiteLoaderArtefact {
    version: String,
    url: Option<String>,
    md5: Option<String>,
    #[serde(default)]
    libraries: Vec<LiteLoaderLibrary>,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct LiteLoaderLibrary {
    name: String,
    url: Option<String>,
}

#[async_trait::async_trait]
impl VersionList for LiteLoaderVersionList {
    fn has_type(&self) -> bool { false }

    async fn refresh(&self, _game_version: Option<&str>) -> Result<()> {
        tracing::info!("Fetching LiteLoader versions");

        let resp = self.client.get(LITELOADER_VERSIONS_URL).send().await
            .map_err(|e| RmcccError::Network(e.to_string()))?;
        if !resp.status().is_success() {
            return Err(RmcccError::HttpStatus { status: resp.status().as_u16(), url: LITELOADER_VERSIONS_URL.into() });
        }

        let root: LiteLoaderRoot = resp.json().await
            .map_err(|e| RmcccError::Network(e.to_string()))?;

        let mut map: HashMap<String, Vec<Arc<dyn RemoteVersion>>> = HashMap::new();
        for (gv, entry) in &root.versions {
            let mut rvs: Vec<Arc<dyn RemoteVersion>> = Vec::new();
            for repo in &entry.repo {
                for v in &repo.versions {
                    let url = v.url.clone().unwrap_or_else(|| {
                        format!("http://dl.liteloader.com/versions/{}/{}/{}/{}", gv, v.version, v.file, v.file)
                    });
                    rvs.push(Arc::new(LiteLoaderRemoteVersion {
                        game_version: gv.clone(),
                        loader_version: v.version.clone(),
                        url,
                        libraries: vec![],
                    }));
                }
            }
            if !rvs.is_empty() {
                map.insert(gv.clone(), rvs);
            }
        }

        if let Ok(mut guard) = self.versions.write() {
            *guard = map;
        }
        Ok(())
    }

    fn is_loaded_any(&self) -> bool { self.versions.read().map(|g| !g.is_empty()).unwrap_or(false) }
    fn is_loaded(&self, game_version: &str) -> bool { self.versions.read().map(|g| g.contains_key(game_version)).unwrap_or(false) }
    fn get_versions(&self, game_version: &str) -> Vec<Arc<dyn RemoteVersion>> {
        self.versions.read().ok().and_then(|g| g.get(game_version).cloned()).unwrap_or_default()
    }
    fn get_version(&self, game_version: &str, remote_version: &str) -> Option<Arc<dyn RemoteVersion>> {
        self.versions.read().ok().and_then(|g| {
            g.get(game_version).and_then(|rvs| rvs.iter().find(|rv| rv.self_version() == remote_version).cloned())
        })
    }
}
