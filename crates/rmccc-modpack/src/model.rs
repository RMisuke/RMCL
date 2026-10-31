//! Modpack model shared by parsers, the installer, and the IPC layer.
//!
//! Mirrors HMCL's `Modpack` / `ModpackManifest` / `ModpackConfiguration` while
//! staying independent of any launcher UI types. Every format parser lowers
//! into [`Modpack`], so the install pipeline is format-agnostic.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::PathBuf;

/// The modpack formats HMCL can install.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ModpackKind {
    /// CurseForge `manifest.json` (+ `overrides/`).
    Curse,
    /// Modrinth `.mrpack` / `modrinth.index.json`.
    Modrinth,
    /// MCBBS `mcbbs.packmeta` / MCBBS-flavoured `manifest.json`.
    Mcbbs,
    /// Server pack `server-manifest.json`.
    Server,
    /// MultiMC / Prism `instance.cfg` + `mmc-pack.json`.
    MultiMc,
    /// HMCL's own export (`modpack.json` + `minecraft/pack.json`).
    Hmcl,
    /// A plain zip containing a `.minecraft` directory (HMCL
    /// `ManuallyCreatedModpackException`).
    Manual,
}

impl ModpackKind {
    /// Stable provider name written to `modpack.cfg` (HMCL `ModpackProvider.getName`).
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Curse => "Curse",
            Self::Modrinth => "Modrinth",
            Self::Mcbbs => "Mcbbs",
            Self::Server => "Server",
            Self::MultiMc => "MultiMC",
            Self::Hmcl => "HMCL",
            Self::Manual => "Manual",
        }
    }

    /// Parse a provider name from `modpack.cfg`.
    pub fn from_provider_name(raw: &str) -> Option<Self> {
        match raw {
            "Curse" => Some(Self::Curse),
            "Modrinth" => Some(Self::Modrinth),
            "Mcbbs" => Some(Self::Mcbbs),
            "Server" => Some(Self::Server),
            "MultiMC" => Some(Self::MultiMc),
            "HMCL" => Some(Self::Hmcl),
            "Manual" => Some(Self::Manual),
            _ => None,
        }
    }

    /// Short label for the UI.
    pub fn display_name(self) -> &'static str {
        match self {
            Self::Curse => "CurseForge",
            Self::Modrinth => "Modrinth",
            Self::Mcbbs => "MCBBS",
            Self::Server => "Server",
            Self::MultiMc => "MultiMC / Prism",
            Self::Hmcl => "HMCL",
            Self::Manual => "Manual",
        }
    }
}

/// Mod loader kinds that can appear in a modpack manifest.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LoaderKind {
    Forge,
    NeoForge,
    Fabric,
    Quilt,
    LiteLoader,
    LegacyFabric,
    OptiFine,
    Cleanroom,
}

impl LoaderKind {
    /// Stable identifier shared with `rmccc-modloader`.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Forge => "forge",
            Self::NeoForge => "neoforge",
            Self::Fabric => "fabric",
            Self::Quilt => "quilt",
            Self::LiteLoader => "liteloader",
            Self::LegacyFabric => "legacyfabric",
            Self::OptiFine => "optifine",
            Self::Cleanroom => "cleanroom",
        }
    }

    /// Display name for the UI.
    pub fn display_name(self) -> &'static str {
        match self {
            Self::Forge => "Forge",
            Self::NeoForge => "NeoForge",
            Self::Fabric => "Fabric",
            Self::Quilt => "Quilt",
            Self::LiteLoader => "LiteLoader",
            Self::LegacyFabric => "Legacy Fabric",
            Self::OptiFine => "OptiFine",
            Self::Cleanroom => "Cleanroom",
        }
    }

    /// Map an HMCL patch id / MultiMC component id to a loader kind.
    pub fn from_manifest_id(raw: &str) -> Option<Self> {
        let normalized = raw.trim().to_ascii_lowercase();
        match normalized.as_str() {
            "forge" | "net.minecraftforge" => Some(Self::Forge),
            "neoforge" | "neo-forge" | "net.neoforged" => Some(Self::NeoForge),
            "fabric" | "fabric-loader" | "net.fabricmc.fabric-loader" => Some(Self::Fabric),
            "quilt" | "quilt-loader" | "org.quiltmc.quilt-loader" => Some(Self::Quilt),
            "liteloader" | "com.mumfrey.liteloader" => Some(Self::LiteLoader),
            "legacyfabric" | "legacy-fabric" | "net.legacyfabric.legacy-fabric-loader" => {
                Some(Self::LegacyFabric)
            }
            "optifine" | "optifine-loader" => Some(Self::OptiFine),
            "cleanroom" | "com.cleanroommc" => Some(Self::Cleanroom),
            _ => None,
        }
    }

    /// Higher value wins when a pack declares several loaders.
    fn install_priority(self) -> u8 {
        match self {
            Self::Forge => 10,
            Self::NeoForge => 9,
            Self::Fabric => 8,
            Self::Quilt => 7,
            Self::LegacyFabric => 6,
            Self::LiteLoader => 5,
            Self::Cleanroom => 4,
            Self::OptiFine => 3,
        }
    }
}

/// One loader requirement from a pack manifest.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PackLoader {
    pub kind: LoaderKind,
    pub version: String,
    /// CurseForge `modLoaders[].primary`.
    #[serde(default)]
    pub primary: bool,
}

impl PackLoader {
    /// Create a loader entry.
    pub fn new(kind: LoaderKind, version: impl Into<String>, primary: bool) -> Self {
        Self {
            kind,
            version: version.into(),
            primary,
        }
    }
}

/// Where a pack file comes from; drives URL/path resolution.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "source", rename_all = "snake_case")]
pub enum PackFileSource {
    /// A direct URL list already present in the manifest.
    Http,
    /// CurseForge project/file pair that may need metadata resolution.
    Curse {
        project_id: u64,
        file_id: u64,
        file_name: Option<String>,
    },
    /// A file served by a server/MCBBS `fileApi`.
    FileApi {
        file_api: String,
        hash: Option<String>,
    },
    /// A file that is expected to already exist in the overrides.
    Local,
}

/// One file the pack wants to place in the instance run directory.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PackFile {
    /// Path relative to the run directory. `None` until Curse metadata resolves.
    pub path: Option<String>,
    /// Download candidates (primary first).
    pub urls: Vec<String>,
    /// Expected SHA-1, when the manifest publishes one.
    pub sha1: Option<String>,
    /// Expected size in bytes, when known.
    pub size: Option<u64>,
    /// A missing required file makes the pack incomplete.
    pub required: bool,
    /// Human-readable origin used in diagnostics.
    pub origin: String,
    pub source: PackFileSource,
}

impl PackFile {
    /// Create a direct-download pack file.
    pub fn http(
        path: impl Into<String>,
        urls: Vec<String>,
        sha1: Option<String>,
        size: Option<u64>,
    ) -> Self {
        Self {
            path: Some(path.into()),
            urls,
            sha1,
            size,
            required: true,
            origin: "manifest".to_string(),
            source: PackFileSource::Http,
        }
    }
}

/// Memory requirement from a pack (MultiMC `OverrideMemory`, MCBBS `minMemory`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct MemorySettings {
    pub min_mb: Option<u32>,
    pub max_mb: Option<u32>,
    pub perm_gen_mb: Option<u32>,
    /// MCBBS `launchInfo.minMemory`: only raise the launcher max when the
    /// global setting is below the pack requirement (HMCL `createMcbbsPostInstallTask`).
    #[serde(default)]
    pub require_at_least: bool,
}

/// Window requirement from a pack (`OverrideWindow`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WindowSettings {
    pub fullscreen: bool,
    pub width: Option<u32>,
    pub height: Option<u32>,
}

/// Launch settings a pack wants applied to the instance.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PackSettings {
    pub memory: Option<MemorySettings>,
    pub java_path: Option<String>,
    pub jvm_args: Option<Vec<String>>,
    pub game_args: Vec<String>,
    pub wrapper: Option<String>,
    pub pre_launch_command: Option<String>,
    pub post_exit_command: Option<String>,
    pub window: Option<WindowSettings>,
    pub show_logs: Option<bool>,
}

impl PackSettings {
    /// Whether anything needs to be written to the instance settings.
    pub fn is_empty(&self) -> bool {
        self.memory.is_none()
            && self.java_path.is_none()
            && self.jvm_args.is_none()
            && self.game_args.is_empty()
            && self.wrapper.is_none()
            && self.pre_launch_command.is_none()
            && self.post_exit_command.is_none()
            && self.window.is_none()
            && self.show_logs.is_none()
    }
}

/// A provider manifest that must be preserved next to the instance.
///
/// HMCL writes `manifest.json` (Curse/MCBBS), `modrinth.index.json`, or
/// `server-manifest.json` so update/repair can re-read the pack.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MetadataFile {
    pub path: String,
    pub content: String,
}

/// Pack icon extracted from the archive (or downloaded later).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackIcon {
    /// Lower-case extension without the dot (`png`, `jpg`, …).
    pub ext: String,
    pub bytes: Vec<u8>,
}

/// Parsed modpack, format-independent.
#[derive(Debug, Clone)]
pub struct Modpack {
    pub kind: ModpackKind,
    pub name: String,
    pub version: Option<String>,
    pub author: Option<String>,
    pub description: String,
    pub game_version: Option<String>,
    pub loaders: Vec<PackLoader>,
    pub files: Vec<PackFile>,
    /// Archive directories (normalized, without leading/trailing slash) that
    /// are unpacked into the instance run directory.
    pub override_dirs: Vec<String>,
    pub settings: PackSettings,
    /// Extra libraries (MCBBS `libraries`) that must be appended as a patch.
    pub extra_libraries: Vec<rmccc_version::Library>,
    pub metadata_files: Vec<MetadataFile>,
    pub icon: Option<PackIcon>,
    /// Original provider manifest as JSON, stored in `modpack.cfg`.
    pub raw_manifest: Value,
    /// Archive path of the pack file (used to re-read overrides).
    pub archive: PathBuf,
    /// Manual packs only: the archive directory that contains `.minecraft`.
    pub manual_root: Option<String>,
    pub warnings: Vec<String>,
}

impl Modpack {
    /// Pick the loader that should actually be installed.
    ///
    /// HMCL installs every detected loader. RMCL's loader installers are
    /// single-loader (Forge/NeoForge run an official installer), so the
    /// primary loader is installed and the rest are reported as warnings.
    pub fn primary_loader(&self) -> Option<&PackLoader> {
        if let Some(primary) = self.loaders.iter().find(|loader| loader.primary) {
            return Some(primary);
        }
        self.loaders
            .iter()
            .max_by_key(|loader| loader.kind.install_priority())
    }

    /// Loaders that will not be installed by the single-loader pipeline.
    pub fn skipped_loaders(&self) -> Vec<&PackLoader> {
        let primary = self.primary_loader();
        self.loaders
            .iter()
            .filter(|loader| Some(*loader) != primary)
            .collect()
    }
}

/// Summary returned by the inspect command (no archive bytes, no network).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModpackInfo {
    pub kind: ModpackKind,
    pub kind_name: String,
    pub name: String,
    pub version: Option<String>,
    pub author: Option<String>,
    pub description: String,
    pub game_version: Option<String>,
    pub loaders: Vec<PackLoader>,
    pub file_count: usize,
    pub is_manual: bool,
    pub warnings: Vec<String>,
}

impl From<&Modpack> for ModpackInfo {
    fn from(pack: &Modpack) -> Self {
        Self {
            kind: pack.kind,
            kind_name: pack.kind.display_name().to_string(),
            name: pack.name.clone(),
            version: pack.version.clone(),
            author: pack.author.clone(),
            description: pack.description.clone(),
            game_version: pack.game_version.clone(),
            loaders: pack.loaders.clone(),
            file_count: pack.files.len(),
            is_manual: pack.kind == ModpackKind::Manual,
            warnings: pack.warnings.clone(),
        }
    }
}

/// File recorded in `modpack.cfg` for update preservation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OverrideEntry {
    /// Relative to the run directory, using `/`.
    pub path: String,
    pub hash: String,
    #[serde(
        default,
        rename = "downloadURL",
        skip_serializing_if = "Option::is_none"
    )]
    pub download_url: Option<String>,
}

/// HMCL-compatible `modpack.cfg` payload.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModpackConfiguration {
    pub manifest: Value,
    #[serde(rename = "type")]
    pub kind: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    pub overrides: Vec<OverrideEntry>,
}

impl ModpackConfiguration {
    /// Build the configuration written after a successful install.
    pub fn new(pack: &Modpack, overrides: Vec<OverrideEntry>) -> Self {
        Self {
            manifest: pack.raw_manifest.clone(),
            kind: pack.kind.as_str().to_string(),
            name: pack.name.clone(),
            version: pack.version.clone(),
            overrides,
        }
    }
}
