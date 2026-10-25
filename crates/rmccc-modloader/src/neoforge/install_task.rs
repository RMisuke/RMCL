//! NeoForge install task — reuses Forge's installer format.
//!
//! HMCL-compatible: detects NEOFORGE.RSA signature, routes to Forge
//! new/old install tasks, then renames the patch id from "forge" to "neoforge".

use crate::analyzer::LibraryType;
use crate::install_task::InstallTask;
use crate::patch::{GameInstancePatch, PRIORITY_LOADER};
use rmccc_error::{Result, RmcccError};
use std::io::Read;

pub struct NeoForgeInstallTask {
    pub game_version: String,
    pub neo_version: String,
    pub installer_url: String,
    /// Shared HTTP client (reused from provider).
    pub client: reqwest::Client,
}

#[async_trait::async_trait]
impl InstallTask for NeoForgeInstallTask {
    async fn execute(&mut self) -> Result<GameInstancePatch> {
        tracing::info!(game_version = %self.game_version, neo_version = %self.neo_version, "Executing NeoForge install task");

        // 批次 4：下载走共享实现（此前固定名临时文件、无 ZIP/SHA 校验、
        // 且执行失败路径不清理——泄漏 ~180MB）。
        let (installer_path, _used_url) =
            crate::installer::download_installer_file(&self.client, &[self.installer_url.clone()])
                .await?;
        let result = self.execute_with_installer(&installer_path).await;
        let _ = std::fs::remove_file(&installer_path);
        result
    }

    fn description(&self) -> &str {
        "Installing NeoForge"
    }
}

impl NeoForgeInstallTask {
    async fn execute_with_installer(
        &self,
        installer_path: &std::path::Path,
    ) -> Result<GameInstancePatch> {
        // Read install_profile.json to detect format
        let profile_json = read_install_profile(installer_path)?;
        let parsed: serde_json::Value = serde_json::from_str(&profile_json)
            .map_err(|e| RmcccError::Other(format!("Invalid install_profile.json: {e}")))?;

        let profile_type = parsed.get("profile").and_then(|v| v.as_str()).unwrap_or("");

        // Check for NEOFORGE.RSA signature inside the installer JAR.
        // NeoForge 1.20.1-era installers use `profile: "forge"` but carry
        // a NEOFORGE.RSA signature file. This is HMCL's three-path routing.
        let has_neoforge_rsa = {
            let file = std::fs::File::open(installer_path)?;
            zip::ZipArchive::new(file)
                .map(|mut a| a.by_name("META-INF/NEOFORGE.RSA").is_ok())
                .unwrap_or(false)
        };

        let patch = if profile_type == "forge" && has_neoforge_rsa {
            // Path A: Old NeoForge (1.20.1 era) masquerading as Forge format
            // Uses Forge's install_profile.json but with NEOFORGE.RSA signature.
            tracing::info!("Detected old NeoForge installer (forge profile + NEOFORGE.RSA)");
            let version_info = crate::forge::ForgeInstallProfile::deserialize_any(&profile_json)?;
            let version_info = version_info.version_info;
            GameInstancePatch::with_loader(
                LibraryType::NeoForge.patch_id(),
                &self.neo_version,
                PRIORITY_LOADER,
                Some(version_info.main_class),
                version_info.arguments,
                version_info.libraries,
            )
        } else if profile_type == "neoforge" || profile_type == "NeoForge" {
            // Path B: New NeoForge (26.2+) with native neoforge profile
            tracing::info!("Detected new NeoForge installer (neoforge profile)");
            let profile: crate::forge::ForgeNewInstallProfile = serde_json::from_str(&profile_json)
                .map_err(|e| RmcccError::Other(format!("Failed to parse: {e}")))?;

            // Read version JSON from installer
            let file = std::fs::File::open(installer_path)?;
            let mut archive = zip::ZipArchive::new(file)
                .map_err(|e| RmcccError::Other(format!("Cannot open NeoForge installer: {e}")))?;
            let json_path = profile.json.trim_start_matches('/');
            let mut version_json = String::new();
            archive
                .by_name(json_path)
                .map_err(|e| RmcccError::Other(format!("Version JSON not found: {e}")))?
                .read_to_string(&mut version_json)?;
            let version_info: rmccc_version::GameVersion = serde_json::from_str(&version_json)
                .map_err(|e| RmcccError::Other(format!("Failed to parse version JSON: {e}")))?;

            GameInstancePatch::with_loader(
                LibraryType::NeoForge.patch_id(),
                &self.neo_version,
                PRIORITY_LOADER,
                Some(version_info.main_class),
                version_info.arguments,
                version_info.libraries,
            )
        } else {
            return Err(RmcccError::Other(format!(
                "Unsupported NeoForge installer profile type: {} (NEOFORGE.RSA: {})",
                profile_type, has_neoforge_rsa
            )));
        };

        Ok(patch)
    }
}

fn read_install_profile(path: &std::path::Path) -> Result<String> {
    let file = std::fs::File::open(path)?;
    let mut archive = zip::ZipArchive::new(file)
        .map_err(|e| RmcccError::Other(format!("Not a valid ZIP: {e}")))?;
    let mut json = String::new();
    archive
        .by_name("install_profile.json")
        .map_err(|e| RmcccError::Other(format!("install_profile.json not found: {e}")))?
        .read_to_string(&mut json)?;
    Ok(json)
}

// Add a helper trait for deserializing old Forge profiles
impl crate::forge::ForgeInstallProfile {
    fn deserialize_any(json: &str) -> Result<Self> {
        serde_json::from_str(json).map_err(|e| RmcccError::Other(format!("Failed to parse: {e}")))
    }
}
