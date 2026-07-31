//! Core types for Minecraft version metadata.

use rmccc_error::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Release type of a Minecraft version.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ReleaseType {
    Release,
    Snapshot,
    OldBeta,
    OldAlpha,
}

/// Basic version info from the manifest list.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VersionInfo {
    pub id: String,
    pub release_type: ReleaseType,
    pub url: String,
    pub release_time: String,
}

/// Full game version metadata (parsed from Mojang version JSON).
///
/// Supports Minecraft's version inheritance model: a version can `inherits_from`
/// a parent version, with optional `patches` applied (e.g. from mod loaders).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GameVersion {
    pub id: String,
    #[serde(rename = "type")]
    pub release_type: ReleaseType,
    #[serde(rename = "mainClass")]
    pub main_class: String,
    #[serde(rename = "assetIndex")]
    pub asset_index: AssetIndex,
    pub downloads: HashMap<String, DownloadEntry>,
    pub libraries: Vec<Library>,
    #[serde(rename = "javaVersion")]
    pub java_version: Option<JavaRequirement>,
    /// Parent version ID (e.g. "1.20.4" for "1.20.4-forge-49.0.0").
    /// Followed during [`super::resolve_version`].
    #[serde(default, rename = "inheritsFrom")]
    pub inherits_from: Option<String>,
    /// Patches applied on top of the parent (e.g. Fabric/Forge loader).
    #[serde(default)]
    pub patches: Vec<GamePatch>,
    /// New-style arguments section (1.13+). Contains JVM and game arguments
    /// with template variables and OS/feature rules.
    #[serde(default)]
    pub arguments: Option<VersionArguments>,
    /// Legacy minecraftArguments string (1.12 and earlier).
    #[serde(default, rename = "minecraftArguments")]
    pub minecraft_arguments: Option<String>,
    /// Original Mojang JSON content, preserved for exact re-serialization.
    /// Fixes Bug #7: Tauri `version_install` must save original camelCase JSON,
    /// not the Rust struct's snake_case serialization.
    #[serde(skip)]
    pub raw_json: Option<String>,
}

/// The new-style arguments section from version JSON (1.13+).
#[derive(Debug, Clone, Serialize, Deserialize)]
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
        rules: Vec<ArgumentRule>,
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

/// A rule that controls whether an argument is included.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArgumentRule {
    #[serde(default)]
    pub action: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub os: Option<OsCondition>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub features: Option<HashMap<String, bool>>,
}

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
/// Patches are not applied directly; they are informational markers used by
/// [`super::resolve_version`] to collect extra libraries and arguments from
/// parent versions in the chain.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GamePatch {
    /// Patch identifier (e.g. "forge", "fabric").
    pub id: String,
    /// Patch version string.
    pub version: String,
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

/// A downloadable entry (client JAR, server JAR, etc.).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DownloadEntry {
    pub url: String,
    pub sha1: String,
    pub size: u64,
    pub path: Option<String>,
}

/// Library dependency.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Library {
    pub name: String,
    pub downloads: Option<LibraryDownloads>,
    pub rules: Option<Vec<LibraryRule>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LibraryDownloads {
    pub artifact: Option<DownloadEntry>,
    /// Platform-specific classifier JARs (natives). Keyed by classifier name
    /// like "natives-macos", "natives-windows", "natives-linux".
    #[serde(default)]
    pub classifiers: Option<std::collections::HashMap<String, DownloadEntry>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LibraryRule {
    pub action: String,
    pub os: Option<OsRule>,
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
    pub component: Option<String>,
}

/// Trait for version providers.
#[async_trait::async_trait]
pub trait VersionProvider: Send + Sync {
    fn provider_id(&self) -> &str;
    async fn fetch_versions(&self) -> Result<Vec<VersionInfo>>;
    async fn fetch_version(&self, id: &str) -> Result<GameVersion>;
}
