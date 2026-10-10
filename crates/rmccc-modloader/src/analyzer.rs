//! Library analyzer — detect installed modloaders by scanning library Maven coordinates.
//!
//! [`LibraryAnalyzer`] scans a [`rmccc_version::GameVersion`]'s libraries and patches
//! to determine which modloaders are installed and their versions.
//!
//! This is the Rust equivalent of HMCL's `LibraryAnalyzer` class.

use rmccc_version::Library;
use std::collections::HashMap;

/// Modloader type identifiers detected from library Maven coordinates.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LibraryType {
    /// The base Minecraft game.
    Minecraft,
    /// Legacy Fabric loader (1.13 and older).
    LegacyFabric,
    /// Legacy Fabric API.
    LegacyFabricApi,
    /// Fabric loader.
    Fabric,
    /// Fabric API.
    FabricApi,
    /// Forge loader.
    Forge,
    /// NeoForge loader.
    NeoForge,
    /// Quilt loader.
    Quilt,
    /// Quilt API (Quilted Fabric API).
    QuiltApi,
    /// LiteLoader.
    LiteLoader,
    /// OptiFine.
    OptiFine,
    /// Cleanroom loader.
    Cleanroom,
}

impl LibraryType {
    /// The patch id used in version JSON (e.g. "forge", "fabric").
    pub fn patch_id(&self) -> &'static str {
        match self {
            LibraryType::Minecraft => "game",
            LibraryType::LegacyFabric => "legacyfabric",
            LibraryType::LegacyFabricApi => "legacyfabric-api",
            LibraryType::Fabric => "fabric",
            LibraryType::FabricApi => "fabric-api",
            LibraryType::Forge => "forge",
            LibraryType::NeoForge => "neoforge",
            LibraryType::Quilt => "quilt",
            LibraryType::QuiltApi => "quilt-api",
            LibraryType::LiteLoader => "liteloader",
            LibraryType::OptiFine => "optifine",
            LibraryType::Cleanroom => "cleanroom",
        }
    }

    /// Look up a `LibraryType` from a patch id string.
    pub fn from_patch_id(id: &str) -> Option<Self> {
        match id {
            "game" => Some(LibraryType::Minecraft),
            "legacyfabric" => Some(LibraryType::LegacyFabric),
            "legacyfabric-api" => Some(LibraryType::LegacyFabricApi),
            "fabric" => Some(LibraryType::Fabric),
            "fabric-api" => Some(LibraryType::FabricApi),
            "forge" => Some(LibraryType::Forge),
            "neoforge" => Some(LibraryType::NeoForge),
            "quilt" => Some(LibraryType::Quilt),
            "quilt-api" => Some(LibraryType::QuiltApi),
            "liteloader" => Some(LibraryType::LiteLoader),
            "optifine" => Some(LibraryType::OptiFine),
            "cleanroom" => Some(LibraryType::Cleanroom),
            _ => None,
        }
    }

    /// Whether this type is a modloader (not an API addon or Minecraft itself).
    pub fn is_modloader(&self) -> bool {
        matches!(
            self,
            LibraryType::Fabric
                | LibraryType::Forge
                | LibraryType::NeoForge
                | LibraryType::Quilt
                | LibraryType::LiteLoader
                | LibraryType::OptiFine
                | LibraryType::Cleanroom
                | LibraryType::LegacyFabric
        )
    }

    /// Detect the modloader type from a single library's Maven coordinates.
    ///
    /// `all_libraries` is the full list, used for cross-detection rules
    /// (e.g. distinguishing Forge from NeoForge).
    pub fn detect(library: &Library, all_libraries: &[Library]) -> Option<Self> {
        let parts: Vec<&str> = library.name.split(':').collect();
        if parts.len() < 3 {
            return None;
        }
        let group = parts[0];
        let artifact = parts[1];

        // Check in priority order (NeoForge before Forge, LegacyFabric before Fabric)

        // Cleanroom
        if group == "com.cleanroommc" && artifact == "cleanroom" {
            return Some(LibraryType::Cleanroom);
        }

        // NeoForge (check before Forge)
        if group == "net.neoforged.fancymodloader" && (artifact == "core" || artifact == "loader") {
            return Some(LibraryType::NeoForge);
        }

        // Forge
        if group == "net.minecraftforge" && (artifact == "forge" || artifact == "fmlloader") {
            // Exclude if NeoForge library is also present
            let has_neoforge = all_libraries.iter().any(|l| {
                let p: Vec<&str> = l.name.split(':').collect();
                p.len() >= 2 && p[0] == "net.neoforged.fancymodloader"
            });
            if has_neoforge {
                return None;
            }
            return Some(LibraryType::Forge);
        }

        // Legacy Fabric (check before Fabric)
        if group == "net.fabricmc" && artifact == "fabric-loader" {
            let has_legacy = all_libraries.iter().any(|l| {
                let p: Vec<&str> = l.name.split(':').collect();
                p.len() >= 2 && p[0] == "net.legacyfabric"
            });
            if has_legacy {
                return Some(LibraryType::LegacyFabric);
            }
            // Exclude if LegacyFabric detection already handled
            // Actually LegacyFabric is detected above with stricter check.
            // This is regular Fabric.
        }

        // Fabric
        if group == "net.fabricmc" && artifact == "fabric-loader" {
            return Some(LibraryType::Fabric);
        }

        // Fabric API
        if group == "net.fabricmc" && artifact == "fabric-api" {
            return Some(LibraryType::FabricApi);
        }

        // Legacy Fabric
        if group == "net.legacyfabric" && artifact == "fabric-loader" {
            return Some(LibraryType::LegacyFabric);
        }

        // Legacy Fabric API
        if group == "net.legacyfabric" && artifact == "legacyfabric-api" {
            return Some(LibraryType::LegacyFabricApi);
        }

        // Quilt
        if group == "org.quiltmc" && artifact == "quilt-loader" {
            return Some(LibraryType::Quilt);
        }

        // Quilt API
        if group == "org.quiltmc" && artifact == "quilt-api" {
            return Some(LibraryType::QuiltApi);
        }

        // LiteLoader
        if group == "com.mumfrey" && artifact == "liteloader" {
            return Some(LibraryType::LiteLoader);
        }

        // OptiFine
        if (group == "optifine" || group == "net.optifine") && artifact == "OptiFine" {
            return Some(LibraryType::OptiFine);
        }

        None
    }
}

/// Information about a detected modloader library.
#[derive(Debug, Clone)]
pub struct LibraryMark {
    /// Patch id (e.g. "forge", "fabric").
    pub library_id: String,
    /// Detected version string.
    pub library_version: Option<String>,
    /// Whether the library comes from a structured patch (clear) or
    /// was detected from raw libraries (just existed).
    pub status: LibraryStatus,
}

/// How the modloader library was found.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LibraryStatus {
    /// The library is part of a structured patch (installed by this launcher).
    Clear,
    /// The library exists in the raw library list (externally installed).
    JustExisted,
}

/// Analyzes a [`rmccc_version::GameVersion`] to detect installed modloaders.
pub struct LibraryAnalyzer {
    /// library_id → (Library, version_string)
    markers: HashMap<String, (Library, Option<String>)>,
    /// The version being analyzed (modified by `remove`).
    version: rmccc_version::GameVersion,
}

impl LibraryAnalyzer {
    /// Analyze a game version to detect installed modloaders.
    ///
    /// Scans both the version's libraries and its patches list.
    pub fn analyze(version: &rmccc_version::GameVersion, game_version: &str) -> Self {
        let mut markers: HashMap<String, (Library, Option<String>)> = HashMap::new();

        // Always mark Minecraft
        markers.insert(
            LibraryType::Minecraft.patch_id().to_string(),
            (
                Library {
                    name: "minecraft".into(),
                    ..Default::default()
                },
                Some(game_version.to_string()),
            ),
        );

        // Scan libraries
        for library in &version.libraries {
            if let Some(ty) = LibraryType::detect(library, &version.libraries) {
                let patch_id = ty.patch_id().to_string();
                let ver = Self::extract_version(library, ty);
                markers.insert(patch_id, (library.clone(), ver));
            }
        }

        // Scan patches
        for patch in &version.patches {
            if patch.id.is_empty() {
                continue;
            }
            // Only add if not already detected from libraries
            markers.entry(patch.id.clone()).or_insert_with(|| {
                (
                    Library {
                        name: format!("__patch__{}", patch.id),
                        ..Default::default()
                    },
                    Some(patch.version.clone()),
                )
            });
        }

        Self {
            markers,
            version: version.clone(),
        }
    }

    /// Check whether a modloader is installed.
    pub fn has(&self, ty: LibraryType) -> bool {
        self.markers.contains_key(ty.patch_id())
    }

    /// Check by patch id string.
    pub fn has_id(&self, id: &str) -> bool {
        self.markers.contains_key(id)
    }

    /// Get the version of an installed modloader.
    pub fn get_version(&self, ty: LibraryType) -> Option<&str> {
        self.markers
            .get(ty.patch_id())
            .and_then(|(_, v)| v.as_deref())
    }

    /// Check whether any modloader is installed.
    pub fn has_any_modloader(&self) -> bool {
        self.markers
            .keys()
            .filter_map(|id| LibraryType::from_patch_id(id))
            .any(|ty| ty.is_modloader())
    }

    /// Check whether the version uses a mod launcher main class.
    pub fn has_mod_launcher(&self) -> bool {
        let main = &self.version.main_class;
        main == MOD_LAUNCHER_MAIN
            || main == LAUNCH_WRAPPER_MAIN
            || main == BOOTSTRAP_LAUNCHER_MAIN
            || main == FORGE_BOOTSTRAP_MAIN
            || main == NEO_FORGE_BOOTSTRAP_MAIN
            || main.starts_with("net.fabricmc")
            || main.starts_with("org.quiltmc")
            || main.starts_with("cpw.mods")
    }

    /// Remove a modloader's libraries and patches from the analyzed version.
    ///
    /// Returns `self` for chaining.
    pub fn remove(&mut self, library_id: &str) -> &mut Self {
        let ty = LibraryType::from_patch_id(library_id);
        self.markers.remove(library_id);

        // Remove matching libraries.
        // We collect indices first to avoid borrow conflicts between
        // the immutable borrow in `detect` and the mutable borrow in `retain`.
        if let Some(ty) = ty {
            let libs = &self.version.libraries;
            let to_remove: Vec<usize> = libs
                .iter()
                .enumerate()
                .filter(|(_, lib)| LibraryType::detect(lib, libs) == Some(ty))
                .map(|(i, _)| i)
                .collect();
            // Remove in reverse order to preserve indices
            for i in to_remove.into_iter().rev() {
                self.version.libraries.remove(i);
            }
        }

        // Remove matching patches
        self.version.patches.retain(|p| p.id != library_id);

        self
    }

    /// Consume the analyzer and return the (possibly modified) version.
    pub fn build(self) -> rmccc_version::GameVersion {
        self.version
    }

    /// Iterate over detected modloader marks.
    pub fn iter_marks(&self) -> impl Iterator<Item = LibraryMark> + '_ {
        self.markers.iter().map(|(id, (_, ver))| LibraryMark {
            library_id: id.clone(),
            library_version: ver.clone(),
            status: {
                if self.version.patches.iter().any(|p| p.id == *id) {
                    LibraryStatus::Clear
                } else {
                    LibraryStatus::JustExisted
                }
            },
        })
    }

    /// Extract a human-readable version from a library.
    fn extract_version(library: &Library, ty: LibraryType) -> Option<String> {
        let parts: Vec<&str> = library.name.split(':').collect();
        if parts.len() < 3 {
            return None;
        }
        let raw_version = parts[2];

        match ty {
            LibraryType::Forge | LibraryType::NeoForge => {
                // Format: "MCVER-FORGEVER" or "MCVER-FORGEVER-BRANCH"
                // Extract the Forge version part.
                if let Some(dash_pos) = raw_version.find('-') {
                    let rest = &raw_version[dash_pos + 1..];
                    // Handle branches like "49.0.50" or "49.0.50-20240720"
                    Some(rest.to_string())
                } else {
                    Some(raw_version.to_string())
                }
            }
            LibraryType::OptiFine => {
                // Format: "MCVER_HD_U_XX" or "MCVER_HD_U_XX_pre"
                if let Some(underscore_pos) = raw_version.find('_') {
                    Some(raw_version[underscore_pos + 1..].to_string())
                } else {
                    Some(raw_version.to_string())
                }
            }
            _ => Some(raw_version.to_string()),
        }
    }
}

// ── Known main class constants ──

/// Vanilla Minecraft main class.
pub const VANILLA_MAIN: &str = "net.minecraft.client.main.Main";
/// Legacy LaunchWrapper main class (Forge 1.12-, OptiFine).
pub const LAUNCH_WRAPPER_MAIN: &str = "net.minecraft.launchwrapper.Launch";
/// ModLauncher main class (Forge 1.13+).
pub const MOD_LAUNCHER_MAIN: &str = "cpw.mods.modlauncher.Launcher";
/// BootstrapLauncher main class (Forge 1.17+).
pub const BOOTSTRAP_LAUNCHER_MAIN: &str = "cpw.mods.bootstraplauncher.BootstrapLauncher";
/// ForgeBootstrap main class.
pub const FORGE_BOOTSTRAP_MAIN: &str = "net.minecraftforge.bootstrap.ForgeBootstrap";
/// NeoForge startup client main class.
pub const NEO_FORGE_BOOTSTRAP_MAIN: &str = "net.neoforged.fml.startup.Client";

/// Set of main classes that are compatible with Forge + OptiFine coexistence.
pub const FORGE_OPTIFINE_MAIN: &[&str] = &[
    VANILLA_MAIN,
    LAUNCH_WRAPPER_MAIN,
    MOD_LAUNCHER_MAIN,
    BOOTSTRAP_LAUNCHER_MAIN,
    FORGE_BOOTSTRAP_MAIN,
    NEO_FORGE_BOOTSTRAP_MAIN,
];

/// Vanilla-compatible main classes (Fabric/Quilt can only install on vanilla).
pub const VANILLA_ONLY_MAIN: &[&str] = &[VANILLA_MAIN];

/// Known Forge tweaker classes (injected as --tweakClass).
pub const FORGE_TWEAKERS: &[&str] = &[
    "net.minecraftforge.legacy._1_5_2.LibraryFixerTweaker",
    "cpw.mods.fml.common.launcher.FMLTweaker",
    "net.minecraftforge.fml.common.launcher.FMLTweaker",
];

/// Known OptiFine tweaker classes.
pub const OPTIFINE_TWEAKERS: &[&str] =
    &["optifine.OptiFineTweaker", "optifine.OptiFineForgeTweaker"];

/// LiteLoader tweaker class.
pub const LITELOADER_TWEAKER: &str = "com.mumfrey.liteloader.launch.LiteLoaderTweaker";

#[cfg(test)]
mod tests {
    use super::*;

    fn make_vanilla_version() -> rmccc_version::GameVersion {
        rmccc_version::GameVersion {
            id: "1.21".into(),
            release_type: rmccc_version::ReleaseType::Release,
            main_class: VANILLA_MAIN.into(),
            asset_index: rmccc_version::AssetIndex {
                id: "17".into(),
                ..Default::default()
            },
            ..Default::default()
        }
    }

    fn make_lib(name: &str) -> Library {
        Library {
            name: name.into(),
            ..Default::default()
        }
    }

    #[test]
    fn test_detect_forge() {
        let lib = make_lib("net.minecraftforge:forge:1.21-49.0.50");
        assert_eq!(LibraryType::detect(&lib, &[]), Some(LibraryType::Forge));
    }

    #[test]
    fn test_detect_fabric() {
        let lib = make_lib("net.fabricmc:fabric-loader:0.16.0");
        assert_eq!(LibraryType::detect(&lib, &[]), Some(LibraryType::Fabric));
    }

    #[test]
    fn test_detect_neoforge() {
        let lib = make_lib("net.neoforged.fancymodloader:loader:3.0.0");
        assert_eq!(LibraryType::detect(&lib, &[]), Some(LibraryType::NeoForge));
    }

    #[test]
    fn test_forge_not_detected_when_neoforge_present() {
        let neoforge_lib = make_lib("net.neoforged.fancymodloader:loader:3.0.0");
        let forge_lib = make_lib("net.minecraftforge:forge:1.21-49.0.50");
        let all = vec![neoforge_lib, forge_lib.clone()];
        // Forge should NOT be detected because NeoForge is present
        assert_eq!(LibraryType::detect(&forge_lib, &all), None);
    }

    #[test]
    fn test_detect_liteloader() {
        let lib = make_lib("com.mumfrey:liteloader:1.12.2");
        assert_eq!(
            LibraryType::detect(&lib, &[]),
            Some(LibraryType::LiteLoader)
        );
    }

    #[test]
    fn test_detect_optifine() {
        let lib = make_lib("optifine:OptiFine:1.21_HD_U_J1");
        assert_eq!(LibraryType::detect(&lib, &[]), Some(LibraryType::OptiFine));
    }

    #[test]
    fn test_detect_quilt() {
        let lib = make_lib("org.quiltmc:quilt-loader:0.26.0");
        assert_eq!(LibraryType::detect(&lib, &[]), Some(LibraryType::Quilt));
    }

    #[test]
    fn test_unknown_library() {
        let lib = make_lib("com.example:unknown:1.0");
        assert_eq!(LibraryType::detect(&lib, &[]), None);
    }

    #[test]
    fn test_analyzer_detects_installed_modloader() {
        let mut version = make_vanilla_version();
        version
            .libraries
            .push(make_lib("net.fabricmc:fabric-loader:0.16.0"));
        version
            .libraries
            .push(make_lib("net.fabricmc:fabric-api:0.100.0"));

        let analyzer = LibraryAnalyzer::analyze(&version, "1.21");

        assert!(analyzer.has(LibraryType::Fabric));
        assert!(analyzer.has(LibraryType::FabricApi));
        assert!(!analyzer.has(LibraryType::Forge));
        assert!(analyzer.has_any_modloader());
    }

    #[test]
    fn test_analyzer_vanilla_has_no_modloader() {
        let version = make_vanilla_version();
        let analyzer = LibraryAnalyzer::analyze(&version, "1.21");
        assert!(!analyzer.has_any_modloader());
        assert!(analyzer.has(LibraryType::Minecraft));
    }

    #[test]
    fn test_analyzer_remove_modloader() {
        let mut version = make_vanilla_version();
        version
            .libraries
            .push(make_lib("net.minecraftforge:forge:1.21-49.0.50"));
        version.patches.push(rmccc_version::GamePatch {
            id: "forge".into(),
            version: "49.0.50".into(),
            ..Default::default()
        });

        let mut analyzer = LibraryAnalyzer::analyze(&version, "1.21");
        assert!(analyzer.has(LibraryType::Forge));

        analyzer.remove("forge");
        let cleaned = analyzer.build();

        assert!(cleaned.patches.iter().all(|p| p.id != "forge"));
        assert!(cleaned.libraries.iter().all(|l| !l.name.contains("forge")));
    }

    #[test]
    fn test_extract_forge_version() {
        let lib = make_lib("net.minecraftforge:forge:1.21-49.0.50");
        let ver = LibraryAnalyzer::extract_version(&lib, LibraryType::Forge);
        assert_eq!(ver, Some("49.0.50".into()));
    }

    #[test]
    fn test_patch_id_roundtrip() {
        for ty in &[
            LibraryType::Forge,
            LibraryType::Fabric,
            LibraryType::NeoForge,
            LibraryType::Quilt,
            LibraryType::OptiFine,
            LibraryType::LiteLoader,
        ] {
            assert_eq!(LibraryType::from_patch_id(ty.patch_id()), Some(*ty));
        }
    }

    #[test]
    fn test_is_modloader() {
        assert!(LibraryType::Forge.is_modloader());
        assert!(LibraryType::Fabric.is_modloader());
        assert!(!LibraryType::FabricApi.is_modloader());
        assert!(!LibraryType::Minecraft.is_modloader());
    }
}
