//! Version repository — resolves Minecraft version inheritance chains.
//!
//! Following HMCL's [`GameRepository.resolve()`](https://github.com/HMCL-dev/HMCL/blob/main/HMCLCore/src/main/java/org/jackhuang/hmcl/game/DefaultGameRepository.java)
//! pattern: follows `inheritsFrom` links and merges libraries, arguments,
//! and metadata from parent versions.

use rmccc_error::{Result, RmcccError};

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
/// if the inheritance chain forms a cycle.
pub async fn resolve_version(
    version_id: &str,
    provider: &dyn VersionProvider,
) -> Result<GameVersion> {
    let mut current = provider.fetch_version(version_id).await?;
    // `resolved` 始终保存"根版本 + 已从祖先继承的字段"的结果，
    // 避免末尾重新 fetch 导致继承结果丢失（P1-04）。
    let mut resolved = current.clone();
    let mut all_libraries = current.libraries.clone();
    let mut visited = vec![version_id.to_string()];

    // Walk the inheritance chain from child to root.
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
        let existing_names: std::collections::HashSet<String> =
            all_libraries.iter().map(|l| l.name.clone()).collect();
        for lib in &parent.libraries {
            if !existing_names.contains(&lib.name) {
                all_libraries.push(lib.clone());
            }
        }

        // Inherit mainClass from the nearest ancestor that defines a non-default one
        // (P1-04：原实现此分支为空，mainClass 永不继承)。
        if (resolved.main_class.is_empty()
            || resolved.main_class == "net.minecraft.client.main.Main")
            && !parent.main_class.is_empty()
            && parent.main_class != "net.minecraft.client.main.Main"
        {
            resolved.main_class = parent.main_class.clone();
        }

        // Inherit assetIndex from the nearest ancestor that has one
        if resolved.asset_index.id.is_empty() {
            resolved.asset_index = parent.asset_index.clone();
        }

        // Inherit java_version from the nearest ancestor that specifies one
        if resolved.java_version.is_none() {
            resolved.java_version = parent.java_version.clone();
        }

        // Continue up the chain
        current = parent;
    }

    // Apply merged libraries to the final result
    resolved.libraries = all_libraries;
    Ok(resolved)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{
        AssetIndex, GameVersion, JavaRequirement, Library, ReleaseType, VersionInfo,
        VersionProvider,
    };

    /// Mock provider serving a fixed version map.
    struct MockProvider {
        versions: std::collections::HashMap<String, GameVersion>,
    }

    impl MockProvider {
        fn new(versions: Vec<GameVersion>) -> Self {
            let map = versions.into_iter().map(|v| (v.id.clone(), v)).collect();
            Self { versions: map }
        }
    }

    #[async_trait::async_trait]
    impl VersionProvider for MockProvider {
        fn provider_id(&self) -> &str {
            "mock"
        }
        async fn fetch_versions(&self) -> Result<Vec<VersionInfo>> {
            Ok(vec![])
        }
        async fn fetch_version(&self, id: &str) -> Result<GameVersion> {
            self.versions
                .get(id)
                .cloned()
                .ok_or_else(|| RmcccError::Other(format!("version not found: {id}")))
        }
    }

    fn make_version(
        id: &str,
        inherits_from: Option<&str>,
        main_class: &str,
        asset_index_id: &str,
        java: Option<u32>,
        lib: &str,
    ) -> GameVersion {
        GameVersion {
            id: id.to_string(),
            release_type: ReleaseType::Release,
            main_class: main_class.to_string(),
            asset_index: AssetIndex {
                id: asset_index_id.to_string(),
                url: "https://example.com/asset.json".into(),
                sha1: "abc".into(),
                size: 100,
                total_size: 1000,
            },
            libraries: vec![Library {
                name: lib.to_string(),
                ..Default::default()
            }],
            java_version: java.map(|mj| JavaRequirement {
                major_version: mj,
                component: None,
            }),
            inherits_from: inherits_from.map(|s| s.to_string()),
            ..Default::default()
        }
    }

    #[tokio::test]
    async fn test_no_inheritance() {
        let provider = MockProvider::new(vec![make_version(
            "1.20.4",
            None,
            "net.minecraft.client.main.Main",
            "12",
            Some(21),
            "test:vanilla:1.0",
        )]);
        let resolved = resolve_version("1.20.4", &provider).await.unwrap();
        assert_eq!(resolved.id, "1.20.4");
        assert_eq!(resolved.libraries.len(), 1);
    }

    /// P1-04：子版本继承父版本的 libraries + assetIndex + javaVersion。
    #[tokio::test]
    async fn test_merges_parent_libraries_and_metadata() {
        let parent = make_version(
            "1.20.4",
            None,
            "net.minecraft.client.main.Main",
            "assets-parent",
            Some(21),
            "test:parent:1.0",
        );
        let child = make_version(
            "1.20.4-forge",
            Some("1.20.4"),
            "net.minecraft.client.main.Main",
            "",
            None,
            "test:forge:1.0",
        );
        let provider = MockProvider::new(vec![parent, child]);

        let resolved = resolve_version("1.20.4-forge", &provider).await.unwrap();
        let names: Vec<&str> = resolved.libraries.iter().map(|l| l.name.as_str()).collect();
        assert!(names.contains(&"test:parent:1.0"));
        assert!(names.contains(&"test:forge:1.0"));
        // assetIndex / javaVersion 从父继承（修复前会被末尾重复 fetch 丢弃）
        assert_eq!(resolved.asset_index.id, "assets-parent");
        assert_eq!(resolved.java_version.as_ref().unwrap().major_version, 21);
    }

    /// P1-04：Forge 场景——父版本非默认 mainClass 被子版本继承。
    #[tokio::test]
    async fn test_inherits_non_default_main_class() {
        let parent = make_version(
            "1.20.4",
            None,
            "net.minecraft.launchwrapper.Launch",
            "assets-parent",
            Some(21),
            "test:parent:1.0",
        );
        let child = make_version(
            "1.20.4-forge",
            Some("1.20.4"),
            "net.minecraft.client.main.Main",
            "",
            None,
            "test:forge:1.0",
        );
        let provider = MockProvider::new(vec![parent, child]);

        let resolved = resolve_version("1.20.4-forge", &provider).await.unwrap();
        assert_eq!(resolved.main_class, "net.minecraft.launchwrapper.Launch");
    }

    /// P1-04：子版本自身的元数据优先，不被父版本覆盖。
    #[tokio::test]
    async fn test_child_own_metadata_wins() {
        let parent = make_version(
            "1.20.4",
            None,
            "net.minecraft.launchwrapper.Launch",
            "assets-parent",
            Some(21),
            "test:parent:1.0",
        );
        let child = make_version(
            "1.20.4-custom",
            Some("1.20.4"),
            "custom.main.Custom",
            "assets-child",
            Some(17),
            "test:child:1.0",
        );
        let provider = MockProvider::new(vec![parent, child]);

        let resolved = resolve_version("1.20.4-custom", &provider).await.unwrap();
        assert_eq!(resolved.main_class, "custom.main.Custom");
        assert_eq!(resolved.asset_index.id, "assets-child");
        assert_eq!(resolved.java_version.as_ref().unwrap().major_version, 17);
    }

    /// P1-04：继承环检测。
    #[tokio::test]
    async fn test_cycle_detection() {
        let a = make_version(
            "a",
            Some("b"),
            "net.minecraft.client.main.Main",
            "",
            None,
            "test:a:1.0",
        );
        let b = make_version(
            "b",
            Some("a"),
            "net.minecraft.client.main.Main",
            "",
            None,
            "test:b:1.0",
        );
        let provider = MockProvider::new(vec![a, b]);

        let err = resolve_version("a", &provider).await.unwrap_err();
        assert!(err.to_string().contains("cycle"));
    }
}
