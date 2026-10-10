//! Quilt mod loader — version discovery and installation.
//!
//! Quilt follows the same pattern as Fabric: version listing from Quilt Meta API,
//! installation via launch metadata parsing.
//!
//! # HMCL alignment
//!
//! Quilt adds hashed mappings support on top of Fabric's intermediary mappings,
//! and routes Maven dependencies based on groupId.
//!
//! # Architecture
//!
//! - [`QuiltVersionList`] implements [`VersionList`]
//! - [`QuiltRemoteVersion`] implements [`RemoteVersion`]
//! - [`QuiltInstallTask`] implements [`InstallTask`]

pub mod install_task;
pub mod remote_version;
pub mod version_list;

pub use install_task::QuiltInstallTask;
pub use remote_version::QuiltRemoteVersion;
pub use version_list::QuiltVersionList;

// ── Legacy ModLoaderProvider (stub — Quilt not yet in ModLoaderType) ──
// QuiltInstallTask provides the new InstallTask interface.
// A legacy ModLoaderProvider wrapper can be added when ModLoaderType::Quilt is added.
