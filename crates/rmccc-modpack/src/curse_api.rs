//! Minimal CurseForge metadata client used to resolve modpack files that only
//! carry `projectID` + `fileID`.
//!
//! Official API requests need an API key (`CURSEFORGE_API_KEY`). Without one we
//! fall back to the public mirror HMCL already lists in its BMCLAPI rules;
//! download URLs returned by both are official `edge.forgecdn.net` links.

use rmccc_error::{Result, RmcccError};
use serde::Deserialize;

const OFFICIAL: &str = "https://api.curseforge.com";
const MIRROR: &str = "https://mod.mcimirror.top/curseforge";

/// Resolved CurseForge file metadata.
#[derive(Debug, Clone)]
pub struct CurseFileInfo {
    pub file_name: String,
    pub download_url: Option<String>,
    pub sha1: Option<String>,
    pub size: Option<u64>,
}

/// CurseForge metadata client.
#[derive(Clone)]
pub struct CurseApi {
    client: reqwest::Client,
    api_key: Option<String>,
}

impl CurseApi {
    /// Build a client. `api_key` is the optional official key.
    pub fn new(client: reqwest::Client, api_key: Option<String>) -> Self {
        let api_key = api_key.filter(|key| !key.trim().is_empty());
        Self { client, api_key }
    }

    /// Read `CURSEFORGE_API_KEY` from the environment.
    pub fn api_key_from_env() -> Option<String> {
        std::env::var("CURSEFORGE_API_KEY").ok()
    }

    async fn get_json(&self, path: &str) -> Result<serde_json::Value> {
        let mut last_error = None;
        if let Some(key) = &self.api_key {
            match self.get_json_at(OFFICIAL, path, Some(key)).await {
                Ok(value) => return Ok(value),
                Err(error) => last_error = Some(error),
            }
        }
        match self.get_json_at(MIRROR, path, None).await {
            Ok(value) => Ok(value),
            Err(error) => Err(last_error.unwrap_or(error)),
        }
    }

    async fn get_json_at(
        &self,
        base: &str,
        path: &str,
        key: Option<&str>,
    ) -> Result<serde_json::Value> {
        let url = format!("{base}{path}");
        let mut request = self
            .client
            .get(&url)
            .header(reqwest::header::ACCEPT, "application/json");
        if let Some(key) = key {
            request = request.header("x-api-key", key);
        }
        let response = request
            .send()
            .await
            .map_err(|e| RmcccError::Network(e.to_string()))?;
        if !response.status().is_success() {
            return Err(RmcccError::HttpStatus {
                status: response.status().as_u16(),
                url,
            });
        }
        response
            .json::<serde_json::Value>()
            .await
            .map_err(|e| RmcccError::Network(format!("invalid CurseForge JSON: {e}")))
    }

    /// Resolve a file's name/URL/hash.
    pub async fn file(&self, project_id: u64, file_id: u64) -> Result<CurseFileInfo> {
        let value = self
            .get_json(&format!("/v1/mods/{project_id}/files/{file_id}"))
            .await?;
        let data = value
            .get("data")
            .ok_or_else(|| RmcccError::Validation("CurseForge response has no data".into()))?;
        let parsed: CurseFileDto = serde_json::from_value(data.clone())
            .map_err(|e| RmcccError::Validation(format!("invalid CurseForge file JSON: {e}")))?;
        let file_name = parsed
            .file_name
            .filter(|name| !name.trim().is_empty())
            .ok_or_else(|| {
                RmcccError::Validation(format!(
                    "CurseForge file {project_id}/{file_id} has no filename"
                ))
            })?;
        let sha1 = parsed
            .hashes
            .into_iter()
            .find(|hash| hash.algo == 1)
            .map(|hash| hash.value)
            .filter(|hash| hash.len() == 40);
        Ok(CurseFileInfo {
            file_name,
            download_url: parsed.download_url,
            sha1,
            size: parsed.file_length,
        })
    }

    /// Resolve a project's class id (12/6945 resourcepack, 6552 shader).
    pub async fn class_id(&self, project_id: u64) -> Result<Option<u32>> {
        let value = self.get_json(&format!("/v1/mods/{project_id}")).await?;
        Ok(value
            .get("data")
            .and_then(|data| data.get("classId"))
            .and_then(serde_json::Value::as_u64)
            .map(|id| id as u32))
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CurseFileDto {
    #[serde(default)]
    file_name: Option<String>,
    #[serde(default)]
    download_url: Option<String>,
    #[serde(default)]
    hashes: Vec<CurseHash>,
    #[serde(default)]
    file_length: Option<u64>,
}

#[derive(Debug, Deserialize)]
struct CurseHash {
    #[serde(default)]
    value: String,
    #[serde(default)]
    algo: u32,
}
