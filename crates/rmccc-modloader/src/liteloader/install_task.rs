//! LiteLoader install task.
//!
//! HMCL-compatible: LiteLoader must be installed AFTER Forge.
//! Priority: 60000 (highest, overrides even OptiFine).

use crate::install_task::InstallTask;
use crate::patch::{GameInstancePatch, PRIORITY_LITELOADER};
use crate::analyzer::{LibraryType, LAUNCH_WRAPPER_MAIN};
use rmccc_error::Result;
use rmccc_version::{Library, VersionArguments, VersionArgument};

pub struct LiteLoaderInstallTask {
    pub game_version: String,
    pub loader_version: String,
    pub download_url: String,
    pub libraries: Vec<String>,
}

#[async_trait::async_trait]
impl InstallTask for LiteLoaderInstallTask {
    async fn execute(&mut self) -> Result<GameInstancePatch> {
        tracing::info!(game_version = %self.game_version, loader = %self.loader_version, "Executing LiteLoader install task");

        // Build library list from Maven coordinates
        let mut libs: Vec<Library> = self.libraries.iter().map(|name| Library {
            name: name.clone(),
            downloads: None,
            rules: None,
        }).collect();

        // Add the main LiteLoader library
        libs.push(Library {
            name: format!("com.mumfrey:liteloader:{}", self.loader_version),
            downloads: None,
            rules: None,
        });

        // LiteLoader uses LAUNCH_WRAPPER with tweaker
        let arguments = VersionArguments {
            jvm: None,
            game: Some(vec![
                VersionArgument::Simple("--tweakClass".into()),
                VersionArgument::Simple("com.mumfrey.liteloader.launch.LiteLoaderTweaker".into()),
            ]),
        };

        Ok(GameInstancePatch::with_loader(
            LibraryType::LiteLoader.patch_id(),
            &self.loader_version,
            PRIORITY_LITELOADER,
            Some(LAUNCH_WRAPPER_MAIN.into()),
            Some(arguments),
            libs,
        ))
    }

    fn description(&self) -> &str { "Installing LiteLoader" }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_liteloader_patch_properties() {
        let patch = GameInstancePatch::with_loader(
            "liteloader", "1.12.2", PRIORITY_LITELOADER,
            Some(LAUNCH_WRAPPER_MAIN.into()),
            Some(VersionArguments {
                jvm: None,
                game: Some(vec![
                    VersionArgument::Simple("--tweakClass".into()),
                    VersionArgument::Simple("com.mumfrey.liteloader.launch.LiteLoaderTweaker".into()),
                ]),
            }),
            vec![Library { name: "com.mumfrey:liteloader:1.12.2".into(), downloads: None, rules: None }],
        );

        assert_eq!(patch.id, "liteloader");
        assert_eq!(patch.priority, PRIORITY_LITELOADER);
        assert_eq!(patch.main_class.unwrap(), LAUNCH_WRAPPER_MAIN);
        assert!(patch.libraries.iter().any(|l| l.name.contains("liteloader")));
    }
}
