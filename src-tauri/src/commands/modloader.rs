//! Mod loader IPC commands.

use crate::state::AppState;
use rmccc_error::IpcError;
use rmccc_modloader::{DependencyManager, LoaderVersion, ModLoaderType};
use std::sync::Arc;
use tauri::Emitter;

/// Map a loader installer stage to the visible task step.
fn step_for_stage(stage: &str) -> &'static str {
    match stage {
        "started" => "prepare",
        "fetch" | "download" => "download",
        "run" | "execute" => "installer",
        "detect" | "repair" | "completed" => "verify",
        _ => "",
    }
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

    // A6 triage：安装器未完成的 loader 不再出现在可选版本里（analyzer 仍可
    // 识别已安装实例）。返回空列表，让向导自然显示"无可用版本"。
    if !lt.is_installable() {
        tracing::warn!(loader = %loader_type, "loader installer incomplete; returning empty version list");
        return Ok(Vec::new());
    }

    // 统一走 DependencyManager（P1-02）：支持全部 8 种 loader
    let platform = {
        let core = state.core.read().await;
        core.platform.descriptor
    };
    let dm = DependencyManager::new(state.http_client.clone()).with_platform(platform);
    let versions = dm.list_versions(&lt, &game_version).await?;
    Ok(versions)
}

#[tauri::command]
#[tracing::instrument(skip(state), fields(request_id = %uuid::Uuid::new_v4()))]
pub async fn modloader_install(
    state: tauri::State<'_, AppState>,
    key: String,
    loader_type: String,
    loader_version: String,
) -> Result<String, IpcError> {
    let lt = ModLoaderType::from_str(&loader_type).ok_or_else(|| IpcError {
        code: rmccc_error::ErrorCode::Unknown,
        message: format!("Unknown mod loader type: {loader_type}"),
        retryable: false,
    })?;

    let app_handle = state.app_handle.clone();

    // 只读一次实例：Java 探测、基础版本解析、安装、新实例提交、索引刷新
    // 全部由 core 的编排完成。
    let instance = {
        let core = state.core.read().await;
        let mut service = core.instances.lock().map_err(|_| IpcError {
            code: rmccc_error::ErrorCode::InternalError,
            message: "Instance service lock poisoned".into(),
            retryable: false,
        })?;
        service.get(&key)?
    };
    let base_game_version = instance.summary.game_version.clone();

    let task = state.tasks.create(
        rmccc_tasks::TaskSpec::new(
            rmccc_tasks::TaskKind::ModloaderInstall,
            format!("安装 {} {}", lt.display_name(), loader_version),
        )
        .subtitle(format!(
            "{} · {}",
            instance.summary.display_name, base_game_version
        ))
        .target(key.clone())
        .tag(loader_type.clone())
        .step("prepare", "准备", 1)
        .step("download", "下载安装器", 6)
        .step("installer", "运行安装器", 8)
        .step("verify", "校验产物", 2),
    );
    task.begin_step("prepare");
    task.message(format!("开始安装 {}…", lt.display_name()));

    // 安装细节回调：modloader crate 的阶段事件直接驱动任务步骤与细节日志。
    // 进度节流由 TaskRegistry 统一负责（100ms/任务），这里不再手写时间窗。
    let progress: rmccc_modloader::InstallProgressCallback = {
        let task = task.clone();
        Arc::new(move |event: rmccc_modloader::InstallProgressEvent| {
            if event.stage == "detail" {
                task.detail(event.message);
                return;
            }
            let step = step_for_stage(event.stage);
            task.message(event.message);
            if !step.is_empty() {
                task.begin_step(step);
                task.set_step_percent(step, event.percent as f64);
            }
        })
    };

    // 统一入口（P1-4）：installer-JAR 与 patch 两条路径的分流已在
    // `rmccc_modloader` 内部，Tauri/CLI 不再各自 match。
    // A2：Forge/NeoForge 安装要运行官方 installer JAR（分钟级）——handle
    // 快照后立即释放读锁，全程不持 core 锁。
    let core = {
        let core = state.core.read().await;
        core.handle()
    };
    let installed = match rmccc_core::modloader::install_loader(
        &core,
        &instance,
        &lt,
        &loader_version,
        Some(progress),
    )
    .await
    {
        Ok(installed) => installed,
        Err(error) => {
            let message = format!("加载器安装失败：{error}");
            task.fail(crate::commands::tasks::error_view(message));
            return Err(error.into());
        }
    };
    tracing::info!(
        key = %key,
        loader = %loader_type,
        version_id = %installed.version_id,
        via_installer_jar = installed.via_installer_jar,
        "Mod loader installed"
    );

    if let Ok(mut service) = core.instances.lock() {
        let _ = service.select_instance(Some(&installed.instance_key));
    }

    task.complete_step("verify");
    task.message(format!("{} 安装完成", lt.display_name()));
    let installed_key = installed.instance_key.clone();
    task.succeed(rmccc_tasks::TaskResultView {
        key: Some(installed_key.clone()),
        instance_id: Some(installed.version_id.clone()),
        directory_id: Some(instance.summary.directory_id.clone()),
        name: Some(installed.version_id.clone()),
        version: Some(installed.version_id.clone()),
        ..rmccc_tasks::TaskResultView::default()
    });

    let _ = app_handle.emit(
        crate::events::INSTANCES_REFRESHED,
        serde_json::json!({ "version": installed.version_id }),
    );

    Ok(installed_key)
}
