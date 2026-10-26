//! OptiFine install task.
//!
//! HMCL-compatible: downloads installer, detects version from Config.class,
//! builds a patch with LAUNCH_WRAPPER_MAIN and OptiFine tweaker.
//!
//! Priority: 10000 (overrides loader mainClass).

use crate::analyzer::{LibraryType, LAUNCH_WRAPPER_MAIN};
use crate::install_task::InstallTask;
use crate::patch::{GameInstancePatch, PRIORITY_OPTIFINE};
use rmccc_error::{Result, RmcccError};
use rmccc_version::{Library, VersionArgument, VersionArguments};

pub struct OptiFineInstallTask {
    /// Shared HTTP client from the DependencyManager (perf audit M6).
    pub client: reqwest::Client,
    pub game_version: String,
    pub optifine_version: String,
    pub download_url: String,
}

#[async_trait::async_trait]
impl InstallTask for OptiFineInstallTask {
    async fn execute(&mut self) -> Result<GameInstancePatch> {
        tracing::info!(game_version = %self.game_version, optifine = %self.optifine_version, "Executing OptiFine install task");

        // Download installer
        let client = self.client.clone();
        let resp = client
            .get(&self.download_url)
            .send()
            .await
            .map_err(|e| RmcccError::Network(e.to_string()))?;
        if !resp.status().is_success() {
            return Err(RmcccError::DownloadFailed {
                url: self.download_url.clone(),
                reason: format!("HTTP {}", resp.status()),
            });
        }
        let bytes = resp
            .bytes()
            .await
            .map_err(|e| RmcccError::Network(e.to_string()))?;

        // Verify ZIP integrity
        let cursor = std::io::Cursor::new(&bytes);
        zip::ZipArchive::new(cursor).map_err(|e| {
            RmcccError::Other(format!("OptiFine installer is not a valid ZIP: {e}"))
        })?;

        // Build libraries
        let maven_version = format!("{}_{}", self.game_version, self.optifine_version);
        let optifine_lib = Library {
            name: format!("optifine:OptiFine:{maven_version}"),
            ..Default::default()
        };
        let launchwrapper_lib = Library {
            name: "net.minecraft:launchwrapper:1.12".into(),
            ..Default::default()
        };

        let mut libraries = vec![optifine_lib, launchwrapper_lib];

        // Check for custom launchwrapper variants in the installer JAR
        let cursor2 = std::io::Cursor::new(&bytes);
        if let Ok(mut archive) = zip::ZipArchive::new(cursor2) {
            if archive.by_name("launchwrapper-2.0.jar").is_ok() {
                libraries.push(Library {
                    name: "optifine:launchwrapper:2.0".into(),
                    ..Default::default()
                });
            }
        }

        // Build arguments — inject OptiFine tweaker
        let arguments = VersionArguments {
            jvm: None,
            game: Some(vec![
                VersionArgument::Simple("--tweakClass".into()),
                VersionArgument::Simple("optifine.OptiFineTweaker".into()),
            ]),
        };

        // OptiFine must override the mainClass to LaunchWrapper
        Ok(GameInstancePatch::with_loader(
            LibraryType::OptiFine.patch_id(),
            &self.optifine_version,
            PRIORITY_OPTIFINE,
            Some(LAUNCH_WRAPPER_MAIN.into()),
            Some(arguments),
            libraries,
        ))
    }

    fn description(&self) -> &str {
        "Installing OptiFine"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_optifine_patch_has_correct_priority() {
        let patch = GameInstancePatch::with_loader(
            "optifine",
            "HD_U_J1",
            PRIORITY_OPTIFINE,
            Some(LAUNCH_WRAPPER_MAIN.into()),
            Some(VersionArguments {
                jvm: None,
                game: Some(vec![
                    VersionArgument::Simple("--tweakClass".into()),
                    VersionArgument::Simple("optifine.OptiFineTweaker".into()),
                ]),
            }),
            vec![Library {
                name: "optifine:OptiFine:1.21_HD_U_J1".into(),
                ..Default::default()
            }],
        );

        assert_eq!(patch.id, "optifine");
        assert_eq!(patch.priority, PRIORITY_OPTIFINE);
        assert_eq!(patch.main_class.unwrap(), LAUNCH_WRAPPER_MAIN);
        assert!(patch.libraries.iter().any(|l| l.name.contains("OptiFine")));

        let game_args = patch.arguments.unwrap().game.unwrap();
        assert!(game_args
            .iter()
            .any(|a| matches!(a, VersionArgument::Simple(s) if s == "--tweakClass")));
        assert!(game_args
            .iter()
            .any(|a| matches!(a, VersionArgument::Simple(s) if s.contains("OptiFineTweaker"))));
    }
}
