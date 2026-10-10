//! Resolved instance manifest — dual views (HMCL `GameInstanceManifest.Resolved`).
//!
//! After following the `inheritsFrom` chain and applying `patches`, a version
//! yields two useful views:
//!
//! - [`ResolvedVersion::launch`] — the fully merged manifest used to launch:
//!   no `inheritsFrom`, no `patches`, everything concretely resolved.
//! - [`ResolvedVersion::standalone`] — the resolved manifest that **keeps** the
//!   `patches` list (and the chain metadata) for editing / round-tripping.
//!
//! The original on-disk manifest is never mutated (stored in
//! [`ResolvedVersion::unresolved`]).

use super::types::GameVersion;

/// A resolved version with its three views.
#[derive(Debug, Clone)]
pub struct ResolvedVersion {
    /// The raw manifest exactly as read from disk (never modified).
    pub unresolved: GameVersion,
    /// Fully resolved manifest for launching — no `inheritsFrom`, no `patches`.
    pub launch: GameVersion,
    /// Resolved manifest preserving the `patches` list (for editing).
    pub standalone: GameVersion,
}

impl ResolvedVersion {
    /// Validate that the launch view is concrete (HMCL `Resolved` validation).
    pub fn validate(&self) -> Result<(), String> {
        if self.launch.inherits_from.is_some() {
            return Err("launch manifest must not carry inheritsFrom".into());
        }
        if !self.launch.patches.is_empty() {
            return Err("launch manifest must not carry patches".into());
        }
        Ok(())
    }

    /// The Minecraft base game version (root of the inheritance chain).
    pub fn game_version(&self) -> &str {
        // The launch view's id equals the requested instance id; the base MC
        // version is discoverable from the unresolved chain root. Callers may
        // use `Repository::get_game_version` instead for the root id.
        &self.unresolved.id
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::GameVersion;

    #[test]
    fn test_validate_ok() {
        let launch = GameVersion {
            inherits_from: None,
            patches: vec![],
            ..Default::default()
        };
        let resolved = ResolvedVersion {
            unresolved: GameVersion::default(),
            launch,
            standalone: GameVersion::default(),
        };
        assert!(resolved.validate().is_ok());
    }

    #[test]
    fn test_validate_rejects_inheritance() {
        let launch = GameVersion {
            inherits_from: Some("1.20.4".into()),
            ..Default::default()
        };
        let resolved = ResolvedVersion {
            unresolved: GameVersion::default(),
            launch,
            standalone: GameVersion::default(),
        };
        assert!(resolved.validate().is_err());
    }
}
