//! Core types for Minecraft version metadata.
//!
//! Following HMCL's `GameInstanceManifest` record: the model carries the full
//! set of fields that appear in a Mojang version JSON so that local
//! `versions/{id}/{id}.json` files can be read verbatim and re-serialized.

use rmccc_error::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use ts_rs::TS;

/// Release type of a Minecraft version.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export_to = "types.ts")]
pub enum ReleaseType {
    Release,
    Snapshot,
    OldBeta,
    OldAlpha,
}

/// Basic version info from the manifest list.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export_to = "types.ts")]
pub struct VersionInfo {
    pub id: String,
    pub release_type: ReleaseType,
    pub url: String,
    pub release_time: String,
}

/// Full game version metadata (parsed from a Mojang version JSON file on disk).
///
/// Mirrors HMCL's `GameInstanceManifest`:
/// - `inherits_from` + `patches` enable the incremental, non-destructive
///   modloader installation model — the original JSON is never mutated.
/// - `raw_json` preserves the exact original content for faithful re-serialization.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GameVersion {
    pub id: String,
    #[serde(rename = "type")]
    pub release_type: ReleaseType,
    #[serde(rename = "mainClass")]
    pub main_class: String,
    /// The JAR file to use as the client. Defaults to the instance id itself
    /// when absent (HMCL `getInstanceJar` fallback).
    #[serde(default, rename = "jar", skip_serializing_if = "Option::is_none")]
    pub jar: Option<String>,
    #[serde(rename = "assetIndex")]
    pub asset_index: AssetIndex,
    pub downloads: HashMap<String, DownloadEntry>,
    pub libraries: Vec<Library>,
    #[serde(rename = "javaVersion")]
    pub java_version: Option<JavaRequirement>,
    /// Parent version ID (e.g. "1.20.4" for "1.20.4-forge-49.0.0").
    /// Followed during [`super::repository::resolve_version`] / repository resolve.
    #[serde(default, rename = "inheritsFrom")]
    pub inherits_from: Option<String>,
    /// Patches applied on top of the parent (e.g. Fabric/Forge loader).
    /// Serialized verbatim; resolve applies them sorted by `priority`.
    #[serde(default)]
    pub patches: Vec<GamePatch>,
    /// New-style arguments section (1.13+). Contains JVM and game arguments
    /// with template variables and OS/feature rules.
    #[serde(default)]
    pub arguments: Option<VersionArguments>,
    /// Legacy minecraftArguments string (1.12 and earlier).
    #[serde(default, rename = "minecraftArguments")]
    pub minecraft_arguments: Option<String>,
    /// Manifest-level compatibility rules (HMCL `compatibilityRules`).
    /// Instances whose rules don't apply to the current environment are hidden
    /// from the repository index (e.g. snapshots on unsupported platforms).
    #[serde(default, rename = "compatibilityRules")]
    pub compatibility_rules: Vec<CompatibilityRule>,
    /// Logging configuration (log4j config objects) — `logging` in version JSON.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub logging: Option<HashMap<String, LoggingInfo>>,
    /// Whether this version is the root of a version group (HMCL `root`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub root: Option<bool>,
    /// Hidden marker (HMCL `hidden`).
    #[serde(default, skip_serializing_if = "is_false")]
    pub hidden: bool,
    /// Oldest launcher version able to parse this manifest.
    #[serde(default, rename = "minimumLauncherVersion", skip_serializing_if = "Option::is_none")]
    pub minimum_launcher_version: Option<u32>,
    /// Forge/MC compliance level.
    #[serde(default, rename = "complianceLevel", skip_serializing_if = "Option::is_none")]
    pub compliance_level: Option<u32>,
    /// Version publish time.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub time: Option<String>,
    /// Version release time.
    #[serde(default, rename = "releaseTime", skip_serializing_if = "Option::is_none")]
    pub release_time: Option<String>,
    /// Original Mojang JSON content, preserved for exact re-serialization.
    /// Fixes Bug #7: Tauri `version_install` must save original camelCase JSON,
    /// not the Rust struct's snake_case serialization.
    #[serde(skip)]
    pub raw_json: Option<String>,
}

fn is_false(v: &bool) -> bool {
    !*v
}

impl Default for GameVersion {
    fn default() -> Self {
        Self {
            id: String::new(),
            release_type: ReleaseType::Release,
            main_class: "net.minecraft.client.main.Main".to_string(),
            jar: None,
            asset_index: AssetIndex::default(),
            downloads: HashMap::new(),
            libraries: Vec::new(),
            java_version: None,
            inherits_from: None,
            patches: Vec::new(),
            arguments: None,
            minecraft_arguments: None,
            compatibility_rules: Vec::new(),
            logging: None,
            root: None,
            hidden: false,
            minimum_launcher_version: None,
            compliance_level: None,
            time: None,
            release_time: None,
            raw_json: None,
        }
    }
}

/// The new-style arguments section from version JSON (1.13+).
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct VersionArguments {
    #[serde(default)]
    pub jvm: Option<Vec<VersionArgument>>,
    #[serde(default)]
    pub game: Option<Vec<VersionArgument>>,
}

/// A single argument from the version JSON. Can be a plain string (with ${variable}
/// templates) or a rule-gated argument with conditions.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum VersionArgument {
    /// Simple string argument, may contain ${variable} templates.
    /// e.g. "-Djava.library.path=${natives_directory}"
    Simple(String),
    /// Rule-based argument that only applies when all rules match.
    Ruled {
        #[serde(default)]
        rules: Vec<CompatibilityRule>,
        value: ArgumentValue,
    },
}

/// The value of a rule-based argument — can be a single string or an array.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ArgumentValue {
    Single(String),
    Multiple(Vec<String>),
}

impl ArgumentValue {
    pub fn as_vec(&self) -> Vec<&str> {
        match self {
            ArgumentValue::Single(s) => vec![s.as_str()],
            ArgumentValue::Multiple(v) => v.iter().map(|s| s.as_str()).collect(),
        }
    }
}

/// A rule that controls whether an argument / library / manifest is included.
///
/// This is the unified compatibility rule type, mirroring HMCL's
/// `CompatibilityRule` (action + os + features). `ArgumentRule` is kept as an
/// alias so existing call sites (arguments, patches) share the same type.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArgumentRule {
    #[serde(default)]
    pub action: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub os: Option<OsCondition>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub features: Option<HashMap<String, bool>>,
}

/// Unified compatibility rule — identical to [`ArgumentRule`].
pub type CompatibilityRule = ArgumentRule;

/// OS condition for argument rules and library rules.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OsCondition {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub arch: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
}

/// A version patch (mod loader injection point).
///
/// Mirrors HMCL's `GameInstancePatch`: patches are written into the version
/// JSON's `patches` array with a `priority`; [`super::repository`] sorts them
/// by priority and merges them on top of the resolved chain. This enables
/// non-destructive, incremental modloader installation and easy rollback
/// (simply remove the patch from the JSON).
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct GamePatch {
    /// Patch identifier (e.g. "forge", "fabric", "optifine").
    pub id: String,
    /// Patch version string.
    #[serde(default)]
    pub version: String,
    /// Merge priority. Higher values override lower ones when merged.
    #[serde(default)]
    pub priority: i32,
    /// Override the main class (set by modloaders that replace the launch entry point).
    #[serde(default, rename = "mainClass", skip_serializing_if = "Option::is_none")]
    pub main_class: Option<String>,
    /// Additional JVM / game arguments.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub arguments: Option<VersionArguments>,
    /// Libraries to add to the classpath.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub libraries: Vec<Library>,
    /// Asset index override (usually None).
    #[serde(default, rename = "assetIndex", skip_serializing_if = "Option::is_none")]
    pub asset_index: Option<AssetIndex>,
    /// Java version requirement override.
    #[serde(default, rename = "javaVersion", skip_serializing_if = "Option::is_none")]
    pub java_version: Option<JavaRequirement>,
    /// Parent version to inherit from (used by Fabric/Quilt profile JSON).
    #[serde(default, rename = "inheritsFrom", skip_serializing_if = "Option::is_none")]
    pub inherits_from: Option<String>,
    /// Compatibility rules for this patch.
    #[serde(default, rename = "compatibilityRules", skip_serializing_if = "Vec::is_empty")]
    pub compatibility_rules: Vec<CompatibilityRule>,
    /// Whether this patch is hidden from the user.
    #[serde(default, skip_serializing_if = "is_false")]
    pub hidden: bool,
    /// Legacy minecraftArguments string (1.12 and earlier).
    #[serde(default, rename = "minecraftArguments", skip_serializing_if = "Option::is_none")]
    pub minecraft_arguments: Option<String>,
}

/// Asset index reference.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssetIndex {
    pub id: String,
    pub url: String,
    pub sha1: String,
    pub size: u64,
    pub total_size: u64,
}

impl Default for AssetIndex {
    fn default() -> Self {
        Self {
            id: String::new(),
            url: String::new(),
            sha1: String::new(),
            size: 0,
            total_size: 0,
        }
    }
}

/// A downloadable entry (client JAR, server JAR, etc.).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DownloadEntry {
    pub url: String,
    pub sha1: String,
    pub size: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
}

impl Default for DownloadEntry {
    fn default() -> Self {
        Self {
            url: String::new(),
            sha1: String::new(),
            size: 0,
            path: None,
        }
    }
}

/// Library dependency.
///
/// Carries the full Mojang field set: `downloads`, `rules`, and the less
/// common `extract` / `natives` / `checksums` / `MMC-hint` / `MMC-filename`
/// fields used by older versions and third-party manifests.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Library {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub downloads: Option<LibraryDownloads>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rules: Option<Vec<LibraryRule>>,
    /// Files to exclude when extracting natives (HMCL `ExtractRules`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub extract: Option<ExtractRules>,
    /// Classifier → native name map, e.g. `{"osx": "natives-osx", ...}`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub natives: Option<HashMap<String, String>>,
    /// Checksums (usually a single SHA-1 entry).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub checksums: Option<Vec<String>>,
    /// MMC-hint for MultiMC compatibility (HMCL `Library.hint`).
    #[serde(default, rename = "MMC-hint", skip_serializing_if = "Option::is_none")]
    pub hint: Option<String>,
    /// MMC-filename override (HMCL `Library.filename`).
    #[serde(default, rename = "MMC-filename", skip_serializing_if = "Option::is_none")]
    pub filename: Option<String>,
}

impl Default for Library {
    fn default() -> Self {
        Self {
            name: String::new(),
            downloads: None,
            rules: None,
            extract: None,
            natives: None,
            checksums: None,
            hint: None,
            filename: None,
        }
    }
}

/// Files to exclude when extracting a native library JAR.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ExtractRules {
    #[serde(default)]
    pub exclude: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LibraryDownloads {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub artifact: Option<DownloadEntry>,
    /// Platform-specific classifier JARs (natives). Keyed by classifier name
    /// like "natives-macos", "natives-windows", "natives-linux".
    #[serde(default)]
    pub classifiers: Option<std::collections::HashMap<String, DownloadEntry>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LibraryRule {
    #[serde(default)]
    pub action: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub os: Option<OsRule>,
}

impl From<LibraryRule> for CompatibilityRule {
    fn from(r: LibraryRule) -> Self {
        Self {
            action: r.action,
            os: r.os.map(|os| OsCondition {
                name: os.name,
                arch: os.arch,
                version: os.version,
            }),
            features: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OsRule {
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub arch: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
}

/// Java version requirement.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JavaRequirement {
    pub major_version: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub component: Option<String>,
}

/// Logging configuration entry (`logging.client` in version JSON).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoggingInfo {
    pub argument: String,
    #[serde(default)]
    pub file: DownloadEntry,
    #[serde(rename = "type")]
    pub log_type: String,
}

/// Trait for version providers (network metadata source during installation).
#[async_trait::async_trait]
pub trait VersionProvider: Send + Sync {
    fn provider_id(&self) -> &str;
    async fn fetch_versions(&self) -> Result<Vec<VersionInfo>>;
    async fn fetch_version(&self, id: &str) -> Result<GameVersion>;
}
