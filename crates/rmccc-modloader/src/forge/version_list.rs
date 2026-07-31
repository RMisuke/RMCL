//! Forge version list — fetches available Forge versions from HMCL metadata API.
//!
//! API: `GET https://hmcl.glavo.site/metadata/forge/`
//!
//! Returns structured JSON with `gameVersions` (game → build numbers) and
//! `number` (build number → version details). More comprehensive than
//! the official `promotions_slim.json`.

use crate::version_list::VersionList;
use crate::remote_version::RemoteVersion;
use crate::forge::remote_version::ForgeRemoteVersion;
use rmccc_error::{RmcccError, Result};
use serde::Deserialize;
use std::collections::HashMap;
use std::sync::{Arc, RwLock};

/// HMCL's custom Forge metadata endpoint.
const FORGE_METADATA_URL: &str = "https://hmcl.glavo.site/metadata/forge/";

/// Version list for Forge loader.
pub struct ForgeVersionList {
    client: reqwest::Client,
    versions: RwLock<HashMap<String, Vec<Arc<dyn RemoteVersion>>>>,
}

impl ForgeVersionList {
    pub fn new(client: reqwest::Client) -> Self {
        Self {
            client,
            versions: RwLock::new(HashMap::new()),
        }
    }
}

#[derive(Debug, Deserialize)]
struct ForgeMetadata {
    #[serde(rename = "mcversion")]
    game_versions: HashMap<String, Vec<i64>>,
    number: HashMap<String, ForgeBuildInfo>,
    #[serde(rename = "webpath")]
    web_path: Option<String>,
    artifact: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ForgeBuildInfo {
    version: String,
    #[serde(default)]
    modified: i64,
    #[serde(default)]
    game_version: String,
    #[serde(default)]
    branch: Option<String>,
    #[serde(default)]
    files: Vec<Vec<String>>,
}

#[async_trait::async_trait]
impl VersionList for ForgeVersionList {
    fn has_type(&self) -> bool {
        false
    }

    async fn refresh(&self, _game_version: Option<&str>) -> Result<()> {
        tracing::info!("Fetching Forge version list from HMCL metadata");

        let resp = self
            .client
            .get(FORGE_METADATA_URL)
            .send()
            .await
            .map_err(|e| RmcccError::Network(e.to_string()))?;

        if !resp.status().is_success() {
            return Err(RmcccError::HttpStatus {
                status: resp.status().as_u16(),
                url: FORGE_METADATA_URL.to_string(),
            });
        }

        let metadata: ForgeMetadata = resp
            .json()
            .await
            .map_err(|e| RmcccError::Network(format!("Failed to parse Forge metadata: {e}")))?;

        let web_path = metadata.web_path.as_deref().unwrap_or("");
        let artifact = metadata.artifact.as_deref().unwrap_or("");

        let mut map: HashMap<String, Vec<Arc<dyn RemoteVersion>>> = HashMap::new();
        for (game_version, build_numbers) in &metadata.game_versions {
            // Normalize game version (handle 1.7.10_pre4 vs 1.7.10-pre4)
            let lookup_version = if game_version == "1.7.10_pre4" {
                "1.7.10-pre4"
            } else {
                game_version.as_str()
            };

            let mut rvs: Vec<Arc<dyn RemoteVersion>> = Vec::new();
            for build_num in build_numbers {
                let num_str = build_num.to_string();
                if let Some(info) = metadata.number.get(&num_str) {
                    // Find the installer file URL
                    let mut installer_url = None;
                    for file in &info.files {
                        if file.len() > 1 && file[1] == "installer" {
                            let classifier = format!(
                                "{}-{}{}",
                                info.game_version,
                                info.version,
                                info.branch.as_ref()
                                    .filter(|b| !b.is_empty())
                                    .map(|b| format!("-{b}"))
                                    .unwrap_or_default()
                            );
                            let file_name = format!(
                                "{}-{}-{}.{}",
                                artifact, classifier, file[1], file[0]
                            );
                            installer_url = Some(format!("{}{}/{}", web_path, classifier, file_name));
                            break;
                        }
                    }

                    if let Some(url) = installer_url {
                        rvs.push(Arc::new(ForgeRemoteVersion {
                            game_version: lookup_version.to_string(),
                            forge_version: info.version.clone(),
                            installer_url: url,
                            modified: info.modified,
                        }));
                    }
                }
            }
            if !rvs.is_empty() {
                map.insert(lookup_version.to_string(), rvs);
            }
        }

        if let Ok(mut guard) = self.versions.write() {
            *guard = map;
        }

        tracing::info!(
            game_count = metadata.game_versions.len(),
            "Forge version list refreshed"
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
        self.versions
            .read()
            .ok()
            .and_then(|g| {
                g.get(game_version).and_then(|rvs| {
                    rvs.iter()
                        .find(|rv| rv.self_version() == remote_version)
                        .cloned()
                })
            })
    }
}
