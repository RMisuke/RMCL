//! 游戏安装 / 修复 / 启动编排（P1-01，P1-3 收敛为唯一入口）。
//!
//! 从 `src-cli/src/context.rs` 抽取：核心编排逻辑集中于此，进度与输出通过
//! 回调传递，呈现层（`println!/eprint!`、Tauri 事件）留在各自的适配层。
//!
//! **P1-3**：Tauri 命令层与 CLI 曾各自复刻安装 / 修复 / 启动三条链路，
//! 修一处漏一处且 GUI 那份没有单测。现在两边都只调用本模块：
//!
//! | 操作 | 编排函数 | 调用方 |
//! |------|----------|--------|
//! | 安装版本 | [`install_game`] + [`commit_install`] | `version_install`（Tauri/CLI） |
//! | 修复资源 | [`repair_instance`] / [`ensure_launch_ready`] | `instance_repair`（Tauri）、`resource repair`（CLI） |
//! | 启动游戏 | [`launch_game`] | `launch_start`（Tauri/CLI） |

use crate::CoreHandle;
use rmccc_auth::Account;
use rmccc_error::{Result, RmcccError};
use rmccc_launch::{ManagedProcess, ProcessListener};
use rmccc_version::GameRepository as _;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
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

/// 一次安装 / 修复的结果摘要（P1-3）。
///
/// 取代此前 "有失败也返回 Ok(())" 的隐式约定：调用方据此决定是否提交
/// profile、是否上报错误，而不是各自去猜下载批次的结果。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SetupReport {
    /// 本次涉及的下载条目数（0 表示本来就不缺东西）。
    pub total_files: usize,
    /// 下载失败的条目数（0 表示全部成功）。
    pub failed_files: u64,
}

impl SetupReport {
    /// 是否全部下载成功（无失败条目）。
    pub fn is_complete(&self) -> bool {
        self.failed_files == 0
    }

    /// 是否无事可做（没有需要下载/修复的条目）。
    pub fn is_noop(&self) -> bool {
        self.total_files == 0
    }
}

fn noop_progress() -> Arc<dyn Fn(&ProgressMsg) + Send + Sync> {
    Arc::new(|_| {})
}

fn emit(progress: Option<&ProgressCb>, msg: ProgressMsg) {
    if let Some(cb) = progress {
        cb(&msg);
    }
}

/// 下载批次的失败条目计数。
///
/// `DownloadManager` 不暴露批次结果查询，失败数只出现在**最终**进度回调
/// （`current_file == None`）。这里在进度回调里摘取一次，供 [`SetupReport`]
/// 汇总——两个适配层不再各自维护一份 `Arc<Mutex<Option<u64>>>`。
#[derive(Clone, Default)]
struct FailureTally(Arc<AtomicU64>);

impl FailureTally {
    fn observe(&self, p: &rmccc_download::DownloadProgress) {
        if p.current_file.is_none() {
            self.0.store(p.failed_files, Ordering::Relaxed);
        }
    }

    fn get(&self) -> u64 {
        self.0.load(Ordering::Relaxed)
    }
}

/// 把下载批次的底层进度回调包装为管道进度回调，并顺带记录失败数。
fn progress_download_cb(
    progress: Option<&ProgressCb>,
    label: &str,
    tally: &FailureTally,
) -> Arc<dyn Fn(rmccc_download::DownloadProgress) + Send + Sync> {
    let label = label.to_string();
    let cb = progress.cloned().unwrap_or_else(noop_progress);
    let tally = tally.clone();
    Arc::new(move |p| {
        tally.observe(&p);
        cb(&ProgressMsg::Download {
            label: label.clone(),
            progress: p,
        })
    })
}

fn repo_lock_error() -> RmcccError {
    RmcccError::Other("game repository lock poisoned".to_string())
}

/// 经 `download_provider` 为下载项注入镜像候选（P1-05）。
///
/// `candidates[0]` 作主 URL，其余作回退候选；Mojang 源下原样返回。
fn inject_candidates(
    provider: &dyn rmccc_download::provider::DownloadProvider,
    items: Vec<rmccc_download::DownloadItem>,
) -> Vec<rmccc_download::DownloadItem> {
    items
        .into_iter()
        .map(|mut item| {
            let candidates = provider.inject_url_with_candidates(&item.url);
            item.url = candidates
                .first()
                .cloned()
                .unwrap_or_else(|| item.url.clone());
            item.candidates = candidates.into_iter().skip(1).collect();
            item
        })
        .collect()
}

/// 解析游戏版本：优先本地 patched JSON（含 `inheritsFrom` 继承链合并），
/// 否则走 Mojang API（复用 `core.version_provider`，P2-01 一致性）。
///
/// 合并全部经 canonical 实现 `rmccc_version::merge`（审计批次 3 收敛）：
/// 此前这里的第三套合并逻辑 arguments 是**整体替换**、且不继承
/// assetIndex/javaVersion/downloads/logging——与本地仓库/网络链路的解析
/// 结果不一致。
pub async fn resolve_version(
    core: &CoreHandle,
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

    // 完整可解析的本地 JSON：无 inheritsFrom 直接用；**声明了 inheritsFrom
    // 同样必须合并**——此前此分支直接返回，父版本字段全部丢失（审计批次 3
    // 顺修的第二个 bug）。
    if let Ok(child) = serde_json::from_str::<rmccc_version::GameVersion>(&content) {
        return match child.inherits_from.clone() {
            None => Ok(child),
            Some(parent_id) => {
                // Perf audit C3: the parent of a modded instance (e.g. 1.20.1
                // under `1.20.1-forge`) is installed locally in practice —
                // resolve through the local repository (offline, no manifest
                // round-trip or deep copy) and only fall back to the network
                // provider when the parent is not indexed.
                let local_parent = core.game_repository.read().ok().and_then(|repo| {
                    if repo.has_instance(&parent_id) {
                        repo.get_resolved(&parent_id).ok().map(|r| r.launch)
                    } else {
                        None
                    }
                });
                let mut resolved = match local_parent {
                    Some(parent) => parent,
                    None => rmccc_version::resolve_version(&parent_id, provider).await?,
                };
                rmccc_version::merge_child_into_parent(&mut resolved, &child);
                resolved.id = game_version_id.to_string();
                Ok(resolved)
            }
        };
    }

    // 部分字段 JSON（安装器产物，缺 assetIndex 等必需字段、typed 解析失败）：
    // raw 解析后经 canonical 宽松合并，语义与 typed 合并完全一致。
    let raw: serde_json::Value = serde_json::from_str(&content)
        .map_err(|e| RmcccError::Other(format!("解析本地版本 JSON 失败: {e}")))?;
    let Some(parent_id) = raw.get("inheritsFrom").and_then(|v| v.as_str()) else {
        return rmccc_version::resolve_version(game_version_id, provider).await;
    };

    let mut resolved = rmccc_version::resolve_version(parent_id, provider).await?;
    rmccc_version::merge_partial_json_into_parent(&mut resolved, &raw);
    resolved.id = game_version_id.to_string();
    Ok(resolved)
}

// ────────────────────────────── Install ──────────────────────────────

/// 安装游戏资源（client jar + 依赖库 + 资源文件 + natives 解压）。
///
/// 三个下载类别合并为**一个批次**：进度单调（前端百分比不再回弹）、
/// 失败数只有一个来源，且磁盘空间可以按总大小预检。阶段文案仍经
/// [`ProgressMsg::Stage`] 上报，呈现层观感不变。
///
/// 只负责"让文件落盘 + 暴露结果"，不碰 profile：安装成功后的提交由
/// [`commit_install`] 完成（CLI 与 Tauri 共用同一条提交路径）。
pub async fn install_game(
    core: &CoreHandle,
    profile: &rmccc_profile::Profile,
    version_id: &str,
    progress: Option<&ProgressCb>,
) -> Result<SetupReport> {
    // 安装目标 = profile 的共享游戏根（profile 创建时即 `resolved_game_root()`，
    // 与 `GameRepository` 索引的是同一个目录，P2-9 单一取值点）。
    let game_dir = profile.game_dir.clone();
    let version = resolve_version(core, version_id, &game_dir).await?;

    for subdir in &["versions", "libraries", "assets"] {
        std::fs::create_dir_all(game_dir.join(subdir))?;
    }

    // 版本 JSON 先落盘：Minecraft 自己读 `versions/{id}/{id}.json` 且要求
    // Mojang 原始 camelCase 格式。已存在（patched/继承链）时不覆盖。
    write_version_json(&game_dir, version_id, &version)?;

    // 一次性解析全部下载项，便于按总大小做磁盘预检。
    let mut items = Vec::new();
    if let Some(item) = client_jar_item(&game_dir, version_id, &version)? {
        items.push(item);
    }
    items.extend(library_items(
        &version,
        &game_dir,
        &core.platform.descriptor,
    ));
    items.extend(asset_items(core, &version, &game_dir).await?);

    let total = items.len();
    if total == 0 {
        // Perf audit 1.4: zip extraction of multiple library jars is blocking
        // IO + CPU — run it on the blocking pool.
        run_extract_natives(
            &version,
            &game_dir,
            version_id,
            core.platform.descriptor,
            progress.cloned(),
        )
        .await?;
        tracing::info!(version = %version_id, "Nothing to download, version already complete");
        return Ok(SetupReport::default());
    }

    // 磁盘空间预检（含 20% 余量，对齐 HMCL）。
    let total_size: u64 = items.iter().filter_map(|i| i.size).sum();
    if total_size > 0 {
        rmccc_download::check_disk_space(&game_dir, (total_size as f64 * 1.2) as u64)?;
    }

    let items = inject_candidates(core.download_provider.as_ref(), items);
    emit(
        progress,
        ProgressMsg::Stage(format!("下载游戏文件 ({total} 个)...")),
    );
    let tally = FailureTally::default();
    let handle = core
        .download_manager
        .download_batch(items, progress_download_cb(progress, "游戏文件", &tally))
        .await?;
    core.download_manager.wait_for_batch(&handle.id).await;
    emit(
        progress,
        ProgressMsg::StageDone(format!("✅ 游戏文件下载完成 ({total} 个)")),
    );

    let failed_files = tally.get();
    let report = SetupReport {
        total_files: total,
        failed_files,
    };

    // 有失败条目时不解压 natives：那会拿残缺的依赖库去解压，报错信息比
    // "有 N 个文件下载失败" 更难定位。
    if report.is_complete() {
        run_extract_natives(
            &version,
            &game_dir,
            version_id,
            core.platform.descriptor,
            progress.cloned(),
        )
        .await?;
    }

    tracing::info!(
        profile_id = %profile.id,
        version = %version_id,
        total_files = total,
        failed_files,
        "Version resources installed"
    );
    Ok(report)
}

/// 安装完成后的提交：把 profile 指向新版本并刷新本地索引。
///
/// 重新从 store 读取 profile（而不是复用安装开始时的快照），避免用几秒前的
/// 旧数据覆盖用户在下载期间改过的设置。
pub fn commit_install(core: &CoreHandle, profile_id: &str, version_id: &str) -> Result<()> {
    let mut profile = core
        .profile_store
        .get(profile_id)?
        .ok_or_else(|| RmcccError::ProfileNotFound(profile_id.to_string()))?;
    profile.game_version = version_id.to_string();
    core.profile_store.save(&profile)?;
    refresh_repository(core)
}

/// 重新扫描本地版本索引（安装 / 修复后调用，让新文件被观察到）。
pub fn refresh_repository(core: &CoreHandle) -> Result<()> {
    core.game_repository
        .write()
        .map_err(|_| repo_lock_error())?
        .refresh()
}

/// 写入 `versions/{id}/{id}.json`（已存在则保留，patched JSON 不被冲掉）。
fn write_version_json(
    game_dir: &Path,
    version_id: &str,
    version: &rmccc_version::GameVersion,
) -> Result<()> {
    let path = game_dir
        .join("versions")
        .join(version_id)
        .join(format!("{version_id}.json"));
    // Perf audit C4: the existence check runs *before* any clone/serialize —
    // patched/inheritance JSON almost always exists, so the old order paid a
    // full `raw_json` clone (or struct serialization) per call for nothing.
    if path.exists() {
        return Ok(());
    }
    // 优先 Mojang 原始 JSON（camelCase）；本地解析出来的版本没有 raw_json 时
    // 退回结构体序列化（引擎仍可启动）。
    let content = match version.raw_json {
        Some(ref raw) => raw.to_string(),
        None => serde_json::to_string_pretty(version)?,
    };
    write_if_absent(&path, &content)
}

/// 写文件，但目标已存在时保持原样。
///
/// 这是 `write_version_json` 的核心语义：patched / 继承链 JSON 绝不能被
/// Mojang 原始元数据覆盖（`inheritsFrom` 一旦丢失，Forge/NeoForge 实例就废了）。
fn write_if_absent(path: &Path, content: &str) -> Result<()> {
    if path.exists() {
        return Ok(());
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, content)?;
    Ok(())
}

/// 客户端 JAR 下载项（元数据里没有 `downloads.client` 时为 `None`）。
fn client_jar_item(
    game_dir: &Path,
    version_id: &str,
    version: &rmccc_version::GameVersion,
) -> Result<Option<rmccc_download::DownloadItem>> {
    let Some(client_dl) = version.downloads.get("client") else {
        return Ok(None);
    };
    let dest = game_dir
        .join("versions")
        .join(version_id)
        .join(format!("{version_id}.jar"));
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent)?;
    }
    Ok(Some(rmccc_download::DownloadItem {
        url: client_dl.url.clone(),
        dest,
        expected_sha1: Some(client_dl.sha1.clone()),
        size: Some(client_dl.size),
        use_cache: true,
        candidates: Vec::new(),
    }))
}

/// 依赖库下载项（按当前平台过滤）。
fn library_items(
    version: &rmccc_version::GameVersion,
    game_dir: &Path,
    platform: &rmccc_platform::PlatformDescriptor,
) -> Vec<rmccc_download::DownloadItem> {
    rmccc_version::libraries::resolve_library_items(
        &version.libraries,
        &game_dir.join("libraries"),
        platform,
    )
}

/// 资源文件下载项（资源索引缺失时会先拉索引）。
async fn asset_items(
    core: &CoreHandle,
    version: &rmccc_version::GameVersion,
    game_dir: &Path,
) -> Result<Vec<rmccc_download::DownloadItem>> {
    rmccc_version::assets::resolve_asset_items(
        &core.http_client,
        &version.asset_index,
        &game_dir.join("assets"),
    )
    .await
}

/// 解压 native 库到 `versions/{id}/natives`。
fn extract_natives(
    version: &rmccc_version::GameVersion,
    game_dir: &Path,
    version_id: &str,
    platform: &rmccc_platform::PlatformDescriptor,
    progress: Option<&ProgressCb>,
) -> Result<()> {
    let natives_dir = game_dir.join("versions").join(version_id).join("natives");
    emit(progress, ProgressMsg::Stage("解压 Native 库...".into()));
    let count = rmccc_launch::decompress_natives(
        &version.libraries,
        &game_dir.join("libraries"),
        &natives_dir,
        false,
        false,
        platform,
    )?;
    let msg = if count > 0 {
        format!("✅ Native 库解压完成 ({count} 个文件)")
    } else {
        "✅ Native 库检查完成 (无需解压)".to_string()
    };
    emit(progress, ProgressMsg::StageDone(msg));
    Ok(())
}

// ────────────────────────────── Repair ──────────────────────────────

/// 完整性修复：按本地仓库解析出的 launch 视图，扫描 client jar / 资源索引与
/// objects / 依赖库，缺失则经 `download_manager` 补齐（含磁盘空间预检）。
///
/// 以**版本 id** 为入口（`GameRepository` 的实例视图），因此 Tauri 的
/// `instance_repair` 与 CLI 的 `resource repair`（经 [`ensure_launch_ready`]
/// 传入 profile 的版本）走的是同一份实现。
///
/// `verify` 决定"文件是否缺失"的校验强度：启动前路径传 `Size`（stat 级，
/// 每次 launch 都跑），显式修复命令传 `Hash`（全量 SHA1 复核）。
pub async fn repair_instance(
    core: &CoreHandle,
    version_id: &str,
    progress: Option<&ProgressCb>,
    verify: rmccc_version::completion::CompletionVerify,
) -> Result<SetupReport> {
    repair_instance_at(
        core,
        version_id,
        &core.config.resolved_game_root(),
        progress,
        verify,
    )
    .await
}

/// Repair a version under an explicit game root (external HMCL directories).
pub async fn repair_instance_at(
    core: &CoreHandle,
    version_id: &str,
    game_root: &Path,
    progress: Option<&ProgressCb>,
    verify: rmccc_version::completion::CompletionVerify,
) -> Result<SetupReport> {
    let primary_root = core.config.resolved_game_root();
    // 解析阶段持有 std RwLock 读锁，必须在 await 前释放。外置目录建临时仓库
    //（refresh 是同步磁盘扫描，放到 blocking 池）。
    let (game_root, resolved) = if game_root == primary_root.as_path() {
        let repo = core.game_repository.read().map_err(|_| repo_lock_error())?;
        let resolved = repo
            .get_resolved(version_id)
            .map_err(|e| RmcccError::Other(e.to_string()))?;
        (repo.game_dir().to_path_buf(), resolved)
    } else {
        let root = game_root.to_path_buf();
        let id = version_id.to_string();
        tokio::task::spawn_blocking(
            move || -> Result<(std::path::PathBuf, rmccc_version::ResolvedVersion)> {
                let mut repo = rmccc_version::DefaultGameRepository::new(root.clone());
                repo.refresh()
                    .map_err(|e| RmcccError::Other(e.to_string()))?;
                let resolved = repo
                    .get_resolved(&id)
                    .map_err(|e| RmcccError::Other(e.to_string()))?;
                Ok((root, resolved))
            },
        )
        .await
        .map_err(|e| RmcccError::Other(format!("repository join error: {e}")))??
    };

    let items = rmccc_version::completion::build_completion_downloads(
        &core.http_client,
        &game_root,
        &resolved,
        &core.platform.descriptor,
        verify,
    )
    .await?;
    let total = items.len();
    if total == 0 {
        tracing::info!(version = %version_id, "Instance is complete — nothing to repair");
        return Ok(SetupReport::default());
    }

    let total_bytes: u64 = items.iter().filter_map(|i| i.size).sum();
    if total_bytes > 0 {
        rmccc_download::check_disk_space(&game_root, total_bytes)?;
    }

    let items = inject_candidates(core.download_provider.as_ref(), items);
    emit(
        progress,
        ProgressMsg::Stage(format!("补齐缺失文件 ({total} 个)...")),
    );
    let tally = FailureTally::default();
    let handle = core
        .download_manager
        .download_batch(items, progress_download_cb(progress, "完整性修复", &tally))
        .await?;
    core.download_manager.wait_for_batch(&handle.id).await;

    // 重新扫描仓库，让缺失计数刷新。
    refresh_repository(core)?;

    emit(
        progress,
        ProgressMsg::StageDone(format!("✅ 缺失文件补齐完成 ({total} 个)")),
    );
    Ok(SetupReport {
        total_files: total,
        failed_files: tally.get(),
    })
}

/// 启动前资源完整性检查与自动补齐（对齐 HMCL `checkGameCompletionAsync`）。
///
/// [`repair_instance`] 的 profile 视图：用 profile 当前指向的版本做修复。
pub async fn ensure_launch_ready(
    core: &CoreHandle,
    profile: &rmccc_profile::Profile,
    progress: Option<&ProgressCb>,
) -> Result<SetupReport> {
    // 启动路径用 Size 级校验（每次 launch 都跑，只 stat 不读文件内容）；
    // 哈希级复核属于显式"检查/修复"命令。下载引擎对替换文件独立复核哈希，
    // 尺寸级修复也不会装上坏内容。
    repair_instance_at(
        core,
        &profile.game_version,
        &rmccc_launch::instance_game_root(&core.config, profile),
        progress,
        rmccc_version::completion::CompletionVerify::Size,
    )
    .await
}

// ────────────────────────────── Launch ──────────────────────────────

/// `extract_natives` on the blocking pool (perf audit 1.4): the extraction
/// unzips several library jars synchronously and must not occupy an async
/// worker. Progress callbacks keep working — the callback is `Send + Sync`.
async fn run_extract_natives(
    version: &rmccc_version::GameVersion,
    game_dir: &std::path::Path,
    version_id: &str,
    descriptor: rmccc_platform::PlatformDescriptor,
    progress: Option<ProgressCb>,
) -> Result<()> {
    let version = version.clone();
    let game_dir = game_dir.to_path_buf();
    let version_id = version_id.to_string();
    tokio::task::spawn_blocking(move || {
        extract_natives(
            &version,
            &game_dir,
            &version_id,
            &descriptor,
            progress.as_ref(),
        )
    })
    .await
    .map_err(|e| RmcccError::Other(format!("natives extraction join error: {e}")))?
}

/// 检测启动所需 Java（自定义路径优先，否则自动探测）。
///
/// 自定义路径不存在时返回 [`RmcccError::JavaNotFound`]——此前 CLI 检查而
/// Tauri 只检查"路径形态"，于是填了个错路径仍会走到 spawn 才失败。
fn detect_launch_java(
    platform: &std::sync::Arc<rmccc_platform::Platform>,
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
            if !bin.exists() {
                return Err(RmcccError::JavaNotFound);
            }
            // A8：**真实探测**自定义 Java（`java -XshowSettings:properties
            // -version`）。此前直接伪造 `{major_version: required_java,
            // is_64bit: true}`——用户装错 JDK 时所有按版本门控的 JVM 参数
            // （--illegal-access/--sun-misc-unsafe-memory-access/encoding 键）
            // 都按"期望版本"生成，且 JavaVersionMismatch 校验被完全跳过。
            let probed = rmccc_platform::java::probe_java_binary(&bin).ok_or_else(|| {
                RmcccError::Other(format!(
                    "custom java at {} is not a working JRE/JDK (probe failed)",
                    bin.display()
                ))
            })?;
            if probed.major_version != required_java {
                return Err(RmcccError::JavaVersionMismatch {
                    found: probed.major_version,
                    required: required_java,
                });
            }
            Ok(probed)
        }
        None => platform.java.find(required_java),
    }
}

/// Resolve auth credentials for launch — unified `Account::log_in()` (P2).
///
/// 有 active 账户时经 [`CoreContext::log_in_active_account`] 静默刷新并返回
/// `AuthInfo`（token 生命周期由 Account 内部承担）；无账户时回退到默认离线账户，
/// 对齐 HMCL：`user_type=msa` + 随机 token。
async fn resolve_launch_auth(core: &CoreHandle) -> Result<(String, String, String, String)> {
    match core.log_in_active_account().await? {
        Some(info) => Ok((info.username, info.uuid, info.access_token, info.user_type)),
        None => {
            let info = rmccc_auth::OfflineAccount::new("Player").play_offline();
            Ok((info.username, info.uuid, info.access_token, info.user_type))
        }
    }
}

/// 累计游玩时长记录：包装呈现层监听器，在游戏进程退出时把本次运行秒数写回
/// profile（HMCL 的"游玩时长"统计）。
///
/// 计时从 `launch()` 返回（进程已 spawn）开始，不含安装/修复/Java 探测时间；
/// 应用在游戏运行中退出时不会有 `on_exit`，本次时长不记（下次启动重新计时）。
struct PlaytimeListener {
    inner: Arc<dyn ProcessListener>,
    store: Arc<rmccc_profile::ProfileStore>,
    profile_id: String,
    started: std::sync::Mutex<Option<std::time::Instant>>,
}

impl PlaytimeListener {
    /// 进程真正起来后开始计时（launch 成功返回处调用）。
    fn mark_started(&self) {
        *self
            .started
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(std::time::Instant::now());
    }
}

impl ProcessListener for PlaytimeListener {
    fn set_process(&self, process: &ManagedProcess) {
        self.inner.set_process(process);
    }

    fn on_log(&self, line: String, is_stderr: bool) {
        self.inner.on_log(line, is_stderr);
    }

    fn on_exit(&self, exit_code: i32, exit_type: rmccc_launch::ExitType) {
        let seconds = self
            .started
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .take()
            .map(|start| start.elapsed().as_secs())
            .unwrap_or(0);
        if seconds > 0 {
            if let Err(e) = self.store.add_play_time(&self.profile_id, seconds) {
                tracing::warn!(
                    profile_id = %self.profile_id,
                    error = %e,
                    "Failed to record play time"
                );
            }
        }
        self.inner.on_exit(exit_code, exit_type);
    }
}

/// Resolve the launch view for a profile, honoring an external game root.
async fn resolve_launch_view(
    core: &CoreHandle,
    profile: &rmccc_profile::Profile,
) -> Result<rmccc_version::GameVersion> {
    if profile.external_root {
        let root = rmccc_launch::instance_game_root(&core.config, profile);
        let id = profile.game_version.clone();
        return tokio::task::spawn_blocking(move || -> Result<rmccc_version::GameVersion> {
            let mut repo = rmccc_version::DefaultGameRepository::new(root);
            repo.refresh()
                .map_err(|e| RmcccError::Other(e.to_string()))?;
            repo.get_resolved(&id)
                .map(|resolved| resolved.launch)
                .map_err(|e| RmcccError::Other(e.to_string()))
        })
        .await
        .map_err(|e| RmcccError::Other(format!("repository join error: {e}")))?;
    }

    let repo = core.game_repository.read().map_err(|_| repo_lock_error())?;
    repo.get_resolved(&profile.game_version)
        .map(|resolved| resolved.launch)
        .map_err(|e| RmcccError::Other(e.to_string()))
}

/// 启动游戏并返回已托管进程（命令行生成、native 解压、log4j 提取、环境变量
/// 注入、进程监控、输出转发全部由 [`rmccc_launch::DefaultLauncher`] 承担）。
///
/// **不阻塞**：进程刚起来就返回，呈现层自行决定等不等（CLI 经 exit 信号通道
/// 等，Tauri 直接返回给前端并由监听器上报退出事件）。启动成功后更新
/// `last_played`。
pub async fn launch_game(
    core: &CoreHandle,
    profile: &rmccc_profile::Profile,
    listener: Arc<dyn ProcessListener>,
    progress: Option<&ProgressCb>,
) -> Result<Arc<ManagedProcess>> {
    // 本地解析 launch 视图（HMCL resolved.launch，离线可用）。外置目录的
    // 版本不在主仓库索引里，按 profile 自己的 game_dir 临时建一份仓库。
    let game_version = resolve_launch_view(core, profile).await?;

    // Perf audit 1.5: the probe spawns real JVM subprocesses (100-500 ms
    // each) — run it on the blocking pool, not an async worker.
    let java_install = {
        let platform = core.platform.clone();
        let profile = profile.clone();
        let game_version = game_version.clone();
        tokio::task::spawn_blocking(move || detect_launch_java(&platform, &profile, &game_version))
            .await
            .map_err(|e| RmcccError::Other(format!("java detect join error: {e}")))?
    }?;
    tracing::info!(
        profile_id = %profile.id,
        java_version = java_install.major_version,
        java_path = %java_install.path.display(),
        "Launching Minecraft"
    );
    let (username, uuid_str, access_token, user_type) = resolve_launch_auth(core).await?;
    let auth = rmccc_launch::AuthInfo::new(username, uuid_str, access_token, user_type);
    let options = rmccc_launch::LaunchOptions::from_profile(
        profile,
        &core.config,
        java_install.clone(),
        core.platform.clone(),
    );

    // 计时的起点是进程成功 spawn（`launch()` 返回），因此包装监听器后才交给
    // launcher；退出时把本次运行时长累加到 profile（GUI/CLI/MCP 同一入口）。
    let playtime = Arc::new(PlaytimeListener {
        inner: listener,
        store: core.profile_store.clone(),
        profile_id: profile.id.clone(),
        started: std::sync::Mutex::new(None),
    });
    let listener: Arc<dyn ProcessListener> = playtime.clone();

    let launcher = rmccc_launch::DefaultLauncher::new(
        // `DefaultLauncher` 的 instance_id 用于定位 `versions/{id}/`（natives 等），
        // 因此传版本 id 而非 profile id。
        profile.game_version.clone(),
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
    playtime.mark_started();
    emit(progress, ProgressMsg::StageDone("游戏已启动。".into()));

    // last_played 在启动成功处更新（此前 CLI 在 exit 后、Tauri 在 spawn 后各写一遍）。
    if let Err(e) = core.profile_store.touch(&profile.id) {
        tracing::warn!(profile_id = %profile.id, error = %e, "Failed to update last_played");
    }

    Ok(managed)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 游玩时长：on_exit 把开始计时后的实际秒数累加进 profile，并继续转发
    /// 内部监听器（呈现层仍能收到退出事件）。
    #[test]
    fn test_playtime_listener_records_elapsed_and_forwards_exit() {
        #[derive(Default)]
        struct RecordingListener {
            exits: std::sync::atomic::AtomicUsize,
        }
        impl ProcessListener for RecordingListener {
            fn on_log(&self, _line: String, _is_stderr: bool) {}
            fn on_exit(&self, _code: i32, _kind: rmccc_launch::ExitType) {
                self.exits.fetch_add(1, Ordering::SeqCst);
            }
        }

        let dir = std::env::temp_dir().join(format!(
            "rmccc-playtime-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let store = Arc::new(rmccc_profile::ProfileStore::new(dir.clone()));
        let profile = rmccc_profile::Profile {
            id: "p1".into(),
            name: "Playtime".into(),
            game_version: "1.20.4".into(),
            play_time_seconds: 30,
            ..Default::default()
        };
        store.save(&profile).unwrap();

        let inner = Arc::new(RecordingListener::default());
        let listener = PlaytimeListener {
            inner: inner.clone(),
            store: store.clone(),
            profile_id: "p1".into(),
            started: std::sync::Mutex::new(Some(
                std::time::Instant::now() - std::time::Duration::from_secs(90),
            )),
        };
        listener.on_exit(0, rmccc_launch::ExitType::Normal);

        assert_eq!(
            store.get("p1").unwrap().unwrap().play_time_seconds,
            120,
            "90s session must add to the existing 30s"
        );
        assert_eq!(
            inner.exits.load(Ordering::SeqCst),
            1,
            "inner listener must still receive on_exit"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

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

    #[test]
    fn test_emit_without_callback_is_noop() {
        emit(None, ProgressMsg::StageDone("done".into()));
    }

    #[test]
    fn test_failure_tally_only_records_final_callback() {
        let tally = FailureTally::default();
        // 中间进度：失败数还未定稿，不应被记住。
        tally.observe(&rmccc_download::DownloadProgress {
            batch_id: "b".into(),
            completed_files: 1,
            total_files: 4,
            downloaded_bytes: 0,
            total_bytes: 0,
            failed_files: 3,
            current_file: Some("a.jar".into()),
        });
        assert_eq!(tally.get(), 0);

        // 最终进度：current_file == None，失败数定稿。
        tally.observe(&rmccc_download::DownloadProgress {
            batch_id: "b".into(),
            completed_files: 4,
            total_files: 4,
            downloaded_bytes: 0,
            total_bytes: 0,
            failed_files: 2,
            current_file: None,
        });
        assert_eq!(tally.get(), 2);
    }

    #[test]
    fn test_setup_report_classification() {
        assert!(SetupReport::default().is_noop());
        assert!(SetupReport::default().is_complete());

        let repaired = SetupReport {
            total_files: 5,
            failed_files: 0,
        };
        assert!(!repaired.is_noop());
        assert!(repaired.is_complete());

        let partial = SetupReport {
            total_files: 5,
            failed_files: 2,
        };
        assert!(!partial.is_noop());
        assert!(!partial.is_complete());
    }

    #[test]
    fn test_inject_candidates_without_mirror_keeps_url() {
        // 无镜像时 `DownloadProvider` 原样返回，候选列表为空（不 panic）。
        let provider = rmccc_download::provider::MojangDownloadProvider;
        let items = inject_candidates(
            &provider,
            vec![rmccc_download::DownloadItem {
                url: "https://example.invalid/a.jar".into(),
                dest: std::path::PathBuf::from("/tmp/a.jar"),
                expected_sha1: None,
                size: None,
                use_cache: false,
                candidates: vec!["stale".into()],
            }],
        );
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].url, "https://example.invalid/a.jar");
        assert!(items[0].candidates.is_empty());
    }

    #[test]
    fn test_write_if_absent_preserves_existing_file() {
        let dir = std::env::temp_dir().join(format!("rmccc-pipeline-{}", uuid_like()));
        let path = dir.join("versions").join("1.20.4").join("1.20.4.json");

        write_if_absent(&path, r#"{"id":"1.20.4"}"#).expect("first write");
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            r#"{"id":"1.20.4"}"#
        );

        // 第二次调用不能覆盖已有内容（patched JSON 必须保留）。
        write_if_absent(&path, r#"{"id":"patched"}"#).expect("second write");
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            r#"{"id":"1.20.4"}"#
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    // ── 编排测试（无网络）：覆盖 GUI / CLI 共用的三条链路 ──

    /// 最小可用版本 JSON：无 client 下载、无依赖库、无资源索引，
    /// 因此 `install_game` / `repair_instance` 都不会触网。
    const MINIMAL_VERSION_JSON: &str = r#"{
        "id": "1.20.4",
        "type": "release",
        "mainClass": "net.minecraft.client.main.Main",
        "assetIndex": { "id": "", "url": "", "sha1": "", "size": 0, "total_size": 0 },
        "downloads": {},
        "libraries": [],
        "javaVersion": null
    }"#;

    fn test_env(tag: &str) -> (std::path::PathBuf, crate::CoreContext) {
        let dir = std::env::temp_dir().join(format!("rmccc-{tag}-{}", uuid_like()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("create data dir");
        let core = crate::CoreContext::new(&dir).expect("CoreContext");
        (dir, core)
    }

    fn seed_version(core: &crate::CoreContext, version_id: &str) {
        let dir = core
            .config
            .resolved_game_root()
            .join("versions")
            .join(version_id);
        std::fs::create_dir_all(&dir).expect("version dir");
        std::fs::write(dir.join(format!("{version_id}.json")), MINIMAL_VERSION_JSON)
            .expect("version json");
    }

    fn seed_profile(
        core: &crate::CoreContext,
        id: &str,
        version_id: &str,
    ) -> rmccc_profile::Profile {
        let profile = rmccc_profile::Profile {
            id: id.to_string(),
            name: format!("test-{id}"),
            game_version: version_id.to_string(),
            game_dir: core.config.resolved_game_root(),
            ..Default::default()
        };
        core.profile_store.save(&profile).expect("save profile");
        profile
    }

    /// 外置目录（HMCL）的版本不在主仓库索引里；launch 视图必须按 profile
    /// 自己的 game_dir 临时建仓库解析，而不是回落到主游戏根。
    #[tokio::test]
    async fn test_external_profile_resolves_from_its_own_root() {
        let (dir, core) = test_env("external-resolve");
        let external_root = dir.join("hmcl/.minecraft");
        let version_id = "1.20.4-Forge";
        let version_dir = external_root.join("versions").join(version_id);
        std::fs::create_dir_all(&version_dir).expect("external version dir");
        let json = MINIMAL_VERSION_JSON.replace("\"1.20.4\"", "\"1.20.4-Forge\"");
        std::fs::write(version_dir.join(format!("{version_id}.json")), json).expect("json");

        let profile = rmccc_profile::Profile {
            game_version: version_id.to_string(),
            game_dir: external_root.clone(),
            external_root: true,
            ..Default::default()
        };

        let view = resolve_launch_view(&core.handle(), &profile)
            .await
            .expect("external view");
        assert_eq!(view.id, version_id);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn test_install_game_noop_when_version_complete() {
        let (dir, core) = test_env("install-noop");
        seed_version(&core, "1.20.4");
        let profile = seed_profile(&core, "p-install", "1.20.4");

        let report = install_game(&core.handle(), &profile, "1.20.4", None)
            .await
            .expect("install_game");

        assert!(report.is_complete(), "no failed files expected");
        assert!(report.is_noop(), "complete version needs no downloads");
        // 版本 JSON 仍在（不被覆盖）。
        let json = core
            .config
            .resolved_game_root()
            .join("versions")
            .join("1.20.4")
            .join("1.20.4.json");
        assert!(json.exists());

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn test_repair_instance_noop_when_nothing_downloadable() {
        let (dir, core) = test_env("repair-noop");
        seed_version(&core, "1.20.4");
        refresh_repository(&core.handle()).expect("refresh");

        let report = repair_instance(
            &core.handle(),
            "1.20.4",
            None,
            rmccc_version::completion::CompletionVerify::Hash,
        )
        .await
        .expect("repair_instance");

        assert!(report.is_complete());
        assert!(report.is_noop());

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn test_ensure_launch_ready_delegates_to_repair() {
        let (dir, core) = test_env("ensure-ready");
        seed_version(&core, "1.20.4");
        refresh_repository(&core.handle()).expect("refresh");
        let profile = seed_profile(&core, "p-ensure", "1.20.4");

        let report = ensure_launch_ready(&core.handle(), &profile, None)
            .await
            .expect("ensure_launch_ready");

        assert!(report.is_noop());

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_commit_install_points_profile_at_version_and_refreshes_index() {
        let (dir, core) = test_env("commit-install");
        seed_version(&core, "1.20.4");
        let profile = seed_profile(&core, "p-commit", "0.0.0-nonexistent");

        commit_install(&core.handle(), "p-commit", "1.20.4").expect("commit_install");

        let updated = core
            .profile_store
            .get(&profile.id)
            .expect("get profile")
            .expect("profile exists");
        assert_eq!(updated.game_version, "1.20.4");
        // 索引已刷新：新版本可被仓库解析。
        let repo = core.game_repository.read().expect("repo lock");
        assert!(repo.get_instance_ids().iter().any(|id| id == "1.20.4"));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_commit_install_rejects_unknown_profile() {
        let (dir, core) = test_env("commit-missing");
        let err = commit_install(&core.handle(), "nope", "1.20.4").expect_err("must fail");
        assert!(matches!(err, RmcccError::ProfileNotFound(_)));
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// 无依赖的唯一后缀（避免引入 uuid 依赖）。
    fn uuid_like() -> String {
        use std::time::{SystemTime, UNIX_EPOCH};
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        format!("{}-{}", std::process::id(), nanos)
    }
}
