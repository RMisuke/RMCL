//! Mod loader IPC commands.

use crate::state::AppState;
use rmccc_error::IpcError;
use rmccc_modloader::{create_provider, LoaderVersion, ModLoaderType};

#[tauri::command]
#[tracing::instrument(skip(state))]
pub async fn modloader_list(
    state: tauri::State<'_, AppState>,
    loader_type: String,
    game_version: String,
) -> Result<Vec<LoaderVersion>, IpcError> {
    let lt = ModLoaderType::from_str(&loader_type).ok_or_else(|| IpcError {
        code: rmccc_error::ErrorCode::Unknown,
        message: format!("Unknown mod loader type: {loader_type}"),
        retryable: false,
    })?;

    let provider = create_provider(&lt, state.http_client.clone());
    let versions = provider.list_versions(&game_version).await?;
    Ok(versions)
}

#[tauri::command]
#[tracing::instrument(skip(state))]
pub async fn modloader_install(
    state: tauri::State<'_, AppState>,
    profile_id: String,
    loader_type: String,
    loader_version: String,
) -> Result<(), IpcError> {
    let lt = ModLoaderType::from_str(&loader_type).ok_or_else(|| IpcError {
        code: rmccc_error::ErrorCode::Unknown,
        message: format!("Unknown mod loader type: {loader_type}"),
        retryable: false,
    })?;

    // Load profile and detect Java
    let (profile, game_dir, java_path) = {
        let core = state.core.read().await;
        let profile = core
            .profile_store
            .get(&profile_id)?
            .ok_or_else(|| IpcError {
                code: rmccc_error::ErrorCode::ProfileNotFound,
                message: format!("Profile not found: {profile_id}"),
                retryable: false,
            })?;

        let game_dir = profile.game_dir.clone();
        let game_version = profile.game_version.clone();

        // Detect Java for running Forge/NeoForge installers
        let required_java = rmccc_version::java_version_for_mc(&game_version);
        let java_install = rmccc_launch::find_java(required_java)?;

        (profile, game_dir, java_install.path)
    };

    // Install the loader
    let provider = create_provider(&lt, state.http_client.clone());
    let version_id = provider
        .install(
            &profile.game_version,
            &loader_version,
            &game_dir,
            &java_path,
        )
        .await?;

    // Update profile
    {
        let core = state.core.read().await;
        let mut profile = core
            .profile_store
            .get(&profile_id)?
            .ok_or_else(|| IpcError {
                code: rmccc_error::ErrorCode::ProfileNotFound,
                message: format!("Profile not found: {profile_id}"),
                retryable: false,
            })?;

        profile.mod_loader = Some(rmccc_profile::ModLoaderConfig {
            loader_type: format!("{lt:?}").to_lowercase(),
            loader_version,
        });
        profile.game_version = version_id;
        core.profile_store.save(&profile)?;
    }

    tracing::info!(
        profile_id = %profile_id,
        loader = %loader_type,
        "Mod loader installed"
    );

    Ok(())
}
