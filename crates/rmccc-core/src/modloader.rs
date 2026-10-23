//! Mod loader 安装编排（审计 P1-4 收敛点）。
//!
//! 此前 Tauri `modloader_install` 与 CLI `modloader_install` 各自复刻了一遍
//! "按 loader 分流 + 解析基础版本 + 写版本 JSON + 更新 profile + 刷新索引"，
//! 而且**两边写法并不一致**（Tauri 写增量 JSON，CLI 写合并后的完整 JSON，
//! 后者会丢掉 HMCL 的回滚/叠加语义）。现在只有这一份实现。
//!
//! 分流本身也下沉到了 [`rmccc_modloader::DependencyManager::install_loader`]：
//! Forge / NeoForge 走官方 installer JAR，其余 6 种打 `GameInstancePatch`。

use crate::CoreContext;
use rmccc_error::{Result, RmcccError};
use rmccc_modloader::{DependencyManager, LoaderInstallOutcome, ModLoaderType};

/// 安装结果。
#[derive(Debug, Clone)]
pub struct InstalledLoader {
    /// 安装后的版本 id（profile 的 `game_version` 指向它）。
    pub version_id: String,
    /// 是否由官方安装器直接产出（Forge / NeoForge）。
    pub via_installer_jar: bool,
}

/// 为 profile 安装 mod loader。
///
/// 步骤（一次调用全部完成，调用方只负责展示进度）：
/// 1. 经平台适配层探测所需 Java；
/// 2. 经注入的 `version_provider` 解析基础（vanilla）版本；
/// 3. `DependencyManager::install_loader` 执行安装；
/// 4. patch 型 loader 写**增量**版本 JSON（`inheritsFrom` + `patches`）；
/// 5. 更新 profile（`mod_loader` + `game_version`）并刷新本地索引。
pub async fn install_loader(
    core: &CoreContext,
    profile: &rmccc_profile::Profile,
    loader_type: &ModLoaderType,
    loader_version: &str,
) -> Result<InstalledLoader> {
    // ── 1. Java（installer JAR 需要真实 JVM 运行）──
    let required_java = rmccc_version::java_version_for_mc(&profile.game_version);
    let java_install = core.platform.java.find(required_java)?;

    // ── 2. 基础版本（走 DI 注入的 provider，与安装链路一致）──
    let base_version =
        rmccc_version::resolve_version(&profile.game_version, core.version_provider.as_ref())
            .await?;

    // 安装目标是共享游戏根（`GameRepository` 索引的目录）。
    let game_dir = core.config.resolved_game_root();

    // ── 3. 安装（分流在 modloader crate 内部）──
    let dm = DependencyManager::new(core.http_client.clone());
    let outcome = dm
        .install_loader(
            loader_type,
            &profile.game_version,
            &base_version,
            loader_version,
            &game_dir,
            &java_install.path,
        )
        .await?;

    // ── 4. 写版本 JSON（仅 patch 型；installer 型已由安装器写好）──
    let via_installer_jar = match &outcome {
        LoaderInstallOutcome::InstallerJar { version_id } => {
            tracing::info!(
                profile_id = %profile.id,
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
                &profile.game_version,
                patch,
            )?;
            false
        }
    };

    let version_id = outcome.version_id().to_string();

    // ── 5. 提交 profile + 刷新索引 ──
    commit_loader(core, &profile.id, loader_type, loader_version, &version_id)?;

    // ── 6. 立刻补齐 loader 自带的库（HMCL 语义）──
    //
    // patch 型的 loader 只是把库**列进** patch，文件还没下载。不在这里补齐的话，
    // 用户装完 loader 直接启动（尤其是 `--no-repair`）会撞上
    // `ClassNotFoundException: net.fabricmc.loader.impl.launch.knot.KnotClient`。
    // 启动前的 `ensure_launch_ready` 也能补，但那次失败体验很差（错误信息在
    // 游戏 stdout 里，而不是安装步骤里）。
    let repaired = crate::pipeline::repair_instance(
        core,
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
        version_id,
        via_installer_jar,
    })
}

/// 写 HMCL 增量版本 JSON：`{ id, inheritsFrom, patches }`。
///
/// 基础版本 JSON 保持不动，因此回滚 = 删除这个目录，多层 loader 可叠加。
fn write_incremental_version_json(
    core: &CoreContext,
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

/// 更新 profile 的 loader 记录与 `game_version`，并重扫本地索引。
fn commit_loader(
    core: &CoreContext,
    profile_id: &str,
    loader_type: &ModLoaderType,
    loader_version: &str,
    version_id: &str,
) -> Result<()> {
    let mut profile = core
        .profile_store
        .get(profile_id)?
        .ok_or_else(|| RmcccError::ProfileNotFound(profile_id.to_string()))?;

    profile.mod_loader = Some(rmccc_profile::ModLoaderConfig {
        loader_type: loader_type.clone(),
        loader_version: loader_version.to_string(),
    });
    profile.game_version = version_id.to_string();
    core.profile_store.save(&profile)?;

    crate::pipeline::refresh_repository(core)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_env(tag: &str) -> (std::path::PathBuf, CoreContext) {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let dir =
            std::env::temp_dir().join(format!("rmccc-loader-{tag}-{}-{nanos}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("data dir");
        let core = CoreContext::new(&dir).expect("CoreContext");
        (dir, core)
    }

    #[test]
    fn commit_loader_updates_profile_and_cannot_find_unknown_profile() {
        let (dir, core) = test_env("commit");

        // 未知 profile → 明确错误
        assert!(matches!(
            commit_loader(&core, "nope", &ModLoaderType::Fabric, "0.15.0", "x"),
            Err(RmcccError::ProfileNotFound(_))
        ));

        // 已知 profile → 写入 loader 记录 + game_version
        let profile = rmccc_profile::Profile {
            id: "p1".into(),
            name: "test".into(),
            game_version: "1.20.4".into(),
            game_dir: core.config.resolved_game_root(),
            ..Default::default()
        };
        core.profile_store.save(&profile).expect("save");
        commit_loader(
            &core,
            "p1",
            &ModLoaderType::Fabric,
            "0.15.0",
            "1.20.4-fabric-0.15.0",
        )
        .expect("commit");

        let updated = core.profile_store.get("p1").expect("get").expect("exists");
        assert_eq!(updated.game_version, "1.20.4-fabric-0.15.0");
        let loader = updated.mod_loader.expect("loader");
        assert_eq!(loader.loader_type, ModLoaderType::Fabric);
        assert_eq!(loader.loader_version, "0.15.0");

        let _ = std::fs::remove_dir_all(&dir);
    }
}
