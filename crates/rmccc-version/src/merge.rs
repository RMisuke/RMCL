//! 版本继承合并的唯一权威实现（2026-10-25 审计批次 3 收敛）。
//!
//! 语义定版见 `docs/ARCHITECTURE.md` §3.3「版本继承合并与规则求值的统一语义」：
//! 此前安装链（`repository::resolve_version`）、启动/本地链
//! （`DefaultGameRepository::resolve`）与 `pipeline::resolve_version` 的本地分支
//! 各有一套字段合并逻辑且互不一致——现在三条链路全部折叠到本模块，回归测试
//! 锁定"同一份 JSON 三链路解析结果一致"。

use std::collections::HashSet;

use super::types::{
    AssetIndex, DownloadEntry, GameVersion, JavaRequirement, Library, LoggingInfo, VersionArguments,
};

/// Mojang 默认主类：子版本声明它等价于"没有自己的 mainClass"。
const DEFAULT_MAIN_CLASS: &str = "net.minecraft.client.main.Main";

/// 子库列表追加进父版本（按 `name` 去重，父的在前）。
pub(crate) fn append_libraries(parent: &mut GameVersion, libs: &[Library]) {
    let existing: HashSet<String> = parent.libraries.iter().map(|l| l.name.clone()).collect();
    for lib in libs {
        if !existing.contains(&lib.name) {
            parent.libraries.push(lib.clone());
        }
    }
}

/// 子 arguments **追加**进父版本（HMCL `addArguments`：父的 jvm/game 在前，
/// 子的后接——不是整体替换）。
pub(crate) fn append_arguments(parent: &mut GameVersion, args: &VersionArguments) {
    let mut merged = parent.arguments.clone().unwrap_or_default();
    if let Some(jvm) = &args.jvm {
        merged.jvm.get_or_insert_with(Vec::new).extend(jvm.clone());
    }
    if let Some(game) = &args.game {
        merged
            .game
            .get_or_insert_with(Vec::new)
            .extend(game.clone());
    }
    parent.arguments = Some(merged);
}

/// Merge a child manifest on top of an already-resolved parent.
///
/// Mirrors HMCL `GameInstanceManifest.merge(parent)`: the child's concrete
/// fields override the parent's; libraries and arguments accumulate.
/// 逐字段语义见模块文档与 ARCHITECTURE.md §3.3。
pub fn merge_child_into_parent(parent: &mut GameVersion, child: &GameVersion) {
    append_libraries(parent, &child.libraries);

    // mainClass — child overrides when it defines a non-default entry.
    if !child.main_class.is_empty() && child.main_class != DEFAULT_MAIN_CLASS {
        parent.main_class = child.main_class.clone();
    }

    // jar — child overrides.
    if let Some(jar) = &child.jar {
        parent.jar = Some(jar.clone());
    }

    // assetIndex — child overrides when non-empty.
    if !child.asset_index.id.is_empty() {
        parent.asset_index = child.asset_index.clone();
    }

    // javaVersion — child overrides.
    if let Some(jv) = &child.java_version {
        parent.java_version = Some(jv.clone());
    }

    // arguments — append (HMCL addArguments).
    if let Some(child_args) = &child.arguments {
        append_arguments(parent, child_args);
    }

    // minecraftArguments — child overrides.
    if let Some(ma) = &child.minecraft_arguments {
        parent.minecraft_arguments = Some(ma.clone());
    }

    // compatibilityRules — accumulate.
    parent
        .compatibility_rules
        .extend(child.compatibility_rules.clone());

    // complianceLevel / root / hidden / time / releaseTime — child overrides.
    if let Some(cl) = child.compliance_level {
        parent.compliance_level = Some(cl);
    }
    if let Some(r) = child.root {
        parent.root = Some(r);
    }
    if child.hidden {
        parent.hidden = true;
    }
    if let Some(t) = &child.time {
        parent.time = Some(t.clone());
    }
    if let Some(rt) = &child.release_time {
        parent.release_time = Some(rt.clone());
    }

    // downloads — child's client overrides.
    if let Some(client) = child.downloads.get("client") {
        parent
            .downloads
            .insert("client".to_string(), client.clone());
    }

    // logging — child overrides.
    if let Some(lg) = &child.logging {
        parent.logging = Some(lg.clone());
    }
}

/// 把**部分**子版本 JSON 合并到已解析的父版本上。
///
/// 安装器产物（如 Forge 安装器写出的子 JSON）常缺 `assetIndex` / `downloads` 等
/// 必需字段，`GameVersion` 的 typed 解析会整体失败——此函数逐字段宽松读取，
/// 出现什么合并什么，语义与 [`merge_child_into_parent`] 完全一致
/// （此前 `pipeline::resolve_version` 为此自带的第三套合并连 `arguments` 都是
/// 整体替换，见审计跨 crate #1）。
pub fn merge_partial_json_into_parent(parent: &mut GameVersion, child: &serde_json::Value) {
    let field = |key: &str| child.get(key).filter(|v| !v.is_null());

    if let Some(v) = field("libraries") {
        if let Ok(libs) = serde_json::from_value::<Vec<Library>>(v.clone()) {
            append_libraries(parent, &libs);
        }
    }
    if let Some(mc) = field("mainClass").and_then(|v| v.as_str()) {
        if !mc.is_empty() && mc != DEFAULT_MAIN_CLASS {
            parent.main_class = mc.to_string();
        }
    }
    if let Some(jar) = field("jar").and_then(|v| v.as_str()) {
        parent.jar = Some(jar.to_string());
    }
    if let Some(v) = field("assetIndex") {
        if let Ok(ai) = serde_json::from_value::<AssetIndex>(v.clone()) {
            if !ai.id.is_empty() {
                parent.asset_index = ai;
            }
        }
    }
    if let Some(v) = field("javaVersion") {
        if let Ok(jv) = serde_json::from_value::<JavaRequirement>(v.clone()) {
            parent.java_version = Some(jv);
        }
    }
    if let Some(v) = field("arguments") {
        if let Ok(args) = serde_json::from_value::<VersionArguments>(v.clone()) {
            append_arguments(parent, &args);
        }
    }
    if let Some(ma) = field("minecraftArguments").and_then(|v| v.as_str()) {
        parent.minecraft_arguments = Some(ma.to_string());
    }
    if let Some(v) = field("downloads") {
        if let Ok(dls) =
            serde_json::from_value::<std::collections::HashMap<String, DownloadEntry>>(v.clone())
        {
            if let Some(client) = dls.get("client") {
                parent
                    .downloads
                    .insert("client".to_string(), client.clone());
            }
        }
    }
    if let Some(v) = field("logging") {
        if let Ok(lg) =
            serde_json::from_value::<std::collections::HashMap<String, LoggingInfo>>(v.clone())
        {
            parent.logging = Some(lg);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{ArgumentValue, VersionArgument};

    /// 参数列表字符串化（测试 JSON 全部是 Simple 参数）。
    fn strings(args: &[VersionArgument]) -> Vec<String> {
        args.iter()
            .map(|a| match a {
                VersionArgument::Simple(s) => s.clone(),
                VersionArgument::Ruled { value, .. } => match value {
                    ArgumentValue::Single(s) => s.clone(),
                    ArgumentValue::Multiple(list) => list.join(" "),
                },
            })
            .collect()
    }

    fn parent() -> GameVersion {
        serde_json::from_value(serde_json::json!({
            "id": "1.20.4",
            "type": "release",
            "mainClass": "net.minecraft.client.main.Main",
            "assetIndex": { "id": "12", "url": "u", "sha1": "s", "size": 1, "total_size": 2 },
            "downloads": {
                "client": { "url": "https://mojang/client.jar", "sha1": "p1", "size": 10 }
            },
            "libraries": [ { "name": "parent:base:1" } ],
            "arguments": {
                "jvm": ["-DparentJvm"],
                "game": ["--parentGame"]
            },
            "logging": {
                "client": {
                    "argument": "-Dlog4j.configurationFile=${path}",
                    "file": { "id": "l1", "sha1": "x", "size": 1, "url": "https://mojang/log.xml" },
                    "type": "log4j2-xml"
                }
            }
        }))
        .expect("parent GameVersion")
    }

    #[test]
    fn typed_child_merge_appends_arguments_and_libraries() {
        let mut merged = parent();
        let child: GameVersion = serde_json::from_value(serde_json::json!({
            "id": "1.20.4-forge",
            "type": "release",
            "mainClass": "cpw.mods.bootstraplauncher.BootstrapLauncher",
            "assetIndex": { "id": "", "url": "", "sha1": "", "size": 0, "total_size": 0 },
            "downloads": {},
            "libraries": [ { "name": "forge:loader:1" } ],
            "arguments": { "jvm": ["-DforgeJvm"], "game": ["--forgeGame"] }
        }))
        .expect("child GameVersion");

        merge_child_into_parent(&mut merged, &child);

        // libraries：父前子后、去重。
        let names: Vec<&str> = merged.libraries.iter().map(|l| l.name.as_str()).collect();
        assert_eq!(names, vec!["parent:base:1", "forge:loader:1"]);
        // mainClass：子非默认 → 覆盖。
        assert_eq!(
            merged.main_class,
            "cpw.mods.bootstraplauncher.BootstrapLauncher"
        );
        // arguments：追加（不是替换）——审计跨 crate #1 的核心症状。
        let args = merged.arguments.expect("arguments");
        assert_eq!(
            strings(args.jvm.as_deref().unwrap_or(&[])),
            ["-DparentJvm", "-DforgeJvm"]
        );
        assert_eq!(
            strings(args.game.as_deref().unwrap_or(&[])),
            ["--parentGame", "--forgeGame"]
        );
        // assetIndex 空 → 继承父。
        assert_eq!(merged.asset_index.id, "12");
        // downloads.client 子未声明 → 继承父。
        assert!(merged.downloads.contains_key("client"));
        // logging 继承。
        assert!(merged.logging.is_some());
    }

    #[test]
    fn typed_child_default_main_class_does_not_override() {
        let mut merged = parent();
        merged.main_class = "custom.Main".to_string();
        let child: GameVersion = serde_json::from_value(serde_json::json!({
            "id": "c", "type": "release",
            "mainClass": "net.minecraft.client.main.Main",
            "assetIndex": { "id": "", "url": "", "sha1": "", "size": 0, "total_size": 0 },
            "downloads": {}, "libraries": []
        }))
        .expect("child");

        merge_child_into_parent(&mut merged, &child);
        assert_eq!(merged.main_class, "custom.Main");
    }

    #[test]
    fn partial_json_merge_matches_typed_semantics() {
        // 安装器产物形态：只有 mainClass/libraries/arguments，缺 assetIndex 等必需
        // 字段——typed 解析必然失败，partial 合并必须给出与 typed 一致的结果。
        let raw = serde_json::json!({
            "id": "1.20.4-loader",
            "mainClass": "net.fabricmc.loader.impl.launch.knot.KnotClient",
            "libraries": [ { "name": "fabric:loader:1" } ],
            "arguments": { "game": ["--fabric"] }
        });
        assert!(serde_json::from_value::<GameVersion>(raw.clone()).is_err());

        let mut merged = parent();
        merge_partial_json_into_parent(&mut merged, &raw);

        assert_eq!(
            merged.main_class,
            "net.fabricmc.loader.impl.launch.knot.KnotClient"
        );
        let names: Vec<&str> = merged.libraries.iter().map(|l| l.name.as_str()).collect();
        assert_eq!(names, vec!["parent:base:1", "fabric:loader:1"]);
        let args = merged.arguments.expect("arguments");
        // arguments 追加而非整体替换（此前 pipeline 侧第三套实现是替换）。
        assert_eq!(
            strings(args.game.as_deref().unwrap_or(&[])),
            ["--parentGame", "--fabric"]
        );
        assert_eq!(strings(args.jvm.as_deref().unwrap_or(&[])), ["-DparentJvm"]);
        // 未声明的字段一律继承父。
        assert_eq!(merged.asset_index.id, "12");
        assert!(merged.downloads.contains_key("client"));
        assert!(merged.logging.is_some());
    }

    #[test]
    fn partial_json_ignores_unknown_and_null_fields() {
        let mut merged = parent();
        let before = merged.clone();
        merge_partial_json_into_parent(
            &mut merged,
            &serde_json::json!({ "mainClass": null, "unknownField": { "a": 1 } }),
        );
        assert_eq!(merged.main_class, before.main_class);
        assert_eq!(merged.libraries.len(), before.libraries.len());
    }
}
