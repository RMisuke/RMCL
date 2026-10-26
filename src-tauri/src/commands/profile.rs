//! Profile IPC commands.

use crate::state::AppState;
use rmccc_error::IpcError;
use rmccc_profile::Profile;
use rmccc_version::GameRepository as _;
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

/// Import a version from an external game directory (HMCL / other launchers)
/// as a first-class, launchable profile.
///
/// The profile's `game_dir` becomes the authoritative root for this version
/// (libraries/assets/versions live there); `external_root = true` tells the
/// launch/repair paths to stop using the launcher-wide game root.
#[tauri::command]
#[tracing::instrument(skip(state))]
pub async fn profile_import_version(
    state: tauri::State<'_, AppState>,
    root: String,
    version_id: String,
) -> Result<Profile, IpcError> {
    if version_id.trim().is_empty() {
        return Err(IpcError {
            code: rmccc_error::ErrorCode::ValidationError,
            message: "version id cannot be empty".into(),
            retryable: false,
        });
    }
    let canonical = std::fs::canonicalize(&root).map_err(|_| IpcError {
        code: rmccc_error::ErrorCode::ValidationError,
        message: format!("游戏目录不存在或不可访问：{root}"),
        retryable: false,
    })?;

    let core = {
        let core = state.core.read().await;
        core.handle()
    };

    // Idempotent: the browse view may call this repeatedly for the same version.
    if let Some(existing) = core
        .profile_store
        .list()?
        .into_iter()
        .find(|p| p.external_root && p.game_version == version_id && p.game_dir == canonical)
    {
        return Ok(existing);
    }

    let root_for_scan = canonical.clone();
    let vid = version_id.clone();
    let detected = tokio::task::spawn_blocking(
        move || -> Result<Option<rmccc_version::loader::LoaderIdentity>, IpcError> {
            let mut repo = rmccc_version::DefaultGameRepository::new(root_for_scan);
            repo.refresh().map_err(IpcError::from)?;
            if !repo.has_instance(&vid) {
                return Err(IpcError {
                    code: rmccc_error::ErrorCode::ProfileNotFound,
                    message: format!("该目录下没有版本 '{vid}'"),
                    retryable: false,
                });
            }
            let resolved = repo
                .get_resolved(&vid)
                .map_err(|e| IpcError::from(rmccc_error::RmcccError::Other(e.to_string())))?;
            Ok(rmccc_version::loader::detect_loader(&resolved.standalone))
        },
    )
    .await
    .map_err(|e| IpcError {
        code: rmccc_error::ErrorCode::InternalError,
        message: format!("version scan join error: {e}"),
        retryable: false,
    })??;

    let mod_loader = detected.and_then(|loader| {
        rmccc_modloader::ModLoaderType::from_str(&loader.kind).map(|loader_type| {
            rmccc_profile::ModLoaderConfig {
                loader_type,
                loader_version: loader.version,
            }
        })
    });

    let profile = Profile {
        id: uuid::Uuid::new_v4().to_string(),
        name: version_id.clone(),
        game_version: version_id,
        mod_loader,
        game_dir: canonical,
        external_root: true,
        // HMCL 的版本目录通常自带 mods/saves，按 always 隔离运行目录。
        isolation: Some("always".into()),
        memory_mb: 4096,
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
pub async fn profile_delete(
    state: tauri::State<'_, AppState>,
    id: String,
    cleanup_game_dir: bool,
) -> Result<(), IpcError> {
    let core = state.core.read().await;
    // 外置目录（HMCL 等）是用户既有游戏目录，只允许移除 profile 记录，
    // 绝不删除其 game_dir——即使前端误传 cleanup=true。
    let cleanup = match core.profile_store.get(&id)? {
        Some(profile) => cleanup_game_dir && !profile.external_root,
        None => cleanup_game_dir,
    };
    core.profile_store.delete(&id, cleanup)?;
    Ok(())
}
