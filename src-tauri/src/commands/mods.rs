//! Mod management IPC commands.
//!
//! Delegates to `CoreContext::mod_sources` for Modrinth/CurseForge API access.

use crate::state::AppState;
use rmccc_error::IpcError;
use rmccc_mods::ModSearchQuery;

#[tauri::command]
#[tracing::instrument(skip(state))]
pub async fn mod_search(
    state: tauri::State<'_, AppState>,
    query: String,
    game_version: Option<String>,
    loader: Option<String>,
    page: Option<u32>,
    limit: Option<u32>,
) -> Result<rmccc_mods::ModSearchResult, IpcError> {
    let core = state.core.read().await;

    let search_query = ModSearchQuery {
        query,
        game_version,
        loader,
        page: page.unwrap_or(1),
        limit: limit.unwrap_or(20).min(100),
    };

    // Use the first available mod source (Modrinth).
    let source = core.mod_sources.first().ok_or_else(|| IpcError {
        code: rmccc_error::ErrorCode::Unknown,
        message: "No mod source available".into(),
        retryable: false,
    })?;

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
    let core = state.core.read().await;

    let source = core.mod_sources.first().ok_or_else(|| IpcError {
        code: rmccc_error::ErrorCode::Unknown,
        message: "No mod source available".into(),
        retryable: false,
    })?;

    source.list_versions(&mod_id).await.map_err(IpcError::from)
}

#[tauri::command]
#[tracing::instrument(skip(state))]
pub async fn mod_get_download_url(
    state: tauri::State<'_, AppState>,
    version_id: String,
) -> Result<String, IpcError> {
    let core = state.core.read().await;

    let source = core.mod_sources.first().ok_or_else(|| IpcError {
        code: rmccc_error::ErrorCode::Unknown,
        message: "No mod source available".into(),
        retryable: false,
    })?;

    source
        .get_download_url(&version_id)
        .await
        .map_err(IpcError::from)
}

#[tauri::command]
#[tracing::instrument(skip(state))]
pub async fn mod_list(
    state: tauri::State<'_, AppState>,
    profile_id: String,
) -> Result<Vec<String>, IpcError> {
    let core = state.core.read().await;
    let profile = core
        .profile_store
        .get(&profile_id)?
        .ok_or_else(|| IpcError {
            code: rmccc_error::ErrorCode::ProfileNotFound,
            message: format!("Profile not found: {profile_id}"),
            retryable: false,
        })?;

    // Mods follow the instance run directory, so an isolated instance keeps
    // its own mods (HMCL behaviour) instead of writing into the shared root.
    let mods_dir = rmccc_launch::instance_mods_dir(&core.config, &profile);
    if !mods_dir.exists() {
        tracing::debug!(profile_id = %profile_id, "mods dir does not exist");
        return Ok(Vec::new());
    }

    let mut mods = Vec::new();
    if let Ok(entries) = std::fs::read_dir(&mods_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().is_some_and(|e| e == "jar") {
                if let Some(name) = path.file_stem().and_then(|n| n.to_str()) {
                    mods.push(name.to_string());
                }
            }
        }
    }
    tracing::info!(profile_id = %profile_id, count = mods.len(), "Mods listed");
    Ok(mods)
}

#[tauri::command]
#[tracing::instrument(skip(state))]
pub async fn mod_toggle(
    state: tauri::State<'_, AppState>,
    profile_id: String,
    mod_filename: String,
    enabled: bool,
) -> Result<(), IpcError> {
    let core = state.core.read().await;
    let profile = core
        .profile_store
        .get(&profile_id)?
        .ok_or_else(|| IpcError {
            code: rmccc_error::ErrorCode::ProfileNotFound,
            message: format!("Profile not found: {profile_id}"),
            retryable: false,
        })?;

    // Mods follow the instance run directory, so an isolated instance keeps
    // its own mods (HMCL behaviour) instead of writing into the shared root.
    let mods_dir = rmccc_launch::instance_mods_dir(&core.config, &profile);
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
fn sanitize_filename(name: &str) -> String {
    name.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || "-_.".contains(c) {
                c
            } else {
                '_'
            }
        })
        .collect()
}

/// Reject path traversal / invalid chars in user-supplied filenames.
fn validate_mod_filename(name: &str) -> Result<(), IpcError> {
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
    profile_id: String,
    version: rmccc_mods::ModVersion,
) -> Result<String, IpcError> {
    let core = state.core.read().await;
    let profile = core
        .profile_store
        .get(&profile_id)?
        .ok_or_else(|| IpcError {
            code: rmccc_error::ErrorCode::ProfileNotFound,
            message: format!("Profile not found: {profile_id}"),
            retryable: false,
        })?;
    let source = core.mod_sources.first().ok_or_else(|| IpcError {
        code: rmccc_error::ErrorCode::Unknown,
        message: "No mod source available".into(),
        retryable: false,
    })?;

    // Mods follow the instance run directory, so an isolated instance keeps
    // its own mods (HMCL behaviour) instead of writing into the shared root.
    let mods_dir = rmccc_launch::instance_mods_dir(&core.config, &profile);
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

    tracing::info!(profile_id = %profile_id, filename = %filename, "Mod installed");
    Ok(filename)
}

/// 移除 Mod：删除 `{game_dir}/mods/{name}` 或 `{game_dir}/mods/disabled/{name}`（P0-02）。
#[tauri::command]
#[tracing::instrument(skip(state))]
pub async fn mod_remove(
    state: tauri::State<'_, AppState>,
    profile_id: String,
    mod_filename: String,
) -> Result<(), IpcError> {
    validate_mod_filename(&mod_filename)?;

    let core = state.core.read().await;
    let profile = core
        .profile_store
        .get(&profile_id)?
        .ok_or_else(|| IpcError {
            code: rmccc_error::ErrorCode::ProfileNotFound,
            message: format!("Profile not found: {profile_id}"),
            retryable: false,
        })?;

    // Mods follow the instance run directory, so an isolated instance keeps
    // its own mods (HMCL behaviour) instead of writing into the shared root.
    let mods_dir = rmccc_launch::instance_mods_dir(&core.config, &profile);
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
