//! NeoForge remote version.

use crate::install_task::InstallTask;
use crate::remote_version::{RemoteVersion, VersionType};
use chrono::{DateTime, Utc};
use rmccc_error::Result;

#[derive(Debug, Clone)]
pub struct NeoForgeRemoteVersion {
    pub game_version: String,
    pub neo_version: String,
    /// 下载候选 = [镜像, 官方回退]（`RemoteVersion::urls` 暴露，安装任务使用）。
    pub installer_urls: Vec<String>,
}

impl NeoForgeRemoteVersion {
    /// Normalize NeoForge version strings for 1.20.1-era naming inconsistencies.
    ///
    /// In the 1.20.1 era, NeoForge used Forge-style naming:
    /// - `"1.20.1-forge-26.2.86"` → `"26.2.86"`
    /// - `"1.20.1-26.2.86"` → `"26.2.86"`
    ///
    /// Versions not matching the 1.20.1 pattern are returned unchanged.
    ///
    /// This mirrors HMCL's [`NeoForgeRemoteVersion.normalize()`].
    pub fn normalize(version: &str) -> String {
        let prefix = "1.20.1-";
        if let Some(rest) = version.strip_prefix(prefix) {
            rest.strip_prefix("forge-").unwrap_or(rest).to_string()
        } else {
            version.to_string()
        }
    }
}

impl RemoteVersion for NeoForgeRemoteVersion {
    fn library_id(&self) -> &str {
        "neoforge"
    }
    fn game_version(&self) -> &str {
        &self.game_version
    }
    fn self_version(&self) -> &str {
        &self.neo_version
    }
    fn urls(&self) -> &[String] {
        &self.installer_urls
    }
    fn release_date(&self) -> Option<DateTime<Utc>> {
        None
    }
    fn version_type(&self) -> VersionType {
        VersionType::Release
    }

    fn create_install_task(
        &self,
        dm: &super::super::DependencyManager,
        _manifest: &rmccc_version::GameVersion,
        ctx: &super::super::remote_version::InstallTaskContext<'_>,
    ) -> Result<Box<dyn InstallTask>> {
        Ok(Box::new(super::install_task::NeoForgeInstallTask {
            game_version: self.game_version.clone(),
            neo_version: self.neo_version.clone(),
            installer_urls: self.installer_urls.clone(),
            java_path: ctx.java_path.to_path_buf(),
            game_dir: ctx.game_dir.to_path_buf(),
            platform: dm.platform_descriptor(),
            client: dm.client_for_tasks(),
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normalize_standard_version() {
        // Non-1.20.1 versions pass through unchanged
        assert_eq!(NeoForgeRemoteVersion::normalize("26.2.86"), "26.2.86");
        assert_eq!(NeoForgeRemoteVersion::normalize("20.4.233"), "20.4.233");
        assert_eq!(NeoForgeRemoteVersion::normalize("47.1.105"), "47.1.105");
    }

    #[test]
    fn test_normalize_1_20_1_forge_prefix() {
        // "1.20.1-forge-XX" → "XX"
        assert_eq!(
            NeoForgeRemoteVersion::normalize("1.20.1-forge-26.2.86"),
            "26.2.86"
        );
        assert_eq!(
            NeoForgeRemoteVersion::normalize("1.20.1-forge-47.1.105"),
            "47.1.105"
        );
    }

    #[test]
    fn test_normalize_1_20_1_no_forge_prefix() {
        // "1.20.1-XX" → "XX"
        assert_eq!(
            NeoForgeRemoteVersion::normalize("1.20.1-26.2.86"),
            "26.2.86"
        );
    }

    #[test]
    fn test_normalize_non_1_20_1_with_forge_in_name() {
        // "20.4-forge-xxx" should NOT be stripped (only 1.20.1 prefix)
        assert_eq!(
            NeoForgeRemoteVersion::normalize("20.4-forge-123"),
            "20.4-forge-123"
        );
    }
}
