//! Version repository — resolves Minecraft version inheritance chains.
//!
//! Following HMCL's [`GameRepository.resolve()`](https://github.com/HMCL-dev/HMCL/blob/main/HMCLCore/src/main/java/org/jackhuang/hmcl/game/DefaultGameRepository.java)
//! pattern: follows `inheritsFrom` links and merges libraries, arguments,
//! and metadata from parent versions.
//!
//! 合并语义的唯一权威实现在 [`super::merge::merge_child_into_parent`]
//! （审计批次 3 收敛，定版见 ARCHITECTURE.md §3.3）：本模块只负责沿链收集
//! 清单，从根到子折叠——不再自带第三套字段合并逻辑。

use rmccc_error::{Result, RmcccError};

use super::types::{GameVersion, VersionProvider};

/// Resolve a version by following its inheritance chain.
///
/// The returned `GameVersion` is fully merged via
/// [`super::merge::merge_child_into_parent`]: libraries accumulate (deduped),
/// `arguments` append, and `mainClass`/`jar`/`assetIndex`/`javaVersion`/
/// `minecraftArguments`/`downloads.client`/`logging` inherit from the nearest
/// ancestor that defines them.
///
/// # Errors
///
/// Returns `RmcccError::Other` if a parent version cannot be fetched or
/// if the inheritance chain forms a cycle.
pub async fn resolve_version(
    version_id: &str,
    provider: &dyn VersionProvider,
) -> Result<GameVersion> {
    // 1. Walk the chain child → root, collecting manifests (with cycle detection).
    let mut chain: Vec<GameVersion> = Vec::new();
    let mut visited: Vec<String> = Vec::new();
    let mut current_id = version_id.to_string();

    loop {
        if visited.contains(&current_id) {
            return Err(RmcccError::Other(format!(
                "Version inheritance cycle detected: {current_id} already visited in chain {:?}",
                visited
            )));
        }
        visited.push(current_id.clone());

        let manifest = provider.fetch_version(&current_id).await?;
        let next = manifest.inherits_from.clone();
        chain.push(manifest);
        match next {
            Some(parent_id) => current_id = parent_id,
            None => break,
        }
    }

    // 2. Fold root → child with the canonical merge (字段语义见 merge.rs）。
    let mut resolved = chain
        .pop()
        .ok_or_else(|| RmcccError::Other(format!("Version chain empty for {version_id}")))?;
    while let Some(child) = chain.pop() {
        super::merge::merge_child_into_parent(&mut resolved, &child);
    }
    resolved.id = version_id.to_string();
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

    /// 审计跨 crate #1：链式解析必须继承**全部**字段——旧实现只合并
    /// libraries/mainClass/assetIndex/javaVersion，arguments/minecraftArguments/
    /// downloads/logging/jar 全部丢失（Forge/Fabric 子版本未重复声明时
    /// 父版本的 JVM 参数与 client jar 下载信息随之丢失）。
    #[tokio::test]
    async fn test_full_field_inheritance_via_canonical_merge() {
        let parent: GameVersion = serde_json::from_value(serde_json::json!({
            "id": "1.12.2",
            "type": "release",
            "mainClass": "net.minecraft.client.main.Main",
            "jar": "1.12.2",
            "assetIndex": { "id": "1.12", "url": "u", "sha1": "s", "size": 1, "total_size": 2 },
            "downloads": { "client": { "url": "https://mojang/1.12.2.jar", "sha1": "p", "size": 1 } },
            "libraries": [ { "name": "parent:lib:1" } ],
            "minecraftArguments": "--username ${auth_player_name}",
            "logging": {
                "client": {
                    "argument": "-Dlog4j.configurationFile=${path}",
                    "file": { "id": "l1", "sha1": "x", "size": 1, "url": "https://mojang/log.xml" },
                    "type": "log4j2-xml"
                }
            }
        }))
        .unwrap();
        let child: GameVersion = serde_json::from_value(serde_json::json!({
            "id": "1.12.2-forge",
            "type": "release",
            "inheritsFrom": "1.12.2",
            "mainClass": "net.minecraft.launchwrapper.Launch",
            "assetIndex": { "id": "", "url": "", "sha1": "", "size": 0, "total_size": 0 },
            "downloads": {},
            "libraries": [ { "name": "forge:lib:1" } ],
            "minecraftArguments": "--username ${auth_player_name} --tweakClass forge"
        }))
        .unwrap();
        let provider = MockProvider::new(vec![parent, child]);

        let resolved = resolve_version("1.12.2-forge", &provider).await.unwrap();

        // mainClass：子非默认 → 覆盖；jar / downloads.client / logging：子未声明 → 继承父。
        assert_eq!(resolved.main_class, "net.minecraft.launchwrapper.Launch");
        assert_eq!(resolved.jar.as_deref(), Some("1.12.2"));
        assert!(resolved.downloads.contains_key("client"));
        assert!(resolved.logging.is_some());
        // minecraftArguments：子覆盖（含 tweakClass）。
        assert!(resolved
            .minecraft_arguments
            .as_deref()
            .unwrap()
            .contains("tweakClass"));
        // libraries：父前子后去重。
        let names: Vec<&str> = resolved.libraries.iter().map(|l| l.name.as_str()).collect();
        assert_eq!(names, vec!["parent:lib:1", "forge:lib:1"]);
        // 最终 id = 请求的版本 id。
        assert_eq!(resolved.id, "1.12.2-forge");
    }
}
