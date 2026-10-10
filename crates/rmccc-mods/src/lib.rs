//! Mod management: Modrinth, CurseForge, local file management.

use ts_rs::TS;

pub mod modrinth;

use rmccc_error::{Result, RmcccError};
use serde::{Deserialize, Serialize};
use std::path::Path;

/// Mod search query.
#[derive(Debug, Clone)]
pub struct ModSearchQuery {
    pub query: String,
    pub game_version: Option<String>,
    pub loader: Option<String>,
    pub page: u32,
    pub limit: u32,
}

/// Mod search result page.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModSearchResult {
    pub hits: Vec<ModInfo>,
    pub total: u64,
    pub page: u32,
}

/// Mod metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModInfo {
    pub id: String,
    pub source: String,
    pub name: String,
    pub slug: String,
    pub authors: Vec<String>,
    pub description: String,
    pub download_count: u64,
    pub icon_url: Option<String>,
    pub categories: Vec<String>,
}

/// A specific version of a mod.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export_to = "types.ts")]
pub struct ModVersion {
    pub id: String,
    pub name: String,
    pub version_number: String,
    pub game_versions: Vec<String>,
    pub loaders: Vec<String>,
    pub download_url: Option<String>,
    /// Original filename from the source (e.g. `my-mod-1.0.0.jar`).
    #[serde(default)]
    pub filename: Option<String>,
    /// SHA1 完整性校验（P3-11）；远端未声明时为 None（跳过校验）。
    #[serde(default)]
    pub sha1: Option<String>,
}

/// Mod dependency.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModDependency {
    pub mod_id: String,
    pub version_constraint: Option<String>,
    pub dependency_type: DependencyType,
    pub source: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DependencyType {
    Required,
    Optional,
    Incompatible,
}

/// Trait for mod sources.
#[async_trait::async_trait]
pub trait ModSource: Send + Sync {
    fn source_id(&self) -> &str;
    async fn search(&self, query: &ModSearchQuery) -> Result<ModSearchResult>;
    async fn list_versions(&self, mod_id: &str) -> Result<Vec<ModVersion>>;
    async fn get_download_url(&self, version_id: &str) -> Result<String>;

    /// Download raw bytes from a URL using the source's HTTP client.
    async fn download_bytes(&self, url: &str) -> Result<Vec<u8>>;

    /// Install a mod version to the given jar path (download + write).
    ///
    /// Uses `version.download_url` if present, otherwise resolves it via
    /// [`Self::get_download_url`]. SHA1 verification is tracked separately (P3).
    async fn install(&self, version: &ModVersion, dest: &Path) -> Result<()> {
        let url = match &version.download_url {
            Some(u) => u.clone(),
            None => self.get_download_url(&version.id).await?,
        };
        let bytes = self.download_bytes(&url).await?;

        // P3-11：远端声明 SHA1 时校验完整性，防止下载损坏/被篡改的 jar。
        if let Some(ref expected) = version.sha1 {
            let actual = {
                use sha1::{Digest, Sha1};
                let mut hasher = Sha1::new();
                hasher.update(&bytes);
                format!("{:x}", hasher.finalize())
            };
            if !actual.eq_ignore_ascii_case(expected) {
                return Err(RmcccError::HashMismatch {
                    path: url,
                    expected: expected.clone(),
                    actual,
                });
            }
        }

        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent).map_err(RmcccError::Io)?;
        }
        std::fs::write(dest, bytes).map_err(RmcccError::Io)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};

    /// Spawn a local HTTP server returning fixed bytes (no network / wiremock).
    fn spawn_bytes_server(body: &'static [u8]) -> String {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        std::thread::spawn(move || {
            if let Ok((mut socket, _)) = listener.accept() {
                let mut buf = [0u8; 4096];
                let _ = socket.read(&mut buf);
                let _ = socket.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 4\r\n\r\n");
                let _ = socket.write_all(body);
            }
        });
        format!("http://{addr}/mod.jar")
    }

    /// 验证 `ModSource::install` 默认实现能把 download_url 下载并写入目标路径（P0-02）。
    #[tokio::test]
    async fn test_install_downloads_to_dest() {
        let url = spawn_bytes_server(b"JAR!");
        let source = crate::modrinth::ModrinthSource::new(reqwest::Client::new());
        let version = ModVersion {
            id: "v1".into(),
            name: "test-mod".into(),
            version_number: "1.0.0".into(),
            game_versions: vec!["1.21".into()],
            loaders: vec!["fabric".into()],
            download_url: Some(url),
            filename: Some("test-mod-1.0.0.jar".into()),
            sha1: None,
        };

        let dest = std::env::temp_dir().join(format!(
            "rmccc-mods-test-{}-{}.jar",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));

        source.install(&version, &dest).await.unwrap();
        let data = std::fs::read(&dest).unwrap();
        assert_eq!(data, b"JAR!");
        let _ = std::fs::remove_file(&dest);
    }
}
