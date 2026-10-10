//! OptiFine mod loader — version discovery and installation.
//!
//! OptiFine installation is unique: it patches the Minecraft JAR or injects
//! a launchwrapper tweaker. It must be installed LAST (highest priority).
//!
//! # Architecture
//!
//! - [`OptiFineVersionList`] implements [`VersionList`] — BMCLAPI endpoint
//! - [`OptiFineRemoteVersion`] implements [`RemoteVersion`]
//! - [`OptiFineInstallTask`] implements [`InstallTask`] — Patcher execution,
//!   launchwrapper extraction, compatibility checks

pub mod install_task;
pub mod remote_version;
pub mod version_list;

pub use install_task::OptiFineInstallTask;
pub use remote_version::OptiFineRemoteVersion;
pub use version_list::OptiFineVersionList;
