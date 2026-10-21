//! Modrinth API v2 integration.
//!
//! Implements the [`ModSource`] trait for modrinth.com.
//! See: <https://docs.modrinth.com/api/>

use crate::{ModInfo, ModSearchQuery, ModSearchResult, ModSource, ModVersion};
use rmccc_error::{Result, RmcccError};
use serde::Deserialize;

const MODRINTH_API_BASE: &str = "https://api.modrinth.com/v2";

/// Modrinth API client implementing [`ModSource`].
pub struct ModrinthSource {
    client: reqwest::Client,
    base_url: String,
}

impl ModrinthSource {
    pub fn new(client: reqwest::Client) -> Self {
        Self {
            client,
            base_url: MODRINTH_API_BASE.to_string(),
        }
    }

    /// 自定义 API base（P1-06：测试可注入 mock server）。
    #[allow(dead_code)] // 仅测试用；保持公开 API 简洁
    pub fn with_base_url(client: reqwest::Client, base_url: String) -> Self {
        Self { client, base_url }
    }
}

#[async_trait::async_trait]
impl ModSource for ModrinthSource {
    fn source_id(&self) -> &str {
        "modrinth"
    }

    async fn search(&self, query: &ModSearchQuery) -> Result<ModSearchResult> {
        let mut facets = Vec::new();

        if let Some(ref gv) = query.game_version {
            facets.push(format!(r#"["versions:{}"]"#, gv));
        }
        if let Some(ref loader) = query.loader {
            facets.push(format!(r#"["categories:{}"]"#, loader));
        }

        let params = [
            ("query", query.query.clone()),
            (
                "offset",
                ((query.page.saturating_sub(1)) * query.limit).to_string(),
            ),
            ("limit", query.limit.to_string()),
        ];

        let mut req = self
            .client
            .get(format!("{}/search", self.base_url))
            .query(&params);

        if !facets.is_empty() {
            req = req.query(&[("facets", format!("[{}]", facets.join(",")))]);
        }

        let resp = req
            .send()
            .await
            .map_err(|e| RmcccError::Network(e.to_string()))?;

        if !resp.status().is_success() {
            return Err(RmcccError::Network(format!(
                "Modrinth search returned {}",
                resp.status()
            )));
        }

        let status = resp.status();
        let body = resp.text().await.map_err(|e| {
            RmcccError::Network(format!("version body decode failed ({status}): {e:?}"))
        })?;

        let raw: SearchResponse = serde_json::from_str(&body)
            .map_err(|e| RmcccError::Other(format!("Modrinth search parse error: {e}")))?;

        let hits: Vec<ModInfo> = raw
            .hits
            .into_iter()
            .map(|h| ModInfo {
                id: h.project_id,
                source: "modrinth".into(),
                name: h.title,
                slug: h.slug,
                authors: vec![h.author],
                description: h.description,
                download_count: h.downloads,
                icon_url: h.icon_url,
                categories: h.categories,
            })
            .collect();

        Ok(ModSearchResult {
            total: raw.total_hits,
            page: query.page,
            hits,
        })
    }

    async fn list_versions(&self, mod_id: &str) -> Result<Vec<ModVersion>> {
        let url = format!("{}/project/{mod_id}/version", self.base_url);
        let resp = self
            .client
            .get(&url)
            .query(&[("loaders", r#"["fabric","forge","neoforge","quilt"]"#)])
            .send()
            .await
            .map_err(|e| RmcccError::Network(e.to_string()))?;

        let body = resp
            .text()
            .await
            .map_err(|e| RmcccError::Network(e.to_string()))?;

        let raw: Vec<VersionEntry> = serde_json::from_str(&body)
            .map_err(|e| RmcccError::Other(format!("Modrinth version parse error: {e}")))?;

        let versions: Vec<ModVersion> = raw
            .into_iter()
            .map(|v| ModVersion {
                id: v.id,
                name: v.name,
                version_number: v.version_number,
                game_versions: v.game_versions,
                loaders: v.loaders,
                download_url: v.files.first().map(|f| f.url.clone()),
                filename: v.files.first().map(|f| f.filename.clone()),
                // P3-11：Modrinth 提供 sha1，填充供 install 校验
                sha1: v.files.first().and_then(|f| f.hashes.sha1.clone()),
            })
            .collect();

        Ok(versions)
    }

    async fn get_download_url(&self, version_id: &str) -> Result<String> {
        let url = format!("{}/version/{version_id}", self.base_url);
        let resp = self
            .client
            .get(&url)
            .send()
            .await
            .map_err(|e| RmcccError::Network(e.to_string()))?;

        let body = resp
            .text()
            .await
            .map_err(|e| RmcccError::Network(e.to_string()))?;

        let raw: VersionEntry = serde_json::from_str(&body)
            .map_err(|e| RmcccError::Other(format!("Modrinth version parse error: {e}")))?;

        raw.files
            .first()
            .map(|f| f.url.clone())
            .ok_or_else(|| RmcccError::Other("No download URL found".into()))
    }

    async fn download_bytes(&self, url: &str) -> Result<Vec<u8>> {
        let resp = self
            .client
            .get(url)
            .send()
            .await
            .map_err(|e| RmcccError::Network(e.to_string()))?;

        if !resp.status().is_success() {
            return Err(RmcccError::Network(format!(
                "Mod download returned {}",
                resp.status()
            )));
        }

        let status = resp.status();
        resp.bytes()
            .await
            .map(|b| b.to_vec())
            .map_err(|e| RmcccError::Network(format!("mod body decode failed ({status}): {e:?}")))
    }
}

// ── Modrinth API response types ──

#[derive(Debug, Deserialize)]
struct SearchResponse {
    hits: Vec<SearchHit>,
    #[serde(rename = "total_hits")]
    total_hits: u64,
}

#[derive(Debug, Deserialize)]
struct SearchHit {
    project_id: String,
    title: String,
    slug: String,
    author: String,
    description: String,
    downloads: u64,
    icon_url: Option<String>,
    categories: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct VersionEntry {
    id: String,
    name: String,
    version_number: String,
    game_versions: Vec<String>,
    loaders: Vec<String>,
    files: Vec<VersionFile>,
}

#[derive(Debug, Deserialize, Default)]
#[allow(dead_code)]
struct VersionFile {
    url: String,
    filename: String,
    #[serde(default)]
    hashes: FileHashes,
}

#[derive(Debug, Deserialize, Default)]
#[allow(dead_code)]
struct FileHashes {
    /// Modrinth 返回的 SHA1（用于下载完整性校验，P3-11）。
    #[serde(default)]
    sha1: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ModSearchQuery, ModSource};
    use std::io::{Read, Write};

    /// 启动本地 HTTP server 返回固定 body（P1-06：无网络集成测试）。
    fn spawn_json_server(body: &'static str) -> String {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let body = body.to_string();
        std::thread::spawn(move || {
            if let Ok((mut socket, _)) = listener.accept() {
                let mut buf = [0u8; 4096];
                let _ = socket.read(&mut buf);
                let headers = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    body.len()
                );
                let _ = socket.write_all(headers.as_bytes());
                let _ = socket.write_all(body.as_bytes());
            }
        });
        format!("http://{addr}")
    }

    /// Modrinth `/search` 响应解析（P1-06）。
    #[tokio::test]
    async fn test_search_parses_hits() {
        let body = r#"{
            "total_hits": 2,
            "hits": [
                {
                    "project_id": "abc123",
                    "title": "Example Mod",
                    "slug": "example-mod",
                    "author": "dev",
                    "description": "A test mod",
                    "downloads": 1234,
                    "icon_url": "https://example.com/icon.png",
                    "categories": ["fabric", "utility"]
                },
                {
                    "project_id": "def456",
                    "title": "Other Mod",
                    "slug": "other-mod",
                    "author": "other",
                    "description": "Another",
                    "downloads": 99,
                    "icon_url": null,
                    "categories": ["forge"]
                }
            ]
        }"#;
        let base = spawn_json_server(body);
        let source = ModrinthSource::with_base_url(reqwest::Client::new(), base);

        let q = ModSearchQuery {
            query: "mod".into(),
            game_version: Some("1.21".into()),
            loader: Some("fabric".into()),
            page: 1,
            limit: 10,
        };
        let result = source.search(&q).await.expect("search should succeed");
        assert_eq!(result.total, 2);
        assert_eq!(result.hits.len(), 2);
        assert_eq!(result.hits[0].id, "abc123");
        assert_eq!(result.hits[0].name, "Example Mod");
        assert_eq!(result.hits[0].categories, vec!["fabric", "utility"]);
        assert_eq!(result.hits[0].source, "modrinth");
    }

    /// Modrinth `/project/{id}/version` 响应解析（P1-06）。
    #[tokio::test]
    async fn test_list_versions_parses() {
        let body = r#"[
            {
                "id": "v1",
                "name": "1.0.0",
                "version_number": "1.0.0",
                "game_versions": ["1.21"],
                "loaders": ["fabric"],
                "files": [
                    { "url": "https://example.com/mod-1.0.0.jar", "filename": "mod-1.0.0.jar" }
                ]
            }
        ]"#;
        let base = spawn_json_server(body);
        let source = ModrinthSource::with_base_url(reqwest::Client::new(), base);

        let versions = source
            .list_versions("abc123")
            .await
            .expect("list versions should succeed");
        assert_eq!(versions.len(), 1);
        assert_eq!(versions[0].id, "v1");
        assert_eq!(versions[0].version_number, "1.0.0");
        assert_eq!(versions[0].game_versions, vec!["1.21"]);
        assert_eq!(versions[0].loaders, vec!["fabric"]);
        assert_eq!(
            versions[0].download_url.as_deref(),
            Some("https://example.com/mod-1.0.0.jar")
        );
        assert_eq!(versions[0].filename.as_deref(), Some("mod-1.0.0.jar"));
    }

    /// Modrinth `/version/{id}` 下载 URL 解析（P1-06）。
    #[tokio::test]
    async fn test_get_download_url() {
        let body = r#"{
            "id": "v1",
            "name": "1.0.0",
            "version_number": "1.0.0",
            "game_versions": ["1.21"],
            "loaders": ["fabric"],
            "files": [
                { "url": "https://example.com/mod.jar", "filename": "mod.jar" }
            ]
        }"#;
        let base = spawn_json_server(body);
        let source = ModrinthSource::with_base_url(reqwest::Client::new(), base);

        let url = source
            .get_download_url("v1")
            .await
            .expect("download url should resolve");
        assert_eq!(url, "https://example.com/mod.jar");
    }
}
