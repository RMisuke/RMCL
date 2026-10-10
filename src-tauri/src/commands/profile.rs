//! Profile IPC commands.

use crate::state::AppState;
use rmccc_error::IpcError;
use rmccc_profile::Profile;
use serde::Deserialize;
use ts_rs::TS;

#[derive(Debug, Deserialize, TS)]
#[ts(export_to = "types.ts")]
pub struct ProfileUpdateOpts {
    // ── Instance-only ──
    pub name: Option<String>,
    pub game_version: Option<String>,
    pub icon: Option<String>,
    /// Isolation strategy; `""` clears the override (follow the global default).
    pub isolation: Option<String>,
    /// Replaces the override set wholesale. The settings form knows which fields
    /// the user chose to override, so sending the full list avoids having to
    /// agree on individual add/remove operations.
    pub override_properties: Option<Vec<String>>,

    // ── Instance values (effective only when listed in `override_properties`) ──
    pub java_path: Option<String>,
    pub jvm_args: Option<Vec<String>>,
    pub game_args: Option<Vec<String>>,
    pub memory_mb: Option<u32>,
    pub min_memory_mb: Option<u32>,
    pub metaspace_mb: Option<u32>,
    pub process_priority: Option<String>,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub fullscreen: Option<bool>,
    pub use_custom_natives: Option<bool>,
    pub use_native_glfw: Option<bool>,
    pub use_native_openal: Option<bool>,
    pub no_generated_jvm_args: Option<bool>,
    pub no_generated_optimizing_jvm_args: Option<bool>,
    pub enable_debug_log_output: Option<bool>,
    pub wrapper: Option<String>,
    pub java_agents: Option<Vec<String>>,
    pub override_jvm_args: Option<Vec<String>>,
    pub pre_launch_command: Option<String>,
    pub post_exit_command: Option<String>,
    pub env_vars: Option<std::collections::HashMap<String, String>>,
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
    if let Some(icon) = opts.icon {
        profile.icon = if icon.is_empty() { None } else { Some(icon) };
    }
    if let Some(isolation) = opts.isolation {
        // Normalise (or clear) so only supported values reach the file.
        profile.isolation = if isolation.trim().is_empty() {
            None
        } else {
            Some(rmccc_config::resolve_isolation(Some(&isolation)).to_string())
        };
    }
    if let Some(overrides) = opts.override_properties {
        // Only accept names we know, so a typo cannot make a field silently
        // un-inheritable.
        profile.override_properties = overrides
            .into_iter()
            .filter(|name| rmccc_profile::fields::ALL.contains(&name.as_str()))
            .collect();
    }

    // Instance values. Stored whether or not the field is currently overridden,
    // so clearing an override does not discard what the user typed.
    if let Some(java_path) = opts.java_path {
        profile.java_path = if java_path.is_empty() {
            None
        } else {
            Some(java_path)
        };
    }
    if let Some(v) = opts.jvm_args {
        profile.jvm_args = v;
    }
    if let Some(v) = opts.game_args {
        profile.game_args = v;
    }
    if let Some(v) = opts.memory_mb {
        profile.memory_mb = v;
    }
    if let Some(v) = opts.min_memory_mb {
        profile.min_memory_mb = v;
    }
    if let Some(v) = opts.metaspace_mb {
        profile.metaspace_mb = v;
    }
    if let Some(v) = opts.process_priority {
        profile.process_priority = v;
    }
    if let Some(v) = opts.width {
        profile.width = if v == 0 { None } else { Some(v) };
    }
    if let Some(v) = opts.height {
        profile.height = if v == 0 { None } else { Some(v) };
    }
    if let Some(v) = opts.fullscreen {
        profile.fullscreen = v;
    }
    if let Some(v) = opts.use_custom_natives {
        profile.use_custom_natives = v;
    }
    if let Some(v) = opts.use_native_glfw {
        profile.use_native_glfw = v;
    }
    if let Some(v) = opts.use_native_openal {
        profile.use_native_openal = v;
    }
    if let Some(v) = opts.no_generated_jvm_args {
        profile.no_generated_jvm_args = v;
    }
    if let Some(v) = opts.no_generated_optimizing_jvm_args {
        profile.no_generated_optimizing_jvm_args = v;
    }
    if let Some(v) = opts.enable_debug_log_output {
        profile.enable_debug_log_output = v;
    }
    if let Some(v) = opts.wrapper {
        profile.wrapper = if v.is_empty() { None } else { Some(v) };
    }
    if let Some(v) = opts.java_agents {
        profile.java_agents = v;
    }
    if let Some(v) = opts.override_jvm_args {
        profile.override_jvm_args = v;
    }
    if let Some(v) = opts.pre_launch_command {
        profile.pre_launch_command = if v.is_empty() { None } else { Some(v) };
    }
    if let Some(v) = opts.post_exit_command {
        profile.post_exit_command = if v.is_empty() { None } else { Some(v) };
    }
    if let Some(v) = opts.env_vars {
        profile.env_vars = v;
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
