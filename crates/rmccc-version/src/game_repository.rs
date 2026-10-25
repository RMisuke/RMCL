//! `GameRepository` — the contract for instance indexing, local resolution,
//! and path mapping (HMCL `GameRepository`).
//!
//! The repository is the **local-disk center** of the version management
//! system: installation writes JSON files, launch reads resolved manifests
//! from here (offline capable). The network `VersionProvider` is used only as
//! a metadata source during installation.

use rmccc_error::Result;
use std::path::{Path, PathBuf};

use super::isolation::IsolationType;
use super::resolved::ResolvedVersion;
use super::types::GameVersion;

/// Repository contract — implemented by [`DefaultGameRepository`](super::default_game_repository).
///
/// All methods are synchronous: the repository never performs network I/O.
pub trait GameRepository: Send + Sync {
    /// The `.minecraft` base directory this repository indexes.
    fn game_dir(&self) -> &Path;

    /// Re-scan `versions/` and rebuild the instance index.
    fn refresh(&mut self) -> Result<()>;

    /// Whether an instance with the given id is indexed.
    fn has_instance(&self, id: &str) -> bool;

    /// All indexed instance ids (folder names under `versions/`).
    fn get_instance_ids(&self) -> Vec<String>;

    /// The raw (unresolved) manifest for an instance.
    fn get_manifest(&self, id: &str) -> Option<&GameVersion>;

    /// The resolved manifest (dual views) for an instance.
    ///
    /// # Errors
    /// - [`rmccc_error::RmcccError::Other`] when the id is not indexed or the
    ///   inheritance chain contains a cycle.
    fn get_resolved(&self, id: &str) -> Result<ResolvedVersion>;

    /// Effective run directory under the given isolation strategy.
    fn get_run_directory(&self, id: &str, isolation: IsolationType) -> PathBuf;

    /// The shared library file path for a Maven-style library name.
    fn get_library_file(&self, id: &str, name: &str) -> PathBuf;

    /// Native libraries directory (`versions/{id}/natives-{platform}`).
    fn get_native_directory(&self, id: &str) -> PathBuf;

    /// Client JAR path (`versions/{id}/{jar}.jar`).
    fn get_instance_jar(&self, id: &str) -> PathBuf;

    /// Asset index file path (`assets/indexes/{asset_id}.json`).
    fn get_asset_index_file(&self, id: &str) -> PathBuf;

    /// Asset object path (`assets/objects/{hash[0:2]}/{hash}`).
    ///
    /// `None` for malformed hashes — see [`crate::file_layout::asset_object`].
    fn get_asset_object(&self, hash: &str) -> Option<PathBuf>;

    /// Mods directory (run-dir dependent for isolated instances).
    fn get_mods_directory(&self, id: &str) -> PathBuf;

    /// The base Minecraft version (root of the inheritance chain).
    fn get_game_version(&self, id: &str) -> String;

    /// Rename an instance (folder + inner json/jar).
    fn rename_instance(&mut self, from: &str, to: &str) -> Result<()>;

    /// Duplicate an instance.
    fn duplicate_instance(&mut self, from: &str, to: &str) -> Result<()>;

    /// Remove an instance from disk and the index.
    fn remove_instance(&mut self, id: &str) -> Result<()>;
}
