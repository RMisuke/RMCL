//! Instance IPC commands — the HMCL-aligned game instance management surface.
//!
//! # id 语义（ADR-002 决策 2）
//!
//! | 命令 | 参数 |
//! |------|------|
//! | [`instance_paths`] / [`instance_check_completion`] / [`instance_repair`] | **profile id**（后两者回落版本 id） |
//! | [`instance_list`] / [`instance_refresh`] | 无参，返回**版本仓库**视图 |
//! | [`instance_rename`] / [`instance_duplicate`] / [`instance_remove`] | **版本 id**（操作 `versions/{id}/`） |

use crate::state::AppState;
use rmccc_error::IpcError;
use rmccc_version::GameRepository as _;
use serde::Serialize;
use tauri::Emitter;
use ts_rs::TS;

/// Serialized instance summary for the frontend.
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export_to = "types.ts")]
pub struct InstanceInfo {
    pub id: String,
    /// Base Minecraft version (root of the inheritance chain).
    pub game_version: String,
    /// Whether the instance carries mod loader patches.
    pub modded: bool,
    /// Whether the client JAR is present on disk.
    pub has_jar: bool,
    /// Whether the manifest JSON is present.
    pub has_json: bool,
    /// Number of missing files (client jar / libraries / assets / natives).
    #[ts(type = "number")]
    pub missing_count: usize,
    /// Detected mod loader (backend id: `forge` / `neo_forge` / `fabric` / …).
    /// Filled from HMCL `patches`, loader libraries, or the main class.
    pub loader_type: Option<String>,
    /// Detected loader version (`None` when only the kind is known).
    pub loader_version: Option<String>,
    /// Version directory on disk (`<root>/versions/<id>`).
    pub path: String,
    /// Whether the directory carries HMCL metadata (`hmclversion.cfg`).
    pub hmcl: bool,
}

/// Serialized missing-file entry for the frontend.
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export_to = "types.ts")]
pub struct MissingFileInfo {
    pub kind: String,
    pub description: String,
    pub path: String,
}

/// Resolve an instance id to its version id **and** authoritative game root.
///
/// Profiles imported from an external game directory carry `external_root` and
/// use their own `game_dir`; everything else keeps the configured game root.
async fn resolve_target(
    core: &rmccc_core::CoreHandle,
    id: &str,
) -> Result<(String, std::path::PathBuf), IpcError> {
    if let Some(profile) = core.profile_store.get(id)? {
        let root = rmccc_launch::instance_game_root(&core.config, &profile);
        return Ok((profile.game_version, root));
    }

    let known = {
        let repo = core.game_repository.read().map_err(|_| locked_error())?;
        repo.get_resolved(id).is_ok()
    };
    if known {
        return Ok((id.to_string(), core.config.resolved_game_root()));
    }

    Err(IpcError {
        code: rmccc_error::ErrorCode::ProfileNotFound,
        message: format!("No profile or installed version with id '{id}'"),
        retryable: false,
    })
}

fn join_error(e: tokio::task::JoinError) -> IpcError {
    IpcError {
        code: rmccc_error::ErrorCode::InternalError,
        message: format!("instance scan join error: {e}"),
        retryable: false,
    }
}

fn locked_error() -> IpcError {
    IpcError {
        code: rmccc_error::ErrorCode::InternalError,
        message: "Game repository lock poisoned".into(),
        retryable: false,
    }
}

#[tauri::command]
#[tracing::instrument(skip(state))]
pub async fn instance_list(
    state: tauri::State<'_, AppState>,
    root: Option<String>,
) -> Result<Vec<InstanceInfo>, IpcError> {
    // Perf audit 1.9: the listing runs a per-instance completion scan
    // (thousands of stats) — take only cheap clones under the core lock, then
    // scan on the blocking pool with no core lock held.
    match root {
        Some(root) if !root.trim().is_empty() => {
            let platform = {
                let core = state.core.read().await;
                core.platform.descriptor
            };
            // 临时仓库不会自动扫描磁盘——必须 refresh() 才有 manifests
            //（漏掉这一步就是"选了目录读不出实例"的根因）。
            tokio::task::spawn_blocking(move || -> Result<Vec<InstanceInfo>, IpcError> {
                let mut repo =
                    rmccc_version::DefaultGameRepository::new(std::path::PathBuf::from(root));
                repo.refresh().map_err(IpcError::from)?;
                Ok(list_instances(&repo, repo.game_dir(), &platform))
            })
            .await
            .map_err(join_error)?
        }
        _ => {
            let (repo, platform) = {
                let core = state.core.read().await;
                (core.game_repository.clone(), core.platform.descriptor)
            };
            tokio::task::spawn_blocking(move || {
                let repo = repo.read().map_err(|_| locked_error())?;
                Ok(list_instances(&repo, repo.game_dir(), &platform))
            })
            .await
            .map_err(join_error)?
        }
    }
}

/// Shared listing over any repository view (primary root or an extra root).
fn list_instances(
    repo: &rmccc_version::DefaultGameRepository,
    game_root: &std::path::Path,
    platform: &rmccc_core::rmccc_platform::PlatformDescriptor,
) -> Vec<InstanceInfo> {
    let ids = repo.get_instance_ids();
    let mut instances = Vec::with_capacity(ids.len());
    for id in ids {
        instances.push(instance_view(repo, game_root, platform, id));
    }
    instances
}

/// One `InstanceInfo` row for any repository (primary or an extra HMCL root).
fn instance_view(
    repo: &rmccc_version::DefaultGameRepository,
    game_root: &std::path::Path,
    platform: &rmccc_core::rmccc_platform::PlatformDescriptor,
    id: String,
) -> InstanceInfo {
    let resolved = repo.get_resolved(&id).ok();
    let standalone = resolved.as_ref().map(|r| &r.standalone);

    // HMCL standalone versions have no `inheritsFrom`; their base Minecraft
    // version lives in the `game` patch. Fall back to the chain walk for
    // launcher-installed versions.
    let mut game_version = repo.get_game_version(&id);
    if game_version == id {
        if let Some(base) = standalone.and_then(rmccc_version::loader::base_version) {
            game_version = base;
        }
    }

    let detected = standalone.and_then(rmccc_version::loader::detect_loader);
    let (loader_type, loader_version) = match detected {
        Some(loader) => (
            Some(loader.kind),
            if loader.version.is_empty() {
                None
            } else {
                Some(loader.version)
            },
        ),
        None => (None, None),
    };

    let has_json = rmccc_version::file_layout::instance_json(game_root, &id).exists();
    let has_jar = rmccc_version::file_layout::instance_jar(game_root, &id).exists();
    let missing_count = resolved
        .as_ref()
        .map(|r| {
            rmccc_version::completion::check_completion(
                game_root,
                r,
                platform,
                rmccc_version::completion::CompletionVerify::Size,
            )
            .len()
        })
        .unwrap_or(1);

    let version_dir = game_root.join("versions").join(&id);
    let modded = repo.is_modded(&id);
    InstanceInfo {
        id,
        game_version,
        modded,
        has_jar,
        has_json,
        missing_count,
        loader_type,
        loader_version,
        path: version_dir.display().to_string(),
        hmcl: version_dir.join("hmclversion.cfg").exists(),
    }
}

#[tauri::command]
#[tracing::instrument(skip(state))]
pub async fn instance_refresh(
    state: tauri::State<'_, AppState>,
) -> Result<Vec<InstanceInfo>, IpcError> {
    let app_handle = state.app_handle.clone();
    // Perf audit 1.9: refresh (full JSON re-parse) + per-instance completion
    // scan run on the blocking pool; the core read lock is held only to clone
    // the repository handle.
    let (repo, platform) = {
        let core = state.core.read().await;
        (core.game_repository.clone(), core.platform.descriptor)
    };
    let instances = tokio::task::spawn_blocking(move || -> Result<Vec<InstanceInfo>, IpcError> {
        {
            let mut repo = repo.write().map_err(|_| locked_error())?;
            repo.refresh().map_err(IpcError::from)?;
        }
        let repo = repo.read().map_err(|_| locked_error())?;
        Ok(list_instances(&repo, repo.game_dir(), &platform))
    })
    .await
    .map_err(join_error)??;

    let _ = app_handle.emit(
        crate::events::INSTANCES_REFRESHED,
        serde_json::json!({ "count": instances.len() }),
    );
    Ok(instances)
}

#[tauri::command]
#[tracing::instrument(skip(state))]
pub async fn instance_rename(
    state: tauri::State<'_, AppState>,
    id: String,
    new_id: String,
) -> Result<(), IpcError> {
    if new_id.trim().is_empty() {
        return Err(IpcError {
            code: rmccc_error::ErrorCode::ValidationError,
            message: "New instance id cannot be empty".into(),
            retryable: false,
        });
    }
    let core = state.core.read().await;
    core.game_repository
        .write()
        .map_err(|_| locked_error())?
        .rename_instance(&id, &new_id)?;
    Ok(())
}

#[tauri::command]
#[tracing::instrument(skip(state))]
pub async fn instance_duplicate(
    state: tauri::State<'_, AppState>,
    id: String,
    new_id: String,
) -> Result<(), IpcError> {
    if new_id.trim().is_empty() {
        return Err(IpcError {
            code: rmccc_error::ErrorCode::ValidationError,
            message: "New instance id cannot be empty".into(),
            retryable: false,
        });
    }
    let core = state.core.read().await;
    core.game_repository
        .write()
        .map_err(|_| locked_error())?
        .duplicate_instance(&id, &new_id)?;
    Ok(())
}

#[tauri::command]
#[tracing::instrument(skip(state))]
pub async fn instance_remove(
    state: tauri::State<'_, AppState>,
    id: String,
) -> Result<(), IpcError> {
    let core = state.core.read().await;
    core.game_repository
        .write()
        .map_err(|_| locked_error())?
        .remove_instance(&id)?;
    Ok(())
}

#[tauri::command]
#[tracing::instrument(skip(state))]
pub async fn instance_check_completion(
    state: tauri::State<'_, AppState>,
    id: String,
) -> Result<Vec<MissingFileInfo>, IpcError> {
    // A2 同款：handle 快照，不持 core 读锁（completion 检查含全量哈希读盘）。
    let core = {
        let core = state.core.read().await;
        core.handle()
    };
    let (version_id, game_root) = resolve_target(&core, &id).await?;
    let resolved = if game_root == core.config.resolved_game_root() {
        let repo = core.game_repository.read().map_err(|_| locked_error())?;
        repo.get_resolved(&version_id)
            .map_err(|e| IpcError::from(rmccc_error::RmcccError::Other(e.to_string())))?
    } else {
        let root = game_root.clone();
        let vid = version_id.clone();
        tokio::task::spawn_blocking(
            move || -> Result<rmccc_version::ResolvedVersion, IpcError> {
                let mut repo = rmccc_version::DefaultGameRepository::new(root);
                repo.refresh().map_err(IpcError::from)?;
                repo.get_resolved(&vid)
                    .map_err(|e| IpcError::from(rmccc_error::RmcccError::Other(e.to_string())))
            },
        )
        .await
        .map_err(join_error)??
    };

    // 显式检查命令：全量 SHA1 复核（启动路径只用 Size 级）。
    let missing = rmccc_version::completion::check_completion(
        &game_root,
        &resolved,
        &core.platform.descriptor,
        rmccc_version::completion::CompletionVerify::Hash,
    );
    let info = missing
        .into_iter()
        .map(|m| MissingFileInfo {
            kind: format!("{:?}", m.kind),
            description: m.description,
            path: m.path.display().to_string(),
        })
        .collect();
    Ok(info)
}

#[tauri::command]
#[tracing::instrument(skip(state))]
pub async fn instance_repair(
    state: tauri::State<'_, AppState>,
    id: String,
) -> Result<usize, IpcError> {
    // P1-3：修复编排收归 `pipeline::repair_instance`（与 CLI 的
    // `resource repair`、启动前的 `ensure_launch_ready` 同一份实现）。
    // A2：修复含分钟级下载——短锁取 handle 快照后立即释放，全程无 core 锁。
    let core = {
        let core = state.core.read().await;
        core.handle()
    };
    let (version_id, game_root) = resolve_target(&core, &id).await?;
    let report = rmccc_core::pipeline::repair_instance_at(
        &core,
        &version_id,
        &game_root,
        None,
        rmccc_version::completion::CompletionVerify::Hash,
    )
    .await?;

    if !report.is_complete() {
        return Err(IpcError {
            code: rmccc_error::ErrorCode::DownloadFailed,
            message: format!(
                "{} of {} files failed to download",
                report.failed_files, report.total_files
            ),
            retryable: true,
        });
    }

    tracing::info!(id = %id, version = %version_id, files = report.total_files, "Instance repair completed");
    Ok(report.total_files)
}

/// Effective (inherited-or-overridden) launch values for an instance.
///
/// 前端设置页用它显示"该字段当前继承时生效值是多少"，从而不必在 TS 里再实现
/// 一遍继承规则（审计 P2-8）。
#[tauri::command]
#[tracing::instrument(skip(state))]
pub async fn instance_resolved_settings(
    state: tauri::State<'_, AppState>,
    id: String,
) -> Result<rmccc_profile::ResolvedInstanceSettings, IpcError> {
    let core = state.core.read().await;
    let profile = core.profile_store.get(&id)?.ok_or_else(|| IpcError {
        code: rmccc_error::ErrorCode::ProfileNotFound,
        message: format!("Profile not found: {id}"),
        retryable: false,
    })?;
    Ok(profile.resolve(&core.config))
}

/// Effective on-disk locations for an instance, so the settings page can show
/// where the run directory and mods actually are (they depend on isolation).
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export_to = "types.ts")]
pub struct InstancePaths {
    /// Shared game root (`.minecraft` equivalent).
    pub game_root: String,
    /// `--gameDir` / process working directory for this instance.
    pub run_dir: String,
    /// Where this instance's mods live.
    pub mods_dir: String,
    /// Normalised isolation value: `never` | `modded` | `always`.
    pub isolation: String,
    /// Whether the run directory differs from the shared game root.
    pub isolated: bool,
}

#[tauri::command]
#[tracing::instrument(skip(state))]
pub async fn instance_paths(
    state: tauri::State<'_, AppState>,
    id: String,
) -> Result<InstancePaths, IpcError> {
    let core = state.core.read().await;
    let profile = core.profile_store.get(&id)?.ok_or_else(|| IpcError {
        code: rmccc_error::ErrorCode::ProfileNotFound,
        message: format!("Profile not found: {id}"),
        retryable: false,
    })?;

    let game_root = rmccc_launch::instance_game_root(&core.config, &profile);
    let run_dir = rmccc_launch::instance_run_dir(&core.config, &profile);
    let mods_dir = rmccc_launch::instance_mods_dir(&core.config, &profile);

    Ok(InstancePaths {
        isolated: run_dir != game_root,
        game_root: game_root.display().to_string(),
        run_dir: run_dir.display().to_string(),
        mods_dir: mods_dir.display().to_string(),
        isolation: profile.effective_isolation(&core.config),
    })
}
