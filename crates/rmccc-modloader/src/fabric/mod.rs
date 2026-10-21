//! Fabric mod loader — 版本发现与 patch 安装。
//!
//! - [`FabricVersionList`] — 从 Fabric Meta API 拉取 game+loader 组合
//! - [`FabricRemoteVersion`] — 持有版本组合，创建安装任务
//! - [`FabricInstallTask`] — 拉 launch metadata，构造 [`crate::GameInstancePatch`]
//!
//! 旧 `FabricInstaller`（`ModLoaderProvider` 实现）已删除：它与本文件的
//! `FabricInstallTask` 是同一件事的两份实现（审计 P1-4）。安装入口统一为
//! [`crate::DependencyManager::install_loader`]。

//! Fabric mod loader — version discovery and installation.
//!
//! # Architecture
//!
//! - [`FabricVersionList`] implements [`VersionList`] — fetches game+loader versions
//!   from Fabric Meta API, builds cartesian product.
//! - [`FabricRemoteVersion`] implements [`RemoteVersion`] — holds game/loader version
//!   combo, creates the install task.
//! - [`FabricInstallTask`] implements [`InstallTask`] — fetches launch metadata,
//!   parses `FabricInfo`, builds a [`GameInstancePatch`].
//!

pub mod install_task;
pub mod remote_version;
pub mod version_list;

pub use install_task::FabricInstallTask;
pub use remote_version::FabricRemoteVersion;
pub use version_list::FabricVersionList;
