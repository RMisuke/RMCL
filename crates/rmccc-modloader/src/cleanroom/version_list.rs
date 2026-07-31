//! Cleanroom version list — Cleanroom Maven metadata.

use crate::version_list::VersionList;
use crate::remote_version::RemoteVersion;
#[allow(unused_imports)]
use crate::cleanroom::remote_version::CleanroomRemoteVersion;
use rmccc_error::{RmcccError, Result};
use serde::Deserialize;
use std::collections::HashMap;
use std::sync::{Arc, RwLock};

const CLEANROOM_MAVEN: &str = "https://maven.cleanroommc.com/releases/com/cleanroommc/cleanroom";

pub struct CleanroomVersionList {
    client: reqwest::Client,
    versions: RwLock<HashMap<String, Vec<Arc<dyn RemoteVersion>>>>,
}

impl CleanroomVersionList {
    pub fn new(client: reqwest::Client) -> Self {
        Self { client, versions: RwLock::new(HashMap::new()) }
    }
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct MavenMetadata {
    versioning: MavenVersioning,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct MavenVersioning {
    versions: Vec<String>,
}

#[async_trait::async_trait]
impl VersionList for CleanroomVersionList {
    fn has_type(&self) -> bool { false }

    async fn refresh(&self, _game_version: Option<&str>) -> Result<()> {
        let url = format!("{CLEANROOM_MAVEN}/maven-metadata.xml");
        let resp = self.client.get(&url).send().await
            .map_err(|e| RmcccError::Network(e.to_string()))?;
        if !resp.status().is_success() {
            return Err(RmcccError::HttpStatus { status: resp.status().as_u16(), url });
        }

        let xml = resp.text().await.map_err(|e| RmcccError::Network(e.to_string()))?;
        // Quick XML parse via quick-xml or string-based — use serde_xml_rs
        // For simplicity, just store versions for now
        tracing::info!(xml_len = xml.len(), "Cleanroom maven metadata fetched");
        Ok(())
    }

    fn is_loaded_any(&self) -> bool { self.versions.read().map(|g| !g.is_empty()).unwrap_or(false) }
    fn is_loaded(&self, _game_version: &str) -> bool { false }
    fn get_versions(&self, _game_version: &str) -> Vec<Arc<dyn RemoteVersion>> { vec![] }
    fn get_version(&self, _game_version: &str, _remote_version: &str) -> Option<Arc<dyn RemoteVersion>> { None }
}
