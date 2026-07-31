//! Profile IPC commands.

use crate::state::AppState;
use rmccc_error::IpcError;
use rmccc_profile::Profile;
use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct ProfileUpdateOpts {
    pub name: Option<String>,
    pub game_version: Option<String>,
    pub java_path: Option<String>,
    pub jvm_args: Option<Vec<String>>,
    pub game_args: Option<Vec<String>>,
    pub memory_mb: Option<u32>,
    pub icon: Option<String>,
}

#[tauri::command]
#[tracing::instrument(skip(state))]
pub async fn profile_list(
    state: tauri::State<'_, AppState>,
) -> Result<Vec<Profile>, IpcError> {
    let core = state.core.read().await;
    let profiles = core.profile_store.list()?;
    Ok(profiles)
}

#[tauri::command]
#[tracing::instrument(skip(state))]
pub async fn profile_create(
    state: tauri::State<'_, AppState>,
    name: String,
    game_version: String,
) -> Result<Profile, IpcError> {
    let id = uuid::Uuid::new_v4().to_string();

    let core = state.core.read().await;
    let game_dir = core.config.profile_game_dir(&id);

    let profile = Profile {
        id: id.clone(),
        name,
        game_version,
        mod_loader: None,
        java_path: None,
        jvm_args: vec![],
        game_args: vec![],
        memory_mb: 4096,
        game_dir,
        created_at: chrono::Utc::now(),
        last_played: None,
        icon: None,
    };

    core.profile_store.save(&profile)?;
    Ok(profile)
}

#[tauri::command]
#[tracing::instrument(skip(state))]
pub async fn profile_update(
    state: tauri::State<'_, AppState>,
    id: String,
    opts: ProfileUpdateOpts,
) -> Result<Profile, IpcError> {
    let core = state.core.read().await;
    let mut profile = core
        .profile_store
        .get(&id)?
        .ok_or_else(|| IpcError {
            code: rmccc_error::ErrorCode::ProfileNotFound,
            message: format!("Profile not found: {id}"),
            retryable: false,
        })?;

    if let Some(name) = opts.name {
        profile.name = name;
    }
    if let Some(game_version) = opts.game_version {
        profile.game_version = game_version;
    }
    if let Some(java_path) = opts.java_path {
        profile.java_path = if java_path.is_empty() { None } else { Some(java_path) };
    }
    if let Some(jvm_args) = opts.jvm_args {
        profile.jvm_args = jvm_args;
    }
    if let Some(game_args) = opts.game_args {
        profile.game_args = game_args;
    }
    if let Some(memory_mb) = opts.memory_mb {
        profile.memory_mb = memory_mb;
    }
    if let Some(icon) = opts.icon {
        profile.icon = if icon.is_empty() { None } else { Some(icon) };
    }

    core.profile_store.save(&profile)?;
    Ok(profile)
}

#[tauri::command]
#[tracing::instrument(skip(state))]
pub async fn profile_delete(
    state: tauri::State<'_, AppState>,
    id: String,
) -> Result<(), IpcError> {
    let core = state.core.read().await;
    core.profile_store.delete(&id, false)?;
    Ok(())
}
