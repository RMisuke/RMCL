//! NeoForge version list — BMCLAPI 镜像优先，maven.neoforged.net 官方回退。
//!
//! # HMCL alignment
//!
//! 官方列表来自 HMCL `NeoForgeOfficialVersionList`：
//! - 1.20.1 世代：`net/neoforged/forge` 元数据（安装器在
//!   `net/neoforged/forge/{v}/forge-{v}-installer.jar`，v 形如 `1.20.1-47.1.105`）；
//! - 1.20.2+：`net/neoforged/neoforge` 元数据（安装器在
//!   `net/neoforged/neoforge/{v}/neoforge-{v}-installer.jar`），
//!   MC 版本号从 NeoForge 版本号推算（HMCL 的分段算法，见
//!   [`derive_mc_version`]）。
//!
//! 镜像（BMCLAPI）作为主源与全库一致；失败时整体回退官方 API。

use crate::neoforge::remote_version::NeoForgeRemoteVersion;
use crate::remote_version::RemoteVersion;
use crate::version_list::VersionList;
use rmccc_error::{Result, RmcccError};
use serde::Deserialize;
use std::collections::HashMap;
use std::sync::{Arc, RwLock};

const BMCLAPI_ROOT: &str = "https://bmclapi2.bangbang93.com";
const OFFICIAL_MAVEN: &str = "https://maven.neoforged.net";
/// 1.20.1 世代的元数据（挂在 net/neoforged/forge 下）。
const OFFICIAL_OLD_META: &str =
    "https://maven.neoforged.net/api/maven/versions/releases/net/neoforged/forge";
/// 1.20.2+ 的元数据。
const OFFICIAL_META: &str =
    "https://maven.neoforged.net/api/maven/versions/releases/net/neoforged/neoforge";

pub struct NeoForgeVersionList {
    client: reqwest::Client,
    versions: RwLock<HashMap<String, Vec<Arc<dyn RemoteVersion>>>>,
}

impl NeoForgeVersionList {
    pub fn new(client: reqwest::Client) -> Self {
        Self {
            client,
            versions: RwLock::new(HashMap::new()),
        }
    }

    /// 官方回退：一次取两份元数据（1.20.1 世代 + 现代），合并入库。
    async fn refresh_official(&self) -> Result<()> {
        let (old, modern) = tokio::try_join!(
            fetch_official_versions(&self.client, OFFICIAL_OLD_META),
            fetch_official_versions(&self.client, OFFICIAL_META),
        )?;

        let mut by_mc: HashMap<String, Vec<Arc<dyn RemoteVersion>>> = HashMap::new();

        for raw in old {
            // 1.20.1 世代：version 形如 "1.20.1-47.1.105"（或含 "forge-"），
            // 归一后作展示版本；安装器在 net/neoforged/forge/ 下。
            let neo_version = NeoForgeRemoteVersion::normalize(&raw);
            let urls = vec![
                format!("{BMCLAPI_ROOT}/neoforge/version/{raw}/download/installer.jar"),
                format!(
                    "{OFFICIAL_MAVEN}/releases/net/neoforged/forge/{raw}/forge-{raw}-installer.jar"
                ),
            ];
            by_mc
                .entry("1.20.1".to_string())
                .or_default()
                .push(Arc::new(NeoForgeRemoteVersion {
                    game_version: "1.20.1".to_string(),
                    neo_version,
                    installer_urls: urls,
                }));
        }

        for raw in modern {
            let Some(mc_version) = derive_mc_version(&raw) else {
                tracing::warn!(version = %raw, "unsupported NeoForge version; skipping");
                continue;
            };
            let neo_version = NeoForgeRemoteVersion::normalize(&raw);
            let urls = vec![
                format!("{BMCLAPI_ROOT}/neoforge/version/{raw}/download/installer.jar"),
                official_installer_url(&raw, &mc_version),
            ];
            by_mc
                .entry(mc_version.clone())
                .or_default()
                .push(Arc::new(NeoForgeRemoteVersion {
                    game_version: mc_version,
                    neo_version,
                    installer_urls: urls,
                }));
        }

        if let Ok(mut guard) = self.versions.write() {
            *guard = by_mc;
        }
        Ok(())
    }

    async fn refresh_bmclapi(&self, gv: &str) -> Result<()> {
        let url = format!("{BMCLAPI_ROOT}/neoforge/list/{gv}");
        tracing::info!(url = %url, "Fetching NeoForge versions");

        let resp = self
            .client
            .get(&url)
            .send()
            .await
            .map_err(|e| RmcccError::Network(e.to_string()))?;
        if !resp.status().is_success() {
            // 镜像失败 → 官方回退（与下载源回退机制同思路）。
            tracing::warn!(status = %resp.status(), "BMCLAPI NeoForge list failed; falling back to official");
            return self.refresh_official().await;
        }

        let entries: Result<Vec<NeoForgeVersionEntry>> = resp
            .json()
            .await
            .map_err(|e| RmcccError::Network(e.to_string()));
        let entries = match entries {
            Ok(e) => e,
            Err(e) => {
                tracing::warn!(error = %e, "BMCLAPI NeoForge list malformed; falling back to official");
                return self.refresh_official().await;
            }
        };

        let mut rvs: Vec<Arc<dyn RemoteVersion>> = Vec::new();
        for entry in &entries {
            let neo_version = NeoForgeRemoteVersion::normalize(&entry.version);
            let urls = vec![
                format!(
                    "{BMCLAPI_ROOT}/neoforge/version/{}/download/installer.jar",
                    entry.version
                ),
                official_installer_url(&entry.version, &entry.mc_version),
            ];
            rvs.push(Arc::new(NeoForgeRemoteVersion {
                game_version: entry.mc_version.clone(),
                neo_version,
                installer_urls: urls,
            }));
        }

        if let Ok(mut guard) = self.versions.write() {
            guard.insert(gv.to_string(), rvs);
        }
        Ok(())
    }
}

#[derive(Debug, Deserialize)]
struct NeoForgeVersionEntry {
    version: String,
    #[serde(rename = "mcversion")]
    mc_version: String,
}

#[derive(Debug, Deserialize)]
struct OfficialVersions {
    versions: Vec<String>,
}

async fn fetch_official_versions(client: &reqwest::Client, url: &str) -> Result<Vec<String>> {
    let resp = client
        .get(url)
        .send()
        .await
        .map_err(|e| RmcccError::Network(e.to_string()))?;
    if !resp.status().is_success() {
        return Err(RmcccError::HttpStatus {
            status: resp.status().as_u16(),
            url: url.to_string(),
        });
    }
    let parsed: OfficialVersions = resp
        .json()
        .await
        .map_err(|e| RmcccError::Network(e.to_string()))?;
    Ok(parsed.versions)
}

/// 官方安装器 URL（按世代分库：1.20.1 → net/neoforged/forge，其余 → neoforge）。
fn official_installer_url(raw_version: &str, mc_version: &str) -> String {
    if mc_version == "1.20.1" {
        format!(
            "{OFFICIAL_MAVEN}/releases/net/neoforged/forge/{raw_version}/forge-{raw_version}-installer.jar"
        )
    } else {
        format!(
            "{OFFICIAL_MAVEN}/releases/net/neoforged/neoforge/{raw_version}/neoforge-{raw_version}-installer.jar"
        )
    }
}

/// HMCL `NeoForgeOfficialVersionList` 的 MC 版本推算：
/// - 主版本 ≥ 26（MC 26.x 新纪元）：次段为 0 → 取前两段，否则取前三段；
///   `+suffix` 变体拼在后面（`26.1.12+beta` → `26.1-beta`）；
/// - 主版本 == 0：快照线，取第二段（HMCL 原样）；
/// - 其余（20.x/21.x 世代）：次段为 0 → 取首段，否则取前两段，前面补 `1.`。
///   例：`20.4.233` → `1.20.4`、`21.0.42` → `1.21`。
pub fn derive_mc_version(version: &str) -> Option<String> {
    let si1 = version.find('.')?;
    let rest1 = &version[si1 + 1..];
    let rel2 = rest1.find('.')?;
    let si2 = si1 + 1 + rel2;
    let major: u32 = version[..si1].parse().ok()?;
    let second: u32 = version[si1 + 1..si2].parse().ok()?;

    if major >= 26 {
        // HMCL 原样：26.x 需要 4 段版本号（如 `26.2.0.86`），缺第三段则跳过；
        // 判据是**第三段**==0（MC 的补位段）→ 取前两段作 MC 版本。
        let si3 = version[si2 + 1..].find('.').map(|r| si2 + 1 + r)?;
        let third: u32 = version[si2 + 1..si3].parse().ok()?;
        let ver = if third == 0 {
            version[..si2].to_string()
        } else {
            version[..si3].to_string()
        };
        // `+` 后缀变体（如预发布）：`26.1.12+beta` → `26.1-beta`
        match version.find('+') {
            Some(sep) => Some(format!("{}-{}", ver, &version[sep + 1..])),
            None => Some(ver),
        }
    } else if major == 0 {
        // HMCL 快照线：取第二段
        Some(version[si1 + 1..si2].to_string())
    } else {
        let ver = if second == 0 {
            version[..si1].to_string()
        } else {
            version[..si2].to_string()
        };
        Some(format!("1.{ver}"))
    }
}

#[async_trait::async_trait]
impl VersionList for NeoForgeVersionList {
    fn has_type(&self) -> bool {
        true
    }

    async fn refresh(&self, game_version: Option<&str>) -> Result<()> {
        match game_version {
            Some(gv) => self.refresh_bmclapi(gv).await,
            // 全量刷新（无具体 MC 版本）：官方元数据覆盖全部世代。
            None => self.refresh_official().await,
        }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_derive_mc_version_modern() {
        assert_eq!(derive_mc_version("20.4.233").as_deref(), Some("1.20.4"));
        assert_eq!(derive_mc_version("21.0.42").as_deref(), Some("1.21"));
        assert_eq!(derive_mc_version("21.1.77").as_deref(), Some("1.21.1"));
        // MC 26.x 新纪元：4 段版本号（HMCL 原样——缺第三段的 26.x 版本跳过）
        assert_eq!(derive_mc_version("26.2.0.86").as_deref(), Some("26.2"));
        assert_eq!(derive_mc_version("26.1.0.12").as_deref(), Some("26.1"));
        assert_eq!(derive_mc_version("26.1.0"), None);
        // 带后缀变体
        assert_eq!(
            derive_mc_version("26.1.0.12+beta").as_deref(),
            Some("26.1-beta")
        );
    }

    #[test]
    fn test_derive_mc_version_malformed() {
        assert_eq!(derive_mc_version("garbage"), None);
        assert_eq!(derive_mc_version(""), None);
        assert_eq!(derive_mc_version("20"), None);
    }

    #[test]
    fn test_official_installer_url_by_generation() {
        assert_eq!(
            official_installer_url("1.20.1-47.1.105", "1.20.1"),
            format!(
                "{OFFICIAL_MAVEN}/releases/net/neoforged/forge/1.20.1-47.1.105/forge-1.20.1-47.1.105-installer.jar"
            )
        );
        assert_eq!(
            official_installer_url("20.4.233", "1.20.4"),
            format!(
                "{OFFICIAL_MAVEN}/releases/net/neoforged/neoforge/20.4.233/neoforge-20.4.233-installer.jar"
            )
        );
    }
}
