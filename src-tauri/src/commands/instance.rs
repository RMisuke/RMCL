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
}

/// Serialized missing-file entry for the frontend.
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export_to = "types.ts")]
pub struct MissingFileInfo {
    pub kind: String,
    pub description: String,
    pub path: String,
}

/// 把 `instance_*` 的 id 解析成**版本 id**（ADR-002 决策 2）。
///
/// 先按 profile id 查（UI 传的就是它），命中则用该实例的 `game_version`；
/// 未命中再当作版本 id 直接查仓库；两者都不存在才报 `ProfileNotFound`。
async fn resolve_version_id(core: &rmccc_core::CoreContext, id: &str) -> Result<String, IpcError> {
    if let Some(profile) = core.profile_store.get(id)? {
        return Ok(profile.game_version);
    }

    let known = {
        let repo = core.game_repository.read().map_err(|_| locked_error())?;
        repo.get_resolved(id).is_ok()
    };
    if known {
        return Ok(id.to_string());
    }

    Err(IpcError {
        code: rmccc_error::ErrorCode::ProfileNotFound,
        message: format!("No profile or installed version with id '{id}'"),
        retryable: false,
    })
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
) -> Result<Vec<InstanceInfo>, IpcError> {
    let core = state.core.read().await;
    let repo = core.game_repository.read().map_err(|_| locked_error())?;
    let game_root = repo.game_dir().to_path_buf();

    let ids = repo.get_instance_ids();
    let mut instances = Vec::with_capacity(ids.len());
    for id in ids {
        let resolved = repo.get_resolved(&id).ok();
        let game_version = repo.get_game_version(&id);
        let modded = repo.is_modded(&id);
        let has_json = rmccc_version::file_layout::instance_json(&game_root, &id).exists();
        let has_jar = rmccc_version::file_layout::instance_jar(&game_root, &id).exists();
        let missing_count = resolved
            .as_ref()
            .map(|r| {
                rmccc_version::completion::check_completion(
                    &game_root,
                    r,
                    &core.platform.descriptor,
                )
                .len()
            })
            .unwrap_or(1);
        instances.push(InstanceInfo {
            id,
            game_version,
            modded,
            has_jar,
            has_json,
            missing_count,
        });
    }
    Ok(instances)
}

#[tauri::command]
#[tracing::instrument(skip(state))]
pub async fn instance_refresh(
    state: tauri::State<'_, AppState>,
) -> Result<Vec<InstanceInfo>, IpcError> {
    let app_handle = state.app_handle.clone();
    let core = state.core.read().await;
    {
        let mut repo = core.game_repository.write().map_err(|_| locked_error())?;
        repo.refresh()?;
    }

    // Broadcast the refreshed instance set (HMCL RefreshedGameInstancesEvent).
    let instances = {
        let repo = core.game_repository.read().map_err(|_| locked_error())?;
        let game_root = repo.game_dir().to_path_buf();
        let ids = repo.get_instance_ids();
        let mut list = Vec::with_capacity(ids.len());
        for id in ids {
            let resolved = repo.get_resolved(&id).ok();
            let missing_count = resolved
                .as_ref()
                .map(|r| {
                    rmccc_version::completion::check_completion(
                        &game_root,
                        r,
                        &core.platform.descriptor,
                    )
                    .len()
                })
                .unwrap_or(1);
            let game_version = repo.get_game_version(&id);
            let modded = repo.is_modded(&id);
            let has_jar = rmccc_version::file_layout::instance_jar(&game_root, &id).exists();
            let has_json = rmccc_version::file_layout::instance_json(&game_root, &id).exists();
            list.push(InstanceInfo {
                id,
                game_version,
                modded,
                has_jar,
                has_json,
                missing_count,
            });
        }
        list
    };

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
    let core = state.core.read().await;
    let version_id = resolve_version_id(&core, &id).await?;

    let repo = core.game_repository.read().map_err(|_| locked_error())?;
    let game_root = repo.game_dir().to_path_buf();
    let resolved = repo
        .get_resolved(&version_id)
        .map_err(|e| IpcError::from(rmccc_error::RmcccError::Other(e.to_string())))?;
    let missing = rmccc_version::completion::check_completion(
        &game_root,
        &resolved,
        &core.platform.descriptor,
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
    let core = state.core.read().await;
    let version_id = resolve_version_id(&core, &id).await?;
    let report = rmccc_core::pipeline::repair_instance(&core, &version_id, None).await?;

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

    let game_root = core.config.resolved_game_root();
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
