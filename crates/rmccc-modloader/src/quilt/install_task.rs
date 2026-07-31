//! Quilt install task — HMCL-aligned installation using launch metadata.
//!
//! Follows the same pattern as Fabric, with additional:
//! - Hashed mappings support (optional)
//! - Intermediary mappings (optional)
//! - Maven repository routing based on groupId

use crate::install_task::InstallTask;
use crate::patch::{GameInstancePatch, PRIORITY_LOADER};
use crate::analyzer::LibraryType;
use rmccc_error::{RmcccError, Result};
use rmccc_version::{Library, VersionArguments, VersionArgument};
use serde::Deserialize;

pub struct QuiltInstallTask {
    pub game_version: String,
    pub loader_version: String,
    pub launch_meta_url: String,
}

#[derive(Debug, Deserialize)]
struct QuiltInfo {
    loader: QuiltMavenInfo,
    #[serde(default)]
    hashed: Option<QuiltMavenInfo>,
    #[serde(default)]
    intermediary: Option<QuiltMavenInfo>,
    #[serde(rename = "launcherMeta")]
    launcher_meta: serde_json::Value,
}

#[derive(Debug, Deserialize)]
struct QuiltMavenInfo {
    maven: String,
    #[allow(dead_code)]
    version: String,
}

#[async_trait::async_trait]
impl InstallTask for QuiltInstallTask {
    async fn execute(&mut self) -> Result<GameInstancePatch> {
        tracing::info!(
            game_version = %self.game_version,
            loader_version = %self.loader_version,
            "Executing Quilt install task"
        );

        let client = reqwest::Client::new();
        let resp = client
            .get(&self.launch_meta_url)
            .send()
            .await
            .map_err(|e| RmcccError::Network(e.to_string()))?;

        if !resp.status().is_success() {
            return Err(RmcccError::HttpStatus {
                status: resp.status().as_u16(),
                url: self.launch_meta_url.clone(),
            });
        }

        let quilt_info: QuiltInfo = resp
            .json()
            .await
            .map_err(|e| RmcccError::Other(format!("Failed to parse Quilt launch metadata: {e}")))?;

        let patch = build_patch(&self.loader_version, &quilt_info)?;

        tracing::info!(
            version = %self.loader_version,
            main_class = ?patch.main_class,
            lib_count = patch.libraries.len(),
            "Quilt patch built"
        );

        Ok(patch)
    }

    fn description(&self) -> &str {
        "Installing Quilt loader"
    }
}

fn build_patch(loader_version: &str, info: &QuiltInfo) -> Result<GameInstancePatch> {
    let launcher_meta = &info.launcher_meta;

    let main_class = match &launcher_meta["mainClass"] {
        serde_json::Value::String(s) => s.clone(),
        serde_json::Value::Object(obj) => obj
            .get("client")
            .and_then(|v| v.as_str())
            .map(String::from)
            .unwrap_or_else(|| "org.quiltmc.loader.impl.launch.knot.KnotClient".into()),
        _ => "org.quiltmc.loader.impl.launch.knot.KnotClient".into(),
    };

    let mut arguments = VersionArguments {
        jvm: None,
        game: None,
    };

    if let Some(lw) = launcher_meta.get("launchwrapper") {
        if let Some(tweakers) = lw.get("tweakers") {
            if let Some(client_tweaker) = tweakers.get("client") {
                if let Some(arr) = client_tweaker.as_array() {
                    if let Some(first) = arr.first().and_then(|v| v.as_str()) {
                        arguments.game = Some(vec![
                            VersionArgument::Simple("--tweakClass".into()),
                            VersionArgument::Simple(first.into()),
                        ]);
                    }
                }
            }
        }
    }

    let mut libraries: Vec<Library> = Vec::new();

    if let Some(libs_obj) = launcher_meta.get("libraries").and_then(|v| v.as_object()) {
        for side in &["common", "server"] {
            if let Some(arr) = libs_obj.get(*side).and_then(|v| v.as_array()) {
                for entry in arr {
                    if let Some(name) = entry.get("name").and_then(|v| v.as_str()) {
                        libraries.push(Library {
                            name: name.into(),
                            downloads: None,
                            rules: None,
                        });
                    }
                }
            }
        }
    }

    // Add intermediary mapping (if present)
    if let Some(ref intermediary) = info.intermediary {
        libraries.push(Library {
            name: intermediary.maven.clone(),
            downloads: None,
            rules: None,
        });
    }

    // Add hashed mapping (if present, may replace intermediary)
    if let Some(ref hashed) = info.hashed {
        libraries.push(Library {
            name: hashed.maven.clone(),
            downloads: None,
            rules: None,
        });
    }

    // Add loader
    libraries.push(Library {
        name: info.loader.maven.clone(),
        downloads: None,
        rules: None,
    });

    Ok(GameInstancePatch::with_loader(
        LibraryType::Quilt.patch_id(),
        loader_version,
        PRIORITY_LOADER,
        Some(main_class),
        Some(arguments),
        libraries,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_patch_from_quilt_meta() {
        let info = QuiltInfo {
            loader: QuiltMavenInfo {
                maven: "org.quiltmc:quilt-loader:0.26.0".into(),
                version: "0.26.0".into(),
            },
            hashed: Some(QuiltMavenInfo {
                maven: "org.quiltmc:hashed:1.21".into(),
                version: "1.21".into(),
            }),
            intermediary: Some(QuiltMavenInfo {
                maven: "net.fabricmc:intermediary:1.21".into(),
                version: "1.21".into(),
            }),
            launcher_meta: serde_json::json!({
                "mainClass": {
                    "client": "org.quiltmc.loader.impl.launch.knot.KnotClient"
                },
                "libraries": {
                    "common": [
                        {"name": "org.quiltmc:quilt-loader-sat4j:0.26.0"}
                    ],
                    "server": []
                }
            }),
        };

        let patch = build_patch("0.26.0", &info).unwrap();

        assert_eq!(patch.id, "quilt");
        assert_eq!(patch.version, "0.26.0");
        assert_eq!(patch.priority, PRIORITY_LOADER);
        assert_eq!(
            patch.main_class,
            Some("org.quiltmc.loader.impl.launch.knot.KnotClient".into())
        );
        // common lib + intermediary + hashed + loader = 4
        assert_eq!(patch.libraries.len(), 4);
        assert!(patch.libraries.iter().any(|l| l.name.contains("quilt-loader")));
        assert!(patch.libraries.iter().any(|l| l.name.contains("hashed")));
        assert!(patch.libraries.iter().any(|l| l.name.contains("intermediary")));
    }
}
