//! Remote version trait — a specific modloader version entry.
//!
//! Each entry in a [`super::version_list::VersionList`] implements
//! [`RemoteVersion`]. It carries the metadata needed to identify
//! and install that specific version.

use crate::install_task::InstallTask;
use chrono::{DateTime, Utc};
use rmccc_error::Result;
use std::path::Path;

/// Type classification for a remote version.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VersionType {
    Uncategorized,
    Release,
    Snapshot,
    Old,
    Pending,
}

/// 单次安装的执行上下文（审计批次 4）。
///
/// 此前 `DependencyManager::install_with_patch` 的 `game_dir`/`java_path`
/// 参数被整体忽略——Forge processors 任务拿不到真实 Java 与游戏目录，
/// 只能退到 PATH `java` 与 `.`。现在随任务构造注入。
pub struct InstallTaskContext<'a> {
    /// 目标游戏根（`.minecraft` 等价物）。
    pub game_dir: &'a Path,
    /// 运行 processor/安装器的 Java 二进制。
    pub java_path: &'a Path,
}

/// A specific modloader version available remotely.
///
/// Implementors provide URLs to the installer/metadata and a factory
/// method to create the corresponding install task.
pub trait RemoteVersion: Send + Sync {
    /// The library identifier (e.g. "forge", "fabric", "neoforge").
    fn library_id(&self) -> &str;

    /// The Minecraft version this modloader version targets.
    fn game_version(&self) -> &str;

    /// The modloader's own version string.
    fn self_version(&self) -> &str;

    /// URLs for the installer or version metadata.
    fn urls(&self) -> &[String];

    /// Release date, if known.
    fn release_date(&self) -> Option<DateTime<Utc>>;

    /// Type classification.
    fn version_type(&self) -> VersionType {
        VersionType::Uncategorized
    }

    /// Create the install task for this version.
    ///
    /// The task will produce a [`GameInstancePatch`] when executed.
    fn create_install_task(
        &self,
        dm: &super::DependencyManager,
        manifest: &rmccc_version::GameVersion,
        ctx: &InstallTaskContext<'_>,
    ) -> Result<Box<dyn InstallTask>>;
}
