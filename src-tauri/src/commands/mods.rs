//! Mod management IPC commands.
//!
//! Delegates to `CoreContext::mod_sources` for Modrinth/CurseForge API access.

use crate::state::AppState;
use rmccc_error::IpcError;
use rmccc_mods::ModSearchQuery;
use serde::Serialize;
use ts_rs::TS;

fn locked_error() -> IpcError {
    IpcError {
        code: rmccc_error::ErrorCode::InternalError,
        message: "Instance service lock poisoned".into(),
        retryable: false,
    }
}

fn load_instance(
    core: &rmccc_core::CoreContext,
    key: &str,
) -> Result<rmccc_instance::InstanceDetail, IpcError> {
    let mut service = core.instances.lock().map_err(|_| locked_error())?;
    Ok(service.get(key)?)
}

#[tauri::command]
#[tracing::instrument(skip(state))]
pub async fn mod_search(
    state: tauri::State<'_, AppState>,
    query: String,
    game_version: Option<String>,
    loader: Option<String>,
    page: Option<u32>,
    limit: Option<u32>,
    index: Option<String>,
) -> Result<rmccc_mods::ModSearchResult, IpcError> {
    // A2: snapshot the source Arc under a short read lock — the network call
    // below must not hold the core lock (a queued writer would stall IPC).
    let source = {
        let core = state.core.read().await;
        core.mod_sources.first().cloned().ok_or_else(|| IpcError {
            code: rmccc_error::ErrorCode::Unknown,
            message: "No mod source available".into(),
            retryable: false,
        })?
    };

    let search_query = ModSearchQuery {
        query,
        game_version,
        loader,
        page: page.unwrap_or(1),
        limit: limit.unwrap_or(20).min(100),
        index,
    };

    // P3-08：记录搜索执行结果（query + 命中数）
    let result = source.search(&search_query).await.map_err(IpcError::from)?;
    tracing::info!(
        query = %search_query.query,
        hits = result.hits.len(),
        total = result.total,
        "Mod search completed"
    );
    Ok(result)
}

#[tauri::command]
#[tracing::instrument(skip(state))]
pub async fn mod_get_versions(
    state: tauri::State<'_, AppState>,
    mod_id: String,
) -> Result<Vec<rmccc_mods::ModVersion>, IpcError> {
    // A2: short read lock, then network call lock-free (perf audit 1.1).
    let source = {
        let core = state.core.read().await;
        core.mod_sources.first().cloned().ok_or_else(|| IpcError {
            code: rmccc_error::ErrorCode::Unknown,
            message: "No mod source available".into(),
            retryable: false,
        })?
    };

    source.list_versions(&mod_id).await.map_err(IpcError::from)
}

#[tauri::command]
#[tracing::instrument(skip(state))]
pub async fn mod_get_download_url(
    state: tauri::State<'_, AppState>,
    version_id: String,
) -> Result<String, IpcError> {
    // A2: short read lock, then network call lock-free (perf audit 1.1).
    let source = {
        let core = state.core.read().await;
        core.mod_sources.first().cloned().ok_or_else(|| IpcError {
            code: rmccc_error::ErrorCode::Unknown,
            message: "No mod source available".into(),
            retryable: false,
        })?
    };

    source
        .get_download_url(&version_id)
        .await
        .map_err(IpcError::from)
}

#[tauri::command]
#[tracing::instrument(skip(state))]
pub async fn mod_list(state: tauri::State<'_, AppState>, key: String) -> Result<ModList, IpcError> {
    // A2: snapshot (config, profile) under a short read lock — no core lock is
    // held across the work below (perf audit 1.1).
    let detail = {
        let core = state.core.read().await;
        load_instance(&core, &key)?
    };

    // Mods follow the instance run directory, so an isolated instance keeps
    // its own mods (HMCL behaviour) instead of writing into the shared root.
    let mods_dir = detail.paths.mods_dir.clone();
    let disabled_dir = mods_dir.join("disabled");

    let scan = |dir: &std::path::Path| -> Vec<String> {
        let mut names: Vec<String> = Vec::new();
        if let Ok(entries) = std::fs::read_dir(dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().is_some_and(|e| e == "jar") {
                    // Full file name (with `.jar`), so the UI can feed it straight
                    // back to `mod_toggle` / `mod_remove`, which expect a file
                    // name rather than a stem.
                    if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                        names.push(name.to_string());
                    }
                }
            }
        }
        names.sort();
        names
    };

    let enabled = scan(&mods_dir);
    let disabled = scan(&disabled_dir);
    tracing::info!(
        key = %key,
        enabled = enabled.len(),
        disabled = disabled.len(),
        "Mods listed"
    );
    Ok(ModList {
        enabled,
        disabled,
        mods_dir: mods_dir.display().to_string(),
    })
}

#[tauri::command]
#[tracing::instrument(skip(state))]
pub async fn mod_toggle(
    state: tauri::State<'_, AppState>,
    key: String,
    mod_filename: String,
    enabled: bool,
) -> Result<(), IpcError> {
    // A2: snapshot (config, profile) under a short read lock — no core lock is
    // held across the work below (perf audit 1.1).
    let detail = {
        let core = state.core.read().await;
        load_instance(&core, &key)?
    };

    // Mods follow the instance run directory, so an isolated instance keeps
    // its own mods (HMCL behaviour) instead of writing into the shared root.
    let mods_dir = detail.paths.mods_dir.clone();
    let disabled_dir = mods_dir.join("disabled");
    let src = if enabled {
        disabled_dir.join(&mod_filename)
    } else {
        mods_dir.join(&mod_filename)
    };
    let dst = if enabled {
        mods_dir.join(&mod_filename)
    } else {
        disabled_dir.join(&mod_filename)
    };

    if !src.exists() {
        return Err(IpcError {
            code: rmccc_error::ErrorCode::Unknown,
            message: format!("Mod file not found: {}", mod_filename),
            retryable: false,
        });
    }

    if enabled {
        std::fs::create_dir_all(&mods_dir)?;
    } else {
        std::fs::create_dir_all(&disabled_dir)?;
    }
    std::fs::rename(&src, &dst)?;

    Ok(())
}

/// Sanitize a filename: keep ASCII alphanumerics and `-_.`, replace the rest with `_`.
pub(crate) fn sanitize_filename(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || "-_.".contains(c) {
                c
            } else {
                '_'
            }
        })
        .collect();
    // 去掉开头的点：`..` 本身不含分隔符、逃不出目录，但生成 `.._x.jar` 这种
    // 隐藏文件既难排查也没意义（审计 P2-11 的同类问题）。
    let trimmed = cleaned.trim_start_matches('.').to_string();
    if trimmed.is_empty() {
        "mod".to_string()
    } else {
        trimmed
    }
}

/// Reject path traversal / invalid chars in user-supplied filenames.
pub(crate) fn validate_mod_filename(name: &str) -> Result<(), IpcError> {
    if name.is_empty()
        || name.contains('/')
        || name.contains('\\')
        || name.contains("..")
        || name.contains('\0')
    {
        return Err(IpcError {
            code: rmccc_error::ErrorCode::ValidationError,
            message: format!("Invalid mod filename: {name}"),
            retryable: false,
        });
    }
    Ok(())
}

/// 安装 Mod：经 `ModSource::install` 下载到 `{game_dir}/mods/`（P0-02）。
#[tauri::command]
#[tracing::instrument(skip(state))]
pub async fn mod_install(
    state: tauri::State<'_, AppState>,
    key: String,
    version: rmccc_mods::ModVersion,
) -> Result<String, IpcError> {
    // A2: snapshot source + (config, profile) under a short read lock — the
    // download below runs lock-free (perf audit 1.1).
    let (source, detail) = {
        let core = state.core.read().await;
        let detail = load_instance(&core, &key)?;
        let source = core.mod_sources.first().cloned().ok_or_else(|| IpcError {
            code: rmccc_error::ErrorCode::Unknown,
            message: "No mod source available".into(),
            retryable: false,
        })?;
        (source, detail)
    };

    // Mods follow the instance run directory, so an isolated instance keeps
    // its own mods (HMCL behaviour) instead of writing into the shared root.
    let mods_dir = detail.paths.mods_dir.clone();
    std::fs::create_dir_all(&mods_dir)?;

    let filename = version
        .filename
        .clone()
        .filter(|f| !f.trim().is_empty())
        .map(|f| sanitize_filename(&f))
        .unwrap_or_else(|| format!("{}.jar", sanitize_filename(&version.name)));
    let filename = if filename.ends_with(".jar") {
        filename
    } else {
        format!("{filename}.jar")
    };
    validate_mod_filename(&filename)?;

    let dest = mods_dir.join(&filename);
    source
        .install(&version, &dest)
        .await
        .map_err(IpcError::from)?;

    tracing::info!(key = %key, filename = %filename, "Mod installed");
    Ok(filename)
}

/// 本地 jar 安装：把用户通过文件选择器选中的 jar 复制到实例 mods 目录（P1-5）。
#[tauri::command]
#[tracing::instrument(skip(state))]
pub async fn mod_install_local(
    state: tauri::State<'_, AppState>,
    key: String,
    source_path: String,
) -> Result<String, IpcError> {
    // A2: snapshot (config, profile) under a short read lock — no core lock is
    // held across the work below (perf audit 1.1).
    let detail = {
        let core = state.core.read().await;
        load_instance(&core, &key)?
    };

    let src = std::path::Path::new(&source_path);
    if !src.is_file() {
        return Err(IpcError {
            code: rmccc_error::ErrorCode::ValidationError,
            message: format!("Not a file: {source_path}"),
            retryable: false,
        });
    }

    let mods_dir = detail.paths.mods_dir.clone();
    std::fs::create_dir_all(&mods_dir)?;

    let filename = src
        .file_name()
        .and_then(|n| n.to_str())
        .map(sanitize_filename)
        .ok_or_else(|| IpcError {
            code: rmccc_error::ErrorCode::ValidationError,
            message: "Invalid file name".into(),
            retryable: false,
        })?;
    let filename = if filename.ends_with(".jar") {
        filename
    } else {
        format!("{filename}.jar")
    };
    validate_mod_filename(&filename)?;

    let dest = mods_dir.join(&filename);
    std::fs::copy(src, &dest)?;
    tracing::info!(key = %key, filename = %filename, "Local mod installed");
    Ok(filename)
}

/// 本地 jar 安装（方案 A）：弹原生文件选择器，把选中的 jar 复制到实例 mods 目录（P1-5）。
#[tauri::command]
#[tracing::instrument(skip(state, app))]
pub async fn mod_pick_and_install(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    key: String,
) -> Result<Option<String>, IpcError> {
    use tauri_plugin_dialog::DialogExt;

    let picked = tauri::async_runtime::spawn_blocking(move || {
        app.dialog()
            .file()
            .add_filter("Mod jar", &["jar"])
            .blocking_pick_file()
    })
    .await
    .map_err(|e| IpcError {
        code: rmccc_error::ErrorCode::Unknown,
        message: format!("file dialog failed: {e}"),
        retryable: false,
    })?;

    let Some(file) = picked else {
        return Ok(None);
    };
    let source = file.into_path().map_err(|e| IpcError {
        code: rmccc_error::ErrorCode::ValidationError,
        message: format!("invalid path: {e}"),
        retryable: false,
    })?;

    // A2: snapshot (config, profile) under a short read lock — no core lock is
    // held across the work below (perf audit 1.1).
    let detail = {
        let core = state.core.read().await;
        load_instance(&core, &key)?
    };

    let mods_dir = detail.paths.mods_dir.clone();
    std::fs::create_dir_all(&mods_dir)?;

    let filename = source
        .file_name()
        .and_then(|n| n.to_str())
        .map(sanitize_filename)
        .ok_or_else(|| IpcError {
            code: rmccc_error::ErrorCode::ValidationError,
            message: "Invalid file name".into(),
            retryable: false,
        })?;
    let filename = if filename.ends_with(".jar") {
        filename
    } else {
        format!("{filename}.jar")
    };
    validate_mod_filename(&filename)?;

    let dest = mods_dir.join(&filename);
    std::fs::copy(&source, &dest)?;
    tracing::info!(key = %key, filename = %filename, "Local mod installed via picker");
    Ok(Some(filename))
}

/// 移除 Mod：删除 `{game_dir}/mods/{name}` 或 `{game_dir}/mods/disabled/{name}`（P0-02）。
#[tauri::command]
#[tracing::instrument(skip(state))]
pub async fn mod_remove(
    state: tauri::State<'_, AppState>,
    key: String,
    mod_filename: String,
) -> Result<(), IpcError> {
    validate_mod_filename(&mod_filename)?;

    // A2: snapshot (config, profile) under a short read lock — no core lock is
    // held across the work below (perf audit 1.1).
    let detail = {
        let core = state.core.read().await;
        load_instance(&core, &key)?
    };

    // Mods follow the instance run directory, so an isolated instance keeps
    // its own mods (HMCL behaviour) instead of writing into the shared root.
    let mods_dir = detail.paths.mods_dir.clone();
    let active = mods_dir.join(&mod_filename);
    let disabled = mods_dir.join("disabled").join(&mod_filename);

    if active.exists() {
        std::fs::remove_file(&active)?;
    } else if disabled.exists() {
        std::fs::remove_file(&disabled)?;
    } else {
        return Err(IpcError {
            code: rmccc_error::ErrorCode::Unknown,
            message: format!("Mod file not found: {mod_filename}"),
            retryable: false,
        });
    }

    Ok(())
}

/// Installed mods for an instance, split by state.
///
/// Disabled mods live in `mods/disabled/`, which is why the listing reports both
/// buckets — a UI that only showed active mods would hide half the state.
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export_to = "types.ts")]
pub struct ModList {
    /// File names of enabled mods (`mods/*.jar`), including the extension.
    pub enabled: Vec<String>,
    /// File names of disabled mods (`mods/disabled/*.jar`).
    pub disabled: Vec<String>,
    /// Where this instance's mods live (follows its run directory).
    pub mods_dir: String,
}

// ── 依赖扫描与一键补装（ADR-013 S3：纯逻辑，去 Agent 化）──
//
// 原 Phase 6.5 的 `mods.dependency_scan` / `mods.apply_plan` Agent 工具上移为
// 宿主命令：不依赖任何 AI 配置，GUI 直接可用（未来 MCP 工具也走同一实现）。

use rmccc_agent::{
    plan_from_report, scan_instance_mods, AppliedMod, AppliedPlanReport, DependencyReport,
    ModInstallPlan, PlannedDependency,
};
use rmccc_error::RmcccError;

/// `mods_dependency_scan` 的返回：扫描报告 + dry-run 计划（UI 直接展示计划，
/// 用户确认后调 `mods_apply_plan`）。
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export_to = "types.ts")]
pub struct ModsDependencyScanView {
    pub report: DependencyReport,
    pub plan: ModInstallPlan,
}

/// 解析实例上下文：mods 目录 / 游戏版本 / 加载器（Modrinth 检索用）。
fn instance_mods_context(
    core: &rmccc_core::CoreContext,
    key: &str,
) -> Result<(std::path::PathBuf, String, Option<String>), IpcError> {
    let detail = load_instance(core, key)?;
    let loader = detail
        .summary
        .loader
        .as_ref()
        .map(|loader| loader.kind.clone());
    Ok((detail.paths.mods_dir, detail.summary.game_version, loader))
}

/// 检查缺失前置（纯本地扫描：fabric.mod.json 依赖图 × 已装集合对比）。
#[tauri::command]
#[tracing::instrument(skip(state))]
pub async fn mods_dependency_scan(
    state: tauri::State<'_, AppState>,
    key: String,
) -> Result<ModsDependencyScanView, IpcError> {
    let core = state.core.read().await;
    let (mods_dir, _, _) = instance_mods_context(&core, &key)?;
    drop(core);

    // 实例还没建过 mods 目录 = 没装任何 mod = 没有可扫的依赖，直接给空报告。
    // Perf audit 1.6: scanning opens + parses every jar (zip central directory
    // + fabric.mod.json) — run it off the async worker.
    let report = if mods_dir.is_dir() {
        tokio::task::spawn_blocking({
            let mods_dir = mods_dir.clone();
            move || scan_instance_mods(&mods_dir)
        })
        .await
        .map_err(|e| RmcccError::Other(format!("dependency scan join error: {e}")))??
    } else {
        DependencyReport {
            mods: Vec::new(),
            dependencies: Vec::new(),
            missing_required: Vec::new(),
            conflicts: Vec::new(),
            unparsed: Vec::new(),
        }
    };
    let plan = plan_from_report(&report, &mods_dir);
    tracing::info!(
        key = %key,
        missing = report.missing_required.len(),
        conflicts = report.conflicts.len(),
        "Dependency scan completed"
    );
    Ok(ModsDependencyScanView { report, plan })
}

/// 指南 §10.6 场景的执行器：Modrinth 解析 + 下载安装（SHA1 校验由
/// `ModSource::install` 负责）。同步 trait 经 `block_in_place` 桥接。
struct ModrinthPlanApplier {
    source: rmccc_mods::modrinth::ModrinthSource,
    game_version: String,
    loader: Option<String>,
}

impl ModrinthPlanApplier {
    async fn install_one(
        &self,
        mods_dir: &std::path::Path,
        dep: &PlannedDependency,
    ) -> rmccc_error::Result<AppliedMod> {
        let search = self
            .source
            .search(&rmccc_mods::ModSearchQuery {
                query: dep.mod_id.clone(),
                game_version: (!self.game_version.is_empty()).then(|| self.game_version.clone()),
                loader: self.loader.clone(),
                page: 0,
                limit: 10,
                index: None,
            })
            .await?;
        let hit = search.hits.first().ok_or_else(|| {
            RmcccError::MissingDependency(format!("no Modrinth project matches '{}'", dep.mod_id))
        })?;
        let versions = self.source.list_versions(&hit.id).await?;
        // 优先"游戏版本 + 加载器"都匹配，其次只匹配游戏版本，最后取最新。
        let version = versions
            .iter()
            .find(|v| {
                v.game_versions.iter().any(|g| g == &self.game_version)
                    && self
                        .loader
                        .as_ref()
                        .is_some_and(|l| v.loaders.iter().any(|v_l| v_l == l))
            })
            .or_else(|| {
                versions
                    .iter()
                    .find(|v| v.game_versions.iter().any(|g| g == &self.game_version))
            })
            .or_else(|| versions.first())
            .ok_or_else(|| {
                RmcccError::MissingDependency(format!(
                    "no downloadable version for '{}' ({})",
                    dep.mod_id, hit.id
                ))
            })?
            .clone();

        // 约束字符串（如 ">=1.0"）以源元数据记录为准，由用户在计划展示时
        // 人工确认——本版本不做 semver 求解。
        let raw_name = version
            .filename
            .clone()
            .unwrap_or_else(|| format!("{}-{}.jar", dep.mod_id, version.version_number));
        let filename = sanitize_filename(&raw_name);
        let dest = mods_dir.join(filename);
        use rmccc_mods::ModSource as _;
        self.source.install(&version, &dest).await?;
        Ok(AppliedMod {
            mod_id: dep.mod_id.clone(),
            file: dest
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default(),
            version: Some(version.version_number.clone()),
        })
    }
}

/// 一键补装缺失前置：执行前重新扫描（"用户确认 + Core 再校验"），
/// 把仍缺失的必需前置经 Modrinth 下载安装。**纯逻辑，不依赖 AI**。
#[tauri::command]
#[tracing::instrument(skip(state))]
pub async fn mods_apply_plan(
    state: tauri::State<'_, AppState>,
    key: String,
) -> Result<AppliedPlanReport, IpcError> {
    let core = state.core.read().await;
    let (mods_dir, game_version, loader) = instance_mods_context(&core, &key)?;
    let source = rmccc_mods::modrinth::ModrinthSource::new(core.http_client.clone());
    drop(core);

    if !mods_dir.is_dir() {
        return Err(IpcError {
            code: rmccc_error::ErrorCode::ValidationError,
            message: format!("mods directory does not exist: {}", mods_dir.display()),
            retryable: false,
        });
    }
    let applier = ModrinthPlanApplier {
        source,
        game_version,
        loader,
    };
    // Perf audit 1.7: the async twin replaces the `block_in_place` bridge —
    // bridging pinned a runtime worker for the whole (minutes-long) install.
    let applier = &applier;
    let report =
        rmccc_agent::apply_missing_dependencies_async(&mods_dir, |mods_dir, dep| async move {
            applier.install_one(&mods_dir, &dep).await
        })
        .await?;
    tracing::info!(
        key = %key,
        installed = report.installed.len(),
        failed = report.failed.len(),
        "Missing dependencies applied"
    );
    Ok(report)
}
