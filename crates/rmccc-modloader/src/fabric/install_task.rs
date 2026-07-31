//! Fabric install task — HMCL-aligned installation using launch metadata.
//!
//! Unlike the legacy `ModLoaderProvider::install` which used the simpler
//! `/profile/json` endpoint, this implementation aligns with HMCL's approach:
//!
//! 1. Fetch launch metadata from `/v2/versions/loader/{game}/{loader}`
//! 2. Parse `FabricInfo` to extract loader.maven, intermediary.maven, launcherMeta
//! 3. Build a [`GameInstancePatch`] with mainClass, libraries, and arguments
//! 4. The caller is responsible for merging the patch and downloading libraries

use crate::install_task::InstallTask;
use crate::patch::{GameInstancePatch, PRIORITY_LOADER};
use crate::analyzer::LibraryType;
use rmccc_error::{RmcccError, Result};
use rmccc_version::{Library, VersionArguments, VersionArgument};
use serde::Deserialize;

/// A Fabric install task that fetches launch metadata and builds a patch.
pub struct FabricInstallTask {
    pub game_version: String,
    pub loader_version: String,
    pub launch_meta_url: String,
}

/// Top-level response from the Fabric launch metadata endpoint.
#[derive(Debug, Deserialize)]
struct FabricInfo {
    loader: FabricMavenInfo,
    intermediary: FabricMavenInfo,
    #[serde(rename = "launcherMeta")]
    launcher_meta: serde_json::Value,
}

#[derive(Debug, Deserialize)]
struct FabricMavenInfo {
    maven: String,
    #[allow(dead_code)]
    version: String,
    #[allow(dead_code)]
    stable: bool,
}

#[async_trait::async_trait]
impl InstallTask for FabricInstallTask {
    async fn execute(&mut self) -> Result<GameInstancePatch> {
        tracing::info!(
            game_version = %self.game_version,
            loader_version = %self.loader_version,
            "Executing Fabric install task"
        );

        // Fetch launch metadata
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

        let fabric_info: FabricInfo = resp
            .json()
            .await
            .map_err(|e| RmcccError::Other(format!("Failed to parse Fabric launch metadata: {e}")))?;

        // Build the patch
        let patch = build_patch(
            &self.loader_version,
            &self.game_version,
            &fabric_info,
        )?;

        tracing::info!(
            version = %self.loader_version,
            main_class = ?patch.main_class,
            lib_count = patch.libraries.len(),
            "Fabric patch built"
        );

        Ok(patch)
    }

    fn description(&self) -> &str {
        "Installing Fabric loader"
    }
}

/// Build a [`GameInstancePatch`] from Fabric launch metadata.
fn build_patch(
    loader_version: &str,
    _game_version: &str,
    info: &FabricInfo,
) -> Result<GameInstancePatch> {
    let launcher_meta = &info.launcher_meta;

    // ── Extract mainClass ──
    let main_class = match &launcher_meta["mainClass"] {
        serde_json::Value::String(s) => s.clone(),
        serde_json::Value::Object(obj) => obj
            .get("client")
            .and_then(|v| v.as_str())
            .map(String::from)
            .unwrap_or_else(|| "net.fabricmc.loader.impl.launch.knot.KnotClient".into()),
        _ => "net.fabricmc.loader.impl.launch.knot.KnotClient".into(),
    };

    // ── Extract arguments (launchwrapper tweakers) ──
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

    // ── Build libraries ──
    let mut libraries: Vec<Library> = Vec::new();

    // Parse library entries from launcherMeta.libraries.{common, server}
    if let Some(libs_obj) = launcher_meta.get("libraries").and_then(|v| v.as_object()) {
        for side in &["common", "server"] {
            if let Some(arr) = libs_obj.get(*side).and_then(|v| v.as_array()) {
                for entry in arr {
                    // Each entry is a Library JSON object with "name" field
                    if let Some(name) = entry.get("name").and_then(|v| v.as_str()) {
                        let lib: Library = serde_json::from_value(entry.clone())
                            .unwrap_or_else(|_| Library {
                                name: name.into(),
                                downloads: None,
                                rules: None,
                            });
                        libraries.push(lib);
                    }
                }
            }
        }
    }

    // Add loader and intermediary as Maven libraries
    let loader_lib = Library {
        name: info.loader.maven.clone(),
        downloads: None,
        rules: None,
    };
    let intermediary_lib = Library {
        name: info.intermediary.maven.clone(),
        downloads: None,
        rules: None,
    };
    libraries.push(loader_lib);
    libraries.push(intermediary_lib);

    Ok(GameInstancePatch::with_loader(
        LibraryType::Fabric.patch_id(),
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
    fn test_build_patch_from_fabric_meta() {
        let info = FabricInfo {
            loader: FabricMavenInfo {
                maven: "net.fabricmc:fabric-loader:0.16.0".into(),
                version: "0.16.0".into(),
                stable: true,
            },
            intermediary: FabricMavenInfo {
                maven: "net.fabricmc:intermediary:1.21".into(),
                version: "1.21".into(),
                stable: true,
            },
            launcher_meta: serde_json::json!({
                "mainClass": {
                    "client": "net.fabricmc.loader.impl.launch.knot.KnotClient"
                },
                "libraries": {
                    "common": [
                        {"name": "net.fabricmc:sponge-mixin:0.15.0+mixindev"}
                    ],
                    "server": []
                }
            }),
        };

        let patch = build_patch("0.16.0", "1.21", &info).unwrap();

        assert_eq!(patch.id, "fabric");
        assert_eq!(patch.version, "0.16.0");
        assert_eq!(patch.priority, PRIORITY_LOADER);
        assert_eq!(
            patch.main_class,
            Some("net.fabricmc.loader.impl.launch.knot.KnotClient".into())
        );
        // Should have common lib + loader + intermediary = 3
        assert_eq!(patch.libraries.len(), 3);
        assert!(patch.libraries.iter().any(|l| l.name.contains("fabric-loader")));
        assert!(patch.libraries.iter().any(|l| l.name.contains("intermediary")));
    }

    #[test]
    fn test_build_patch_string_main_class() {
        let info = FabricInfo {
            loader: FabricMavenInfo {
                maven: "net.fabricmc:fabric-loader:0.14.0".into(),
                version: "0.14.0".into(),
                stable: true,
            },
            intermediary: FabricMavenInfo {
                maven: "net.fabricmc:intermediary:1.20.4".into(),
                version: "1.20.4".into(),
                stable: true,
            },
            launcher_meta: serde_json::json!({
                "mainClass": "net.fabricmc.loader.launch.knot.KnotClient",
                "libraries": {
                    "common": [],
                    "server": []
                }
            }),
        };

        let patch = build_patch("0.14.0", "1.20.4", &info).unwrap();
        assert_eq!(
            patch.main_class,
            Some("net.fabricmc.loader.launch.knot.KnotClient".into())
        );
    }

    #[test]
    fn test_build_patch_with_launchwrapper() {
        let info = FabricInfo {
            loader: FabricMavenInfo {
                maven: "net.fabricmc:fabric-loader:0.12.0".into(),
                version: "0.12.0".into(),
                stable: true,
            },
            intermediary: FabricMavenInfo {
                maven: "net.fabricmc:intermediary:1.16.5".into(),
                version: "1.16.5".into(),
                stable: true,
            },
            launcher_meta: serde_json::json!({
                "mainClass": "net.fabricmc.loader.launch.knot.KnotClient",
                "libraries": {
                    "common": [],
                    "server": []
                },
                "launchwrapper": {
                    "tweakers": {
                        "client": ["net.fabricmc.loader.launch.FabricTweaker"]
                    }
                }
            }),
        };

        let patch = build_patch("0.12.0", "1.16.5", &info).unwrap();
        let args = patch.arguments.unwrap();
        let game_args = args.game.unwrap();
        // Should contain --tweakClass and the tweaker
        assert!(game_args.iter().any(|a| {
            matches!(a, VersionArgument::Simple(s) if s == "--tweakClass")
        }));
        assert!(game_args.iter().any(|a| {
            matches!(a, VersionArgument::Simple(s) if s.contains("FabricTweaker"))
        }));
    }
}
