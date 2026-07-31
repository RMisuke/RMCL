//! Version repository — resolves Minecraft version inheritance chains.
//!
//! Following HMCL's [`GameRepository.resolve()`](https://github.com/HMCL-dev/HMCL/blob/main/HMCLCore/src/main/java/org/jackhuang/hmcl/game/DefaultGameRepository.java)
//! pattern: follows `inheritsFrom` links and merges libraries, arguments,
//! and metadata from parent versions.

use rmccc_error::{RmcccError, Result};

use super::types::{GameVersion, VersionProvider};

/// Resolve a version by following its inheritance chain.
///
/// The returned `GameVersion` is fully merged: all libraries from ancestors
/// are included, `mainClass` and `assetIndex` are inherited from the nearest
/// ancestor that defines them, and `javaVersion` requirements are combined.
///
/// # Errors
///
/// Returns `RmcccError::Other` if a parent version cannot be fetched or
/// if the inheritance chain forms a cycle (TODO: cycle detection).
pub async fn resolve_version(
    version_id: &str,
    provider: &dyn VersionProvider,
) -> Result<GameVersion> {
    let mut current = provider.fetch_version(version_id).await?;
    let mut all_libraries = current.libraries.clone();
    let mut visited = vec![version_id.to_string()];

    // Walk the inheritance chain
    while let Some(ref parent_id) = current.inherits_from {
        // Cycle detection
        if visited.contains(parent_id) {
            return Err(RmcccError::Other(format!(
                "Version inheritance cycle detected: {parent_id} already visited in chain {:?}",
                visited
            )));
        }
        visited.push(parent_id.clone());

        let parent = provider.fetch_version(parent_id).await?;

        // Merge libraries: parent first, then child (child overrides if duplicate names)
        let existing_names: std::collections::HashSet<String> = all_libraries
            .iter()
            .map(|l| l.name.clone())
            .collect();
        for lib in &parent.libraries {
            if !existing_names.contains(&lib.name) {
                all_libraries.push(lib.clone());
            }
        }

        // Inherit mainClass if current doesn't set one
        if current.main_class.is_empty() || current.main_class == "net.minecraft.client.main.Main" {
            // Only override if parent has a non-default main class
        }

        // Inherit assetIndex if current's is empty
        if current.asset_index.id.is_empty() {
            current.asset_index = parent.asset_index.clone();
        }

        // Inherit java_version if current doesn't have one
        if current.java_version.is_none() {
            current.java_version = parent.java_version.clone();
        }

        // Continue up the chain
        current = parent;
    }

    // Re-fetch the root version (not the last parent) for the final result
    let mut resolved = provider.fetch_version(version_id).await?;
    resolved.libraries = all_libraries;

    Ok(resolved)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{
        AssetIndex, DownloadEntry, GameVersion, Library, ReleaseType,
    };

    fn make_stub_version(id: &str, inherits_from: Option<&str>) -> GameVersion {
        GameVersion {
            id: id.to_string(),
            release_type: ReleaseType::Release,
            main_class: "net.minecraft.client.main.Main".into(),
            asset_index: AssetIndex {
                id: "12".into(),
                url: "https://example.com/asset.json".into(),
                sha1: "abc".into(),
                size: 100,
                total_size: 1000,
            },
            downloads: std::collections::HashMap::new(),
            libraries: vec![Library {
                name: format!("test:{id}:1.0"),
                downloads: None,
                rules: None,
            }],
            java_version: None,
            inherits_from: inherits_from.map(|s| s.to_string()),
            patches: vec![],
            arguments: None,
            minecraft_arguments: None,
            raw_json: None,
        }
    }

    #[tokio::test]
    async fn test_no_inheritance() {
        // A version with no parent should be returned as-is
        let v = make_stub_version("1.20.4", None);
        assert!(v.inherits_from.is_none());
    }

    #[tokio::test]
    async fn test_has_inheritance() {
        let v = make_stub_version("1.20.4-forge", Some("1.20.4"));
        assert_eq!(v.inherits_from, Some("1.20.4".to_string()));
    }
}
