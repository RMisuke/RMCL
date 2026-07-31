//! Legacy Fabric mod loader — version discovery and installation.

pub mod version_list;
pub mod remote_version;
pub mod install_task;

pub use version_list::LegacyFabricVersionList;
pub use remote_version::LegacyFabricRemoteVersion;
pub use install_task::LegacyFabricInstallTask;
