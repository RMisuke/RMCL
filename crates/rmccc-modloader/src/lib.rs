//! Mod loader installation: Fabric, Forge, NeoForge, Quilt, LiteLoader, OptiFine.
//!
//! # Architecture
//!
//! The modloader system is organized into three layers:
//!
//! 1. **Version discovery** ([`VersionList`]) — fetches available versions from remote APIs
//! 2. **Install execution** ([`InstallTask`]) — downloads installers, runs processors, produces patches
//! 3. **Dependency management** ([`DependencyManager`]) — unified entry point, routes to version list + install task
//!
//! Patches ([`GameInstancePatch`]) are layered onto [`rmccc_version::GameVersion`]
//! via [`GameInstancePatch::merge_into`], enabling non-destructive modloader installation.

#![allow(clippy::double_must_use)] // 工具链 1.99.1 新 lint：async_trait 宏生成的 #[must_use] 与 Future/Result 重叠，非业务逻辑

pub mod analyzer;
pub mod install_task;
pub mod installer;
pub mod patch;
pub mod remote_version;
pub mod version_list;

pub mod cleanroom;
pub mod fabric;
pub mod forge;
pub mod legacyfabric;
pub mod liteloader;
pub mod neoforge;
pub mod optifine;
pub mod quilt;

use rmccc_error::{Result, RmcccError};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use ts_rs::TS;

// ── Re-exports ──

pub use analyzer::{
    LibraryAnalyzer, LibraryMark, LibraryStatus, LibraryType, BOOTSTRAP_LAUNCHER_MAIN,
    FORGE_BOOTSTRAP_MAIN, FORGE_OPTIFINE_MAIN, FORGE_TWEAKERS, LAUNCH_WRAPPER_MAIN,
    LITELOADER_TWEAKER, MOD_LAUNCHER_MAIN, NEO_FORGE_BOOTSTRAP_MAIN, OPTIFINE_TWEAKERS,
    VANILLA_MAIN, VANILLA_ONLY_MAIN,
};
pub use install_task::InstallTask;
pub use installer::{installer_urls, needs_installer_jar, InstallerJarRunner};
pub use patch::{
    GameInstancePatch, PRIORITY_LITELOADER, PRIORITY_LOADER, PRIORITY_MC, PRIORITY_OPTIFINE,
};
pub use remote_version::{RemoteVersion, VersionType};
pub use version_list::VersionList;

// ── ModLoaderType — expanded for all 8 modloaders ──

/// Supported modloader types (GUI-friendly, all 8 modloaders).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export_to = "types.ts")]
pub enum ModLoaderType {
    Fabric,
    Forge,
    NeoForge,
    Quilt,
    LiteLoader,
    OptiFine,
    Cleanroom,
    LegacyFabric,
}

impl ModLoaderType {
    #[allow(clippy::should_implement_trait)]
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "fabric" => Some(Self::Fabric),
            "forge" => Some(Self::Forge),
            "neoforge" => Some(Self::NeoForge),
            "quilt" => Some(Self::Quilt),
            "liteloader" => Some(Self::LiteLoader),
            "optifine" => Some(Self::OptiFine),
            "cleanroom" => Some(Self::Cleanroom),
            "legacyfabric" => Some(Self::LegacyFabric),
            _ => None,
        }
    }

    /// All known types — including analyzer-only recognition for instances
    /// that already carry these loaders.
    pub fn all() -> &'static [Self] {
        &[
            Self::Fabric,
            Self::Forge,
            Self::NeoForge,
            Self::Quilt,
            Self::OptiFine,
            Self::LiteLoader,
            Self::Cleanroom,
            Self::LegacyFabric,
        ]
    }

    /// Loaders currently offered for **new installs**.
    ///
    /// OptiFine / LiteLoader / Cleanroom installers produce broken artifacts
    /// today (audit 2026-10-25 A6: OptiFine never runs the patcher,
    /// LiteLoader's library coordinates are unreachable, Cleanroom is a
    /// stub). They stay in [`Self::all`] so the analyzer can recognize
    /// already-installed instances, but install flows must not offer them.
    pub fn installable() -> &'static [Self] {
        &[
            Self::Fabric,
            Self::Forge,
            Self::NeoForge,
            Self::Quilt,
            Self::LegacyFabric,
        ]
    }

    /// Whether this loader can be installed right now (see [`Self::installable`]).
    pub fn is_installable(&self) -> bool {
        Self::installable().contains(self)
    }

    /// Display name for GUI.
    pub fn display_name(&self) -> &'static str {
        match self {
            Self::Fabric => "Fabric",
            Self::Forge => "Forge",
            Self::NeoForge => "NeoForge",
            Self::Quilt => "Quilt",
            Self::LiteLoader => "LiteLoader",
            Self::OptiFine => "OptiFine",
            Self::Cleanroom => "Cleanroom",
            Self::LegacyFabric => "Legacy Fabric",
        }
    }

    /// Library id used in DependencyManager (e.g. "forge", "fabric").
    pub fn library_id(&self) -> &'static str {
        match self {
            Self::Fabric => "fabric",
            Self::Forge => "forge",
            Self::NeoForge => "neoforge",
            Self::Quilt => "quilt",
            Self::LiteLoader => "liteloader",
            Self::OptiFine => "optifine",
            Self::Cleanroom => "cleanroom",
            Self::LegacyFabric => "legacyfabric",
        }
    }
}

/// A specific loader version available for a game version.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export_to = "types.ts")]
pub struct LoaderVersion {
    pub version: String,
    pub stable: bool,
}

// ── DependencyManager — unified entry point ──

/// Progress callback for version list loading and installation.
pub type ProgressCallback = Arc<dyn Fn(&str, u32, u32) + Send + Sync>;

/// loader 安装产物。
///
/// 两种形态来自 Minecraft 生态的现实差异，而不是我们的实现选择：
/// 官方安装器型的 loader 由安装器自己写版本 JSON，patch 型的 loader 由我们
/// 写增量 JSON（HMCL 语义，可回滚、可叠加）。
#[derive(Debug, Clone)]
pub enum LoaderInstallOutcome {
    /// 官方安装器已写好自己的版本 JSON；只需用这个 id 更新 profile。
    InstallerJar { version_id: String },
    /// 需要调用方写增量 JSON：`{ id: version_id, inheritsFrom: game_version, patches: [...] }`。
    Patch {
        version_id: String,
        /// 装箱：`GameInstancePatch` 比另一个变体大一个数量级，直接内联会让
        /// 整个 enum 膨胀（clippy::large_enum_variant）。
        patch: Box<GameInstancePatch>,
    },
}

impl LoaderInstallOutcome {
    /// 安装完成后的版本 id（profile 的 `game_version` 应指向它）。
    pub fn version_id(&self) -> &str {
        match self {
            LoaderInstallOutcome::InstallerJar { version_id } => version_id,
            LoaderInstallOutcome::Patch { version_id, .. } => version_id,
        }
    }
}

/// Unified dependency manager — routes modloader install requests
/// to the appropriate [`VersionList`] and [`InstallTask`].
///
/// # GUI compatibility
///
/// All methods accept optional [`ProgressCallback`] for UI progress bars.
/// The manager can be wrapped in `Arc<DependencyManager>` for shared access
/// from Tauri state or CLI context.
pub struct DependencyManager {
    #[allow(dead_code)]
    client: reqwest::Client,
    version_lists: HashMap<String, Arc<dyn VersionList>>,
}

impl DependencyManager {
    /// Create a new `DependencyManager` with all version lists pre-registered.
    pub fn new(client: reqwest::Client) -> Self {
        let mut dm = Self {
            client: client.clone(),
            version_lists: HashMap::new(),
        };

        dm.register(
            "fabric",
            Arc::new(fabric::FabricVersionList::new(client.clone())),
        );
        dm.register(
            "quilt",
            Arc::new(quilt::QuiltVersionList::new(client.clone())),
        );
        dm.register(
            "forge",
            Arc::new(forge::ForgeVersionList::new(client.clone())),
        );
        dm.register(
            "neoforge",
            Arc::new(neoforge::NeoForgeVersionList::new(client.clone())),
        );
        dm.register(
            "liteloader",
            Arc::new(liteloader::LiteLoaderVersionList::new(client.clone())),
        );
        dm.register(
            "optifine",
            Arc::new(optifine::OptiFineVersionList::new(client.clone())),
        );
        dm.register(
            "legacyfabric",
            Arc::new(legacyfabric::LegacyFabricVersionList::new(client.clone())),
        );
        dm.register(
            "cleanroom",
            Arc::new(cleanroom::CleanroomVersionList::new(client.clone())),
        );

        dm
    }

    fn register(&mut self, id: &str, vl: Arc<dyn VersionList>) {
        self.version_lists.insert(id.to_string(), vl);
    }

    /// List available versions for a modloader on a Minecraft version.
    ///
    /// # GUI usage
    /// Call this to populate version dropdowns.
    pub async fn list_versions(
        &self,
        loader_type: &ModLoaderType,
        game_version: &str,
    ) -> Result<Vec<LoaderVersion>> {
        let id = loader_type.library_id();
        let vl = self.get_vl(id)?;
        vl.load(game_version).await?;

        let versions: Vec<LoaderVersion> = vl
            .get_versions(game_version)
            .iter()
            .map(|rv| LoaderVersion {
                version: rv.self_version().to_string(),
                stable: true,
            })
            .collect();
        Ok(versions)
    }

    /// **唯一**的 loader 安装入口（审计 P1-4）。
    ///
    /// 两类 loader 由这里内部路由，调用方（`rmccc-core` 的编排、Tauri、CLI）
    /// 不再各自 `match`：
    ///
    /// | loader | 安装方式 | 产物 |
    /// |---|---|---|
    /// | Forge / NeoForge | 运行官方 installer JAR | 安装器自写 `versions/<id>/<id>.json` |
    /// | 其余 6 种 | 打 [`GameInstancePatch`] | 调用方写增量 JSON（`inheritsFrom` + `patches`） |
    pub async fn install_loader(
        &self,
        loader_type: &ModLoaderType,
        game_version: &str,
        base_version: &rmccc_version::GameVersion,
        loader_version: &str,
        game_dir: &Path,
        java_path: &Path,
    ) -> Result<LoaderInstallOutcome> {
        // A6 triage: incomplete installers fail fast with a clear message
        // instead of "succeeding" into an unbootable instance.
        if !loader_type.is_installable() {
            return Err(RmcccError::Other(format!(
                "{} installer is incomplete and disabled for new installs \
                 (see docs/audits/BACKEND_AUDIT_2026-10-25, A6)",
                loader_type.display_name()
            )));
        }
        if installer::needs_installer_jar(loader_type) {
            let runner = installer::InstallerJarRunner {
                client: &self.client,
                java_path,
                game_dir,
            };
            let version_id = runner
                .run(loader_type, game_version, loader_version)
                .await?;
            return Ok(LoaderInstallOutcome::InstallerJar { version_id });
        }

        let (mut merged, patch) = self
            .install_with_patch(
                loader_type,
                game_version,
                base_version,
                loader_version,
                game_dir,
                java_path,
            )
            .await?;
        // `install_with_patch` 已把 patch 合进 merged；这里只取 patch 生成的 id。
        merged.id = format!("{game_version}-{}-{}", patch.id, patch.version);
        Ok(LoaderInstallOutcome::Patch {
            version_id: merged.id.clone(),
            patch: Box::new(patch),
        })
    }

    /// 安装并把结果直接合进 [`rmccc_version::GameVersion`]（不带增量 patch）。
    pub async fn install(
        &self,
        loader_type: &ModLoaderType,
        game_version: &str,
        base_version: &rmccc_version::GameVersion,
        loader_version: &str,
        game_dir: &Path,
        java_path: &Path,
    ) -> Result<rmccc_version::GameVersion> {
        self.install_with_patch(
            loader_type,
            game_version,
            base_version,
            loader_version,
            game_dir,
            java_path,
        )
        .await
        .map(|(version, _)| version)
    }

    /// Install a modloader and return **both** the merged version and the
    /// [`GameInstancePatch`].
    ///
    /// The patch enables the HMCL-aligned incremental model: the caller can
    /// serialize a version JSON that carries `inheritsFrom` + `patches` instead
    /// of a frozen merged manifest (rollback = remove the patch).
    pub async fn install_with_patch(
        &self,
        loader_type: &ModLoaderType,
        game_version: &str,
        base_version: &rmccc_version::GameVersion,
        loader_version: &str,
        _game_dir: &Path,
        _java_path: &Path,
    ) -> Result<(rmccc_version::GameVersion, GameInstancePatch)> {
        let id = loader_type.library_id();
        let vl = self.get_vl(id)?;
        vl.load(game_version).await?;

        let rv = vl
            .get_version(game_version, loader_version)
            .ok_or_else(|| {
                RmcccError::Other(format!(
                    "Version {loader_version} not found for {id} on MC {game_version}"
                ))
            })?;

        let cleaned = self.remove_library(base_version, id);
        let mut task = rv.create_install_task(self, &cleaned)?;
        let patch = task.execute().await?;
        let mut merged = cleaned;
        patch.merge_into(&mut merged);
        Ok((merged, patch))
    }

    /// Auto-detect installer type from a local JAR file and install.
    pub async fn install_from_file(
        &self,
        base_version: &rmccc_version::GameVersion,
        installer_path: &Path,
    ) -> Result<rmccc_version::GameVersion> {
        let file = std::fs::File::open(installer_path)?;
        let mut archive = zip::ZipArchive::new(file)
            .map_err(|e| RmcccError::Other(format!("Not a valid ZIP: {e}")))?;

        // Try reading install_profile.json
        let profile_json = {
            let mut s = String::new();
            if let Ok(mut entry) = archive.by_name("install_profile.json") {
                std::io::Read::read_to_string(&mut entry, &mut s).ok();
            }
            s
        };

        if !profile_json.is_empty() {
            let parsed: serde_json::Value = serde_json::from_str(&profile_json)
                .map_err(|e| RmcccError::Other(format!("Invalid install_profile.json: {e}")))?;

            // Check for Cleanroom / NeoForge / Forge
            if parsed.get("spec").is_some() || parsed.get("install").is_some() {
                let profile_type = parsed.get("profile").and_then(|v| v.as_str()).unwrap_or("");
                let id = if profile_type == "neoforge" || profile_type == "NeoForge" {
                    "neoforge"
                } else if profile_type == "cleanroom" {
                    "cleanroom"
                } else {
                    "forge"
                };

                let vl = self.get_vl(id)?;
                let game_version = base_version.id.clone();
                vl.load(&game_version).await?;

                // Find matching version from install profile
                let mc_ver = parsed
                    .get("minecraft")
                    .and_then(|v| v.as_str())
                    .unwrap_or(&game_version);
                let versions = vl.get_versions(mc_ver);
                if let Some(rv) = versions.into_iter().next() {
                    let mut cleaned = self.remove_library(base_version, id);
                    let mut task = rv.create_install_task(self, &cleaned)?;
                    let patch = task.execute().await?;
                    patch.merge_into(&mut cleaned);
                    return Ok(cleaned);
                }
            }
        }

        // Try OptiFine detection (Config.class)
        let has_config = archive.by_name("Config.class").is_ok()
            || archive.by_name("net/optifine/Config.class").is_ok()
            || archive.by_name("notch/net/optifine/Config.class").is_ok();

        if has_config {
            let vl = self.get_vl("optifine")?;
            let game_version = base_version.id.clone();
            vl.load(&game_version).await?;
            let versions = vl.get_versions(&game_version);
            if let Some(rv) = versions.into_iter().next() {
                let mut cleaned = self.remove_library(base_version, "optifine");
                let mut task = rv.create_install_task(self, &cleaned)?;
                let patch = task.execute().await?;
                patch.merge_into(&mut cleaned);
                return Ok(cleaned);
            }
        }

        Err(RmcccError::Other("Unrecognized installer file".into()))
    }

    /// Remove a previously installed modloader from a version.
    pub fn remove_library(
        &self,
        version: &rmccc_version::GameVersion,
        library_id: &str,
    ) -> rmccc_version::GameVersion {
        let game_version = version.id.clone();
        let mut analyzer = LibraryAnalyzer::analyze(version, &game_version);
        analyzer.remove(library_id);
        analyzer.build()
    }

    fn get_vl(&self, id: &str) -> Result<&Arc<dyn VersionList>> {
        self.version_lists
            .get(id)
            .ok_or_else(|| RmcccError::Other(format!("Unknown library: {id}")))
    }
}
