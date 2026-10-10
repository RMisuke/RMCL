//! Settings IPC commands.

use crate::state::AppState;
use rmccc_config::AppConfig;
use rmccc_error::IpcError;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// Serializable Java installation info for the frontend.
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export_to = "types.ts")]
pub struct JavaInfo {
    pub path: String,
    pub major_version: u32,
    pub vendor: String,
    pub is_64bit: bool,
}

/// Settings update payload from frontend.
#[derive(Debug, Deserialize, TS)]
#[ts(export_to = "types.ts")]
pub struct SettingsUpdate {
    pub game_root_dir: Option<String>,
    pub default_java_path: Option<String>,
    pub default_min_memory_mb: Option<u32>,
    pub default_max_memory_mb: Option<u32>,
    pub download_concurrency: Option<u32>,
    pub mirror_url: Option<String>,
}

#[tauri::command]
#[tracing::instrument(skip(state))]
pub async fn settings_get(state: tauri::State<'_, AppState>) -> Result<AppConfig, IpcError> {
    let core = state.core.read().await;
    Ok(core.config.clone())
}

#[tauri::command]
#[tracing::instrument(skip(state))]
pub async fn settings_set(
    state: tauri::State<'_, AppState>,
    opts: SettingsUpdate,
) -> Result<AppConfig, IpcError> {
    let mut core = state.core.write().await;

    if let Some(dir) = opts.game_root_dir {
        let path = std::path::PathBuf::from(&dir);
        // Validate: path must be an absolute, existing directory
        if !path.is_absolute() {
            return Err(IpcError {
                code: rmccc_error::ErrorCode::IoError,
                message: format!("Game directory must be an absolute path: {dir}"),
                retryable: false,
            });
        }
        // Create if not exists
        std::fs::create_dir_all(&path).map_err(|e| IpcError {
            code: rmccc_error::ErrorCode::IoError,
            message: format!("Cannot create game directory: {e}"),
            retryable: false,
        })?;
        core.config.game_root_dir = Some(path);
    }

    if let Some(java) = opts.default_java_path {
        let p = std::path::PathBuf::from(&java);
        if java.is_empty() {
            core.config.default_java_path = None;
        } else if !p.exists() {
            return Err(IpcError {
                code: rmccc_error::ErrorCode::JavaNotFound,
                message: format!("Java not found at: {java}"),
                retryable: false,
            });
        } else {
            core.config.default_java_path = Some(java);
        }
    }

    if let Some(m) = opts.default_min_memory_mb {
        core.config.default_min_memory_mb = m.clamp(128, 65536);
    }
    if let Some(m) = opts.default_max_memory_mb {
        core.config.default_max_memory_mb = m.clamp(256, 65536);
    }
    if let Some(c) = opts.download_concurrency {
        core.config.download_concurrency = c.clamp(1, 64);
    }
    if let Some(m) = opts.mirror_url {
        core.config.mirror_url = if m.is_empty() { None } else { Some(m) };
    }

    // P4：经 FileSaver 防抖写盘（200ms 合并连续 settings_set），退出时统一 flush。
    let data_dir = core.config.resolved_data_dir();
    let path = rmccc_config::paths::launcher_settings_path(&data_dir);
    let content = serde_json::to_string_pretty(&core.config)?;
    core.file_saver.save(path, content);

    Ok(core.config.clone())
}

#[tauri::command]
#[tracing::instrument(skip(_state))]
pub async fn java_detect(_state: tauri::State<'_, AppState>) -> Result<Vec<JavaInfo>, IpcError> {
    let installations = rmccc_launch::detect_java_installations();
    let infos: Vec<JavaInfo> = installations
        .into_iter()
        .map(|j| JavaInfo {
            path: j.path.display().to_string(),
            major_version: j.major_version,
            vendor: j.vendor,
            is_64bit: j.is_64bit,
        })
        .collect();
    Ok(infos)
}
