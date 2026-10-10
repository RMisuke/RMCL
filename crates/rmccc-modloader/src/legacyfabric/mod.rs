//! Legacy Fabric mod loader — version discovery and installation.

pub mod install_task;
pub mod remote_version;
pub mod version_list;

pub use install_task::LegacyFabricInstallTask;
pub use remote_version::LegacyFabricRemoteVersion;
pub use version_list::LegacyFabricVersionList;
