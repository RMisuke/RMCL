//! Mojang version manifest and version JSON parsing.

use rmccc_error::{Result, RmcccError};

use super::types::{GameVersion, ReleaseType, VersionInfo};
use super::VersionProvider;

/// Mojang public APIs.
const VERSION_MANIFEST_URL: &str =
    "https://piston-meta.mojang.com/mc/game/version_manifest_v2.json";

/// Mojang version provider — fetches from the official API.
///
/// The `version_manifest_url` can be overridden for testing with wiremock.
pub struct MojangVersionProvider {
    client: reqwest::Client,
    /// URL for the version manifest (defaults to Mojang official).
    pub version_manifest_url: String,
    /// 上次响应的 ETag（P2-09 条件请求 If-None-Match）。
    etag: std::sync::Mutex<Option<String>>,
    /// 进程内版本列表缓存（P2-21 请求级缓存 + 304 回退）。
    cached_versions: std::sync::Mutex<Option<Vec<VersionInfo>>>,
}

impl MojangVersionProvider {
    pub fn new(client: reqwest::Client) -> Self {
        Self {
            client,
            version_manifest_url: VERSION_MANIFEST_URL.to_string(),
            etag: std::sync::Mutex::new(None),
            cached_versions: std::sync::Mutex::new(None),
        }
    }
}

#[async_trait::async_trait]
impl VersionProvider for MojangVersionProvider {
    fn provider_id(&self) -> &str {
        "mojang"
    }

    async fn fetch_versions(&self) -> Result<Vec<VersionInfo>> {
        tracing::info!("Fetching version manifest from Mojang");

        // P2-09/21：带 If-None-Match 条件请求；304 时直接用进程内缓存，避免全量下载。
        let etag = self.etag.lock().unwrap_or_else(|p| p.into_inner()).clone();
        let mut req = self.client.get(&self.version_manifest_url);
        if let Some(e) = &etag {
            req = req.header(reqwest::header::IF_NONE_MATCH, e.clone());
        }
        let resp = req
            .send()
            .await
            .map_err(|e| RmcccError::Network(e.to_string()))?;

        if resp.status() == reqwest::StatusCode::NOT_MODIFIED {
            if let Some(cached) = self
                .cached_versions
                .lock()
                .unwrap_or_else(|p| p.into_inner())
                .clone()
            {
                tracing::debug!("Version manifest 304 — using in-memory cache");
                return Ok(cached);
            }
        }

        if !resp.status().is_success() {
            return Err(RmcccError::HttpStatus {
                status: resp.status().as_u16(),
                url: VERSION_MANIFEST_URL.to_string(),
            });
        }

        // 记录新 ETag 供下次条件请求
        if let Some(v) = resp.headers().get(reqwest::header::ETAG) {
            if let Ok(s) = v.to_str() {
                *self.etag.lock().unwrap_or_else(|p| p.into_inner()) = Some(s.to_string());
            }
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
        // 缓存到进程内（P2-21）
        *self
            .cached_versions
            .lock()
            .unwrap_or_else(|p| p.into_inner()) = Some(versions.clone());
        Ok(versions)
    }

    async fn fetch_version(&self, id: &str) -> Result<GameVersion> {
        // P2-19：优先用进程内缓存找 URL，避免每次 fetch_version 都全量拉 manifest。
        let info_url = self
            .cached_versions
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .clone()
            .and_then(|vs| vs.into_iter().find(|v| v.id == id).map(|v| v.url));

        let url = match info_url {
            Some(u) => u,
            None => {
                let versions = self.fetch_versions().await?;
                versions
                    .iter()
                    .find(|v| v.id == id)
                    .ok_or_else(|| RmcccError::Other(format!("Version not found: {id}")))?
                    .url
                    .clone()
            }
        };

        tracing::info!(version_id = %id, "Fetching version details");

        let resp = self
            .client
            .get(&url)
            .send()
            .await
            .map_err(|e| RmcccError::Network(e.to_string()))?;

        if !resp.status().is_success() {
            return Err(RmcccError::HttpStatus {
                status: resp.status().as_u16(),
                url: url.clone(),
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
///
/// `pub(crate)` so the local [`DefaultGameRepository`](super::default_game_repository)
/// can parse `versions/{id}/{id}.json` files from disk with the exact same
/// typed model used by the network provider.
pub(crate) fn parse_version_json(id: &str, json: &str) -> Result<GameVersion> {
    let v: serde_json::Value = serde_json::from_str(json)?;

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

    // Parse libraries — typed serde derive (G13). `Library` carries the full
    // field set (downloads / rules / extract / natives / checksums / MMC-hint /
    // MMC-filename), so a single `from_value` keeps a single source of truth.
    // Unparseable entries are skipped with a warning: an empty-name
    // `Library::default()` placeholder used to slip into classpaths and
    // download lists (audit 2026-10-25, batch 1).
    let libraries: Vec<super::types::Library> = v["libraries"]
        .as_array()
        .map(|arr| {
            arr.iter()
                .filter_map(|lib| {
                    match serde_json::from_value::<super::types::Library>(lib.clone()) {
                        Ok(parsed) => Some(parsed),
                        Err(e) => {
                            tracing::warn!(
                                error = %e,
                                lib = %lib["name"],
                                "Skipping unparseable library entry"
                            );
                            None
                        }
                    }
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
            component: v["javaVersion"]["component"]
                .as_str()
                .map(|s| s.to_string()),
        })
    } else {
        None
    };

    // Parse inheritance
    let inherits_from = v["inheritsFrom"].as_str().map(|s| s.to_string());

    // Parse arguments (new-style, 1.13+)
    let arguments = if v["arguments"].is_object() {
        let jvm = v["arguments"]["jvm"]
            .as_array()
            .map(|arr| parse_version_arguments(arr));
        let game = v["arguments"]["game"]
            .as_array()
            .map(|arr| parse_version_arguments(arr));
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

    // ── HMCL-aligned fields (G3) ──
    // jar: client JAR identity, defaults to the instance id at resolve time.
    let jar = v["jar"].as_str().map(|s| s.to_string());

    // Manifest-level compatibility rules (HMCL `compatibilityRules`).
    let compatibility_rules: Vec<super::types::CompatibilityRule> = v["compatibilityRules"]
        .as_array()
        .map(|arr| parse_compatibility_rules(arr))
        .unwrap_or_default();

    // Logging configuration (log4j2 config objects).
    let logging = v["logging"].as_object().map(|logging| {
        let mut map = std::collections::HashMap::new();
        for (key, val) in logging {
            map.insert(
                key.clone(),
                super::types::LoggingInfo {
                    argument: val["argument"].as_str().unwrap_or("").to_string(),
                    file: super::types::DownloadEntry {
                        url: val["file"]["url"].as_str().unwrap_or("").to_string(),
                        sha1: val["file"]["sha1"].as_str().unwrap_or("").to_string(),
                        size: val["file"]["size"].as_u64().unwrap_or(0),
                        path: None,
                    },
                    log_type: val["type"].as_str().unwrap_or("log4j2-xml").to_string(),
                },
            );
        }
        map
    });

    let root = v["root"].as_bool();
    let hidden = v["hidden"].as_bool().unwrap_or(false);
    let minimum_launcher_version = v["minimumLauncherVersion"].as_u64().map(|n| n as u32);
    let compliance_level = v["complianceLevel"].as_u64().map(|n| n as u32);
    let time = v["time"].as_str().map(|s| s.to_string());
    let release_time = v["releaseTime"].as_str().map(|s| s.to_string());

    // Patches — full `GamePatch` (incremental modloader model). Patches are
    // parsed verbatim and applied by the repository's resolve (sorted by priority).
    let patches: Vec<super::types::GamePatch> = v["patches"]
        .as_array()
        .map(|arr| {
            arr.iter()
                .map(|p| super::types::GamePatch {
                    id: p["id"].as_str().unwrap_or("").to_string(),
                    version: p["version"].as_str().unwrap_or("").to_string(),
                    priority: p["priority"].as_i64().unwrap_or(0) as i32,
                    main_class: p["mainClass"].as_str().map(|s| s.to_string()),
                    arguments: {
                        let jvm = p["arguments"]["jvm"]
                            .as_array()
                            .map(|a| parse_version_arguments(a));
                        let game = p["arguments"]["game"]
                            .as_array()
                            .map(|a| parse_version_arguments(a));
                        if jvm.is_some() || game.is_some() {
                            Some(super::types::VersionArguments { jvm, game })
                        } else {
                            None
                        }
                    },
                    libraries: parse_patch_libraries(&p["libraries"]),
                    asset_index: p["assetIndex"]
                        .as_object()
                        .map(|_| super::types::AssetIndex {
                            id: p["assetIndex"]["id"].as_str().unwrap_or("").to_string(),
                            url: p["assetIndex"]["url"].as_str().unwrap_or("").to_string(),
                            sha1: p["assetIndex"]["sha1"].as_str().unwrap_or("").to_string(),
                            size: p["assetIndex"]["size"].as_u64().unwrap_or(0),
                            total_size: p["assetIndex"]["totalSize"].as_u64().unwrap_or(0),
                        }),
                    java_version: p["javaVersion"].as_object().map(|_| {
                        super::types::JavaRequirement {
                            major_version: p["javaVersion"]["majorVersion"].as_u64().unwrap_or(8)
                                as u32,
                            component: p["javaVersion"]["component"]
                                .as_str()
                                .map(|s| s.to_string()),
                        }
                    }),
                    inherits_from: p["inheritsFrom"].as_str().map(|s| s.to_string()),
                    compatibility_rules: p["compatibilityRules"]
                        .as_array()
                        .map(|arr| parse_compatibility_rules(arr))
                        .unwrap_or_default(),
                    hidden: p["hidden"].as_bool().unwrap_or(false),
                    minecraft_arguments: p["minecraftArguments"].as_str().map(|s| s.to_string()),
                })
                .collect()
        })
        .unwrap_or_default();

    Ok(GameVersion {
        id: id.to_string(),
        release_type,
        main_class,
        jar,
        asset_index,
        downloads,
        libraries,
        java_version,
        inherits_from,
        patches,
        arguments,
        minecraft_arguments,
        compatibility_rules,
        logging,
        root,
        hidden,
        minimum_launcher_version,
        compliance_level,
        time,
        release_time,
        raw_json: Some(json.to_string()),
    })
}

/// Parse a `compatibilityRules` / `rules` array into unified compatibility rules.
fn parse_compatibility_rules(arr: &[serde_json::Value]) -> Vec<super::types::CompatibilityRule> {
    arr.iter()
        .map(|rule| super::types::CompatibilityRule {
            action: rule["action"].as_str().unwrap_or("allow").to_string(),
            os: rule["os"].as_object().map(|os| super::types::OsCondition {
                name: os
                    .get("name")
                    .and_then(|n| n.as_str().map(|s| s.to_string())),
                arch: os
                    .get("arch")
                    .and_then(|a| a.as_str().map(|s| s.to_string())),
                version: os
                    .get("version")
                    .and_then(|v| v.as_str().map(|s| s.to_string())),
            }),
            features: rule["features"].as_object().map(|f| {
                f.iter()
                    .map(|(k, v)| (k.clone(), v.as_bool().unwrap_or(false)))
                    .collect()
            }),
        })
        .collect()
}

/// Parse a `libraries` array into typed libraries (used inside patches).
///
/// Reuses [`Library`]'s serde derive so patch libraries carry the same full
/// field set as top-level libraries.
fn parse_patch_libraries(value: &serde_json::Value) -> Vec<super::types::Library> {
    value
        .as_array()
        .map(|arr| {
            arr.iter()
                .map(|lib| {
                    serde_json::from_value::<super::types::Library>(lib.clone()).unwrap_or_default()
                })
                .collect()
        })
        .unwrap_or_default()
}

/// Parse a JSON array of version arguments into our typed representation.
/// Each element can be a simple string or a rule-gated object.
fn parse_version_arguments(arr: &[serde_json::Value]) -> Vec<super::types::VersionArgument> {
    arr.iter()
        .filter_map(|v| {
            if let Some(s) = v.as_str() {
                Some(super::types::VersionArgument::Simple(s.to_string()))
            } else if v.is_object() {
                let rules = v["rules"]
                    .as_array()
                    .map(|r| {
                        r.iter()
                            .map(|rule| super::types::ArgumentRule {
                                action: rule["action"].as_str().unwrap_or("allow").to_string(),
                                os: rule["os"].as_object().map(|os| super::types::OsCondition {
                                    name: os
                                        .get("name")
                                        .and_then(|n| n.as_str().map(|s| s.to_string())),
                                    arch: os
                                        .get("arch")
                                        .and_then(|a| a.as_str().map(|s| s.to_string())),
                                    version: os
                                        .get("version")
                                        .and_then(|v| v.as_str().map(|s| s.to_string())),
                                }),
                                features: rule["features"].as_object().map(|f| {
                                    f.iter()
                                        .map(|(k, v)| (k.clone(), v.as_bool().unwrap_or(false)))
                                        .collect()
                                }),
                            })
                            .collect()
                    })
                    .unwrap_or_default();

                let value = if let Some(val) = v["value"].as_str() {
                    super::types::ArgumentValue::Single(val.to_string())
                } else {
                    let arr = v["value"].as_array()?;
                    super::types::ArgumentValue::Multiple(
                        arr.iter()
                            .filter_map(|e| e.as_str().map(|s| s.to_string()))
                            .collect(),
                    )
                };

                Some(super::types::VersionArgument::Ruled { rules, value })
            } else {
                None
            }
        })
        .collect()
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
