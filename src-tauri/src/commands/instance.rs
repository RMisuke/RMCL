//! Instance IPC commands — game directories, version-backed instances, settings.
//!
//! Every command takes the composite `{directory_id}/{instance_id}` key (or the
//! two parts explicitly), so the same version id may exist in several roots
//! without ambiguity (ADR-016).

use crate::commands::tasks::TaskStart;
use crate::state::AppState;
use rmccc_error::IpcError;
pub use rmccc_instance::{
    CacheCleanReport, DirectoryCandidate, GameDirectory, GameDirectoryKind, GameDirectoryOverview,
    GlobalGameSettings, InstanceDetail, InstancePaths, InstanceSettingsPatch, InstanceStorage,
    InstanceSummary, ResolvedInstanceSettings, StorageOverview,
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

// ── Storage management ─────────────────────────────────────────────

fn blocking_error(e: impl std::fmt::Display) -> IpcError {
    IpcError {
        code: rmccc_error::ErrorCode::InternalError,
        message: format!("storage task failed: {e}"),
        retryable: false,
    }
}

#[tauri::command]
#[tracing::instrument(skip(state))]
pub async fn storage_overview(
    state: tauri::State<'_, AppState>,
) -> Result<StorageOverview, IpcError> {
    let (data_dir, directories) = {
        let core = state.core.read().await;
        let service = core.instances.lock().map_err(|_| locked_error())?;
        (
            service.data_dir().to_path_buf(),
            service.directories().to_vec(),
        )
    };
    tauri::async_runtime::spawn_blocking(move || {
        rmccc_instance::storage_overview(&data_dir, &directories)
    })
    .await
    .map_err(blocking_error)
}

/// Remove only the regenerable cache (`{data_dir}/cache`).
#[tauri::command]
#[tracing::instrument(skip(state))]
pub async fn storage_clean_cache(
    state: tauri::State<'_, AppState>,
) -> Result<CacheCleanReport, IpcError> {
    let data_dir = {
        let core = state.core.read().await;
        let service = core.instances.lock().map_err(|_| locked_error())?;
        service.data_dir().to_path_buf()
    };
    tauri::async_runtime::spawn_blocking(move || rmccc_instance::clean_caches(&data_dir))
        .await
        .map_err(blocking_error)
}

/// Per-instance sizes for the instance cards (selected directory by default).
#[tauri::command]
#[tracing::instrument(skip(state))]
pub async fn instance_sizes(
    state: tauri::State<'_, AppState>,
    directory_id: Option<String>,
) -> Result<Vec<InstanceStorage>, IpcError> {
    let directory = {
        let core = state.core.read().await;
        let service = core.instances.lock().map_err(|_| locked_error())?;
        match directory_id.as_deref() {
            Some(id) => service.directory(id)?.clone(),
            None => service.selected_directory().clone(),
        }
    };
    tauri::async_runtime::spawn_blocking(move || rmccc_instance::instances_in(&directory))
        .await
        .map_err(blocking_error)
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

/// Resolve an HMCL-style folder id to a path under the instance.
///
/// Keeping this match in one place is the security boundary: the frontend
/// sends a symbolic id, never a path. Unknown ids are rejected.
fn instance_folder_path(paths: &InstancePaths, folder: &str) -> Option<PathBuf> {
    Some(match folder {
        "run" => paths.run_dir.clone(),
        "instance" => paths.instance_dir.clone(),
        "game" => paths.game_dir.clone(),
        "mods" => paths.mods_dir.clone(),
        "resourcepacks" => paths.resourcepacks_dir.clone(),
        "saves" => paths.saves_dir.clone(),
        "schematics" => paths.run_dir.join("schematics"),
        "shaderpacks" => paths.shaderpacks_dir.clone(),
        "screenshots" => paths.screenshots_dir.clone(),
        "config" => paths.config_dir.clone(),
        "logs" => paths.logs_dir.clone(),
        "crash-reports" => paths.run_dir.join("crash-reports"),
        _ => return None,
    })
}

/// Open one of an instance's well-known folders in the system file manager.
///
/// The folder is selected by a closed enum-like string (HMCL's "浏览" menu);
/// the frontend never sends a raw path, so this cannot be used to open an
/// arbitrary directory. Missing leaf folders (mods/saves/...) are created so
/// a freshly installed instance still gives immediate feedback.
#[tauri::command]
#[tracing::instrument(skip(state))]
pub async fn instance_reveal(
    state: tauri::State<'_, AppState>,
    key: String,
    folder: Option<String>,
) -> Result<(), IpcError> {
    let requested = folder.as_deref().unwrap_or("run");
    let target = {
        let core = state.core.read().await;
        let mut service = core.instances.lock().map_err(|_| locked_error())?;
        let paths = service.paths(&key)?;
        instance_folder_path(&paths, requested).ok_or_else(|| IpcError {
            code: rmccc_error::ErrorCode::ValidationError,
            message: format!("unknown instance folder: {requested}"),
            retryable: false,
        })?
    };

    std::fs::create_dir_all(&target).map_err(|e| IpcError {
        code: rmccc_error::ErrorCode::Unknown,
        message: format!("Failed to create instance folder {}: {e}", target.display()),
        retryable: false,
    })?;
    tauri_plugin_opener::open_path(&target, None::<&str>).map_err(|e| IpcError {
        code: rmccc_error::ErrorCode::Unknown,
        message: format!("Failed to open instance folder: {e}"),
        retryable: false,
    })
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
    let task = state.tasks.create(
        rmccc_tasks::TaskSpec::new(rmccc_tasks::TaskKind::InstanceRepair, "修复实例文件")
            .subtitle(detail.summary.display_name.clone())
            .target(key.clone())
            .step("check", "检查缺失", 1)
            .step("download", "补全文件", 8)
            .step("commit", "提交", 1),
    );
    task.begin_step("check");
    task.complete_step("check");
    task.begin_step("download");

    let progress_task = task.clone();
    let progress: rmccc_core::pipeline::ProgressCb =
        Arc::new(move |msg: &rmccc_core::pipeline::ProgressMsg| match msg {
            rmccc_core::pipeline::ProgressMsg::Download { label, progress } => {
                progress_task.message(label.clone());
                progress_task.download(
                    "download",
                    rmccc_tasks::DownloadUpdate {
                        completed_files: progress.completed_files,
                        total_files: progress.total_files,
                        downloaded_bytes: progress.downloaded_bytes,
                        total_bytes: progress.total_bytes,
                        current_file: progress.current_file.clone(),
                    },
                );
            }
            rmccc_core::pipeline::ProgressMsg::Stage(stage)
            | rmccc_core::pipeline::ProgressMsg::StageDone(stage) => {
                progress_task.message(stage.clone());
            }
        });

    let report = rmccc_core::pipeline::repair_instance_at(
        &core,
        &detail.summary.id,
        &detail.paths.game_dir,
        Some(&progress),
        rmccc_version::completion::CompletionVerify::Hash,
    )
    .await
    .map_err(|error| {
        let ipc: IpcError = error.into();
        task.fail(crate::commands::tasks::ipc_error_view(&ipc));
        ipc
    })?;

    if !report.is_complete() {
        task.fail(crate::commands::tasks::error_view(format!(
            "{} 个文件下载失败",
            report.failed_files
        )));
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
    task.complete_step("download");
    task.clear_download();
    task.begin_step("commit");
    task.complete_step("commit");
    task.succeed(rmccc_tasks::TaskResultView {
        key: Some(key.clone()),
        instance_id: Some(detail.summary.id.clone()),
        directory_id: Some(detail.summary.directory_id.clone()),
        name: Some(detail.summary.display_name.clone()),
        installed_files: Some(report.total_files as u64),
        ..rmccc_tasks::TaskResultView::default()
    });
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
) -> Result<TaskStart, IpcError> {
    let instance_id = instance_id
        .filter(|id| !id.trim().is_empty())
        .unwrap_or_else(|| mc_version.clone());
    rmccc_instance::validate_instance_id(&instance_id)?;
    let has_loader = loader_type.is_some() && loader_version.is_some();
    if has_loader && instance_id == mc_version {
        return Err(IpcError {
            code: rmccc_error::ErrorCode::ValidationError,
            message: "loader installs need an instance name different from the base version".into(),
            retryable: false,
        });
    }

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

    let profile_id = format!("{directory_id}/{instance_id}");
    let subtitle = match &loader_type {
        Some(loader) => format!("{mc_version} · {loader}"),
        None => mc_version.clone(),
    };
    let mut spec = rmccc_tasks::TaskSpec::new(
        rmccc_tasks::TaskKind::InstanceInstall,
        format!("安装 {instance_id}"),
    )
    .subtitle(subtitle)
    .target(profile_id.clone())
    .cancellable(true)
    .step("prepare", "准备", 1)
    .step("game", "游戏文件", 10);
    if let Some(loader) = &loader_type {
        spec = spec.tag(loader.clone()).step("loader", "加载器", 6);
    }
    spec = spec.step("commit", "提交实例", 1);

    let task = state.tasks.create(spec);
    let task_id = task.id().to_string();
    let app_handle = state.app_handle.clone();
    tokio::spawn(async move {
        let fail_task = task.clone();
        if let Err(error) = run_install_background(
            core,
            app_handle,
            InstallRequest {
                directory_id,
                game_dir,
                mc_version,
                instance_id,
                loader_type,
                loader_version,
            },
            task,
        )
        .await
        {
            tracing::error!("Instance install failed: {error:?}");
            fail_task.fail(crate::commands::tasks::ipc_error_view(&error));
        }
    });

    Ok(TaskStart::new(task_id))
}

/// Aborts a helper task on every return path, including `?` early exits.
struct AbortOnDrop(tokio::task::JoinHandle<()>);

impl AbortOnDrop {
    fn abort(&self) {
        self.0.abort();
    }
}

impl Drop for AbortOnDrop {
    fn drop(&mut self) {
        self.0.abort();
    }
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
    task: rmccc_tasks::TaskHandle,
) -> Result<(), IpcError> {
    let InstallRequest {
        directory_id,
        game_dir,
        mc_version,
        instance_id,
        loader_type,
        loader_version,
    } = request;
    // 有 loader 时先把原版装到标准版本 id，loader 的父版本才能被解析；
    // 没 loader 时直接装进用户自定义的实例目录。
    let has_loader = loader_type.is_some() && loader_version.is_some();
    let base_id = if has_loader {
        mc_version.clone()
    } else {
        instance_id.clone()
    };

    task.begin_step("prepare");
    task.message("准备安装…");

    // Batch id is learned from the first progress event; cancel/pause actions
    // address the live batch through this slot. The watcher covers the race
    // where the user cancels before the batch exists.
    let batch_slot = Arc::new(std::sync::Mutex::new(None::<String>));
    {
        let manager = core.download_manager.clone();
        let slot = batch_slot.clone();
        task.set_cancel_action(move || {
            if let Some(batch_id) = slot.lock().ok().and_then(|value| value.clone()) {
                manager.cancel_batch(&batch_id);
            }
        });
    }
    {
        let manager = core.download_manager.clone();
        let slot = batch_slot.clone();
        task.set_pause_action(move |paused| {
            if let Some(batch_id) = slot.lock().ok().and_then(|value| value.clone()) {
                if paused {
                    manager.pause(&batch_id);
                } else {
                    manager.resume(&batch_id);
                }
            }
        });
    }

    let cancel_flag = task.cancel_flag();
    let watcher_slot = batch_slot.clone();
    let watcher_manager = core.download_manager.clone();
    let watcher = AbortOnDrop(tokio::spawn(async move {
        loop {
            if cancel_flag.load(std::sync::atomic::Ordering::Relaxed) {
                if let Some(batch_id) = watcher_slot.lock().ok().and_then(|value| value.clone()) {
                    watcher_manager.cancel_batch(&batch_id);
                }
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        }
    }));

    let progress_cb: rmccc_core::pipeline::ProgressCb = {
        let task = task.clone();
        let slot = batch_slot.clone();
        let manager = core.download_manager.clone();
        Arc::new(move |msg: &rmccc_core::pipeline::ProgressMsg| match msg {
            rmccc_core::pipeline::ProgressMsg::Download { label, progress } => {
                if let Ok(mut batch) = slot.lock() {
                    *batch = Some(progress.batch_id.clone());
                }
                if task.is_paused() {
                    manager.pause(&progress.batch_id);
                }
                if task.cancel_requested() {
                    manager.cancel_batch(&progress.batch_id);
                }
                task.message(label.clone());
                task.download(
                    "game",
                    rmccc_tasks::DownloadUpdate {
                        completed_files: progress.completed_files,
                        total_files: progress.total_files,
                        downloaded_bytes: progress.downloaded_bytes,
                        total_bytes: progress.total_bytes,
                        current_file: progress.current_file.clone(),
                    },
                );
            }
            rmccc_core::pipeline::ProgressMsg::Stage(stage)
            | rmccc_core::pipeline::ProgressMsg::StageDone(stage) => {
                task.message(stage.clone());
            }
        })
    };

    task.begin_step("game");
    let report = match rmccc_core::pipeline::install_game_into(
        &core,
        &game_dir,
        &mc_version,
        &base_id,
        Some(&progress_cb),
    )
    .await
    {
        Ok(report) => report,
        Err(error) => {
            watcher.abort();
            if task.cancel_requested() {
                task.cancelled();
                return Ok(());
            }
            return Err(error.into());
        }
    };
    if task.cancel_requested() {
        watcher.abort();
        task.cancelled();
        return Ok(());
    }
    if !report.is_complete() {
        watcher.abort();
        task.fail(crate::commands::tasks::error_view(format!(
            "{} 个游戏文件下载失败",
            report.failed_files
        )));
        return Ok(());
    }
    task.complete_step("game");
    task.clear_download();
    task.clear_pause_action();

    let mut detail = rmccc_core::pipeline::commit_install(&core, &directory_id, &base_id)?;
    if let Ok(mut service) = core.instances.lock() {
        let _ = service.select_instance(Some(&detail.summary.key));
    }

    // Optional loader: run it after the vanilla instance is fully committed so
    // the patch/installer can resolve its base manifest offline.
    if let (Some(loader_type), Some(loader_version)) = (loader_type, loader_version) {
        let Some(loader) = rmccc_modloader::ModLoaderType::from_str(&loader_type) else {
            watcher.abort();
            task.fail(crate::commands::tasks::error_view(format!(
                "unknown loader type: {loader_type}"
            )));
            return Ok(());
        };
        task.begin_step("loader");
        task.clear_download();
        let loader_progress: rmccc_modloader::InstallProgressCallback = {
            let task = task.clone();
            Arc::new(move |event: rmccc_modloader::InstallProgressEvent| {
                if event.stage == "detail" {
                    task.detail(event.message);
                    return;
                }
                task.message(event.message);
                task.set_step_percent("loader", event.percent as f64);
            })
        };
        match rmccc_core::modloader::install_loader(
            &core,
            &detail,
            &loader,
            &loader_version,
            Some(loader_progress),
        )
        .await
        {
            Ok(installed) => {
                let mut service = core.instances.lock().map_err(|_| locked_error())?;
                // Loader 安装器生成的 id 通常带 `-<loader>-<version>`；向导里的
                // 自定义实例名在 loader 成功后再改名，避免改名后父版本断链。
                let installed = if installed.version_id != instance_id {
                    service.rename(&installed.instance_key, &instance_id)?
                } else {
                    service.get(&installed.instance_key)?
                };
                service.select_instance(Some(&installed.summary.key))?;
                detail = installed;
                task.complete_step("loader");
            }
            Err(error) => {
                watcher.abort();
                if task.cancel_requested() {
                    task.cancelled();
                    return Ok(());
                }
                let message = format!("加载器安装失败：{error}");
                task.fail_step("loader", message.clone());
                task.fail(crate::commands::tasks::error_view(message));
                return Ok(());
            }
        }
    }

    if task.cancel_requested() {
        watcher.abort();
        task.cancelled();
        return Ok(());
    }
    watcher.abort();

    task.begin_step("commit");
    task.complete_step("commit");
    task.message(format!("实例 {instance_id} 安装完成"));
    let _ = app_handle.emit(
        crate::events::INSTANCES_REFRESHED,
        serde_json::json!({ "key": detail.summary.key, "version": mc_version }),
    );
    task.succeed(rmccc_tasks::TaskResultView {
        key: Some(detail.summary.key.clone()),
        instance_id: Some(detail.summary.id.clone()),
        directory_id: Some(directory_id),
        manual_directory: None,
        name: Some(detail.summary.display_name.clone()),
        installed_files: None,
        version: Some(mc_version),
    });
    tracing::info!(key = %detail.summary.key, "Instance install complete");
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

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_paths() -> InstancePaths {
        let run = PathBuf::from("/game/versions/demo");
        InstancePaths {
            game_dir: PathBuf::from("/game"),
            instance_dir: PathBuf::from("/game/versions/demo"),
            run_dir: run.clone(),
            mods_dir: run.join("mods"),
            saves_dir: run.join("saves"),
            resourcepacks_dir: run.join("resourcepacks"),
            shaderpacks_dir: run.join("shaderpacks"),
            screenshots_dir: run.join("screenshots"),
            logs_dir: run.join("logs"),
            config_dir: run.join("config"),
            json_file: run.join("demo.json"),
            jar_file: run.join("demo.jar"),
            settings_file: run.join(".rmccc/instance.json"),
            natives_dir: run.join("natives"),
        }
    }

    #[test]
    fn folder_shortcuts_resolve_to_known_directories_only() {
        let paths = sample_paths();
        assert_eq!(
            instance_folder_path(&paths, "run"),
            Some(PathBuf::from("/game/versions/demo"))
        );
        assert_eq!(
            instance_folder_path(&paths, "crash-reports"),
            Some(PathBuf::from("/game/versions/demo/crash-reports"))
        );
        assert_eq!(
            instance_folder_path(&paths, "mods"),
            Some(PathBuf::from("/game/versions/demo/mods"))
        );
        assert!(instance_folder_path(&paths, "../escape").is_none());
        assert!(instance_folder_path(&paths, "/tmp").is_none());
    }
}
