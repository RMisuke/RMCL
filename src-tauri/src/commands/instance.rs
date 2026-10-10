//! Instance IPC commands — the HMCL-aligned game instance management surface.
//!
//! These commands operate on the local [`GameRepository`] (indexed from
//! `versions/` under the game root): listing, refreshing, renaming,
//! duplicating, removing, and completeness-checking / repairing instances.

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
            .map(|r| rmccc_version::completion::check_completion(&game_root, r).len())
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
                .map(|r| rmccc_version::completion::check_completion(&game_root, r).len())
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
    let repo = core.game_repository.read().map_err(|_| locked_error())?;
    let game_root = repo.game_dir().to_path_buf();
    let resolved = repo
        .get_resolved(&id)
        .map_err(|e| IpcError::from(rmccc_error::RmcccError::Other(e.to_string())))?;
    let missing = rmccc_version::completion::check_completion(&game_root, &resolved);
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
    // Collect the resolved manifest + game root under the lock, then release it
    // before awaiting downloads (std::sync::RwLock guards are not Send).
    let (game_root, resolved, http_client) = {
        let core = state.core.read().await;
        let repo = core.game_repository.read().map_err(|_| locked_error())?;
        let game_root = repo.game_dir().to_path_buf();
        let resolved = repo
            .get_resolved(&id)
            .map_err(|e| IpcError::from(rmccc_error::RmcccError::Other(e.to_string())))?;
        (game_root, resolved, core.http_client.clone())
    };

    let items =
        rmccc_version::completion::build_completion_downloads(&http_client, &game_root, &resolved)
            .await?;

    if items.is_empty() {
        tracing::info!(id = %id, "Instance is complete — nothing to repair");
        return Ok(0);
    }

    let count = items.len();
    let core = state.core.read().await;
    core.download_manager
        .download_batch(items, std::sync::Arc::new(|_| {}))
        .await?;

    // Re-scan the repository so missing counts refresh.
    core.game_repository
        .write()
        .map_err(|_| locked_error())?
        .refresh()?;

    tracing::info!(id = %id, files = count, "Instance repair completed");
    Ok(count)
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
