//! Mod loader IPC commands.

use crate::state::AppState;
use rmccc_error::IpcError;
use rmccc_modloader::{DependencyManager, LoaderVersion, ModLoaderType};
use rmccc_version::GameRepository as _;
use tauri::Emitter;

/// 发射 modloader 安装进度事件（P2-06）。
/// `InstallTask` 无细粒度进度回调，故采用阶段级事件（0/10/50/100%），
/// 前端可据此展示安装中/完成/失败状态。
fn emit_modloader_progress(
    app_handle: &tauri::AppHandle,
    profile_id: &str,
    loader_type: &str,
    game_version: &str,
    stage: &str,
    percent: u8,
    message: &str,
) {
    let _ = app_handle.emit(
        crate::events::MODLOADER_INSTALL_PROGRESS,
        serde_json::json!({
            "profile_id": profile_id,
            "loader_type": loader_type,
            "game_version": game_version,
            "stage": stage,
            "percent": percent,
            "message": message,
        }),
    );
}

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

    // 统一走 DependencyManager（P1-02）：支持全部 8 种 loader
    let dm = DependencyManager::new(state.http_client.clone());
    let versions = dm.list_versions(&lt, &game_version).await?;
    Ok(versions)
}

#[tauri::command]
#[tracing::instrument(skip(state), fields(request_id = %uuid::Uuid::new_v4()))]
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

    let app_handle = state.app_handle.clone();

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

        // The loader writes a new version manifest, which `GameRepository` must
        // index — so target the shared game root, like `version_install` does.
        let game_dir = core.config.resolved_game_root();
        let game_version = profile.game_version.clone();

        // Detect Java for running Forge/NeoForge installers
        let required_java = rmccc_version::java_version_for_mc(&game_version);
        let java_install = rmccc_launch::find_java(required_java)?;

        (profile, game_dir, java_install.path)
    };

    emit_modloader_progress(
        &app_handle,
        &profile_id,
        &loader_type,
        &profile.game_version,
        "started",
        0,
        &format!("开始安装 {} ...", lt.display_name()),
    );

    // 统一入口（P1-02）：与 CLI 一致的分流。
    //  - Forge/NeoForge：legacy `ModLoaderProvider`（需 java_path 运行 installer JAR）作兼容壳
    //  - 其余 6 种（Fabric/Quilt/LiteLoader/OptiFine/Cleanroom/LegacyFabric）：DependencyManager，
    //    把 patch 后的 GameVersion 落盘到 versions/{id}/{id}.json
    let version_id = match &lt {
        ModLoaderType::Forge | ModLoaderType::NeoForge => {
            emit_modloader_progress(
                &app_handle,
                &profile_id,
                &loader_type,
                &profile.game_version,
                "installing",
                30,
                "运行安装器 JAR 中...",
            );
            let core = state.core.read().await;
            let provider = core.modloader_provider(&lt)?;
            provider
                .install(
                    &profile.game_version,
                    &loader_version,
                    &game_dir,
                    &java_path,
                )
                .await?
        }
        _ => {
            emit_modloader_progress(
                &app_handle,
                &profile_id,
                &loader_type,
                &profile.game_version,
                "installing",
                30,
                "解析基础版本...",
            );
            let dm = DependencyManager::new(state.http_client.clone());
            // P2-01：走 DI 注入的 version_provider，而非直接构造 MojangVersionProvider
            let base_version = {
                let core = state.core.read().await;
                rmccc_version::resolve_version(
                    &profile.game_version,
                    core.version_provider.as_ref(),
                )
                .await?
            };

            emit_modloader_progress(
                &app_handle,
                &profile_id,
                &loader_type,
                &profile.game_version,
                "installing",
                60,
                "应用 modloader patch...",
            );
            let (_merged, patch) = dm
                .install_with_patch(
                    &lt,
                    &profile.game_version,
                    &base_version,
                    &loader_version,
                    &game_dir,
                    &java_path,
                )
                .await?;

            // ── HMCL 增量语义（G4）：不写死合并结果，而是写 inheritsFrom + patches。
            // 基础版本 JSON 保持不动；回滚 = 删除 patch；多层 loader 可叠加。
            let new_version_id = format!("{}-{}-{}", profile.game_version, patch.id, patch.version);
            let version_dir = game_dir.join("versions").join(&new_version_id);
            std::fs::create_dir_all(&version_dir)?;
            let json_path = version_dir.join(format!("{new_version_id}.json"));
            let incremental = rmccc_version::GameVersion {
                id: new_version_id.clone(),
                inherits_from: Some(profile.game_version.clone()),
                patches: vec![patch.to_game_patch()],
                ..Default::default()
            };
            std::fs::write(&json_path, serde_json::to_string_pretty(&incremental)?)?;
            new_version_id
        }
    };

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
            loader_type: lt.clone(),
            loader_version,
        });
        profile.game_version = version_id.clone();
        core.profile_store.save(&profile)?;

        // Refresh the local repository so the new inheriting instance is indexed
        // and launch resolves it from disk (offline-capable).
        core.game_repository
            .write()
            .map_err(|_| rmccc_error::RmcccError::Other("game repository lock poisoned".into()))?
            .refresh()?;
    }

    emit_modloader_progress(
        &app_handle,
        &profile_id,
        &loader_type,
        &profile.game_version,
        "completed",
        100,
        "安装完成",
    );

    let _ = app_handle.emit(
        crate::events::INSTANCES_REFRESHED,
        serde_json::json!({ "version": profile.game_version }),
    );

    tracing::info!(
        profile_id = %profile_id,
        loader = %loader_type,
        "Mod loader installed"
    );

    Ok(())
}
