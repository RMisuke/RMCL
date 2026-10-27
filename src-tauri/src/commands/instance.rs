//! Instance IPC commands — game directories, version-backed instances, settings.
//!
//! Every command takes the composite `{directory_id}/{instance_id}` key (or the
//! two parts explicitly), so the same version id may exist in several roots
//! without ambiguity (ADR-016).

use crate::state::AppState;
use rmccc_error::IpcError;
pub use rmccc_instance::{
    DirectoryCandidate, GameDirectory, GameDirectoryKind, GameDirectoryOverview,
    GlobalGameSettings, InstanceDetail, InstancePaths, InstanceSettingsPatch, InstanceSummary,
    ResolvedInstanceSettings,
};
use serde::Serialize;
use std::path::PathBuf;
use std::sync::Arc;
use tauri::Emitter;
use ts_rs::TS;

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
        message: "Instance service lock poisoned".into(),
        retryable: false,
    }
}

// ── Game directories ────────────────────────────────────────────────

#[tauri::command]
#[tracing::instrument(skip(state))]
pub async fn game_directory_list(
    state: tauri::State<'_, AppState>,
) -> Result<Vec<GameDirectory>, IpcError> {
    let core = state.core.read().await;
    let service = core.instances.lock().map_err(|_| locked_error())?;
    Ok(service.directories().to_vec())
}

#[tauri::command]
#[tracing::instrument(skip(state))]
pub async fn game_directory_add(
    state: tauri::State<'_, AppState>,
    path: String,
    name: Option<String>,
    external: Option<bool>,
) -> Result<GameDirectory, IpcError> {
    let core = state.core.read().await;
    let mut service = core.instances.lock().map_err(|_| locked_error())?;
    let kind = if external.unwrap_or(true) {
        GameDirectoryKind::External
    } else {
        GameDirectoryKind::Managed
    };
    let path = PathBuf::from(path);
    let name = name.filter(|n| !n.trim().is_empty()).unwrap_or_else(|| {
        path.file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("Game directory")
            .to_string()
    });
    Ok(service.add_directory(name, &path, kind)?)
}

#[tauri::command]
#[tracing::instrument(skip(state))]
pub async fn game_directory_update(
    state: tauri::State<'_, AppState>,
    id: String,
    name: Option<String>,
    path: Option<String>,
) -> Result<GameDirectory, IpcError> {
    let core = state.core.read().await;
    let mut service = core.instances.lock().map_err(|_| locked_error())?;
    Ok(service.update_directory(&id, name, path.map(PathBuf::from))?)
}

#[tauri::command]
#[tracing::instrument(skip(state))]
pub async fn game_directory_remove(
    state: tauri::State<'_, AppState>,
    id: String,
) -> Result<(), IpcError> {
    let core = state.core.read().await;
    let mut service = core.instances.lock().map_err(|_| locked_error())?;
    service.remove_directory(&id)?;
    Ok(())
}

#[tauri::command]
#[tracing::instrument(skip(state))]
pub async fn game_directory_selected(
    state: tauri::State<'_, AppState>,
) -> Result<GameDirectory, IpcError> {
    let core = state.core.read().await;
    let service = core.instances.lock().map_err(|_| locked_error())?;
    Ok(service.selected_directory().clone())
}

#[tauri::command]
#[tracing::instrument(skip(state))]
pub async fn game_directory_select(
    state: tauri::State<'_, AppState>,
    id: String,
) -> Result<GameDirectory, IpcError> {
    let core = state.core.read().await;
    let mut service = core.instances.lock().map_err(|_| locked_error())?;
    Ok(service.select_directory(&id)?)
}

#[tauri::command]
#[tracing::instrument(skip(state))]
pub async fn game_directory_overview(
    state: tauri::State<'_, AppState>,
) -> Result<Vec<GameDirectoryOverview>, IpcError> {
    let core = state.core.read().await;
    let mut service = core.instances.lock().map_err(|_| locked_error())?;
    Ok(service.overviews())
}

/// Scan standard launcher locations (HMCL / PCL / official `.minecraft`).
#[tauri::command]
#[tracing::instrument(skip(state))]
pub async fn game_directory_scan(
    state: tauri::State<'_, AppState>,
) -> Result<Vec<DirectoryCandidate>, IpcError> {
    let core = state.core.read().await;
    let service = core.instances.lock().map_err(|_| locked_error())?;
    Ok(service.scan_directories())
}

/// Reveal a registered game directory in the system file manager.
#[tauri::command]
#[tracing::instrument(skip(state))]
pub async fn game_directory_reveal(
    state: tauri::State<'_, AppState>,
    id: String,
) -> Result<(), IpcError> {
    let path = {
        let core = state.core.read().await;
        let service = core.instances.lock().map_err(|_| locked_error())?;
        service.directory(&id)?.path.clone()
    };
    tauri_plugin_opener::open_path(&path, None::<&str>).map_err(|e| IpcError {
        code: rmccc_error::ErrorCode::Unknown,
        message: format!("Failed to open game directory: {e}"),
        retryable: false,
    })
}

// ── Instances ───────────────────────────────────────────────────────

#[tauri::command]
#[tracing::instrument(skip(state))]
pub async fn instance_list(
    state: tauri::State<'_, AppState>,
    directory_id: Option<String>,
    all: Option<bool>,
    include_hidden: Option<bool>,
) -> Result<Vec<InstanceDetail>, IpcError> {
    let core = state.core.read().await;
    let mut service = core.instances.lock().map_err(|_| locked_error())?;
    Ok(service.list_details(
        directory_id.as_deref(),
        all.unwrap_or(false),
        include_hidden.unwrap_or(false),
    )?)
}

#[tauri::command]
#[tracing::instrument(skip(state))]
pub async fn instance_get(
    state: tauri::State<'_, AppState>,
    key: String,
) -> Result<InstanceDetail, IpcError> {
    let core = state.core.read().await;
    let mut service = core.instances.lock().map_err(|_| locked_error())?;
    Ok(service.get(&key)?)
}

#[tauri::command]
#[tracing::instrument(skip(state))]
pub async fn instance_refresh(
    state: tauri::State<'_, AppState>,
    directory_id: Option<String>,
    include_hidden: Option<bool>,
) -> Result<Vec<InstanceSummary>, IpcError> {
    let instances = {
        let core = state.core.read().await;
        let mut service = core.instances.lock().map_err(|_| locked_error())?;
        service.refresh(directory_id.as_deref(), include_hidden.unwrap_or(false))?
    };
    let _ = state.app_handle.emit(
        crate::events::INSTANCES_REFRESHED,
        serde_json::json!({ "count": instances.len() }),
    );
    Ok(instances)
}

#[tauri::command]
#[tracing::instrument(skip(state))]
pub async fn instance_update_settings(
    state: tauri::State<'_, AppState>,
    key: String,
    opts: InstanceSettingsPatch,
) -> Result<InstanceDetail, IpcError> {
    let core = state.core.read().await;
    let mut service = core.instances.lock().map_err(|_| locked_error())?;
    Ok(service.update_settings(&key, opts)?)
}

#[tauri::command]
#[tracing::instrument(skip(state))]
pub async fn instance_resolved_settings(
    state: tauri::State<'_, AppState>,
    key: String,
) -> Result<ResolvedInstanceSettings, IpcError> {
    let core = state.core.read().await;
    let mut service = core.instances.lock().map_err(|_| locked_error())?;
    Ok(service.resolved_settings(&key)?)
}

#[tauri::command]
#[tracing::instrument(skip(state))]
pub async fn instance_paths(
    state: tauri::State<'_, AppState>,
    key: String,
) -> Result<InstancePaths, IpcError> {
    let core = state.core.read().await;
    let mut service = core.instances.lock().map_err(|_| locked_error())?;
    Ok(service.paths(&key)?)
}

#[tauri::command]
#[tracing::instrument(skip(state))]
pub async fn instance_select(
    state: tauri::State<'_, AppState>,
    key: Option<String>,
) -> Result<(), IpcError> {
    let core = state.core.read().await;
    let mut service = core.instances.lock().map_err(|_| locked_error())?;
    service.select_instance(key.as_deref())?;
    Ok(())
}

#[tauri::command]
#[tracing::instrument(skip(state))]
pub async fn instance_rename(
    state: tauri::State<'_, AppState>,
    key: String,
    new_id: String,
) -> Result<InstanceDetail, IpcError> {
    let core = state.core.read().await;
    let mut service = core.instances.lock().map_err(|_| locked_error())?;
    let detail = service.rename(&key, &new_id)?;
    drop(service);
    let _ = state.app_handle.emit(
        crate::events::INSTANCES_REFRESHED,
        serde_json::json!({ "count": 1 }),
    );
    Ok(detail)
}

#[tauri::command]
#[tracing::instrument(skip(state))]
pub async fn instance_duplicate(
    state: tauri::State<'_, AppState>,
    key: String,
    new_id: String,
    copy_saves: Option<bool>,
) -> Result<InstanceDetail, IpcError> {
    let core = state.core.read().await;
    let mut service = core.instances.lock().map_err(|_| locked_error())?;
    let detail = service.duplicate(&key, &new_id, copy_saves.unwrap_or(false))?;
    drop(service);
    let _ = state.app_handle.emit(
        crate::events::INSTANCES_REFRESHED,
        serde_json::json!({ "count": 1 }),
    );
    Ok(detail)
}

#[tauri::command]
#[tracing::instrument(skip(state))]
pub async fn instance_remove(
    state: tauri::State<'_, AppState>,
    key: String,
    delete_files: Option<bool>,
) -> Result<(), IpcError> {
    let core = state.core.read().await;
    let mut service = core.instances.lock().map_err(|_| locked_error())?;
    service.remove(&key, delete_files.unwrap_or(false))?;
    drop(service);
    let _ = state.app_handle.emit(
        crate::events::INSTANCES_REFRESHED,
        serde_json::json!({ "count": 1 }),
    );
    Ok(())
}

/// Ensure an already-installed version becomes a first-class instance.
#[tauri::command]
#[tracing::instrument(skip(state))]
pub async fn instance_import(
    state: tauri::State<'_, AppState>,
    directory_id: String,
    id: String,
) -> Result<InstanceDetail, IpcError> {
    let core = state.core.read().await;
    let mut service = core.instances.lock().map_err(|_| locked_error())?;
    Ok(service.ensure_instance(&directory_id, &id)?)
}

// ── Completeness ────────────────────────────────────────────────────

#[tauri::command]
#[tracing::instrument(skip(state))]
pub async fn instance_check_completion(
    state: tauri::State<'_, AppState>,
    key: String,
) -> Result<Vec<MissingFileInfo>, IpcError> {
    let core = {
        let core = state.core.read().await;
        core.handle()
    };
    let (detail, resolved) = {
        let mut service = core.instances.lock().map_err(|_| locked_error())?;
        service.resolved_version(&key)?
    };

    // 显式检查命令：全量 SHA1 复核（启动路径只用 Size 级）。
    let missing = rmccc_version::completion::check_completion(
        &detail.paths.game_dir,
        &resolved,
        &core.platform.descriptor,
        rmccc_version::completion::CompletionVerify::Hash,
    );
    Ok(missing
        .into_iter()
        .map(|m| MissingFileInfo {
            kind: format!("{:?}", m.kind),
            description: m.description,
            path: m.path.display().to_string(),
        })
        .collect())
}

#[tauri::command]
#[tracing::instrument(skip(state))]
pub async fn instance_repair(
    state: tauri::State<'_, AppState>,
    key: String,
) -> Result<usize, IpcError> {
    let core = {
        let core = state.core.read().await;
        core.handle()
    };
    let detail = {
        let mut service = core.instances.lock().map_err(|_| locked_error())?;
        service.get(&key)?
    };

    let report = rmccc_core::pipeline::repair_instance_at(
        &core,
        &detail.summary.id,
        &detail.paths.game_dir,
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

    if let Ok(mut service) = core.instances.lock() {
        let _ = service.refresh(Some(detail.summary.directory_id.as_str()), true);
    }
    tracing::info!(key = %key, files = report.total_files, "Instance repair completed");
    Ok(report.total_files)
}

// ── Installation ────────────────────────────────────────────────────

#[tauri::command]
#[tracing::instrument(skip(state), fields(request_id = %uuid::Uuid::new_v4()))]
pub async fn instance_install(
    state: tauri::State<'_, AppState>,
    directory_id: String,
    mc_version: String,
    instance_id: Option<String>,
    loader_type: Option<String>,
    loader_version: Option<String>,
) -> Result<(), IpcError> {
    let instance_id = instance_id
        .filter(|id| !id.trim().is_empty())
        .unwrap_or_else(|| mc_version.clone());
    rmccc_instance::validate_instance_id(&instance_id)?;

    let core = {
        let core = state.core.read().await;
        core.handle()
    };
    let game_dir = {
        let service = core.instances.lock().map_err(|_| locked_error())?;
        service.game_dir(&directory_id)?
    };
    let target_json = rmccc_version::file_layout::instance_json(&game_dir, &instance_id);
    if target_json.exists() {
        return Err(IpcError {
            code: rmccc_error::ErrorCode::ValidationError,
            message: format!("instance '{instance_id}' already exists in this directory"),
            retryable: false,
        });
    }

    let app_handle = state.app_handle.clone();
    let task_handle = app_handle.clone();
    tokio::spawn(async move {
        if let Err(e) = run_install_background(
            core,
            task_handle.clone(),
            InstallRequest {
                directory_id: directory_id.clone(),
                game_dir,
                mc_version: mc_version.clone(),
                instance_id: instance_id.clone(),
                loader_type: loader_type.clone(),
                loader_version: loader_version.clone(),
            },
        )
        .await
        {
            tracing::error!(
                directory = %directory_id,
                instance = %instance_id,
                version = %mc_version,
                "Instance install failed: {e:?}"
            );
            let _ = task_handle.emit(
                crate::events::DOWNLOAD_ERROR,
                serde_json::json!({
                    "batch_id": "",
                    "directory_id": directory_id,
                    "instance_id": instance_id,
                    "version": mc_version,
                    "error": e.message,
                }),
            );
        }
    });

    Ok(())
}

struct InstallRequest {
    directory_id: String,
    game_dir: PathBuf,
    mc_version: String,
    instance_id: String,
    loader_type: Option<String>,
    loader_version: Option<String>,
}

async fn run_install_background(
    core: rmccc_core::CoreHandle,
    app_handle: tauri::AppHandle,
    request: InstallRequest,
) -> Result<(), IpcError> {
    let InstallRequest {
        directory_id,
        game_dir,
        mc_version,
        instance_id,
        loader_type,
        loader_version,
    } = request;
    let last_batch_id = Arc::new(std::sync::Mutex::new(None::<String>));
    let progress_handle = app_handle.clone();
    let emitter = Arc::new(crate::events::ThrottledEmitter::new(
        app_handle.clone(),
        core.config.event_throttle_ms,
    ));
    let cb_last_batch = last_batch_id.clone();
    let log_key = format!("{directory_id}/{instance_id}");
    let key_for_progress = log_key.clone();
    let progress_cb: rmccc_core::pipeline::ProgressCb =
        Arc::new(move |msg: &rmccc_core::pipeline::ProgressMsg| match msg {
            rmccc_core::pipeline::ProgressMsg::Download { progress, .. } => {
                *cb_last_batch.lock().unwrap() = Some(progress.batch_id.clone());
                emitter.emit_progress(&crate::events::DownloadProgressPayload {
                    batch_id: progress.batch_id.clone(),
                    completed_files: progress.completed_files,
                    total_files: progress.total_files,
                    downloaded_bytes: progress.downloaded_bytes,
                    total_bytes: progress.total_bytes,
                    current_file: progress.current_file.clone(),
                });
                let _ = progress_handle.emit(
                    crate::events::DOWNLOAD_PROGRESS,
                    serde_json::json!({
                        "batch_id": progress.batch_id,
                        "completed_files": progress.completed_files,
                        "total_files": progress.total_files,
                        "downloaded_bytes": progress.downloaded_bytes,
                        "total_bytes": progress.total_bytes,
                        "current_file": progress.current_file,
                        "key": key_for_progress,
                    }),
                );
            }
            rmccc_core::pipeline::ProgressMsg::Stage(stage)
            | rmccc_core::pipeline::ProgressMsg::StageDone(stage) => {
                tracing::debug!(key = %log_key, "{stage}");
            }
        });

    let report =
        rmccc_core::pipeline::install_game(&core, &game_dir, &instance_id, Some(&progress_cb))
            .await?;
    let real_batch_id = last_batch_id.lock().unwrap().clone().unwrap_or_default();

    if !report.is_complete() {
        let _ = app_handle.emit(
            crate::events::DOWNLOAD_ERROR,
            serde_json::json!({
                "batch_id": real_batch_id,
                "directory_id": directory_id,
                "instance_id": instance_id,
                "version": mc_version,
                "failed_files": report.failed_files,
            }),
        );
        return Ok(());
    }

    let mut detail = rmccc_core::pipeline::commit_install(&core, &directory_id, &instance_id)?;
    {
        let mut service = core.instances.lock().map_err(|_| locked_error())?;
        service.select_instance(Some(&detail.summary.key))?;
    }

    // Optional loader: run it after the vanilla instance is fully committed so
    // the patch/installer can resolve its base manifest offline.
    if let (Some(loader_type), Some(loader_version)) = (loader_type, loader_version) {
        let Some(loader) = rmccc_modloader::ModLoaderType::from_str(&loader_type) else {
            let _ = app_handle.emit(
                crate::events::DOWNLOAD_ERROR,
                serde_json::json!({
                    "batch_id": real_batch_id,
                    "directory_id": directory_id,
                    "instance_id": instance_id,
                    "version": mc_version,
                    "error": format!("unknown loader type: {loader_type}"),
                }),
            );
            return Ok(());
        };
        match rmccc_core::modloader::install_loader(&core, &detail, &loader, &loader_version, None)
            .await
        {
            Ok(installed) => {
                if let Ok(mut service) = core.instances.lock() {
                    let _ = service.select_instance(Some(&installed.instance_key));
                    if let Ok(updated) = service.get(&installed.instance_key) {
                        detail = updated;
                    }
                }
            }
            Err(e) => {
                let _ = app_handle.emit(
                    crate::events::DOWNLOAD_ERROR,
                    serde_json::json!({
                        "batch_id": real_batch_id,
                        "directory_id": directory_id,
                        "instance_id": instance_id,
                        "version": mc_version,
                        "error": format!("loader install failed: {e}"),
                    }),
                );
                return Ok(());
            }
        }
    }

    let _ = app_handle.emit(
        crate::events::DOWNLOAD_COMPLETE,
        serde_json::json!({
            "batch_id": real_batch_id,
            "directory_id": directory_id,
            "instance_id": instance_id,
            "key": detail.summary.key,
            "version": mc_version,
        }),
    );
    let _ = app_handle.emit(
        crate::events::INSTANCES_REFRESHED,
        serde_json::json!({ "key": detail.summary.key, "version": mc_version }),
    );
    tracing::info!(
        key = %detail.summary.key,
        version = %mc_version,
        "Instance install complete"
    );
    Ok(())
}

// ── Global preset ───────────────────────────────────────────────────

#[tauri::command]
#[tracing::instrument(skip(state))]
pub async fn instance_global_settings(
    state: tauri::State<'_, AppState>,
) -> Result<GlobalGameSettings, IpcError> {
    let core = state.core.read().await;
    let service = core.instances.lock().map_err(|_| locked_error())?;
    Ok(service.global_settings().clone())
}

#[tauri::command]
#[tracing::instrument(skip(state))]
pub async fn instance_update_global_settings(
    state: tauri::State<'_, AppState>,
    settings: GlobalGameSettings,
) -> Result<GlobalGameSettings, IpcError> {
    let core = state.core.read().await;
    let mut service = core.instances.lock().map_err(|_| locked_error())?;
    service.update_global_settings(settings)?;
    Ok(service.global_settings().clone())
}
