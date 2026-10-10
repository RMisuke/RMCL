//! Minecraft version management: Mojang API, asset resolution, library resolution,
//! offline cache, local instance repository (HMCL-aligned), and version metadata types.
//!
//! The version management system is **local-disk centered**: [`DefaultGameRepository`]
//! indexes `versions/` and resolves manifests offline; the network
//! [`MojangVersionProvider`] is used only as a metadata source during installation.

pub mod arguments;
pub mod assets;
pub mod cache;
pub mod compatibility;
pub mod completion;
pub mod default_game_repository;
pub mod file_layout;
pub mod game_repository;
pub mod isolation;
pub mod libraries;
pub mod manifest;
pub mod repository;
pub mod resolved;
pub mod types;

pub use arguments::{
    build_template_vars, parse_legacy_arguments, replace_templates, resolve_arguments,
};
pub use default_game_repository::DefaultGameRepository;
pub use game_repository::GameRepository;
pub use isolation::IsolationType;
pub use manifest::MojangVersionProvider;
pub use repository::resolve_version;
pub use resolved::ResolvedVersion;
pub use types::{
    ArgumentRule, ArgumentValue, AssetIndex, CompatibilityRule, DownloadEntry, ExtractRules,
    GamePatch, GameVersion, JavaRequirement, Library, LibraryDownloads, LibraryRule,
    LoggingInfo, OsCondition, OsRule, ReleaseType, VersionArgument, VersionArguments,
    VersionInfo, VersionProvider,
};

/// Map MC version string to minimum Java major version.
pub fn java_version_for_mc(mc_version: &str) -> u32 {
    let parts: Vec<&str> = mc_version.split('.').collect();
    if parts.len() < 2 {
        return 21;
    }

    let minor: u32 = parts.get(1).and_then(|s| s.parse().ok()).unwrap_or(0);
    let patch: u32 = parts.get(2).and_then(|s| s.parse().ok()).unwrap_or(0);

    match minor {
        0..=16 => 8,
        17 => 16,
        18..=20 if patch < 5 => 17,
        _ => 21,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_java_version_mapping() {
        assert_eq!(java_version_for_mc("1.16.5"), 8);
        assert_eq!(java_version_for_mc("1.17"), 16);
        assert_eq!(java_version_for_mc("1.20.4"), 17);
        assert_eq!(java_version_for_mc("1.20.5"), 21);
    }
}
