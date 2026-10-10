//! GameInstancePatch — layered patch for non-destructive modloader installation.
//!
//! HMCL's patch system allows modloaders to augment a Minecraft version's
//! manifest without modifying the original JSON. Patches carry priority;
//! higher-priority patches override lower ones when merged.
//!
//! ## Priority constants
//!
//! | Constant | Value | Use case |
//! |----------|-------|----------|
//! | `PRIORITY_MC` | 0 | Base Minecraft version |
//! | `PRIORITY_LOADER` | 30000 | Forge, Fabric, NeoForge, Quilt |
//! | `PRIORITY_OPTIFINE` | 10000 | OptiFine (overrides mainClass) |
//! | `PRIORITY_LITELOADER` | 60000 | LiteLoader (overrides everything) |

use rmccc_version::{ArgumentRule, AssetIndex, JavaRequirement, Library, VersionArguments};
use serde::{Deserialize, Serialize};

/// Priority for the Minecraft base layer.
pub const PRIORITY_MC: i32 = 0;

/// Priority for loader patches (Forge, Fabric, NeoForge, Quilt).
pub const PRIORITY_LOADER: i32 = 30000;

/// Priority for OptiFine (must override loader mainClass).
pub const PRIORITY_OPTIFINE: i32 = 10000;

/// Priority for LiteLoader (must run after Forge + OptiFine).
pub const PRIORITY_LITELOADER: i32 = 60000;

/// A version patch produced by modloader installation.
///
/// Patches are layered onto a [`rmccc_version::GameVersion`] via
/// [`GameInstancePatch::merge_into`]. Fields with higher priority
/// values override lower-priority ones.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GameInstancePatch {
    /// Patch identifier (e.g. "forge", "fabric", "optifine").
    pub id: String,
    /// Version of the modloader.
    pub version: String,
    /// Merge priority. Higher values win.
    pub priority: i32,
    /// Override the main class (set by modloaders that replace the launch entry point).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub main_class: Option<String>,
    /// Additional JVM / game arguments.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub arguments: Option<VersionArguments>,
    /// Libraries to add to the classpath.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub libraries: Vec<Library>,
    /// Asset index override (usually None).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub asset_index: Option<AssetIndex>,
    /// Java version requirement override.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub java_version: Option<JavaRequirement>,
    /// Parent version to inherit from (used by Fabric/Quilt profile JSON).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub inherits_from: Option<String>,
    /// Compatibility rules for this patch.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub compatibility_rules: Vec<ArgumentRule>,
    /// Whether this patch is hidden from the user.
    #[serde(default)]
    pub hidden: bool,

    // Non-serialized fields
    /// Legacy minecraftArguments string (1.12 and earlier).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub minecraft_arguments: Option<String>,
}

impl GameInstancePatch {
    /// Create a minimal patch with just the id.
    pub fn new(id: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            version: String::new(),
            priority: PRIORITY_LOADER,
            main_class: None,
            arguments: None,
            libraries: Vec::new(),
            asset_index: None,
            java_version: None,
            inherits_from: None,
            compatibility_rules: Vec::new(),
            hidden: false,
            minecraft_arguments: None,
        }
    }

    /// Create a patch for a modloader with the essential metadata.
    pub fn with_loader(
        id: impl Into<String>,
        version: impl Into<String>,
        priority: i32,
        main_class: Option<String>,
        arguments: Option<VersionArguments>,
        libraries: Vec<Library>,
    ) -> Self {
        Self {
            id: id.into(),
            version: version.into(),
            priority,
            main_class,
            arguments,
            libraries,
            ..Default::default()
        }
    }

    /// Merge this patch into a [`rmccc_version::GameVersion`].
    ///
    /// Higher-priority fields override lower ones. The merge follows
    /// these rules:
    ///
    /// - **mainClass**: last non-None wins (highest priority)
    /// - **arguments**: merged; higher-priority args appended
    /// - **libraries**: appended (deduplication by name is caller's responsibility)
    /// - **javaVersion**: last non-None wins
    /// - **assetIndex**: last non-None wins
    /// - **inheritsFrom**: last non-None wins
    /// - **patches**: this patch's id+version is appended to `version.patches`
    pub fn merge_into(&self, version: &mut rmccc_version::GameVersion) {
        // Merge libraries: append to existing, don't replace
        if !self.libraries.is_empty() {
            version.libraries.extend(self.libraries.clone());
        }

        // Override mainClass if present
        if let Some(ref mc) = self.main_class {
            version.main_class = mc.clone();
        }

        // Override asset index
        if let Some(ref ai) = self.asset_index {
            version.asset_index = ai.clone();
        }

        // Override Java version requirement
        if let Some(ref jv) = self.java_version {
            version.java_version = Some(jv.clone());
        }

        // Override inheritsFrom
        if let Some(ref parent) = self.inherits_from {
            version.inherits_from = Some(parent.clone());
        }

        // Merge arguments: higher-priority args appended
        if let Some(ref patch_args) = self.arguments {
            if let Some(ref mut existing) = version.arguments {
                // Merge JVM args
                if let Some(ref patch_jvm) = patch_args.jvm {
                    let mut jvm = existing.jvm.take().unwrap_or_default();
                    jvm.extend(patch_jvm.clone());
                    existing.jvm = Some(jvm);
                }
                // Merge game args
                if let Some(ref patch_game) = patch_args.game {
                    let mut game = existing.game.take().unwrap_or_default();
                    game.extend(patch_game.clone());
                    existing.game = Some(game);
                }
            } else {
                version.arguments = Some(patch_args.clone());
            }
        }

        // Merge legacy minecraftArguments
        if let Some(ref ma) = self.minecraft_arguments {
            version.minecraft_arguments = Some(ma.clone());
        }

        // Append to patches list (full patch — enables incremental round-trip).
        version.patches.push(self.to_game_patch());
    }

    /// Convert this patch to the version-layer [`rmccc_version::GamePatch`],
    /// preserving every field so it can be serialized into a version JSON's
    /// `patches` array (HMCL incremental model — the original JSON is never
    /// mutated; rollback = removing the patch).
    pub fn to_game_patch(&self) -> rmccc_version::GamePatch {
        rmccc_version::GamePatch {
            id: self.id.clone(),
            version: self.version.clone(),
            priority: self.priority,
            main_class: self.main_class.clone(),
            arguments: self.arguments.clone(),
            libraries: self.libraries.clone(),
            asset_index: self.asset_index.clone(),
            java_version: self.java_version.clone(),
            inherits_from: self.inherits_from.clone(),
            compatibility_rules: self.compatibility_rules.clone(),
            hidden: self.hidden,
            minecraft_arguments: self.minecraft_arguments.clone(),
        }
    }
}

impl Default for GameInstancePatch {
    fn default() -> Self {
        Self {
            id: String::new(),
            version: String::new(),
            priority: PRIORITY_LOADER,
            main_class: None,
            arguments: None,
            libraries: Vec::new(),
            asset_index: None,
            java_version: None,
            inherits_from: None,
            compatibility_rules: Vec::new(),
            hidden: false,
            minecraft_arguments: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rmccc_version::{ReleaseType, VersionArgument};

    fn make_vanilla_game_version() -> rmccc_version::GameVersion {
        rmccc_version::GameVersion {
            id: "1.21".into(),
            release_type: ReleaseType::Release,
            main_class: "net.minecraft.client.main.Main".into(),
            asset_index: AssetIndex {
                id: "17".into(),
                ..Default::default()
            },
            java_version: Some(JavaRequirement {
                major_version: 21,
                component: None,
            }),
            ..Default::default()
        }
    }

    #[test]
    fn test_patch_merge_main_class() {
        let mut version = make_vanilla_game_version();

        let patch = GameInstancePatch::with_loader(
            "forge",
            "49.0.50",
            PRIORITY_LOADER,
            Some("cpw.mods.modlauncher.Launcher".into()),
            None,
            vec![],
        );

        patch.merge_into(&mut version);

        assert_eq!(version.main_class, "cpw.mods.modlauncher.Launcher");
        assert_eq!(version.patches.len(), 1);
        assert_eq!(version.patches[0].id, "forge");
        assert_eq!(version.patches[0].version, "49.0.50");
    }

    #[test]
    fn test_patch_merge_arguments() {
        let mut version = make_vanilla_game_version();

        let jvm_arg = VersionArgument::Simple("-Dforge.enabled=true".into());
        let args = VersionArguments {
            jvm: Some(vec![jvm_arg]),
            game: None,
        };

        let patch = GameInstancePatch::with_loader(
            "forge",
            "49.0.50",
            PRIORITY_LOADER,
            None,
            Some(args),
            vec![],
        );

        patch.merge_into(&mut version);

        let merged = version.arguments.unwrap();
        assert!(merged.jvm.is_some());
        let jvm_args = merged.jvm.unwrap();
        assert!(jvm_args
            .iter()
            .any(|a| { matches!(a, VersionArgument::Simple(s) if s == "-Dforge.enabled=true") }));
    }

    #[test]
    fn test_patch_merge_appends_libraries() {
        let mut version = make_vanilla_game_version();
        version.libraries.push(Library {
            name: "com.example:vanilla-lib:1.0".into(),
            ..Default::default()
        });

        let forge_lib = Library {
            name: "net.minecraftforge:forge:1.21-49.0.50".into(),
            ..Default::default()
        };

        let patch = GameInstancePatch::with_loader(
            "forge",
            "49.0.50",
            PRIORITY_LOADER,
            None,
            None,
            vec![forge_lib],
        );

        patch.merge_into(&mut version);

        assert_eq!(version.libraries.len(), 2);
        assert!(version
            .libraries
            .iter()
            .any(|l| l.name.contains("vanilla-lib")));
        assert!(version.libraries.iter().any(|l| l.name.contains("forge")));
    }
}
