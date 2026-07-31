//! Forge mod loader — version discovery and installation.
//!
//! Forge is the most complex modloader to install. It supports two installer
//! formats:
//!
//! - **New** (1.13+): [`ForgeNewInstallTask`] — parses `install_profile.json`,
//!   extracts embedded libraries, downloads processor dependencies, and executes
//!   processor steps with template variable substitution.
//! - **Old** (1.12-): [`ForgeOldInstallTask`] — extracts the universal JAR from
//!   the installer, uses embedded `versionInfo` directly as patch metadata.
//!
//! [`ForgeInstallTask`] routes between the two based on the presence of the
//! `spec` key in `install_profile.json`.
//!
//! # Backward compatibility
//!
//! [`ForgeInstaller`] still implements [`super::ModLoaderProvider`] for existing
//! CLI and Tauri commands.

pub mod profile;
pub mod version_list;
pub mod remote_version;
pub mod old_install;
pub mod new_install;
pub mod install_task;

pub use profile::{ForgeInstallProfile, ForgeNewInstallProfile, Processor, DataEntry};
pub use version_list::ForgeVersionList;
pub use remote_version::ForgeRemoteVersion;
pub use old_install::ForgeOldInstallTask;
pub use new_install::ForgeNewInstallTask;
pub use install_task::{ForgeInstallTask, ForgeInstallState};

// ── Legacy ModLoaderProvider (backward compat) ──

use crate::{LoaderVersion, ModLoaderProvider, ModLoaderType};
use rmccc_error::{RmcccError, Result};
use std::path::Path;

const FORGE_METADATA_URL: &str = "https://hmcl.glavo.site/metadata/forge/";

/// Legacy Forge installer — downloads and runs the official Forge installer JAR.
pub struct ForgeInstaller {
    client: reqwest::Client,
}

impl ForgeInstaller {
    pub fn new(client: reqwest::Client) -> Self {
        Self { client }
    }

    async fn list_versions_fallback(&self, game_version: &str) -> Result<Vec<LoaderVersion>> {
        const PROMOTIONS_URL: &str =
            "https://files.minecraftforge.net/net/minecraftforge/forge/promotions_slim.json";

        tracing::info!(
            game_version = %game_version,
            "Falling back to Forge promotions_slim"
        );

        let resp = self
            .client
            .get(PROMOTIONS_URL)
            .send()
            .await
            .map_err(|e| RmcccError::Network(e.to_string()))?;

        if !resp.status().is_success() {
            return Err(RmcccError::HttpStatus {
                status: resp.status().as_u16(),
                url: PROMOTIONS_URL.to_string(),
            });
        }

        let json: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| RmcccError::Network(e.to_string()))?;

        let mut versions = Vec::new();
        if let Some(promos) = json["promos"].as_object() {
            let latest_key = format!("{game_version}-latest");
            let recommended_key = format!("{game_version}-recommended");
            if let Some(latest) = promos.get(&latest_key).and_then(|v| v.as_str()) {
                versions.push(LoaderVersion { version: latest.to_string(), stable: false });
            }
            if let Some(recommended) = promos.get(&recommended_key).and_then(|v| v.as_str()) {
                if versions.iter().all(|v| v.version != recommended) {
                    versions.push(LoaderVersion { version: recommended.to_string(), stable: true });
                }
            }
        }
        Ok(versions)
    }
}

#[async_trait::async_trait]
impl ModLoaderProvider for ForgeInstaller {
    fn loader_type(&self) -> ModLoaderType {
        ModLoaderType::Forge
    }

    async fn list_versions(&self, game_version: &str) -> Result<Vec<LoaderVersion>> {
        tracing::info!(game_version = %game_version, "Fetching Forge versions from HMCL metadata");

        let resp = self
            .client
            .get(FORGE_METADATA_URL)
            .send()
            .await
            .map_err(|e| RmcccError::Network(e.to_string()))?;

        if !resp.status().is_success() {
            return self.list_versions_fallback(game_version).await;
        }

        let json: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| RmcccError::Network(e.to_string()))?;

        let mut versions = Vec::new();
        if let Some(game_versions) = json["gameVersions"].as_object() {
            let normalized_key = game_version.replace('-', "_");
            let entries = game_versions
                .get(game_version)
                .or_else(|| game_versions.get(&normalized_key));

            if let Some(builds) = entries.and_then(|v| v.as_array()) {
                let numbers = json["number"].as_object();
                for build_num in builds {
                    let num_str = build_num.to_string();
                    if let Some(numbers) = numbers {
                        if let Some(entry) = numbers.get(&num_str) {
                            if let Some(ver) = entry["version"].as_str() {
                                versions.push(LoaderVersion {
                                    version: ver.to_string(),
                                    stable: entry["modified"]
                                        .as_i64()
                                        .map(|m| m > 0)
                                        .unwrap_or(false),
                                });
                            }
                        }
                    }
                }
            }
        }

        if !versions.is_empty() {
            versions.sort_by(|a, b| b.version.cmp(&a.version));
            return Ok(versions);
        }

        self.list_versions_fallback(game_version).await
    }

    async fn install(
        &self,
        game_version: &str,
        loader_version: &str,
        game_dir: &Path,
        java_path: &Path,
    ) -> Result<String> {
        let version_id = format!("{game_version}-forge-{loader_version}");
        tracing::info!(
            game_version = %game_version,
            loader_version = %loader_version,
            version_id = %version_id,
            "Installing Forge"
        );

        let forge_version = format!("{game_version}-{loader_version}");
        let installer_url = format!(
            "https://maven.minecraftforge.net/net/minecraftforge/forge/{forge_version}/forge-{forge_version}-installer.jar"
        );
        let installer_path = game_dir.join(format!("forge-{game_version}-{loader_version}-installer.jar"));

        tracing::info!(url = %installer_url, "Downloading Forge installer");
        let resp = self
            .client
            .get(&installer_url)
            .send()
            .await
            .map_err(|e| RmcccError::Network(e.to_string()))?;

        if !resp.status().is_success() {
            return Err(RmcccError::DownloadFailed {
                url: installer_url,
                reason: format!("HTTP {}", resp.status()),
            });
        }

        let bytes = resp.bytes().await.map_err(|e| RmcccError::Network(e.to_string()))?;
        let cursor = std::io::Cursor::new(&bytes);
        match zip::ZipArchive::new(cursor) {
            Ok(_) => tracing::debug!("Forge installer ZIP integrity check passed"),
            Err(e) => return Err(RmcccError::Other(format!("Forge installer is not a valid ZIP/JAR: {e}"))),
        }
        std::fs::write(&installer_path, &bytes)?;

        // Create stub launcher_profiles.json if missing (required by NeoForge/Forge installers)
        let profiles_path = game_dir.join("launcher_profiles.json");
        if !profiles_path.exists() {
            std::fs::write(&profiles_path, "{\"profiles\":{}}")?;
        }

        tracing::info!(java = %java_path.display(), "Running Forge installer");
        let output = tokio::process::Command::new(java_path)
            .arg("-jar")
            .arg(&installer_path)
            .arg("--installClient")
            .arg(game_dir)
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .output()
            .await
            .map_err(|e| RmcccError::Other(format!("Failed to run Forge installer: {e}")))?;

        let _ = std::fs::remove_file(&installer_path);

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            let exit_code = output.status.code().map(|c| c.to_string()).unwrap_or_else(|| "unknown".into());
            return Err(RmcccError::Other(format!("Forge installer failed (exit {exit_code}): {stderr}")));
        }

        let version_json_path = game_dir.join("versions").join(&version_id).join(format!("{version_id}.json"));
        if !version_json_path.exists() {
            tracing::warn!("Forge installer completed but version JSON not found at {}", version_json_path.display());
            let versions_dir = game_dir.join("versions");
            if let Ok(entries) = std::fs::read_dir(&versions_dir) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.extension().map(|e| e == "json").unwrap_or(false) {
                        if let Ok(content) = std::fs::read_to_string(&path) {
                            if content.contains("forge") {
                                if let Some(name) = path.file_stem() {
                                    let found = name.to_string_lossy().to_string();
                                    tracing::info!("Found Forge version JSON: {}", found);
                                    return Ok(found);
                                }
                            }
                        }
                    }
                }
            }
        }

        tracing::info!(version_id = %version_id, "Forge installed successfully");
        Ok(version_id)
    }
}
