//! Fabric mod loader — version discovery and installation.
//!
//! # Architecture
//!
//! - [`FabricVersionList`] implements [`VersionList`] — fetches game+loader versions
//!   from Fabric Meta API, builds cartesian product.
//! - [`FabricRemoteVersion`] implements [`RemoteVersion`] — holds game/loader version
//!   combo, creates the install task.
//! - [`FabricInstallTask`] implements [`InstallTask`] — fetches launch metadata,
//!   parses `FabricInfo`, builds a [`GameInstancePatch`].
//!
//! # Backward compatibility
//!
//! [`FabricInstaller`] still implements [`super::ModLoaderProvider`] for existing
//! CLI and Tauri commands that haven't migrated to the new trait architecture.

pub mod install_task;
pub mod remote_version;
pub mod version_list;

pub use install_task::FabricInstallTask;
pub use remote_version::FabricRemoteVersion;
pub use version_list::FabricVersionList;

// ── Legacy ModLoaderProvider (backward compat) ──

use crate::{LoaderVersion, ModLoaderProvider, ModLoaderType};
use rmccc_error::{Result, RmcccError};
use std::path::Path;

const FABRIC_META_BASE: &str = "https://meta.fabricmc.net/v2";

/// Legacy Fabric installer — uses the `/profile/json` endpoint.
pub struct FabricInstaller {
    client: reqwest::Client,
}

impl FabricInstaller {
    pub fn new(client: reqwest::Client) -> Self {
        Self { client }
    }

    async fn fetch_profile_json(
        &self,
        game_version: &str,
        loader_version: &str,
    ) -> Result<serde_json::Value> {
        let url = format!(
            "{FABRIC_META_BASE}/versions/loader/{game_version}/{loader_version}/profile/json"
        );

        tracing::info!(
            game_version = %game_version,
            loader_version = %loader_version,
            "Fetching Fabric profile JSON"
        );

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

        let json: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| RmcccError::Other(format!("Failed to parse Fabric profile JSON: {e}")))?;

        Ok(json)
    }
}

#[async_trait::async_trait]
impl ModLoaderProvider for FabricInstaller {
    fn loader_type(&self) -> ModLoaderType {
        ModLoaderType::Fabric
    }

    async fn list_versions(&self, game_version: &str) -> Result<Vec<LoaderVersion>> {
        let url = format!("{FABRIC_META_BASE}/versions/loader/{game_version}");
        tracing::info!(game_version = %game_version, "Fetching Fabric loader versions");

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

        #[derive(serde::Deserialize)]
        struct FabricLoaderEntry {
            loader: FabricLoaderInfo,
        }

        #[derive(serde::Deserialize)]
        struct FabricLoaderInfo {
            version: String,
            stable: bool,
        }

        let entries: Vec<FabricLoaderEntry> = resp
            .json()
            .await
            .map_err(|e| RmcccError::Network(e.to_string()))?;

        Ok(entries
            .into_iter()
            .map(|entry| LoaderVersion {
                version: entry.loader.version,
                stable: entry.loader.stable,
            })
            .collect())
    }

    async fn install(
        &self,
        game_version: &str,
        loader_version: &str,
        game_dir: &Path,
        _java_path: &Path,
    ) -> Result<String> {
        let version_id = format!("{game_version}-fabric-{loader_version}");
        let versions_dir = game_dir.join("versions").join(&version_id);
        std::fs::create_dir_all(&versions_dir)?;

        tracing::info!(
            game_version = %game_version,
            loader_version = %loader_version,
            version_id = %version_id,
            "Installing Fabric"
        );

        let profile = self
            .fetch_profile_json(game_version, loader_version)
            .await?;

        let main_class = match &profile["mainClass"] {
            serde_json::Value::String(s) => s.clone(),
            serde_json::Value::Object(obj) => obj
                .get("client")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
                .unwrap_or_else(|| "net.fabricmc.loader.impl.launch.knot.KnotClient".to_string()),
            _ => "net.fabricmc.loader.impl.launch.knot.KnotClient".to_string(),
        };

        let mut version_json = profile;
        if let Some(obj) = version_json.as_object_mut() {
            obj.insert(
                "id".to_string(),
                serde_json::Value::String(version_id.clone()),
            );
            obj.insert(
                "mainClass".to_string(),
                serde_json::Value::String(main_class),
            );
        }

        let json_path = versions_dir.join(format!("{version_id}.json"));
        std::fs::write(&json_path, serde_json::to_string_pretty(&version_json)?)?;

        tracing::info!(version_id = %version_id, "Fabric installed successfully");
        Ok(version_id)
    }
}
