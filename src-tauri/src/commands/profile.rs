//! Profile IPC commands.

use crate::state::AppState;
use rmccc_error::IpcError;
use rmccc_profile::Profile;
use serde::Deserialize;
use ts_rs::TS;

#[derive(Debug, Deserialize, TS)]
#[ts(export_to = "types.ts")]
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
pub async fn profile_list(state: tauri::State<'_, AppState>) -> Result<Vec<Profile>, IpcError> {
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
    if name.trim().is_empty() {
        return Err(IpcError {
            code: rmccc_error::ErrorCode::ValidationError,
            message: "Profile name cannot be empty".into(),
            retryable: false,
        });
    }
    if name.len() > 64 {
        return Err(IpcError {
            code: rmccc_error::ErrorCode::ValidationError,
            message: format!("Profile name too long (max 64, got {})", name.len()),
            retryable: false,
        });
    }

    let id = uuid::Uuid::new_v4().to_string();

    let core = state.core.read().await;

    // The profile's game directory is the *shared* game root (`.minecraft`
    // equivalent), which is also what `GameRepository` indexes and what installs
    // write into. Giving each profile its own nested directory made the layout
    // per-profile isolated while the repository scanned the shared root, so
    // `get_resolved()` could never find an installed version (launch failed).
    // Per-profile isolation belongs to `IsolationType` and affects only the
    // *run* directory, not where versions/libraries/assets live.
    let game_root = core.config.resolved_game_root();
    let game_dir = game_root.clone();

    let profile = Profile {
        id: id.clone(),
        name,
        game_version,
        memory_mb: 4096,
        game_dir,
        ..Default::default()
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
    let mut profile = core.profile_store.get(&id)?.ok_or_else(|| IpcError {
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
        profile.java_path = if java_path.is_empty() {
            None
        } else {
            Some(java_path)
        };
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
pub async fn profile_delete(state: tauri::State<'_, AppState>, id: String) -> Result<(), IpcError> {
    let core = state.core.read().await;
    core.profile_store.delete(&id, false)?;
    Ok(())
}
