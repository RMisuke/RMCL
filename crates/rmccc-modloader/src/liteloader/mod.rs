//! LiteLoader mod loader — version discovery and installation.
//!
//! LiteLoader must be installed AFTER Forge. It injects a launchwrapper
//! tweaker with priority 60000 (overrides everything).

pub mod install_task;
pub mod remote_version;
pub mod version_list;

pub use install_task::LiteLoaderInstallTask;
pub use remote_version::LiteLoaderRemoteVersion;
pub use version_list::LiteLoaderVersionList;
