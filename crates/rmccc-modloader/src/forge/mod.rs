//! Forge mod loader — 版本发现 + 官方安装器。
//!
//! - [`ForgeVersionList`] — HMCL metadata 的版本列表
//! - [`ForgeRemoteVersion`] — 版本组合 → 安装任务
//! - [`ForgeInstallTask`] / [`ForgeNewInstallTask`] / [`ForgeOldInstallTask`] —
//!   install_profile 解析与 processors 执行
//!
//! Forge 的实际安装走**官方 installer JAR**（见 [`crate::installer`]）：
//! 旧 `ForgeInstaller`（`ModLoaderProvider` 实现）里的"下载 installer 并运行"
//! 逻辑已收归 [`crate::installer::InstallerJarRunner`]，只有一份（审计 P1-4）。

//! Forge mod loader — version discovery and installation.
//!
//! Forge is the most complex modloader to install. It supports two installer
//! formats:
//!
//! - **New** (1.13+): [`ForgeNewInstallTask`] — parses `install_profile.json`,
//!   extracts embedded libraries, downloads processor dependencies, and executes
//!   processor steps with template variable substitution.
//! - **Old** (1.12-): [`ForgeOldInstallTask`] — extracts the universal JAR from
//!   the installer, uses embedded `versionInfo` directly as patch metadata.
//!
//! [`ForgeInstallTask`] routes between the two based on the presence of the
//! `spec` key in `install_profile.json`.
//!
//! # Backward compatibility
//!
//! [`ForgeInstaller`] still implements [`super::ModLoaderProvider`] for existing
//! CLI and Tauri commands.

pub mod install_task;
pub mod new_install;
pub mod old_install;
pub mod profile;
pub mod remote_version;
pub mod version_list;

pub use install_task::{ForgeInstallState, ForgeInstallTask};
pub use new_install::ForgeNewInstallTask;
pub use old_install::ForgeOldInstallTask;
pub use profile::{DataEntry, ForgeInstallProfile, ForgeNewInstallProfile, Processor};
pub use remote_version::ForgeRemoteVersion;
pub use version_list::ForgeVersionList;
