//! Forge install profile data structures.
//!
//! Parsed from the `install_profile.json` inside Forge installer JARs.
//! Two formats exist:
//!
//! - **New** (1.13+): `ForgeNewInstallProfile` — contains `spec`, `version`, `processors`, `data`, `libraries`
//! - **Old** (1.12-): `ForgeInstallProfile` — contains `install` and `versionInfo`

use rmccc_version::Library;
use serde::Deserialize;

/// Old-format Forge install profile (1.12 and earlier).
///
/// Found inside installer JAR at `install_profile.json`.
/// Contains the version metadata directly.
#[derive(Debug, Clone, Deserialize)]
pub struct ForgeInstallProfile {
    pub install: ForgeInstall,
    #[serde(rename = "versionInfo")]
    pub version_info: rmccc_version::GameVersion,
}

/// Installation metadata in old-format profiles.
#[derive(Debug, Clone, Deserialize)]
pub struct ForgeInstall {
    /// Minecraft version this installer targets.
    pub minecraft: String,
    /// Maven artifact descriptor for the universal JAR.
    pub path: ForgeArtifact,
    /// Path inside the installer JAR to the universal JAR.
    #[serde(rename = "filePath")]
    pub file_path: String,
}

/// Maven artifact descriptor used in old Forge install profiles.
#[derive(Debug, Clone, Deserialize)]
pub struct ForgeArtifact {
    pub domain: Option<String>,
    pub name: String,
    pub version: Option<String>,
}

impl ForgeArtifact {
    /// Build a Maven-style library name from this artifact.
    pub fn to_library_name(&self) -> String {
        let domain = self.domain.as_deref().unwrap_or("net.minecraftforge");
        let version = self.version.as_deref().unwrap_or("");
        format!("{}:{}:{}", domain, self.name, version)
    }
}

// ── New-format (1.13+) ──

/// New-format Forge install profile (1.13+).
///
/// Contains processor definitions and data map for
/// building the modded environment.
#[derive(Debug, Clone, Deserialize)]
pub struct ForgeNewInstallProfile {
    /// Specification version.
    pub spec: i32,
    /// Minecraft version this installer targets.
    pub minecraft: String,
    /// Path to the version JSON inside the installer JAR.
    pub json: String,
    /// Forge version string.
    pub version: String,
    /// Optional Maven artifact for the main JAR.
    #[serde(default)]
    pub path: Option<String>,
    /// Libraries required by processors.
    #[serde(default)]
    pub libraries: Vec<Library>,
    /// Processor definitions — executed sequentially.
    #[serde(default)]
    pub processors: Vec<Processor>,
    /// Data entries for template variable substitution.
    #[serde(default)]
    pub data: std::collections::HashMap<String, DataEntry>,
}

/// A single processor step in the Forge installation pipeline.
///
/// Each processor is a Java program that performs one transformation
/// (e.g. downloading mappings, patching the Minecraft JAR, merging libraries).
#[derive(Debug, Clone, Deserialize)]
pub struct Processor {
    /// Which sides this processor runs on. We only run "client" processors.
    #[serde(default)]
    pub sides: Option<Vec<String>>,
    /// Maven artifact descriptor for the executable JAR.
    pub jar: String,
    /// Maven artifact descriptors for the classpath dependencies.
    #[serde(default)]
    pub classpath: Vec<String>,
    /// Arguments to pass to the processor JAR.
    ///
    /// Each argument can contain template variables:
    /// - `{key}` — lookup in the data map
    /// - `[group:name:version]` — resolve to artifact file path
    /// - `'text'` — literal string
    #[serde(default)]
    pub args: Vec<String>,
    /// Output file paths → expected SHA-1 checksums.
    ///
    /// Used for idempotency: if all outputs exist with correct hashes,
    /// the processor is skipped.
    #[serde(default)]
    pub outputs: std::collections::HashMap<String, String>,
}

impl Processor {
    /// Check whether this processor should run on the client side.
    pub fn is_client_side(&self) -> bool {
        match &self.sides {
            None => true, // No sides restriction = runs everywhere
            Some(sides) => sides.iter().any(|s| s == "client"),
        }
    }
}

/// A data entry for template variable substitution.
#[derive(Debug, Clone, Deserialize)]
pub struct DataEntry {
    /// The client-side value.
    ///
    /// Can be:
    /// - `[group:name:version]` — artifact path
    /// - `'text'` — literal string
    /// - `text` — file inside installer JAR (extracted to temp dir)
    pub client: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_new_install_profile() {
        let json = r#"{
            "spec": 1,
            "minecraft": "1.21",
            "json": "/version.json",
            "version": "49.0.50",
            "libraries": [],
            "processors": [
                {
                    "sides": ["client"],
                    "jar": "net.minecraftforge:forge:1.21-49.0.50:installer",
                    "classpath": ["net.minecraftforge:forge:1.21-49.0.50:universal"],
                    "args": ["--task", "DOWNLOAD_MOJMAPS", "--version", "{MC_VERSION}", "--output", "{MOJMAPS_OUTPUT}"],
                    "outputs": {
                        "{MOJMAPS_OUTPUT}": "abc123"
                    }
                }
            ],
            "data": {
                "MC_VERSION": {"client": "'1.21'"},
                "MOJMAPS_OUTPUT": {"client": "[net.minecraft:client:1.21-mojmaps:txt]"}
            }
        }"#;

        let profile: ForgeNewInstallProfile = serde_json::from_str(json).unwrap();
        assert_eq!(profile.spec, 1);
        assert_eq!(profile.minecraft, "1.21");
        assert_eq!(profile.version, "49.0.50");
        assert_eq!(profile.processors.len(), 1);
        assert!(profile.processors[0].is_client_side());
        assert_eq!(profile.data.len(), 2);
    }

    #[test]
    fn test_parse_old_install_profile() {
        let json = r#"{
            "install": {
                "minecraft": "1.12.2",
                "path": {
                    "domain": "net.minecraftforge",
                    "name": "forge",
                    "version": "1.12.2-14.23.5.2859"
                },
                "filePath": "maven/net/minecraftforge/forge/1.12.2-14.23.5.2859/forge-1.12.2-14.23.5.2859-universal.jar"
            },
            "versionInfo": {
                "id": "1.12.2-forge-14.23.5.2859",
                "type": "release",
                "mainClass": "net.minecraft.launchwrapper.Launch",
                "assetIndex": {"id": "1.12", "url": "", "sha1": "", "size": 0, "total_size": 0},
                "downloads": {},
                "libraries": [],
                "minecraftArguments": "--tweakClass net.minecraftforge.fml.common.launcher.FMLTweaker"
            }
        }"#;

        let profile: ForgeInstallProfile = serde_json::from_str(json).unwrap();
        assert_eq!(profile.install.minecraft, "1.12.2");
        assert_eq!(profile.version_info.id, "1.12.2-forge-14.23.5.2859");
        assert_eq!(profile.version_info.main_class, "net.minecraft.launchwrapper.Launch");
        assert!(profile.version_info.minecraft_arguments.is_some());
    }

    #[test]
    fn test_forge_artifact_to_library_name() {
        let a = ForgeArtifact {
            domain: Some("net.minecraftforge".into()),
            name: "forge".into(),
            version: Some("1.12.2-14.23.5.2859".into()),
        };
        assert_eq!(
            a.to_library_name(),
            "net.minecraftforge:forge:1.12.2-14.23.5.2859"
        );
    }
}
