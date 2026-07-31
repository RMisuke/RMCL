//! LiteLoader mod loader — version discovery and installation.
//!
//! LiteLoader must be installed AFTER Forge. It injects a launchwrapper
//! tweaker with priority 60000 (overrides everything).

pub mod version_list;
pub mod remote_version;
pub mod install_task;

pub use version_list::LiteLoaderVersionList;
pub use remote_version::LiteLoaderRemoteVersion;
pub use install_task::LiteLoaderInstallTask;
