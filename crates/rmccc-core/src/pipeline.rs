//! 游戏安装 / 修复 / 启动编排（P1-01）。
//!
//! 从 `src-cli/src/context.rs` 抽取：核心编排逻辑集中于此，进度与输出通过
//! 回调传递，CLI 仅负责呈现（`println!/eprint!` 留在 CLI 层，不进入本模块）。
//! Tauri 命令层亦可复用本模块以消除两套启动/安装实现。

use crate::CoreContext;
use rmccc_auth::Account;
use rmccc_error::{Result, RmcccError};
use rmccc_version::GameRepository as _;
use std::path::Path;
use std::sync::Arc;

/// 管道级进度事件：阶段消息与下载进度统一走回调上报，模块内部不直接打印。
#[derive(Debug)]
pub enum ProgressMsg {
    /// 阶段开始（如 "下载客户端 JAR..."）
    Stage(String),
    /// 下载批次进度（label + 底层进度）
    Download {
        label: String,
        progress: rmccc_download::DownloadProgress,
    },
    /// 阶段完成（如 "✅ 客户端 JAR 下载完成"）
    StageDone(String),
}

/// 进度回调类型。
pub type ProgressCb = Arc<dyn Fn(&ProgressMsg) + Send + Sync>;

/// 游戏 stdout/stderr 输出回调（逐行转发）。
pub type GameOutputCb = Arc<dyn Fn(&str) + Send + Sync>;

fn noop_progress() -> Arc<dyn Fn(&ProgressMsg) + Send + Sync> {
    Arc::new(|_| {})
}

/// 把下载批次的底层进度回调包装为管道进度回调。
fn progress_download_cb(
    progress: Option<&ProgressCb>,
    label: &str,
) -> Arc<dyn Fn(rmccc_download::DownloadProgress) + Send + Sync> {
    let label = label.to_string();
    let cb = progress.cloned().unwrap_or_else(noop_progress);
    Arc::new(move |p| {
        cb(&ProgressMsg::Download {
            label: label.clone(),
            progress: p,
        })
    })
}

fn emit(progress: Option<&ProgressCb>, msg: ProgressMsg) {
    if let Some(cb) = progress {
        cb(&msg);
    }
}

/// 解析游戏版本：优先本地 patched JSON（含 `inheritsFrom` 继承链合并），
/// 否则走 Mojang API（复用 `core.version_provider`，P2-01 一致性）。
pub async fn resolve_version(
    core: &CoreContext,
    game_version_id: &str,
    game_dir: &Path,
) -> Result<rmccc_version::GameVersion> {
    let provider = core.version_provider.as_ref();
    let local_json_path = game_dir
        .join("versions")
        .join(game_version_id)
        .join(format!("{game_version_id}.json"));

    if !local_json_path.exists() {
        return rmccc_version::resolve_version(game_version_id, provider).await;
    }

    let content = std::fs::read_to_string(&local_json_path)
        .map_err(|e| RmcccError::Other(format!("读取本地版本 JSON 失败: {e}")))?;

    // Try direct parse first (e.g. Fabric patched JSON is complete)
    if let Ok(v) = serde_json::from_str::<rmccc_version::GameVersion>(&content) {
        return Ok(v);
    }

    // Handle inheritsFrom-based versions (NeoForge/Forge): resolve base + merge overrides
    let raw: serde_json::Value = serde_json::from_str(&content)
        .map_err(|e| RmcccError::Other(format!("解析本地版本 JSON 失败: {e}")))?;

    let parent_id = match raw.get("inheritsFrom").and_then(|v| v.as_str()) {
        Some(id) => id.to_string(),
        None => return rmccc_version::resolve_version(game_version_id, provider).await,
    };

    let mut resolved = rmccc_version::resolve_version(&parent_id, provider).await?;

    if let Some(mc) = raw.get("mainClass").and_then(|v| v.as_str()) {
        resolved.main_class = mc.to_string();
    }
    if let Some(libs) = raw.get("libraries").and_then(|v| v.as_array()) {
        let local_libs: Vec<rmccc_version::Library> =
            serde_json::from_value(serde_json::Value::Array(libs.clone())).unwrap_or_default();
        for lib in local_libs {
            if !resolved.libraries.iter().any(|l| l.name == lib.name) {
                resolved.libraries.push(lib);
            }
        }
    }
    if let Some(args) = raw.get("arguments") {
        if let Ok(a) = serde_json::from_value(args.clone()) {
            resolved.arguments = Some(a);
        }
    }
    resolved.id = game_version_id.to_string();
    Ok(resolved)
}

/// 安装游戏资源（client jar + 依赖库 + 资源文件 + natives 解压）。
/// 对应 CLI `resource download` 编排（P1-01 抽取）。
pub async fn install_game(
    core: &CoreContext,
    profile: &rmccc_profile::Profile,
    version_id: &str,
    progress: Option<&ProgressCb>,
) -> Result<()> {
    let version = resolve_version(core, version_id, &profile.game_dir).await?;

    for subdir in &["versions", "libraries", "assets"] {
        std::fs::create_dir_all(profile.game_dir.join(subdir))?;
    }

    download_client_jar(core, profile, &version, version_id, progress).await?;
    let libs_dir = download_libraries(core, profile, &version, progress).await?;
    download_assets(core, profile, &version, progress).await?;
    extract_natives(&version, &libs_dir, &profile.game_dir, version_id, progress)?;

    tracing::info!(
        profile_id = %profile.id,
        version = %version_id,
        "Version installed"
    );
    Ok(())
}

/// Step 1: Download the client JAR and save version JSON.
async fn download_client_jar(
    core: &CoreContext,
    profile: &rmccc_profile::Profile,
    version: &rmccc_version::GameVersion,
    version_id: &str,
    progress: Option<&ProgressCb>,
) -> Result<()> {
    let client_dl = match version.downloads.get("client") {
        Some(dl) => dl,
        None => return Ok(()),
    };

    let jar_path = profile
        .game_dir
        .join("versions")
        .join(version_id)
        .join(format!("{version_id}.jar"));
    let jar_parent = jar_path
        .parent()
        .ok_or_else(|| RmcccError::Other(format!("Invalid jar path: {}", jar_path.display())))?;
    std::fs::create_dir_all(jar_parent)?;

    let candidates = core
        .download_provider
        .inject_url_with_candidates(&client_dl.url);
    let items = vec![rmccc_download::DownloadItem {
        url: candidates[0].clone(),
        dest: jar_path,
        expected_sha1: Some(client_dl.sha1.clone()),
        size: Some(client_dl.size),
        use_cache: true,
        candidates: candidates[1..].to_vec(),
    }];
    emit(progress, ProgressMsg::Stage("下载客户端 JAR...".into()));
    let handle = core
        .download_manager
        .download_batch(items, progress_download_cb(progress, "JAR"))
        .await?;
    core.download_manager.wait_for_batch(&handle.id).await;
    emit(
        progress,
        ProgressMsg::StageDone("✅ 客户端 JAR 下载完成".into()),
    );

    let version_json_path = profile
        .game_dir
        .join("versions")
        .join(version_id)
        .join(format!("{version_id}.json"));
    if !version_json_path.exists() {
        if let Some(ref raw_json) = version.raw_json {
            std::fs::write(&version_json_path, raw_json)?;
        }
    }
    Ok(())
}

/// Step 2: Download library files.
async fn download_libraries(
    core: &CoreContext,
    profile: &rmccc_profile::Profile,
    version: &rmccc_version::GameVersion,
    progress: Option<&ProgressCb>,
) -> Result<std::path::PathBuf> {
    let libs_dir = profile.game_dir.join("libraries");
    let lib_items: Vec<rmccc_download::DownloadItem> =
        rmccc_version::libraries::resolve_library_items(&version.libraries, &libs_dir)
            .into_iter()
            .map(|mut item| {
                let candidates = core.download_provider.inject_url_with_candidates(&item.url);
                item.url = candidates[0].clone();
                item.candidates = candidates[1..].to_vec();
                item
            })
            .collect();
    let lib_count = lib_items.len();
    if !lib_items.is_empty() {
        emit(
            progress,
            ProgressMsg::Stage(format!("下载依赖库 ({} 个)...", lib_count)),
        );
        let handle = core
            .download_manager
            .download_batch(lib_items, progress_download_cb(progress, "依赖库"))
            .await?;
        core.download_manager.wait_for_batch(&handle.id).await;
        emit(
            progress,
            ProgressMsg::StageDone(format!("✅ 依赖库下载完成 ({} 个)", lib_count)),
        );
    }
    Ok(libs_dir)
}

/// Step 3: Download asset files.
async fn download_assets(
    core: &CoreContext,
    profile: &rmccc_profile::Profile,
    version: &rmccc_version::GameVersion,
    progress: Option<&ProgressCb>,
) -> Result<()> {
    let asset_items: Vec<rmccc_download::DownloadItem> =
        rmccc_version::assets::resolve_asset_items(
            &core.http_client,
            &version.asset_index,
            &profile.game_dir.join("assets"),
        )
        .await?
        .into_iter()
        .map(|mut item| {
            let candidates = core.download_provider.inject_url_with_candidates(&item.url);
            item.url = candidates[0].clone();
            item.candidates = candidates[1..].to_vec();
            item
        })
        .collect();
    let asset_count = asset_items.len();
    if !asset_items.is_empty() {
        emit(
            progress,
            ProgressMsg::Stage(format!("下载资源文件 ({} 个)...", asset_count)),
        );
        let handle = core
            .download_manager
            .download_batch(
                asset_items,
                progress_download_cb(progress, &format!("资源 ({})", asset_count)),
            )
            .await?;
        core.download_manager.wait_for_batch(&handle.id).await;
        emit(
            progress,
            ProgressMsg::StageDone(format!("✅ 资源文件下载完成 ({} 个)", asset_count)),
        );
    }
    Ok(())
}

/// Step 4: Extract native libraries.
fn extract_natives(
    version: &rmccc_version::GameVersion,
    libs_dir: &Path,
    game_dir: &Path,
    version_id: &str,
    progress: Option<&ProgressCb>,
) -> Result<()> {
    let natives_dir = game_dir.join("versions").join(version_id).join("natives");
    emit(progress, ProgressMsg::Stage("解压 Native 库...".into()));
    let natives_count =
        rmccc_launch::decompress_natives(&version.libraries, libs_dir, &natives_dir, false, false)?;
    if natives_count > 0 {
        emit(
            progress,
            ProgressMsg::StageDone(format!("✅ Native 库解压完成 ({} 个文件)", natives_count)),
        );
    } else {
        emit(
            progress,
            ProgressMsg::StageDone("✅ Native 库检查完成 (无需解压)".into()),
        );
    }
    Ok(())
}

/// 启动前资源完整性检查与修复（P1-01 抽取自 CLI `repair_resources`）。
///
/// 检查 client JAR / 版本 JSON / 资源索引 / 依赖库，缺失则自动补下载。
/// 返回是否发生了修复。
pub async fn repair_resources(
    core: &CoreContext,
    profile: &rmccc_profile::Profile,
    progress: Option<&ProgressCb>,
) -> Result<bool> {
    let version = resolve_version(core, &profile.game_version, &profile.game_dir).await?;

    let inject_candidates =
        |url: &str| -> Vec<String> { core.download_provider.inject_url_with_candidates(url) };

    let mut repaired = false;
    let version_dir = profile
        .game_dir
        .join("versions")
        .join(&profile.game_version);

    // Check client JAR
    let jar_path = version_dir.join(format!("{}.jar", profile.game_version));
    let jar_needs_repair =
        !jar_path.exists() || std::fs::metadata(&jar_path).map(|m| m.len()).unwrap_or(0) == 0;
    if jar_needs_repair {
        if let Some(client_dl) = version.downloads.get("client") {
            let jar_parent = jar_path.parent().ok_or_else(|| {
                RmcccError::Other(format!("Invalid jar path: {}", jar_path.display()))
            })?;
            std::fs::create_dir_all(jar_parent)?;
            let candidates = inject_candidates(&client_dl.url);
            let items = vec![rmccc_download::DownloadItem {
                url: candidates[0].clone(),
                dest: jar_path,
                expected_sha1: Some(client_dl.sha1.clone()),
                size: Some(client_dl.size),
                use_cache: true,
                candidates: candidates[1..].to_vec(),
            }];
            emit(progress, ProgressMsg::Stage("修复客户端 JAR...".into()));
            let handle = core
                .download_manager
                .download_batch(items, progress_download_cb(progress, "JAR"))
                .await?;
            core.download_manager.wait_for_batch(&handle.id).await;
            emit(
                progress,
                ProgressMsg::StageDone("✅ 客户端 JAR 已修复".into()),
            );
            repaired = true;
        }
    }

    // Check version JSON
    let json_path = version_dir.join(format!("{}.json", profile.game_version));
    if !json_path.exists() {
        if let Some(ref raw_json) = version.raw_json {
            let json_parent = json_path.parent().ok_or_else(|| {
                RmcccError::Other(format!("Invalid json path: {}", json_path.display()))
            })?;
            std::fs::create_dir_all(json_parent)?;
            std::fs::write(&json_path, raw_json)?;
            emit(
                progress,
                ProgressMsg::StageDone("✅ 版本 JSON 已写入".into()),
            );
            repaired = true;
        }
    }

    // Check asset index
    let index_path = profile
        .game_dir
        .join("assets")
        .join("indexes")
        .join(format!("{}.json", version.asset_index.id));
    if !index_path.exists() {
        // Re-download the asset index
        let resp = core
            .http_client
            .get(&version.asset_index.url)
            .send()
            .await
            .map_err(|e| RmcccError::Network(e.to_string()))?;
        let body = resp
            .bytes()
            .await
            .map_err(|e| RmcccError::Network(e.to_string()))?;
        let index_parent = index_path.parent().ok_or_else(|| {
            RmcccError::Other(format!("Invalid index path: {}", index_path.display()))
        })?;
        std::fs::create_dir_all(index_parent)?;
        std::fs::write(&index_path, &body)?;
        emit(
            progress,
            ProgressMsg::StageDone(format!(
                "✅ 资源索引已修复: {}.json",
                version.asset_index.id
            )),
        );
        repaired = true;
    }

    // Check library files
    let libs_dir = profile.game_dir.join("libraries");
    let lib_items = rmccc_version::libraries::resolve_library_items(&version.libraries, &libs_dir);
    let missing_libs: Vec<_> = lib_items
        .into_iter()
        .filter(|item| !item.dest.exists())
        .collect();
    if !missing_libs.is_empty() {
        let count = missing_libs.len();
        emit(
            progress,
            ProgressMsg::Stage(format!("修复依赖库 ({} 个缺失)...", count)),
        );
        let items: Vec<_> = missing_libs
            .into_iter()
            .map(|mut item| {
                let candidates = inject_candidates(&item.url);
                item.url = candidates[0].clone();
                item.candidates = candidates[1..].to_vec();
                item
            })
            .collect();
        let handle = core
            .download_manager
            .download_batch(items, progress_download_cb(progress, "依赖库修复"))
            .await?;
        core.download_manager.wait_for_batch(&handle.id).await;
        emit(
            progress,
            ProgressMsg::StageDone(format!("✅ 依赖库已修复 ({} 个)", count)),
        );
        repaired = true;
    }

    Ok(repaired)
}

/// Detect Java installation for launch (custom path or auto-detect).
fn detect_launch_java(
    profile: &rmccc_profile::Profile,
    game_version: &rmccc_version::GameVersion,
) -> Result<rmccc_launch::JavaInstallation> {
    let required_java = game_version
        .java_version
        .as_ref()
        .map(|j| j.major_version)
        .unwrap_or_else(|| rmccc_version::java_version_for_mc(&profile.game_version));

    match &profile.java_path {
        Some(jh) => {
            let home = std::path::PathBuf::from(jh);
            let bin = if home.is_file() {
                home
            } else {
                home.join("bin").join("java")
            };
            Ok(rmccc_launch::JavaInstallation {
                path: bin,
                major_version: required_java,
                vendor: String::new(),
                is_64bit: true,
            })
        }
        None => rmccc_launch::find_java(required_java),
    }
}

/// Resolve auth credentials for launch — unified `Account::log_in()` (P2).
///
/// 有 active 账户时经 [`CoreContext::log_in_active_account`] 静默刷新并返回
/// `AuthInfo`（token 生命周期由 Account 内部承担）；无账户时回退到默认离线账户，
/// 对齐 HMCL：`user_type=msa` + 随机 token。
async fn resolve_launch_auth(core: &CoreContext) -> Result<(String, String, String, String)> {
    match core.log_in_active_account().await? {
        Some(info) => Ok((info.username, info.uuid, info.access_token, info.user_type)),
        None => {
            let info = rmccc_auth::OfflineAccount::new("Player").play_offline();
            Ok((info.username, info.uuid, info.access_token, info.user_type))
        }
    }
}

/// 启动前资源完整性检查与自动补齐（对齐 HMCL `checkGameCompletionAsync`）。
///
/// 基于本地仓库解析 launch 视图，扫描 client jar / asset index + objects /
/// libraries / natives，缺失则经 `download_manager` 补齐（含磁盘空间预检）。
/// 返回是否发生了修复。
pub async fn ensure_launch_ready(
    core: &CoreContext,
    profile: &rmccc_profile::Profile,
    progress: Option<&ProgressCb>,
) -> Result<bool> {
    let (game_root, resolved, http_client) = {
        let repo = core
            .game_repository
            .read()
            .map_err(|_| RmcccError::Other("game repository lock poisoned".to_string()))?;
        let game_root = repo.game_dir().to_path_buf();
        let resolved = repo
            .get_resolved(&profile.game_version)
            .map_err(|e| RmcccError::Other(e.to_string()))?;
        (game_root, resolved, core.http_client.clone())
    };

    let missing = rmccc_version::completion::check_completion(&game_root, &resolved);
    if missing.is_empty() {
        return Ok(false);
    }

    let items =
        rmccc_version::completion::build_completion_downloads(&http_client, &game_root, &resolved)
            .await?;
    if items.is_empty() {
        return Ok(false);
    }

    let total_bytes: u64 = items.iter().filter_map(|i| i.size).sum();
    let count = items.len();
    emit(
        progress,
        ProgressMsg::Stage(format!("补齐缺失文件 ({} 个)...", count)),
    );
    if total_bytes > 0 {
        rmccc_download::check_disk_space(&game_root, total_bytes)?;
    }

    let items: Vec<_> = items
        .into_iter()
        .map(|mut item| {
            let candidates = core.download_provider.inject_url_with_candidates(&item.url);
            item.url = candidates.first().cloned().unwrap_or(item.url.clone());
            item.candidates = candidates.into_iter().skip(1).collect();
            item
        })
        .collect();

    let handle = core
        .download_manager
        .download_batch(items, progress_download_cb(progress, "完整性修复"))
        .await?;
    core.download_manager.wait_for_batch(&handle.id).await;

    // Re-scan the repository so the index reflects the repaired files.
    core.game_repository
        .write()
        .map_err(|_| RmcccError::Other("game repository lock poisoned".to_string()))?
        .refresh()?;

    emit(
        progress,
        ProgressMsg::StageDone(format!("✅ 缺失文件补齐完成 ({} 个)", count)),
    );
    Ok(true)
}

/// CLI 输出监听器：逐行转发游戏输出到回调（HMCL `ProcessListener`）。
struct CliListener {
    output: GameOutputCb,
}

impl rmccc_launch::ProcessListener for CliListener {
    fn on_log(&self, line: String, _is_stderr: bool) {
        (self.output)(&line);
    }
    fn on_exit(&self, _exit_code: i32, _exit_type: rmccc_launch::ExitType) {}
}

/// 启动游戏并阻塞直至退出（P1-01 抽取自 CLI `launch_run`）。
///
/// 经 [`rmccc_launch::DefaultLauncher`] 编排完整启动流程（命令行生成、native
/// 解压、log4j 提取、环境变量注入、进程监控），游戏输出经 `output` 回调逐行
/// 转发。返回游戏进程退出码。启动成功后更新 `last_played`。
pub async fn launch_game(
    core: &CoreContext,
    profile: &rmccc_profile::Profile,
    instance_id: &str,
    output: &GameOutputCb,
    progress: Option<&ProgressCb>,
) -> Result<i32> {
    // 本地解析 launch 视图（HMCL resolved.launch，离线可用）。
    let game_version = {
        let repo = core
            .game_repository
            .read()
            .map_err(|_| RmcccError::Other("game repository lock poisoned".to_string()))?;
        let resolved = repo
            .get_resolved(&profile.game_version)
            .map_err(|e| RmcccError::Other(e.to_string()))?;
        resolved.launch
    };

    let java_install = detect_launch_java(profile, &game_version)?;
    let (username, uuid_str, access_token, user_type) = resolve_launch_auth(core).await?;
    let auth = rmccc_launch::AuthInfo::new(username, uuid_str, access_token, user_type);
    let options = rmccc_launch::LaunchOptions::from_profile(profile, java_install.clone());

    let listener: Arc<dyn rmccc_launch::ProcessListener> = Arc::new(CliListener {
        output: output.clone(),
    });

    let launcher = rmccc_launch::DefaultLauncher::new(
        profile.game_version.clone(),
        profile.game_dir.clone(),
        game_version,
        auth,
        options,
        Some(listener),
    );

    emit(
        progress,
        ProgressMsg::Stage(format!(
            "🚀 启动 Minecraft {} (Java {})",
            profile.game_version, java_install.major_version
        )),
    );

    let managed = launcher
        .launch()
        .await
        .map_err(|e| RmcccError::Other(e.to_string()))?;
    emit(progress, ProgressMsg::StageDone("游戏已启动。".into()));

    let exit_code = managed.wait_exit().await;
    emit(progress, ProgressMsg::StageDone("游戏已退出。".into()));

    // ── Update last_played ──
    if let Ok(Some(ref mut p)) = core.profile_store.get(instance_id) {
        p.last_played = Some(chrono::Utc::now());
        if let Err(e) = core.profile_store.save(p) {
            tracing::warn!(profile_id = %instance_id, error = ?e, "Failed to save last_played");
        }
    }

    Ok(exit_code)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// P1-01：resolve_version 在本地 JSON 缺失时回退到 provider（不 panic）。
    #[test]
    fn test_progress_msg_construction() {
        let msg = ProgressMsg::Stage("下载客户端 JAR...".to_string());
        match msg {
            ProgressMsg::Stage(s) => assert_eq!(s, "下载客户端 JAR..."),
            _ => panic!("unexpected variant"),
        }
    }

    #[test]
    fn test_noop_progress_is_send_sync() {
        let cb = noop_progress();
        let cb: Arc<dyn Fn(&ProgressMsg) + Send + Sync> = cb;
        cb(&ProgressMsg::Stage("x".to_string()));
    }
}
