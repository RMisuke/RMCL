//! Mod loader 安装编排（审计 P1-4 收敛点）。
//!
//! 此前 Tauri `modloader_install` 与 CLI `modloader_install` 各自复刻了一遍
//! "按 loader 分流 + 解析基础版本 + 写版本 JSON + 更新 profile + 刷新索引"，
//! 而且**两边写法并不一致**（Tauri 写增量 JSON，CLI 写合并后的完整 JSON，
//! 后者会丢掉 HMCL 的回滚/叠加语义）。现在只有这一份实现。
//!
//! 分流本身也下沉到了 [`rmccc_modloader::DependencyManager::install_loader`]：
//! Forge / NeoForge 走官方 installer JAR，其余 6 种打 `GameInstancePatch`。

use crate::CoreHandle;
use rmccc_error::Result;
use rmccc_modloader::{
    DependencyManager, InstallProgressCallback, InstallProgressEvent, LoaderInstallOutcome,
    ModLoaderType,
};

/// 安装结果。
#[derive(Debug, Clone)]
pub struct InstalledLoader {
    /// 安装后的版本 id（也就是新实例 id）。
    pub version_id: String,
    /// 新实例的 `directory/id` key。
    pub instance_key: String,
    /// 是否由官方安装器直接产出（Forge / NeoForge）。
    pub via_installer_jar: bool,
}

/// 为某个实例安装 mod loader。
///
/// 步骤（一次调用全部完成，调用方只负责展示进度）：
/// 1. 经平台适配层探测所需 Java；
/// 2. 经注入的 `version_provider` 解析基础（vanilla）版本；
/// 3. `DependencyManager::install_loader` 执行安装；
/// 4. patch 型 loader 写**增量**版本 JSON（`inheritsFrom` + `patches`）；
/// 5. 更新 profile（`mod_loader` + `game_version`）并刷新本地索引。
///
/// `progress` 为 `Some` 时逐阶段上报安装细节（下载字节、安装器 stdout、
/// 补库阶段），供 Task Center 展示；`None` 时行为不变。
pub async fn install_loader(
    core: &CoreHandle,
    instance: &rmccc_instance::InstanceDetail,
    loader_type: &ModLoaderType,
    loader_version: &str,
    progress: Option<InstallProgressCallback>,
) -> Result<InstalledLoader> {
    // ── 1. Java（installer JAR 需要真实 JVM 运行）──
    let required_java = rmccc_version::java_version_for_mc(&instance.summary.game_version);
    let java_install = core.platform.java.find(required_java)?;

    // ── 2. 基础版本（走 DI 注入的 provider，与安装链路一致）──
    let base_version = rmccc_version::resolve_version(
        &instance.summary.game_version,
        core.version_provider.as_ref(),
    )
    .await?;

    // 安装目标是该实例所属的游戏根。
    let game_dir = instance.paths.game_dir.clone();

    // ── 2.5 原版 client jar 就位保障 ──
    // installer 型 loader（Forge 安装器 --installClient）与 patch 管线里的
    // PROCESS_MINECRAFT_JAR 处理器都要读 `versions/{base}/{base}.jar`。
    // GUI 向导保证"先装原版再装 loader"，CLI/MCP 没有这个顺序约定——缺 jar
    // 时补一次基础版本修复（已完整则 no-op）。
    let base_client_jar = game_dir
        .join("versions")
        .join(&instance.summary.game_version)
        .join(format!("{}.jar", instance.summary.game_version));
    if !base_client_jar.exists() {
        tracing::info!(
            version = %instance.summary.game_version,
            "Base client jar missing before loader install; repairing base version"
        );
        crate::pipeline::repair_instance(
            core,
            &game_dir,
            &instance.summary.game_version,
            None,
            rmccc_version::completion::CompletionVerify::Hash,
        )
        .await?;
    }

    // ── 3. 安装（分流在 modloader crate 内部）──
    // 平台描述符经注入传递（批次 4：Forge processors 的 classpath 分隔符）。
    let dm =
        DependencyManager::new(core.http_client.clone()).with_platform(core.platform.descriptor);
    let outcome = dm
        .install_loader(
            loader_type,
            &instance.summary.game_version,
            &base_version,
            loader_version,
            &game_dir,
            &java_install.path,
            progress.clone(),
        )
        .await?;

    // ── 4. 写版本 JSON（仅 patch 型；installer 型已由安装器写好）──
    let via_installer_jar = match &outcome {
        LoaderInstallOutcome::InstallerJar { version_id } => {
            tracing::info!(
                instance = %instance.summary.key,
                loader = %loader_type.library_id(),
                version_id = %version_id,
                "Loader installed via installer JAR"
            );
            true
        }
        LoaderInstallOutcome::Patch { version_id, patch } => {
            write_incremental_version_json(
                core,
                &game_dir,
                version_id,
                &instance.summary.game_version,
                patch,
            )?;
            false
        }
    };

    let version_id = outcome.version_id().to_string();

    // ── 5. 提交实例设置 + 刷新索引 ──
    commit_loader(core, &instance.summary.directory_id, &version_id)?;

    // ── 6. 立刻补齐 loader 自带的库（HMCL 语义）──
    //
    // patch 型的 loader 只是把库**列进** patch，文件还没下载。不在这里补齐的话，
    // 用户装完 loader 直接启动（尤其是 `--no-repair`）会撞上
    // `ClassNotFoundException: net.fabricmc.loader.impl.launch.knot.KnotClient`。
    // 启动前的 `ensure_launch_ready` 也能补，但那次失败体验很差（错误信息在
    // 游戏 stdout 里，而不是安装步骤里）。
    if let Some(cb) = &progress {
        cb(InstallProgressEvent {
            stage: "repair",
            percent: 75,
            message: "下载 loader 依赖库（字节进度见下载任务）...".to_string(),
        });
    }
    let repaired = crate::pipeline::repair_instance(
        core,
        &game_dir,
        &version_id,
        None,
        rmccc_version::completion::CompletionVerify::Hash,
    )
    .await?;
    if !repaired.is_complete() {
        tracing::warn!(
            version_id = %version_id,
            failed_files = repaired.failed_files,
            "Loader installed but some libraries failed to download"
        );
    } else if !repaired.is_noop() {
        tracing::info!(
            version_id = %version_id,
            files = repaired.total_files,
            "Downloaded loader libraries"
        );
    }

    Ok(InstalledLoader {
        instance_key: rmccc_instance::InstanceService::key(
            &instance.summary.directory_id,
            &version_id,
        )?,
        version_id,
        via_installer_jar,
    })
}

/// 写 HMCL 增量版本 JSON：`{ id, inheritsFrom, patches }`。
///
/// 基础版本 JSON 保持不动，因此回滚 = 删除这个目录，多层 loader 可叠加。
fn write_incremental_version_json(
    core: &CoreHandle,
    game_dir: &std::path::Path,
    version_id: &str,
    parent_id: &str,
    patch: &rmccc_modloader::GameInstancePatch,
) -> Result<()> {
    let version_dir = game_dir.join("versions").join(version_id);
    std::fs::create_dir_all(&version_dir)?;

    let incremental = rmccc_version::GameVersion {
        id: version_id.to_string(),
        inherits_from: Some(parent_id.to_string()),
        patches: vec![patch.to_game_patch()],
        ..Default::default()
    };
    let json = serde_json::to_string_pretty(&incremental)?;
    std::fs::write(version_dir.join(format!("{version_id}.json")), json)?;

    tracing::info!(
        version_id = %version_id,
        parent = %parent_id,
        game_dir = %core.config.resolved_game_root().display(),
        "Wrote incremental loader version JSON"
    );
    Ok(())
}

/// 刷新目录索引并确保新实例的设置存在（隔离策略按创建时固定）。
fn commit_loader(core: &CoreHandle, directory_id: &str, version_id: &str) -> Result<()> {
    crate::pipeline::commit_install(core, directory_id, version_id)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn commit_loader_rejects_unknown_directory() {
        let dir = std::env::temp_dir().join(format!(
            "rmccc-loader-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let core = crate::CoreContext::new(&dir).unwrap();
        assert!(commit_loader(&core.handle(), "nope", "1.20.4").is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
