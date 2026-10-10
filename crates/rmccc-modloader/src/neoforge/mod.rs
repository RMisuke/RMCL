//! NeoForge mod loader — version discovery and installation.
//!
//! NeoForge reuses Forge's installer profile format but has its own
//! version discovery API and installer detection logic.
//!
//! # Architecture
//!
//! - [`NeoForgeVersionList`] implements [`VersionList`] — BMCLAPI endpoint
//! - [`NeoForgeRemoteVersion`] implements [`RemoteVersion`]
//! - [`NeoForgeInstallTask`] implements [`InstallTask`] — detects NEOFORGE.RSA
//!   signature, routes to Forge's new/old install tasks with patch id renaming

pub mod install_task;
pub mod remote_version;
pub mod version_list;

pub use install_task::NeoForgeInstallTask;
pub use remote_version::NeoForgeRemoteVersion;
pub use version_list::NeoForgeVersionList;

// ── Legacy ModLoaderProvider (backward compat) ──

use crate::{LoaderVersion, ModLoaderProvider, ModLoaderType};
use rmccc_error::{Result, RmcccError};
use std::path::Path;

const NEOFORGE_MAVEN: &str = "https://maven.neoforged.net/releases/net/neoforged";
const NEOFORGE_VERSIONS_URL: &str =
    "https://maven.neoforged.net/api/maven/versions/releases/net/neoforged/neoforge";

/// Legacy NeoForge installer — downloads and runs the official installer JAR.
pub struct NeoForgeInstaller {
    client: reqwest::Client,
}

impl NeoForgeInstaller {
    pub fn new(client: reqwest::Client) -> Self {
        Self { client }
    }
}

#[async_trait::async_trait]
impl ModLoaderProvider for NeoForgeInstaller {
    fn loader_type(&self) -> ModLoaderType {
        ModLoaderType::NeoForge
    }

    async fn list_versions(&self, game_version: &str) -> Result<Vec<LoaderVersion>> {
        tracing::info!(game_version = %game_version, "Fetching NeoForge versions");
        let major_minor: Vec<&str> = game_version.split('.').collect();
        let prefix = if major_minor.len() >= 2 {
            format!("{}.{}", major_minor[1], major_minor.get(2).unwrap_or(&"0"))
        } else {
            game_version.to_string()
        };

        let resp = self
            .client
            .get(NEOFORGE_VERSIONS_URL)
            .send()
            .await
            .map_err(|e| RmcccError::Network(e.to_string()))?;

        if !resp.status().is_success() {
            return Err(RmcccError::HttpStatus {
                status: resp.status().as_u16(),
                url: NEOFORGE_VERSIONS_URL.to_string(),
            });
        }

        #[derive(serde::Deserialize)]
        struct MavenVersions {
            versions: Vec<String>,
        }

        let data: MavenVersions = resp
            .json()
            .await
            .map_err(|e| RmcccError::Network(e.to_string()))?;
        Ok(data
            .versions
            .into_iter()
            .filter(|v| v.starts_with(&prefix))
            .map(|v| LoaderVersion {
                stable: !v.contains("beta") && !v.contains("alpha"),
                version: v,
            })
            .collect())
    }

    async fn install(
        &self,
        game_version: &str,
        loader_version: &str,
        game_dir: &Path,
        java_path: &Path,
    ) -> Result<String> {
        let version_id = format!("{game_version}-neoforge-{loader_version}");
        tracing::info!(game_version = %game_version, loader_version = %loader_version, version_id = %version_id, "Installing NeoForge");

        // Try both artifact names: "neoforge" (newer) and "forge" (older, MC 1.20.1 era)
        let candidate_urls = [
            format!("{NEOFORGE_MAVEN}/neoforge/{loader_version}/neoforge-{loader_version}-installer.jar"),
            format!("{NEOFORGE_MAVEN}/forge/{loader_version}/forge-{loader_version}-installer.jar"),
        ];

        let mut installer_bytes = None;
        let mut last_err = String::new();
        for url in &candidate_urls {
            let resp = self
                .client
                .get(url)
                .send()
                .await
                .map_err(|e| RmcccError::Network(e.to_string()))?;
            if resp.status().is_success() {
                let bytes = resp
                    .bytes()
                    .await
                    .map_err(|e| RmcccError::Network(e.to_string()))?;
                installer_bytes = Some((bytes, url.clone()));
                break;
            }
            last_err = format!("{} — HTTP {}", url, resp.status());
        }

        let (bytes, used_url) = installer_bytes.ok_or_else(|| RmcccError::DownloadFailed {
            url: candidate_urls[0].clone(),
            reason: format!("All URLs failed. Last error: {last_err}"),
        })?;

        tracing::info!(url = %used_url, "NeoForge installer downloaded");
        let installer_path = game_dir.join("neoforge-installer.jar");
        std::fs::write(&installer_path, &bytes)?;

        // Create stub launcher_profiles.json if missing (required by NeoForge/Forge installers)
        let profiles_path = game_dir.join("launcher_profiles.json");
        if !profiles_path.exists() {
            std::fs::write(&profiles_path, "{\"profiles\":{}}")?;
        }

        let output = tokio::process::Command::new(java_path)
            .arg("-jar")
            .arg(&installer_path)
            .arg("--installClient")
            .arg(game_dir)
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .output()
            .await
            .map_err(|e| RmcccError::Other(format!("Failed to run NeoForge installer: {e}")))?;

        let _ = std::fs::remove_file(&installer_path);
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(RmcccError::Other(format!(
                "NeoForge installer failed: {stderr}"
            )));
        }

        // Detect the actual version ID created by the installer
        let versions_dir = game_dir.join("versions");
        let actual_version = if versions_dir.exists() {
            let mut entries: Vec<_> = std::fs::read_dir(&versions_dir)
                .ok()
                .into_iter()
                .flatten()
                .filter_map(|e| e.ok())
                .filter(|e| e.file_type().map(|t| t.is_dir()).unwrap_or(false))
                .filter_map(|e| {
                    let name = e.file_name();
                    let name = name.to_string_lossy().to_string();
                    // Find directories created after the vanilla one
                    if name != game_version && !name.starts_with('.') {
                        Some(name)
                    } else {
                        None
                    }
                })
                .collect();
            entries.sort();
            entries.last().unwrap_or(&version_id).clone()
        } else {
            version_id
        };

        tracing::info!(actual_version = %actual_version, "NeoForge installer created version");
        Ok(actual_version)
    }
}
