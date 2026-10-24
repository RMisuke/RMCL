//! 实例 Mod 依赖诊断与补装（**纯逻辑，不依赖 AI**；ADR-013 去 Agent 化）。
//!
//! 首个端到端场景："检查这个实例的模组兼容性并修复缺失前置"。
//! 本模块实现其中的全部确定性逻辑：
//!
//! 1. 扫描实例 `mods/` 下的 jar，解析 `fabric.mod.json` 的 `depends` / `breaks`；
//! 2. 生成 [`DependencyReport`]（已装 / 依赖 / 缺失必需 / 冲突 / 未解析）；
//! 3. 生成 [`ModInstallPlan`]（dry-run 计划，供 UI / MCP 调用方向用户展示）；
//! 4. [`apply_missing_dependencies`] 执行补装：执行前**重新扫描**（"用户确认 +
//!    Core 再校验"），把仍缺失的必需前置交给 [`PlanApplier`]（宿主注入）落地。
//!
//! 由此 [`rmccc_mods::ModDependency`] 有了真实消费方（审计 P3-2）。
//!
//! # 覆盖范围
//!
//! v1 只解析 Fabric 元数据。Forge/NeoForge 的 `META-INF/mods.toml` 需要 TOML 解析，
//! 为避免为此引入新依赖（体积红线），这些 jar 会进入 `unparsed` 并在计划里明确说明，
//! **不会**被当成"兼容"。

use rmccc_error::{Result, RmcccError};
use rmccc_mods::{DependencyType, ModDependency};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::fs::File;
use std::path::{Path, PathBuf};
use ts_rs::TS;
use zip::ZipArchive;

/// Fabric 元数据来源；`Unsupported` 表示未解析（不是"无依赖"）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export_to = "types.ts")]
pub enum MetadataSource {
    FabricJson,
    Unsupported,
}

/// 已安装 Mod 的最小元数据。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export_to = "types.ts")]
pub struct InstalledMod {
    pub file: String,
    pub mod_id: String,
    pub name: String,
    pub version: Option<String>,
    pub metadata: MetadataSource,
}

/// 计划中的一条依赖（不执行）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export_to = "types.ts")]
pub struct PlannedDependency {
    pub mod_id: String,
    pub constraint: Option<String>,
    pub source: String,
    /// 哪些已装 Mod 需要它（用于向用户解释"为什么要装"）。
    pub required_by: Vec<String>,
}

/// 已装 Mod 声明的不兼容项。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export_to = "types.ts")]
pub struct ModConflict {
    pub mod_id: String,
    pub declared_by: String,
    pub constraint: Option<String>,
}

/// 只读扫描结果。
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export_to = "types.ts")]
pub struct DependencyReport {
    pub mods: Vec<InstalledMod>,
    /// 所有声明出的依赖（含 optional / incompatible），已消费 [`ModDependency`]。
    pub dependencies: Vec<ModDependency>,
    pub missing_required: Vec<PlannedDependency>,
    pub conflicts: Vec<ModConflict>,
    /// 存在但无法解析元数据的 jar 文件名。
    pub unparsed: Vec<String>,
}

/// 安装计划：**永远 dry-run**，是否执行由调用方（GUI 确认 / MCP 确认流）决定。
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export_to = "types.ts")]
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

    // 只有解析出元数据的 mod 才算"已安装"：无元数据的 jar（Forge/损坏文件）
    // 以文件名 stem 进入集合会错误地满足同名依赖，漏报缺失前置（审计 P2-28）。
    let installed: BTreeSet<String> = mods
        .iter()
        .filter(|m| m.metadata == MetadataSource::FabricJson)
        .map(|m| m.mod_id.clone())
        .collect();
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

/// 补装缺失必需依赖（原 Phase 6.5 `mods.apply_plan` 写工具的去 Agent 化形态）。
///
/// **执行前重新扫描**（"用户确认 + Core 再校验"）：确认期间实例状态可能变化，
/// 以此刻扫描出的仍缺失项为准；没有缺失则直接返回空报告。
/// 文件写入只发生在 [`PlanApplier`]（宿主注入）内部。
pub fn apply_missing_dependencies(
    mods_dir: &Path,
    applier: &dyn PlanApplier,
) -> Result<AppliedPlanReport> {
    ensure_dir(mods_dir)?;
    let report = scan_instance_mods(mods_dir)?;
    if report.missing_required.is_empty() {
        return Ok(AppliedPlanReport::default());
    }
    applier.apply(mods_dir, &report.missing_required)
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

/// 补装执行器：把"缺失依赖 → 已安装"落地（宿主注入，ADR-013）。
///
/// 本 crate 不持有 `CoreContext`（架构约束），真正的解析（Modrinth slug /
/// 版本匹配加载器与游戏版本）与下载写入由宿主实现（`src-tauri` 的
/// `ModrinthPlanApplier`）。实现方必须对每个 jar 做 SHA1 校验，
/// 并只写入传入的 `mods_dir`。
///
/// 同步约束：宿主的异步下载在实现内部用 `tokio::task::block_in_place` +
/// `Handle::block_on` 桥接。
pub trait PlanApplier: Send + Sync {
    fn apply(
        &self,
        mods_dir: &Path,
        dependencies: &[PlannedDependency],
    ) -> Result<AppliedPlanReport>;
}

/// 单个依赖的安装结果。
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, TS)]
#[ts(export_to = "types.ts")]
pub struct AppliedMod {
    pub mod_id: String,
    pub file: String,
    pub version: Option<String>,
}

/// 单个依赖的失败原因。
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, TS)]
#[ts(export_to = "types.ts")]
pub struct FailedMod {
    pub mod_id: String,
    pub reason: String,
}

/// `apply_missing_dependencies` 的执行结果。
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, TS, Default)]
#[ts(export_to = "types.ts")]
pub struct AppliedPlanReport {
    pub installed: Vec<AppliedMod>,
    pub failed: Vec<FailedMod>,
    /// 执行前的重新扫描发现的仍缺失项（= installed ∪ failed 的 id 集合）。
    pub attempted: Vec<String>,
}

fn ensure_dir(path: &Path) -> Result<()> {
    if !path.is_dir() {
        return Err(RmcccError::Validation(format!(
            "directory not found: {}",
            path.display()
        )));
    }
    Ok(())
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
    fn unparsed_jar_does_not_satisfy_dependencies() {
        // P2-28 回归：名为 indium.jar 的无元数据 jar（Forge mod 或损坏文件）
        // 不得让 sodium 的 `depends: indium` 被视为已满足。
        let dir = temp_dir("unparsed-dep");
        write_jar(&dir, "indium.jar", None);
        write_jar(
            &dir,
            "sodium.jar",
            Some(r#"{"id":"sodium","version":"0.5","depends":{"indium":">=1.0"}}"#),
        );
        let report = scan_instance_mods(&dir).expect("scan");
        assert_eq!(
            report.missing_required[0].mod_id, "indium",
            "unparsed jar must not satisfy the dependency"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// 真实落盘的假执行器：为每个依赖写一个带 fabric.mod.json 的 jar，
    /// 使下一次扫描能看到依赖已满足（验证"执行前再校验"语义）。
    struct WritingApplier;

    impl PlanApplier for WritingApplier {
        fn apply(
            &self,
            mods_dir: &Path,
            dependencies: &[PlannedDependency],
        ) -> Result<AppliedPlanReport> {
            let mut installed = Vec::new();
            for dep in dependencies {
                let file = format!("{}.jar", dep.mod_id);
                write_jar(
                    mods_dir,
                    &file,
                    Some(&format!(r#"{{"id":"{}","version":"1.0"}}"#, dep.mod_id)),
                );
                installed.push(AppliedMod {
                    mod_id: dep.mod_id.clone(),
                    file,
                    version: Some("1.0".into()),
                });
            }
            Ok(AppliedPlanReport {
                attempted: dependencies.iter().map(|d| d.mod_id.clone()).collect(),
                installed,
                failed: Vec::new(),
            })
        }
    }

    #[test]
    fn apply_missing_re_scans_before_installing() {
        let dir = temp_dir("apply");
        write_jar(
            &dir,
            "a.jar",
            Some(r#"{"id":"a","version":"1","depends":{"indium":">=1.0"}}"#),
        );
        let first = apply_missing_dependencies(&dir, &WritingApplier).expect("apply");
        assert_eq!(first.attempted, vec!["indium".to_string()]);
        assert_eq!(first.installed.len(), 1);

        // 第二次：上一轮已落盘，重新扫描应为空，不再触发执行器。
        let second = apply_missing_dependencies(&dir, &WritingApplier).expect("apply again");
        assert!(second.attempted.is_empty());
        assert!(second.installed.is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn apply_missing_reports_applier_failures() {
        struct FailingApplier;
        impl PlanApplier for FailingApplier {
            fn apply(
                &self,
                _mods_dir: &Path,
                dependencies: &[PlannedDependency],
            ) -> Result<AppliedPlanReport> {
                Ok(AppliedPlanReport {
                    attempted: dependencies.iter().map(|d| d.mod_id.clone()).collect(),
                    installed: Vec::new(),
                    failed: dependencies
                        .iter()
                        .map(|d| FailedMod {
                            mod_id: d.mod_id.clone(),
                            reason: "no Modrinth match".into(),
                        })
                        .collect(),
                })
            }
        }
        let dir = temp_dir("apply-fail");
        write_jar(
            &dir,
            "a.jar",
            Some(r#"{"id":"a","version":"1","depends":{"ghost":">=1.0"}}"#),
        );
        let report = apply_missing_dependencies(&dir, &FailingApplier).expect("apply");
        assert_eq!(report.failed.len(), 1);
        assert_eq!(report.failed[0].mod_id, "ghost");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
