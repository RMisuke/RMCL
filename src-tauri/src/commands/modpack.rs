//! Modpack IPC commands: inspect, install, and native file picker.
//!
//! Installs are unified tasks: `modpack_install` creates a TaskSnapshot and
//! returns its id; the task worker maps `ModpackProgress` onto visible steps.
//! Cancellation is `task_cancel`, which flips the task's cooperative flag.

use crate::state::AppState;
use base64::Engine;
use rmccc_error::IpcError;
use rmccc_modpack::{ModpackKind, PackLoader};
use serde::Serialize;
use std::path::PathBuf;
use std::sync::Arc;
use tauri::Emitter;
use ts_rs::TS;

/// Modpack metadata shown in the install wizard.
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export_to = "types.ts")]
pub struct ModpackInspectView {
    pub path: String,
    pub kind: String,
    pub kind_name: String,
    pub name: String,
    pub suggested_id: String,
    pub version: Option<String>,
    pub author: Option<String>,
    pub description: String,
    pub game_version: Option<String>,
    pub loaders: Vec<ModpackLoaderView>,
    pub file_count: usize,
    pub is_manual: bool,
    pub icon_data_url: Option<String>,
    pub warnings: Vec<String>,
}

/// One declared loader in the wizard.
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export_to = "types.ts")]
pub struct ModpackLoaderView {
    pub kind: String,
    pub name: String,
    pub version: String,
}

impl From<&PackLoader> for ModpackLoaderView {
    fn from(loader: &PackLoader) -> Self {
        Self {
            kind: loader.kind.as_str().to_string(),
            name: loader.kind.display_name().to_string(),
            version: loader.version.clone(),
        }
    }
}

/// Returned immediately after an install is scheduled.
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export_to = "types.ts")]
pub struct ModpackInstallStart {
    pub task_id: String,
}

fn icon_data_url(icon: &rmccc_modpack::PackIcon) -> Option<String> {
    if icon.bytes.is_empty() {
        return None;
    }
    let mime = match icon.ext.as_str() {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "bmp" => "image/bmp",
        "apng" => "image/apng",
        _ => return None,
    };
    Some(format!(
        "data:{mime};base64,{}",
        base64::engine::general_purpose::STANDARD.encode(&icon.bytes)
    ))
}

/// Inspect a local `.zip` / `.mrpack` before installing.
#[tauri::command]
#[tracing::instrument(skip(path))]
pub async fn modpack_inspect(path: String) -> Result<ModpackInspectView, IpcError> {
    let archive = PathBuf::from(&path);
    if !archive.is_file() {
        return Err(IpcError {
            code: rmccc_error::ErrorCode::ValidationError,
            message: format!("modpack file not found: {path}"),
            retryable: false,
        });
    }
    let path_for_task = archive.clone();
    let pack =
        tauri::async_runtime::spawn_blocking(move || rmccc_modpack::read_modpack(&path_for_task))
            .await
            .map_err(|error| IpcError {
                code: rmccc_error::ErrorCode::InternalError,
                message: format!("modpack inspect task failed: {error}"),
                retryable: false,
            })??;

    let info = rmccc_modpack::ModpackInfo::from(&pack);
    let suggested_id = rmccc_core::modpack::suggested_instance_name(&info, &archive);
    Ok(ModpackInspectView {
        path,
        kind: info.kind.as_str().to_string(),
        kind_name: info.kind_name,
        name: info.name,
        suggested_id,
        version: info.version,
        author: info.author,
        description: info.description,
        game_version: info.game_version,
        loaders: info.loaders.iter().map(ModpackLoaderView::from).collect(),
        file_count: info.file_count,
        is_manual: info.is_manual,
        icon_data_url: pack.icon.as_ref().and_then(icon_data_url),
        warnings: info.warnings,
    })
}

/// Native picker for `.zip` / `.mrpack` (returns `None` on cancel).
#[tauri::command]
#[tracing::instrument(skip(app))]
pub async fn modpack_pick_file(app: tauri::AppHandle) -> Result<Option<String>, IpcError> {
    use tauri_plugin_dialog::DialogExt;

    let picked = tauri::async_runtime::spawn_blocking(move || {
        app.dialog()
            .file()
            .add_filter("Minecraft modpack", &["zip", "mrpack"])
            .blocking_pick_file()
    })
    .await
    .map_err(|error| IpcError {
        code: rmccc_error::ErrorCode::InternalError,
        message: format!("modpack dialog failed: {error}"),
        retryable: false,
    })?;
    match picked {
        Some(path) => {
            let path = path.into_path().map_err(|error| IpcError {
                code: rmccc_error::ErrorCode::ValidationError,
                message: format!("invalid modpack path: {error}"),
                retryable: false,
            })?;
            Ok(Some(path.display().to_string()))
        }
        None => Ok(None),
    }
}

/// Schedule a modpack install. Returns a task id; progress arrives by event.
#[tauri::command]
#[tracing::instrument(skip(state), fields(task_id = tracing::field::Empty))]
pub async fn modpack_install(
    state: tauri::State<'_, AppState>,
    path: String,
    directory_id: String,
    instance_name: Option<String>,
) -> Result<ModpackInstallStart, IpcError> {
    let archive = PathBuf::from(&path);
    if !archive.is_file() {
        return Err(IpcError {
            code: rmccc_error::ErrorCode::ValidationError,
            message: format!("modpack file not found: {path}"),
            retryable: false,
        });
    }

    // Validate early (and fail before spawning a task) so the dialog can show
    // an actionable error while still open.
    let archive_for_inspect = archive.clone();
    let info = tauri::async_runtime::spawn_blocking(move || {
        rmccc_modpack::inspect_modpack(&archive_for_inspect)
    })
    .await
    .map_err(|error| IpcError {
        code: rmccc_error::ErrorCode::InternalError,
        message: format!("modpack inspect task failed: {error}"),
        retryable: false,
    })??;
    if info.kind != ModpackKind::Manual && info.game_version.is_none() {
        return Err(IpcError {
            code: rmccc_error::ErrorCode::ValidationError,
            message: "modpack does not declare a Minecraft version".into(),
            retryable: false,
        });
    }

    let core = {
        let core = state.core.read().await;
        core.handle()
    };
    // Validate the target directory before detaching the task.
    {
        let service = core.instances.lock().map_err(|_| IpcError {
            code: rmccc_error::ErrorCode::InternalError,
            message: "instance service lock poisoned".into(),
            retryable: false,
        })?;
        service.game_dir(&directory_id)?;
    }

    let mut spec = rmccc_tasks::TaskSpec::new(
        rmccc_tasks::TaskKind::ModpackInstall,
        format!("安装整合包 {}", info.name),
    )
    .subtitle(format!(
        "{} · {}",
        info.kind_name,
        info.game_version
            .clone()
            .unwrap_or_else(|| "未知版本".into())
    ))
    .target(directory_id.clone())
    .tag(info.kind.as_str().to_string())
    .cancellable(true)
    .step("prepare", "读取清单", 2);
    if !info.is_manual {
        spec = spec.step("game", "游戏本体", 8);
        if !info.loaders.is_empty() {
            spec = spec.step("loader", "加载器", 5);
        }
        spec = spec.step("libraries", "前置库", 2);
    }
    spec = spec.step("overrides", "覆盖文件", 3);
    if !info.is_manual {
        spec = spec.step("files", "整合包文件", 10);
    }
    spec = spec
        .step("metadata", "元数据", 1)
        .step("finalize", "收尾", 1);
    let task = state.tasks.create(spec);
    let task_id = task.id().to_string();
    let task_id_for_log = task_id.clone();

    let app_handle = state.app_handle.clone();
    let request = rmccc_core::modpack::ModpackInstallRequest {
        archive,
        directory_id,
        instance_name,
    };
    let cancel = task.cancel_flag();
    tauri::async_runtime::spawn(async move {
        let progress: rmccc_core::modpack::ModpackProgressCb = {
            let task = task.clone();
            Arc::new(
                move |event: rmccc_core::modpack::ModpackProgress| match event {
                    rmccc_core::modpack::ModpackProgress::Stage {
                        phase,
                        percent,
                        message,
                    } => {
                        task.begin_step(phase);
                        task.set_step_percent(phase, percent as f64);
                        task.message(message);
                        if phase != "files" {
                            task.clear_download();
                        }
                    }
                    rmccc_core::modpack::ModpackProgress::Overrides { completed, total } => {
                        task.begin_step("overrides");
                        task.set_step_progress(
                            "overrides",
                            completed,
                            total,
                            None,
                            Some(format!("解压 {completed}/{total} 个文件")),
                        );
                        task.message(format!("解压 {completed}/{total} 个文件"));
                    }
                    rmccc_core::modpack::ModpackProgress::Downloads {
                        completed,
                        total,
                        current,
                    } => {
                        task.download(
                            "files",
                            rmccc_tasks::DownloadUpdate {
                                completed_files: completed,
                                total_files: total,
                                downloaded_bytes: 0,
                                total_bytes: 0,
                                current_file: current.clone(),
                            },
                        );
                        task.message(
                            current
                                .clone()
                                .unwrap_or_else(|| format!("{completed}/{total} 文件")),
                        );
                    }
                },
            )
        };

        let result =
            rmccc_core::modpack::install_modpack(&core, request, Some(progress), Some(cancel))
                .await;
        match result {
            Ok(installed) => {
                for warning in &installed.warnings {
                    task.warn(warning.clone());
                }
                task.message(format!("整合包 {} 安装完成", installed.name));
                let _ = app_handle.emit(
                    crate::events::INSTANCES_REFRESHED,
                    serde_json::json!({
                        "key": installed.key,
                        "directory_id": installed.directory_id,
                        "manual_directory": installed.manual_directory,
                    }),
                );
                task.succeed(rmccc_tasks::TaskResultView {
                    key: installed.key.clone(),
                    instance_id: installed.instance_id.clone(),
                    directory_id: Some(installed.directory_id.clone()),
                    manual_directory: installed.manual_directory.clone(),
                    name: Some(installed.name.clone()),
                    installed_files: None,
                    version: info.game_version.clone(),
                });
                tracing::info!(
                    task_id = %task_id_for_log,
                    kind = installed.kind.as_str(),
                    key = ?installed.key,
                    "Modpack installed"
                );
            }
            Err(error) => {
                let message = error.to_string();
                if task.cancel_requested() || message.contains("cancelled") {
                    task.cancelled();
                } else {
                    task.fail(crate::commands::tasks::error_view(format!(
                        "安装失败：{error}"
                    )));
                }
                tracing::warn!(task_id = %task_id_for_log, error = %error, "Modpack install failed");
            }
        }
    });

    Ok(ModpackInstallStart { task_id })
}
