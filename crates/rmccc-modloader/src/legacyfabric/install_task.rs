//! Legacy Fabric install task.

use crate::analyzer::LibraryType;
use crate::install_task::InstallTask;
use crate::patch::{GameInstancePatch, PRIORITY_LOADER};
use rmccc_error::{Result, RmcccError};
use rmccc_version::{Library, VersionArguments};

pub struct LegacyFabricInstallTask {
    /// Shared HTTP client from the DependencyManager (perf audit M6).
    pub client: reqwest::Client,
    pub game_version: String,
    pub loader_version: String,
    pub meta_url: String,
}

#[async_trait::async_trait]
impl InstallTask for LegacyFabricInstallTask {
    async fn execute(&mut self) -> Result<GameInstancePatch> {
        tracing::info!(game_version = %self.game_version, loader = %self.loader_version, "Executing Legacy Fabric install task");

        let client = self.client.clone();
        let resp = client
            .get(&self.meta_url)
            .send()
            .await
            .map_err(|e| RmcccError::Network(e.to_string()))?;
        if !resp.status().is_success() {
            return Err(RmcccError::HttpStatus {
                status: resp.status().as_u16(),
                url: self.meta_url.clone(),
            });
        }

        let json: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| RmcccError::Network(e.to_string()))?;

        // Extract mainClass
        let main_class = match &json["launcherMeta"]["mainClass"] {
            serde_json::Value::String(s) => s.clone(),
            serde_json::Value::Object(obj) => obj
                .get("client")
                .and_then(|v| v.as_str())
                .map(String::from)
                .unwrap_or_else(|| "net.fabricmc.loader.launch.knot.KnotClient".into()),
            _ => "net.fabricmc.loader.launch.knot.KnotClient".into(),
        };

        // Extract libraries（批次 4：客户端取 common+client，此前误取 server 侧）
        let mut libraries: Vec<Library> = Vec::new();
        if let Some(libs) = json["launcherMeta"]["libraries"].as_object() {
            for side in &["common", "client"] {
                if let Some(arr) = libs.get(*side).and_then(|v| v.as_array()) {
                    for entry in arr {
                        if let Some(name) = entry.get("name").and_then(|v| v.as_str()) {
                            libraries.push(Library {
                                name: name.into(),
                                ..Default::default()
                            });
                        }
                    }
                }
            }
        }

        // Add loader + intermediary
        if let Some(loader) = json["loader"].as_object() {
            if let Some(maven) = loader.get("maven").and_then(|v| v.as_str()) {
                libraries.push(Library {
                    name: maven.into(),
                    ..Default::default()
                });
            }
        }
        if let Some(intermediary) = json["intermediary"].as_object() {
            if let Some(maven) = intermediary.get("maven").and_then(|v| v.as_str()) {
                libraries.push(Library {
                    name: maven.into(),
                    ..Default::default()
                });
            }
        }

        Ok(GameInstancePatch::with_loader(
            LibraryType::LegacyFabric.patch_id(),
            &self.loader_version,
            PRIORITY_LOADER,
            Some(main_class),
            Some(VersionArguments {
                jvm: None,
                game: None,
            }),
            libraries,
        ))
    }

    fn description(&self) -> &str {
        "Installing Legacy Fabric"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_legacy_fabric_patch_id() {
        assert_eq!(LibraryType::LegacyFabric.patch_id(), "legacyfabric");
    }
}
