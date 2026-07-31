//! Mojang version manifest and version JSON parsing.

use rmccc_error::{RmcccError, Result};

use super::types::{GameVersion, ReleaseType, VersionInfo};
use super::VersionProvider;

/// Mojang public APIs.
const VERSION_MANIFEST_URL: &str =
    "https://piston-meta.mojang.com/mc/game/version_manifest_v2.json";

/// Mojang version provider — fetches from the official API.
pub struct MojangVersionProvider {
    client: reqwest::Client,
}

impl MojangVersionProvider {
    pub fn new(client: reqwest::Client) -> Self {
        Self { client }
    }
}

#[async_trait::async_trait]
impl VersionProvider for MojangVersionProvider {
    fn provider_id(&self) -> &str {
        "mojang"
    }

    async fn fetch_versions(&self) -> Result<Vec<VersionInfo>> {
        tracing::info!("Fetching version manifest from Mojang");

        let resp = self
            .client
            .get(VERSION_MANIFEST_URL)
            .send()
            .await
            .map_err(|e| RmcccError::Network(e.to_string()))?;

        if !resp.status().is_success() {
            return Err(RmcccError::HttpStatus {
                status: resp.status().as_u16(),
                url: VERSION_MANIFEST_URL.to_string(),
            });
        }

        #[derive(serde::Deserialize)]
        struct ManifestResponse {
            versions: Vec<ManifestVersion>,
        }

        #[derive(serde::Deserialize)]
        struct ManifestVersion {
            id: String,
            #[serde(rename = "type")]
            release_type: String,
            url: String,
            #[serde(rename = "releaseTime")]
            release_time: String,
        }

        let manifest: ManifestResponse = resp
            .json()
            .await
            .map_err(|e| RmcccError::Network(e.to_string()))?;

        let versions: Vec<VersionInfo> = manifest
            .versions
            .into_iter()
            .map(|v| VersionInfo {
                id: v.id,
                release_type: match v.release_type.as_str() {
                    "snapshot" => ReleaseType::Snapshot,
                    "old_beta" | "old_alpha" => ReleaseType::OldBeta,
                    _ => ReleaseType::Release,
                },
                url: v.url,
                release_time: v.release_time,
            })
            .collect();

        tracing::info!("Fetched {} versions from Mojang", versions.len());
        Ok(versions)
    }

    async fn fetch_version(&self, id: &str) -> Result<GameVersion> {
        // First find the version URL from the manifest
        let versions = self.fetch_versions().await?;
        let info = versions
            .iter()
            .find(|v| v.id == id)
            .ok_or_else(|| RmcccError::Other(format!("Version not found: {id}")))?;

        tracing::info!(version_id = %id, "Fetching version details");

        let resp = self
            .client
            .get(&info.url)
            .send()
            .await
            .map_err(|e| RmcccError::Network(e.to_string()))?;

        if !resp.status().is_success() {
            return Err(RmcccError::HttpStatus {
                status: resp.status().as_u16(),
                url: info.url.clone(),
            });
        }

        let body = resp
            .text()
            .await
            .map_err(|e| RmcccError::Network(e.to_string()))?;

        parse_version_json(id, &body)
    }
}

/// Parse the Mojang version JSON format.
fn parse_version_json(id: &str, json: &str) -> Result<GameVersion> {
    let v: serde_json::Value =
        serde_json::from_str(json)?;

    let main_class = v["mainClass"]
        .as_str()
        .unwrap_or("net.minecraft.client.main.Main")
        .to_string();

    let release_type = match v["type"].as_str().unwrap_or("release") {
        "snapshot" => ReleaseType::Snapshot,
        "old_beta" | "old_alpha" => ReleaseType::OldBeta,
        _ => ReleaseType::Release,
    };

    // Parse asset index
    let asset_index = super::types::AssetIndex {
        id: v["assetIndex"]["id"].as_str().unwrap_or(id).to_string(),
        url: v["assetIndex"]["url"].as_str().unwrap_or("").to_string(),
        sha1: v["assetIndex"]["sha1"].as_str().unwrap_or("").to_string(),
        size: v["assetIndex"]["size"].as_u64().unwrap_or(0),
        total_size: v["assetIndex"]["totalSize"].as_u64().unwrap_or(0),
    };

    // Parse libraries
    let libraries: Vec<super::types::Library> = v["libraries"]
        .as_array()
        .map(|arr| {
            arr.iter()
                .map(|lib| super::types::Library {
                    name: lib["name"].as_str().unwrap_or("").to_string(),
                    downloads: {
                        let artifact = &lib["downloads"]["artifact"];
                        let artifact_entry = if artifact.is_object() {
                            let url = artifact["url"].as_str().unwrap_or("").to_string();
                            if url.is_empty() {
                                tracing::warn!(
                                    lib_name = %lib["name"],
                                    "Library artifact has no download URL"
                                );
                                None
                            } else {
                                Some(super::types::DownloadEntry {
                                    url,
                                    sha1: artifact["sha1"].as_str().unwrap_or("").to_string(),
                                    size: artifact["size"].as_u64().unwrap_or(0),
                                    path: artifact["path"].as_str().map(|s| s.to_string()),
                                })
                            }
                        } else {
                            None
                        };

                        // Parse classifiers (natives)
                        let classifiers = lib["downloads"]["classifiers"].as_object().map(|cls| {
                            let mut map = std::collections::HashMap::new();
                            for (key, val) in cls {
                                map.insert(key.clone(), super::types::DownloadEntry {
                                    url: val["url"].as_str().unwrap_or("").to_string(),
                                    sha1: val["sha1"].as_str().unwrap_or("").to_string(),
                                    size: val["size"].as_u64().unwrap_or(0),
                                    path: val["path"].as_str().map(|s| s.to_string()),
                                });
                            }
                            map
                        });

                        if artifact_entry.is_some() || classifiers.is_some() {
                            Some(super::types::LibraryDownloads {
                                artifact: artifact_entry,
                                classifiers,
                            })
                        } else {
                            None
                        }
                    },
                    rules: lib["rules"].as_array().map(|rules| {
                        rules
                            .iter()
                            .map(|rule| super::types::LibraryRule {
                                action: rule["action"].as_str().unwrap_or("allow").to_string(),
                                os: rule["os"].as_object().map(|os| super::types::OsRule {
                                    name: os.get("name").and_then(|n| n.as_str().map(|s| s.to_string())),
                                    arch: os.get("arch").and_then(|a| a.as_str().map(|s| s.to_string())),
                                    version: os.get("version").and_then(|v| v.as_str().map(|s| s.to_string())),
                                }),
                            })
                            .collect()
                    }),
                })
                .collect()
        })
        .unwrap_or_default();

    // Parse downloads (client JAR)
    let mut downloads = std::collections::HashMap::new();
    if let Some(client) = v["downloads"]["client"].as_object() {
        downloads.insert(
            "client".to_string(),
            super::types::DownloadEntry {
                url: client["url"].as_str().unwrap_or("").to_string(),
                sha1: client["sha1"].as_str().unwrap_or("").to_string(),
                size: client["size"].as_u64().unwrap_or(0),
                path: None,
            },
        );
    }

    // Parse Java requirement
    let java_version = if v["javaVersion"].is_object() {
        Some(super::types::JavaRequirement {
            major_version: v["javaVersion"]["majorVersion"].as_u64().unwrap_or(8) as u32,
            component: v["javaVersion"]["component"].as_str().map(|s| s.to_string()),
        })
    } else {
        None
    };

    // Parse inheritance
    let inherits_from = v["inheritsFrom"].as_str().map(|s| s.to_string());

    // Parse arguments (new-style, 1.13+)
    let arguments = if v["arguments"].is_object() {
        let jvm = v["arguments"]["jvm"].as_array().map(|arr| {
            parse_version_arguments(arr)
        });
        let game = v["arguments"]["game"].as_array().map(|arr| {
            parse_version_arguments(arr)
        });
        if jvm.is_some() || game.is_some() {
            Some(super::types::VersionArguments { jvm, game })
        } else {
            None
        }
    } else {
        None
    };

    // Parse legacy minecraftArguments (pre-1.13)
    let minecraft_arguments = v["minecraftArguments"].as_str().map(|s| s.to_string());

    Ok(GameVersion {
        id: id.to_string(),
        release_type,
        main_class,
        asset_index,
        downloads,
        libraries,
        java_version,
        inherits_from,
        patches: Vec::new(),
        arguments,
        minecraft_arguments,
        raw_json: Some(json.to_string()),
    })
}

/// Parse a JSON array of version arguments into our typed representation.
/// Each element can be a simple string or a rule-gated object.
fn parse_version_arguments(arr: &[serde_json::Value]) -> Vec<super::types::VersionArgument> {
    arr.iter().filter_map(|v| {
        if let Some(s) = v.as_str() {
            Some(super::types::VersionArgument::Simple(s.to_string()))
        } else if v.is_object() {
            let rules = v["rules"].as_array().map(|r| {
                r.iter().map(|rule| super::types::ArgumentRule {
                    action: rule["action"].as_str().unwrap_or("allow").to_string(),
                    os: rule["os"].as_object().map(|os| super::types::OsCondition {
                        name: os.get("name").and_then(|n| n.as_str().map(|s| s.to_string())),
                        arch: os.get("arch").and_then(|a| a.as_str().map(|s| s.to_string())),
                        version: os.get("version").and_then(|v| v.as_str().map(|s| s.to_string())),
                    }),
                    features: rule["features"].as_object().map(|f| {
                        f.iter().map(|(k, v)| (k.clone(), v.as_bool().unwrap_or(false))).collect()
                    }),
                }).collect()
            }).unwrap_or_default();

            let value = if let Some(val) = v["value"].as_str() {
                super::types::ArgumentValue::Single(val.to_string())
            } else if let Some(arr) = v["value"].as_array() {
                super::types::ArgumentValue::Multiple(
                    arr.iter().filter_map(|e| e.as_str().map(|s| s.to_string())).collect()
                )
            } else {
                return None;
            };

            Some(super::types::VersionArgument::Ruled { rules, value })
        } else {
            None
        }
    }).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_minimal_version_json() {
        let json = r#"{
            "id": "1.20.4",
            "type": "release",
            "mainClass": "net.minecraft.client.main.Main",
            "assetIndex": {
                "id": "12",
                "url": "https://example.com/assets.json",
                "sha1": "abc123",
                "size": 123456,
                "totalSize": 500000
            },
            "downloads": {
                "client": {
                    "url": "https://example.com/client.jar",
                    "sha1": "def456",
                    "size": 10000000
                }
            },
            "libraries": [],
            "javaVersion": {
                "component": "java-runtime-gamma",
                "majorVersion": 17
            }
        }"#;

        let version = parse_version_json("1.20.4", json).unwrap();
        assert_eq!(version.id, "1.20.4");
        assert_eq!(version.main_class, "net.minecraft.client.main.Main");
        assert_eq!(version.asset_index.id, "12");
        assert_eq!(version.asset_index.sha1, "abc123");
        assert_eq!(version.downloads.get("client").unwrap().sha1, "def456");
        assert_eq!(version.java_version.unwrap().major_version, 17);
    }
}
