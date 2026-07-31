//! Version IPC commands.

use crate::state::AppState;
use rmccc_error::IpcError;
use rmccc_version::{MojangVersionProvider, VersionInfo, VersionProvider};

#[tauri::command]
#[tracing::instrument(skip(state))]
pub async fn version_list(
    state: tauri::State<'_, AppState>,
) -> Result<Vec<VersionInfo>, IpcError> {
    let provider = MojangVersionProvider::new(state.http_client.clone());

    let versions = match provider.fetch_versions().await {
        Ok(versions) => versions,
        Err(e) if e.is_retryable() => {
            tracing::warn!("Network error fetching versions, trying cache: {e}");
            let cache_dir = rmccc_config::paths::cache_dir();
            let cache = rmccc_version::cache::VersionCache::new(&cache_dir, 3600);
            cache.read()?.unwrap_or_default()
        }
        Err(e) => return Err(e.into()),
    };

    // Write cache in background
    let versions_for_cache = versions.clone();
    let cache_dir = rmccc_config::paths::cache_dir();
    tokio::spawn(async move {
        let cache = rmccc_version::cache::VersionCache::new(&cache_dir, 3600);
        if let Err(e) = cache.write(&versions_for_cache) {
            tracing::warn!("Failed to update version cache: {e}");
        }
    });

    Ok(versions)
}

#[tauri::command]
#[tracing::instrument(skip(state))]
pub async fn version_install(
    state: tauri::State<'_, AppState>,
    version: String,
    profile_id: String,
) -> Result<(), IpcError> {
    // ── Load profile (short lock) ──
    let (mut profile, game_dir) = {
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
        (profile, game_dir)
    };

    // ── Fetch version metadata (no lock) ──
    let provider = MojangVersionProvider::new(state.http_client.clone());
    let game_version = provider.fetch_version(&version).await?;

    // ── Prepare directories ──
    let versions_dir = game_dir.join("versions").join(&version);
    std::fs::create_dir_all(&versions_dir)?;

    // ── Build download items ──
    let client_entry = game_version.downloads.get("client").ok_or_else(|| IpcError {
        code: rmccc_error::ErrorCode::DownloadFailed,
        message: "Client JAR not found in version metadata".into(),
        retryable: false,
    })?;

    let client_item = rmccc_download::DownloadItem {
        url: client_entry.url.clone(),
        dest: versions_dir.join(format!("{}.jar", version)),
        expected_sha1: Some(client_entry.sha1.clone()),
        size: Some(client_entry.size),
        use_cache: true,
        candidates: Vec::new(),
    };

    let lib_items = rmccc_version::libraries::resolve_library_items(
        &game_version.libraries,
        &game_dir.join("libraries"),
    );

    let asset_items = rmccc_version::assets::resolve_asset_items(
        &state.http_client,
        &game_version.asset_index,
        &game_dir.join("assets"),
    )
    .await?;

    let mut all_items = vec![client_item];
    all_items.extend(lib_items);
    all_items.extend(asset_items);

    let total_size: u64 = all_items.iter().filter_map(|i| i.size).sum();
    let required = (total_size as f64 * 1.2) as u64;
    rmccc_download::check_disk_space(&game_dir, required)?;

    tracing::info!(
        profile_id = %profile_id,
        version = %version,
        total_items = all_items.len(),
        total_size_mb = total_size / 1024 / 1024,
        "Starting version install"
    );

    // ── Download (short lock for download_manager access) ──
    let _handle = {
        let core = state.core.read().await;
        core.download_manager
            .download_batch(all_items, std::sync::Arc::new(|_p| {
                // Progress will be emitted via ThrottledEmitter
            }))
            .await?
    };

    // ── Save version metadata and update profile (short lock) ──
    {
        let version_json_path = versions_dir.join(format!("{}.json", version));
        // Bug #7: Save the original Mojang JSON (camelCase) not the Rust struct (snake_case).
        // Minecraft itself reads versions/{id}/{id}.json and expects the original format.
        if let Some(ref raw) = game_version.raw_json {
            std::fs::write(&version_json_path, raw)?;
        } else {
            // Fallback: serialize from the parsed struct (less ideal but functional)
            let fallback = serde_json::to_string_pretty(&game_version)?;
            std::fs::write(&version_json_path, fallback)?;
        }

        profile.game_version = version.clone();
        let core = state.core.read().await;
        core.profile_store.save(&profile)?;
    }

    tracing::info!(profile_id = %profile_id, version = %version, "Version install complete");
    Ok(())
}
