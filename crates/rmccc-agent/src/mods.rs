//! 只读 Mod 兼容性工具（Phase 6.6，指南 §10.6）。
//!
//! 首个端到端场景："检查这个实例的模组兼容性并修复缺失前置"。
//! 本模块只做**只读**的前半段：
//!
//! 1. 扫描实例 `mods/` 下的 jar，解析 `fabric.mod.json` 的 `depends` / `breaks`；
//! 2. 生成 [`DependencyReport`]（已装 / 依赖 / 缺失必需 / 冲突 / 未解析）；
//! 3. 生成 [`ModInstallPlan`]（`dry_run = true`，`requires_confirmation = true`），**不执行写入**。
//!
//! 由此 [`rmccc_mods::ModDependency`] 第一次有了真实消费方（审计 P3-2）。
//!
//! # 覆盖范围
//!
//! v1 只解析 Fabric 元数据。Forge/NeoForge 的 `META-INF/mods.toml` 需要 TOML 解析，
//! 为避免为此引入新依赖（体积红线），这些 jar 会进入 `unparsed` 并在计划里明确说明，
//! **不会**被当成"兼容"。

use rmccc_error::{Result, RmcccError};
use rmccc_mods::{DependencyType, ModDependency};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};
use std::fs::File;
use std::path::{Path, PathBuf};
use zip::ZipArchive;

use crate::tools::{
    object_schema, AgentTool, AgentToolDescriptor, ConfirmationPolicy, RiskLevel, ToolRegistry,
};

/// Fabric 元数据来源；`Unsupported` 表示未解析（不是"无依赖"）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MetadataSource {
    FabricJson,
    Unsupported,
}

/// 已安装 Mod 的最小元数据。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InstalledMod {
    pub file: String,
    pub mod_id: String,
    pub name: String,
    pub version: Option<String>,
    pub metadata: MetadataSource,
}

/// 计划中的一条依赖（不执行）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlannedDependency {
    pub mod_id: String,
    pub constraint: Option<String>,
    pub source: String,
    /// 哪些已装 Mod 需要它（用于向用户解释"为什么要装"）。
    pub required_by: Vec<String>,
}

/// 已装 Mod 声明的不兼容项。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModConflict {
    pub mod_id: String,
    pub declared_by: String,
    pub constraint: Option<String>,
}

/// 只读扫描结果。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DependencyReport {
    pub mods: Vec<InstalledMod>,
    /// 所有声明出的依赖（含 optional / incompatible），已消费 [`ModDependency`]。
    pub dependencies: Vec<ModDependency>,
    pub missing_required: Vec<PlannedDependency>,
    pub conflicts: Vec<ModConflict>,
    /// 存在但无法解析元数据的 jar 文件名。
    pub unparsed: Vec<String>,
}

/// 安装计划：**永远 dry-run**，写操作由 Phase 6.5 的审批链路接手。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModInstallPlan {
    pub dry_run: bool,
    pub requires_confirmation: bool,
    pub mods_dir: String,
    pub dependencies: Vec<PlannedDependency>,
    pub conflicts: Vec<ModConflict>,
    pub notes: Vec<String>,
}

/// 扫描实例 `mods/` 目录（只看启用的顶层 `.jar`，不进入 `disabled/`）。
pub fn scan_instance_mods(mods_dir: &Path) -> Result<DependencyReport> {
    if !mods_dir.is_dir() {
        return Err(RmcccError::Validation(format!(
            "mods directory does not exist: {}",
            mods_dir.display()
        )));
    }

    let mut jars: Vec<PathBuf> = std::fs::read_dir(mods_dir)?
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "jar"))
        .collect();
    jars.sort();

    let mut mods = Vec::new();
    let mut dependencies = Vec::new();
    let mut unparsed = Vec::new();

    for jar in jars {
        let file = jar
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or_default()
            .to_string();
        match read_fabric_metadata(&jar) {
            Ok(Some(meta)) => {
                let declared_by = meta.id.clone();
                for (mod_id, constraint) in meta.depends {
                    dependencies.push(ModDependency {
                        mod_id,
                        version_constraint: constraint,
                        dependency_type: DependencyType::Required,
                        source: format!("fabric.mod.json:{declared_by}"),
                    });
                }
                for (mod_id, constraint) in meta.optional {
                    dependencies.push(ModDependency {
                        mod_id,
                        version_constraint: constraint,
                        dependency_type: DependencyType::Optional,
                        source: format!("fabric.mod.json:{declared_by}"),
                    });
                }
                for (mod_id, constraint) in meta.breaks {
                    dependencies.push(ModDependency {
                        mod_id,
                        version_constraint: constraint,
                        dependency_type: DependencyType::Incompatible,
                        source: format!("fabric.mod.json:{declared_by}"),
                    });
                }
                mods.push(InstalledMod {
                    file,
                    mod_id: meta.id,
                    name: meta.name,
                    version: Some(meta.version),
                    metadata: MetadataSource::FabricJson,
                });
            }
            Ok(None) => {
                mods.push(InstalledMod {
                    file: file.clone(),
                    mod_id: file.trim_end_matches(".jar").to_string(),
                    name: file.trim_end_matches(".jar").to_string(),
                    version: None,
                    metadata: MetadataSource::Unsupported,
                });
                unparsed.push(file);
            }
            Err(error) => {
                tracing::warn!(jar = %file, %error, "failed to read mod metadata");
                mods.push(InstalledMod {
                    file: file.clone(),
                    mod_id: file.trim_end_matches(".jar").to_string(),
                    name: file.trim_end_matches(".jar").to_string(),
                    version: None,
                    metadata: MetadataSource::Unsupported,
                });
                unparsed.push(file);
            }
        }
    }

    let installed: BTreeSet<String> = mods.iter().map(|m| m.mod_id.clone()).collect();
    let mut missing: BTreeMap<String, PlannedDependency> = BTreeMap::new();
    let mut conflicts = Vec::new();

    for dependency in &dependencies {
        if is_platform_dependency(&dependency.mod_id) {
            continue;
        }
        match dependency.dependency_type {
            DependencyType::Required => {
                if !installed.contains(&dependency.mod_id) {
                    let required_by = declaring_mod(dependency);
                    missing
                        .entry(dependency.mod_id.clone())
                        .and_modify(|entry| {
                            if let Some(by) = &required_by {
                                if !entry.required_by.contains(by) {
                                    entry.required_by.push(by.clone());
                                }
                            }
                        })
                        .or_insert_with(|| PlannedDependency {
                            mod_id: dependency.mod_id.clone(),
                            constraint: dependency.version_constraint.clone(),
                            source: dependency.source.clone(),
                            required_by: required_by.into_iter().collect(),
                        });
                }
            }
            DependencyType::Incompatible => {
                if installed.contains(&dependency.mod_id) {
                    conflicts.push(ModConflict {
                        mod_id: dependency.mod_id.clone(),
                        declared_by: declaring_mod(dependency)
                            .unwrap_or_else(|| dependency.source.clone()),
                        constraint: dependency.version_constraint.clone(),
                    });
                }
            }
            DependencyType::Optional => {}
        }
    }

    Ok(DependencyReport {
        mods,
        dependencies,
        missing_required: missing.into_values().collect(),
        conflicts,
        unparsed,
    })
}

/// 把报告变成可审阅的 dry-run 计划。
pub fn plan_from_report(report: &DependencyReport, mods_dir: &Path) -> ModInstallPlan {
    let mut notes = vec![
        "只读扫描：本计划不会安装、删除或修改任何文件".to_string(),
        "版本约束以源元数据记录的字符串为准，安装前仍需由 Core 复核".to_string(),
    ];
    if !report.unparsed.is_empty() {
        notes.push(format!(
            "{} 个 jar 的元数据无法解析（Forge/NeoForge TOML 或损坏文件），未纳入兼容性判断",
            report.unparsed.len()
        ));
    }
    if report.conflicts.is_empty() {
        notes.push("未发现声明式不兼容".to_string());
    } else {
        notes.push(format!(
            "发现 {} 组声明式不兼容，需人工确认后再安装",
            report.conflicts.len()
        ));
    }
    ModInstallPlan {
        dry_run: true,
        requires_confirmation: true,
        mods_dir: mods_dir.display().to_string(),
        dependencies: report.missing_required.clone(),
        conflicts: report.conflicts.clone(),
        notes,
    }
}

/// 默认只读工具集（Phase 6.6）：已装 Mod、依赖扫描、已装版本。
pub fn read_only_registry() -> Result<ToolRegistry> {
    let mut registry = ToolRegistry::new();
    registry.register(std::sync::Arc::new(InstalledModsTool))?;
    registry.register(std::sync::Arc::new(DependencyScanTool))?;
    registry.register(std::sync::Arc::new(InstalledVersionsTool))?;
    Ok(registry)
}

struct FabricMeta {
    id: String,
    name: String,
    version: String,
    depends: Vec<(String, Option<String>)>,
    optional: Vec<(String, Option<String>)>,
    breaks: Vec<(String, Option<String>)>,
}

#[derive(Deserialize)]
struct RawFabricMeta {
    id: String,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    version: Option<String>,
    #[serde(default)]
    depends: Option<Value>,
    #[serde(default)]
    suggests: Option<Value>,
    #[serde(default)]
    breaks: Option<Value>,
}

fn read_fabric_metadata(jar: &Path) -> Result<Option<FabricMeta>> {
    let file = File::open(jar)?;
    let mut archive = match ZipArchive::new(file) {
        Ok(archive) => archive,
        Err(error) => {
            return Err(RmcccError::Validation(format!(
                "not a readable jar ({}): {error}",
                jar.display()
            )))
        }
    };
    let mut entry = match archive.by_name("fabric.mod.json") {
        Ok(entry) => entry,
        Err(zip::result::ZipError::FileNotFound) => return Ok(None),
        Err(error) => return Err(RmcccError::Other(format!("zip error: {error}"))),
    };
    let mut raw = String::new();
    use std::io::Read;
    entry.read_to_string(&mut raw)?;
    let parsed: RawFabricMeta = serde_json::from_str(&raw)
        .map_err(|e| RmcccError::Validation(format!("invalid fabric.mod.json: {e}")))?;
    Ok(Some(FabricMeta {
        id: parsed.id,
        name: parsed.name.unwrap_or_default(),
        version: parsed.version.unwrap_or_default(),
        depends: parse_dependency_map(parsed.depends.as_ref()),
        optional: parse_dependency_map(parsed.suggests.as_ref()),
        breaks: parse_dependency_map(parsed.breaks.as_ref()),
    }))
}

/// `depends`/`suggests`/`breaks` 接受两种形状：
/// `{"mod": ">=1.0"}` 或 `[{"id":"mod","version":">=1.0"}]`。
fn parse_dependency_map(value: Option<&Value>) -> Vec<(String, Option<String>)> {
    let mut out = Vec::new();
    match value {
        Some(Value::Object(map)) => {
            for (id, constraint) in map {
                let constraint = match constraint {
                    Value::String(text) => Some(text.clone()),
                    Value::Array(alternatives) => {
                        let joined = alternatives
                            .iter()
                            .filter_map(Value::as_str)
                            .collect::<Vec<_>>()
                            .join(" | ");
                        (!joined.is_empty()).then_some(joined)
                    }
                    _ => None,
                };
                out.push((id.clone(), constraint));
            }
        }
        Some(Value::Array(entries)) => {
            for entry in entries {
                let Some(id) = entry.get("id").and_then(Value::as_str) else {
                    continue;
                };
                let constraint = entry
                    .get("version")
                    .and_then(Value::as_str)
                    .map(str::to_string);
                out.push((id.to_string(), constraint));
            }
        }
        _ => {}
    }
    out.sort();
    out
}

/// 从 `fabric.mod.json:<declared_by>` 回溯声明者。
fn declaring_mod(dependency: &ModDependency) -> Option<String> {
    dependency
        .source
        .strip_prefix("fabric.mod.json:")
        .map(str::to_string)
}

fn is_platform_dependency(mod_id: &str) -> bool {
    matches!(
        mod_id.to_ascii_lowercase().as_str(),
        "minecraft"
            | "java"
            | "fabricloader"
            | "fabric-loader"
            | "forge"
            | "neoforge"
            | "quilt_loader"
            | "quilt-loader"
            | "liteloader"
    )
}

fn input_dir(input: &Value, key: &str) -> Result<PathBuf> {
    let raw = input
        .get(key)
        .and_then(Value::as_str)
        .ok_or_else(|| RmcccError::Validation(format!("missing '{key}' string")))?;
    let path = PathBuf::from(raw);
    if !path.is_absolute() {
        return Err(RmcccError::Validation(format!(
            "'{key}' must be an absolute path, got '{raw}'"
        )));
    }
    if !path.is_dir() {
        return Err(RmcccError::Validation(format!(
            "'{key}' is not a directory: {raw}"
        )));
    }
    Ok(path)
}

fn dir_input_schema(key: &str) -> Value {
    object_schema(json!({ key: { "type": "string", "minLength": 1 } }), &[key])
}

fn descriptor(
    id: &str,
    name: &str,
    description: &str,
    input_schema: Value,
    output_schema: Value,
) -> AgentToolDescriptor {
    AgentToolDescriptor {
        id: id.into(),
        plugin_id: None,
        name: name.into(),
        description: description.into(),
        input_schema,
        output_schema,
        required_capabilities: vec![],
        risk_level: RiskLevel::ReadOnly,
        timeout_ms: 5_000,
        confirmation_policy: ConfirmationPolicy::Never,
    }
}

/// `mods.installed` — 列出实例已装 Mod 的最小元数据。
struct InstalledModsTool;

impl AgentTool for InstalledModsTool {
    fn descriptor(&self) -> &AgentToolDescriptor {
        use std::sync::OnceLock;
        static DESCRIPTOR: OnceLock<AgentToolDescriptor> = OnceLock::new();
        DESCRIPTOR.get_or_init(|| {
            descriptor(
                "mods.installed",
                "列出已装 Mod",
                "读取实例 mods/ 目录，返回每个 jar 的 mod id / 名称 / 版本；不修改文件。",
                dir_input_schema("mods_dir"),
                json!({ "type": "object" }),
            )
        })
    }

    fn execute(&self, input: &Value) -> Result<Value> {
        let dir = input_dir(input, "mods_dir")?;
        let report = scan_instance_mods(&dir)?;
        Ok(json!({
            "mods": report.mods,
            "unparsed": report.unparsed,
        }))
    }
}

/// `mods.dependency_scan` — 缺失前置/冲突 + dry-run 安装计划。
struct DependencyScanTool;

impl AgentTool for DependencyScanTool {
    fn descriptor(&self) -> &AgentToolDescriptor {
        use std::sync::OnceLock;
        static DESCRIPTOR: OnceLock<AgentToolDescriptor> = OnceLock::new();
        DESCRIPTOR.get_or_init(|| {
            descriptor(
                "mods.dependency_scan",
                "检查 Mod 依赖与冲突",
                "解析实例内 Fabric 模组的依赖，返回缺失的必需前置、声明式不兼容与 dry-run 安装计划；不执行安装。",
                dir_input_schema("mods_dir"),
                json!({ "type": "object" }),
            )
        })
    }

    fn execute(&self, input: &Value) -> Result<Value> {
        let dir = input_dir(input, "mods_dir")?;
        let report = scan_instance_mods(&dir)?;
        let plan = plan_from_report(&report, &dir);
        Ok(json!({ "report": report, "plan": plan }))
    }
}

/// `versions.installed` — 列出 `versions/` 下的版本目录与关键文件是否存在。
struct InstalledVersionsTool;

impl AgentTool for InstalledVersionsTool {
    fn descriptor(&self) -> &AgentToolDescriptor {
        use std::sync::OnceLock;
        static DESCRIPTOR: OnceLock<AgentToolDescriptor> = OnceLock::new();
        DESCRIPTOR.get_or_init(|| {
            descriptor(
                "versions.installed",
                "列出已装版本",
                "扫描 versions/ 目录，返回版本 id 以及 client jar / manifest 是否存在；不下载、不修改。",
                dir_input_schema("versions_dir"),
                json!({ "type": "object" }),
            )
        })
    }

    fn execute(&self, input: &Value) -> Result<Value> {
        let dir = input_dir(input, "versions_dir")?;
        let mut versions = Vec::new();
        for entry in std::fs::read_dir(&dir)?.flatten() {
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }
            let Some(id) = path.file_name().and_then(|name| name.to_str()) else {
                continue;
            };
            let version_dir = &path;
            versions.push(json!({
                "id": id,
                "has_json": version_dir.join(format!("{id}.json")).is_file(),
                "has_jar": version_dir.join(format!("{id}.jar")).is_file(),
            }));
        }
        versions.sort_by(|a, b| a["id"].as_str().cmp(&b["id"].as_str()));
        Ok(json!({ "versions": versions }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use zip::write::SimpleFileOptions;

    fn temp_dir(tag: &str) -> PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let dir = std::env::temp_dir().join(format!(
            "rmccc-agent-mods-{tag}-{}-{nanos}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("temp dir");
        dir
    }

    fn write_jar(dir: &Path, name: &str, fabric_json: Option<&str>) {
        let file = File::create(dir.join(name)).expect("jar");
        let mut writer = zip::ZipWriter::new(file);
        let options =
            SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
        if let Some(body) = fabric_json {
            writer
                .start_file("fabric.mod.json", options)
                .expect("entry");
            writer.write_all(body.as_bytes()).expect("write");
        }
        writer.finish().expect("finish");
    }

    #[test]
    fn scan_detects_missing_required_and_ignores_platform_dependencies() {
        let dir = temp_dir("missing");
        write_jar(
            &dir,
            "sodium.jar",
            Some(
                r#"{
                    "id": "sodium",
                    "name": "Sodium",
                    "version": "0.5.8",
                    "depends": {
                        "minecraft": ">=1.20",
                        "fabricloader": ">=0.14",
                        "indium": ">=1.0"
                    }
                }"#,
            ),
        );
        let report = scan_instance_mods(&dir).expect("scan");
        assert_eq!(report.mods.len(), 1);
        assert_eq!(report.mods[0].mod_id, "sodium");
        assert_eq!(report.mods[0].metadata, MetadataSource::FabricJson);
        assert_eq!(report.missing_required.len(), 1);
        assert_eq!(report.missing_required[0].mod_id, "indium");
        assert_eq!(
            report.missing_required[0].constraint.as_deref(),
            Some(">=1.0")
        );
        assert_eq!(report.missing_required[0].required_by, vec!["sodium"]);
        assert!(report.conflicts.is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn optional_dependencies_and_satisfied_dependencies_are_not_missing() {
        let dir = temp_dir("satisfied");
        write_jar(
            &dir,
            "a.jar",
            Some(r#"{"id":"a","version":"1.0","depends":{"b":">=1"},"suggests":{"c":"*"}}"#),
        );
        write_jar(&dir, "b.jar", Some(r#"{"id":"b","version":"1.0"}"#));
        let report = scan_instance_mods(&dir).expect("scan");
        assert!(report.missing_required.is_empty());
        assert!(report
            .dependencies
            .iter()
            .any(|d| d.mod_id == "c" && d.dependency_type == DependencyType::Optional));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn breaks_produce_conflicts() {
        let dir = temp_dir("conflict");
        write_jar(
            &dir,
            "a.jar",
            Some(r#"{"id":"a","version":"1","breaks":{"b":"<2"}}"#),
        );
        write_jar(&dir, "b.jar", Some(r#"{"id":"b","version":"1"}"#));
        let report = scan_instance_mods(&dir).expect("scan");
        assert_eq!(report.conflicts.len(), 1);
        assert_eq!(report.conflicts[0].mod_id, "b");
        assert_eq!(report.conflicts[0].declared_by, "a");
        let plan = plan_from_report(&report, &dir);
        assert!(plan.notes.iter().any(|note| note.contains("声明式不兼容")));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn jars_without_metadata_are_unparsed_not_silently_compatible() {
        let dir = temp_dir("unparsed");
        write_jar(&dir, "forge-mod.jar", None);
        std::fs::write(dir.join("broken.jar"), b"not a zip").expect("broken jar");
        let report = scan_instance_mods(&dir).expect("scan");
        assert_eq!(report.mods.len(), 2);
        assert_eq!(report.unparsed.len(), 2);
        assert!(report
            .mods
            .iter()
            .all(|m| m.metadata == MetadataSource::Unsupported));
        let plan = plan_from_report(&report, &dir);
        assert!(plan.notes.iter().any(|note| note.contains("无法解析")));
        assert!(plan.dry_run);
        assert!(plan.requires_confirmation);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn scan_rejects_missing_directory() {
        let missing = std::env::temp_dir().join("rmccc-agent-mods-does-not-exist");
        let _ = std::fs::remove_dir_all(&missing);
        assert!(scan_instance_mods(&missing).is_err());
    }

    #[test]
    fn registry_exposes_three_read_only_tools() {
        let registry = read_only_registry().expect("registry");
        assert_eq!(registry.len(), 3);
        for descriptor in registry.descriptors() {
            assert_eq!(descriptor.risk_level, RiskLevel::ReadOnly);
            assert_eq!(descriptor.confirmation_policy, ConfirmationPolicy::Never);
        }
    }

    #[test]
    fn dependency_tool_returns_plan_without_writing() {
        let dir = temp_dir("tool");
        write_jar(
            &dir,
            "a.jar",
            Some(r#"{"id":"a","version":"1","depends":{"missing-lib":">=2"}}"#),
        );
        let before: Vec<_> = std::fs::read_dir(&dir)
            .expect("read")
            .flatten()
            .map(|e| e.file_name())
            .collect();
        let registry = read_only_registry().expect("registry");
        let result = registry
            .invoke(
                "mods.dependency_scan",
                &json!({ "mods_dir": dir.to_string_lossy() }),
            )
            .expect("invoke");
        assert!(result.ok);
        assert_eq!(result.output["plan"]["dry_run"], true);
        assert_eq!(result.output["plan"]["requires_confirmation"], true);
        assert_eq!(
            result.output["report"]["missing_required"][0]["mod_id"],
            "missing-lib"
        );
        let after: Vec<_> = std::fs::read_dir(&dir)
            .expect("read")
            .flatten()
            .map(|e| e.file_name())
            .collect();
        assert_eq!(before.len(), after.len(), "scan must not create files");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn tools_reject_relative_paths_and_unknown_fields() {
        let registry = read_only_registry().expect("registry");
        assert!(registry
            .invoke("mods.installed", &json!({ "mods_dir": "relative/mods" }))
            .is_err());
        assert!(registry
            .invoke(
                "mods.installed",
                &json!({ "mods_dir": "/tmp", "extra": true })
            )
            .is_err());
    }

    #[test]
    fn installed_versions_tool_reports_files() {
        let dir = temp_dir("versions");
        let version_dir = dir.join("1.20.4");
        std::fs::create_dir_all(&version_dir).expect("version dir");
        std::fs::write(version_dir.join("1.20.4.json"), b"{}").expect("json");
        std::fs::write(version_dir.join("1.20.4.jar"), b"jar").expect("jar");
        std::fs::create_dir_all(dir.join("1.21")).expect("empty version dir");

        let registry = read_only_registry().expect("registry");
        let result = registry
            .invoke(
                "versions.installed",
                &json!({ "versions_dir": dir.to_string_lossy() }),
            )
            .expect("invoke");
        let versions = result.output["versions"].as_array().expect("array");
        assert_eq!(versions.len(), 2);
        assert_eq!(versions[0]["id"], "1.20.4");
        assert_eq!(versions[0]["has_json"], true);
        assert_eq!(versions[0]["has_jar"], true);
        assert_eq!(versions[1]["has_json"], false);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
