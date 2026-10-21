//! NeoForge mod loader — 版本发现 + 官方安装器。
//!
//! - [`NeoForgeVersionList`] — NeoForged Maven 的版本列表
//! - [`NeoForgeRemoteVersion`] — 版本组合 → 安装任务
//!
//! 实际安装走**官方 installer JAR**（NeoForge 在 MC 1.20.1 时代仍发布在
//! `forge/` artifact 下，两种名字都会尝试），由
//! [`crate::installer::InstallerJarRunner`] 统一执行（审计 P1-4）。

//! NeoForge mod loader — version discovery and installation.
//!
//! NeoForge reuses Forge's installer profile format but has its own
//! version discovery API and installer detection logic.
//!
//! # Architecture
//!
//! - [`NeoForgeVersionList`] implements [`VersionList`] — BMCLAPI endpoint
//! - [`NeoForgeRemoteVersion`] implements [`RemoteVersion`]
//! - [`NeoForgeInstallTask`] implements [`InstallTask`] — detects NEOFORGE.RSA
//!   signature, routes to Forge's new/old install tasks with patch id renaming

pub mod install_task;
pub mod remote_version;
pub mod version_list;

pub use install_task::NeoForgeInstallTask;
pub use remote_version::NeoForgeRemoteVersion;
pub use version_list::NeoForgeVersionList;
