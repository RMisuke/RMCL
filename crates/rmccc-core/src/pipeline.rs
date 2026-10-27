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
                let local_parent = core.instances.lock().ok().and_then(|mut instances| {
                    instances
                        .resolved_in_root(game_dir, &parent_id)
                        .ok()
                        .map(|resolved| resolved.launch)
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
    game_dir: &Path,
    version_id: &str,
    progress: Option<&ProgressCb>,
) -> Result<SetupReport> {
    let game_dir = game_dir.to_path_buf();
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
        version = %version_id,
        game_dir = %game_dir.display(),
        total_files = total,
        failed_files,
        "Version resources installed"
    );
    Ok(report)
}

/// 安装完成后的提交：确保实例设置存在、固定隔离策略并刷新目录索引。
///
/// 版本目录本身就是实例（ADR-016），所以这里不再"把 profile 指向新版本"。
pub fn commit_install(
    core: &CoreHandle,
    directory_id: &str,
    version_id: &str,
) -> Result<rmccc_instance::InstanceDetail> {
    let mut instances = core.instances.lock().map_err(|_| repo_lock_error())?;
    instances.refresh(Some(directory_id), true)?;
    instances.ensure_instance(directory_id, version_id)
}

/// 重新扫描某个目录的版本索引（安装 / 修复后调用）。
pub fn refresh_repository(core: &CoreHandle, directory_id: &str) -> Result<()> {
    core.instances
        .lock()
        .map_err(|_| repo_lock_error())?
        .refresh(Some(directory_id), true)
        .map(|_| ())
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
    game_dir: &Path,
    version_id: &str,
    progress: Option<&ProgressCb>,
    verify: rmccc_version::completion::CompletionVerify,
) -> Result<SetupReport> {
    repair_instance_at(core, version_id, game_dir, progress, verify).await
}

/// Repair a version under an explicit game root (external HMCL directories).
pub async fn repair_instance_at(
    core: &CoreHandle,
    version_id: &str,
    game_root: &Path,
    progress: Option<&ProgressCb>,
    verify: rmccc_version::completion::CompletionVerify,
) -> Result<SetupReport> {
    // 解析阶段只持实例服务锁做同步读取，随后立即释放；下载/解压期间不持锁。
    let (game_root, resolved) = {
        let mut instances = core.instances.lock().map_err(|_| repo_lock_error())?;
        let resolved = instances.resolved_in_root(game_root, version_id)?;
        (game_root.to_path_buf(), resolved)
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

    // 版本文件已补齐；调用方负责刷新仓库/实例列表。

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
    instance: &rmccc_instance::InstanceDetail,
    progress: Option<&ProgressCb>,
) -> Result<SetupReport> {
    // 启动路径用 Size 级校验（每次 launch 都跑，只 stat 不读文件内容）；
    // 哈希级复核属于显式"检查/修复"命令。下载引擎对替换文件独立复核哈希，
    // 尺寸级修复也不会装上坏内容。
    repair_instance_at(
        core,
        &instance.summary.id,
        &instance.paths.game_dir,
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

/// 检测启动所需 Java。
///
/// 选择顺序由实例生效设置决定（HMCL `JavaVersionType`）：自动探测 / 自定义
/// 路径 / 指定主版本 / 指定已检测运行时。自定义路径仍做真实探测与版本门控
/// （`not_check_jvm` 可关闭门控）。
fn detect_launch_java(
    platform: &std::sync::Arc<rmccc_platform::Platform>,
    instance: &rmccc_instance::InstanceDetail,
    game_version: &rmccc_version::GameVersion,
) -> Result<rmccc_launch::JavaInstallation> {
    use rmccc_instance::JavaType;

    let effective = &instance.effective;
    let required_java = game_version
        .java_version
        .as_ref()
        .map(|j| j.major_version)
        .unwrap_or_else(|| rmccc_version::java_version_for_mc(&instance.summary.game_version));

    match effective.java_type {
        JavaType::Auto => platform.java.find(required_java),
        JavaType::Custom => {
            let raw = effective.custom_java_path.trim();
            if raw.is_empty() {
                return Err(RmcccError::JavaNotFound);
            }
            let home = std::path::PathBuf::from(raw);
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
            // Java is forward compatible in the launcher's selection model:
            // only reject a runtime that is *older* than the version the game
            // manifest requires (Java 25 satisfies "Java 21+"). The previous
            // `!=` check rejected perfectly valid newer runtimes.
            if !effective.not_check_jvm && probed.major_version < required_java {
                return Err(RmcccError::JavaVersionMismatch {
                    found: probed.major_version,
                    required: required_java,
                });
            }
            if probed.major_version > required_java {
                tracing::warn!(
                    found = probed.major_version,
                    required = required_java,
                    "Using a newer Java runtime than the version requires"
                );
            }
            Ok(probed)
        }
        JavaType::Version => {
            let wanted = effective
                .custom_java_version
                .trim()
                .parse::<u32>()
                .map_err(|_| {
                    RmcccError::Validation(format!(
                        "invalid Java version: {:?}",
                        effective.custom_java_version
                    ))
                })?;
            let all = platform.java.detect_all()?;
            all.into_iter()
                .find(|java| java.major_version == wanted)
                .ok_or(RmcccError::JavaNotFound)
        }
        JavaType::Detected => {
            let wanted = effective.detected_java_version.trim();
            if wanted.is_empty() {
                return platform.java.find(required_java);
            }
            let all = platform.java.detect_all()?;
            // Version match is the portable part; the path hash is a
            // best-effort tiebreaker that we do not recompute here.
            all.into_iter()
                .find(|java| java.major_version.to_string() == wanted)
                .or_else(|| platform.java.find(required_java).ok())
                .ok_or(RmcccError::JavaNotFound)
        }
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
/// 实例设置（HMCL 的"游玩时长"统计）。
///
/// 计时从 `launch()` 返回（进程已 spawn）开始，不含安装/修复/Java 探测时间；
/// 应用在游戏运行中退出时不会有 `on_exit`，本次时长不记（下次启动重新计时）。
struct PlaytimeListener {
    inner: Arc<dyn ProcessListener>,
    instances: Arc<std::sync::Mutex<rmccc_instance::InstanceService>>,
    key: String,
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
            if let Ok(mut instances) = self.instances.lock() {
                if let Err(e) = instances.add_play_time(&self.key, seconds) {
                    tracing::warn!(
                        instance = %self.key,
                        error = %e,
                        "Failed to record play time"
                    );
                }
            }
        }
        self.inner.on_exit(exit_code, exit_type);
    }
}

/// 启动游戏并返回已托管进程（命令行生成、native 解压、log4j 提取、环境变量
/// 注入、进程监控、输出转发全部由 [`rmccc_launch::DefaultLauncher`] 承担）。
///
/// **不阻塞**：进程刚起来就返回，呈现层自行决定等不等（CLI 经 exit 信号通道
/// 等，Tauri 直接返回给前端并由监听器上报退出事件）。启动成功后更新
/// `last_played`。
pub async fn launch_game(
    core: &CoreHandle,
    instance: &rmccc_instance::InstanceDetail,
    listener: Arc<dyn ProcessListener>,
    progress: Option<&ProgressCb>,
) -> Result<Arc<ManagedProcess>> {
    // 本地解析 launch 视图（HMCL resolved.launch，离线可用）。实例服务缓存
    // 每个游戏目录的仓库，外部目录同样走同一入口。
    let resolved = {
        let mut instances = core.instances.lock().map_err(|_| repo_lock_error())?;
        instances
            .resolved_version(&instance.summary.key)
            .map(|(_, resolved)| resolved)?
    };
    let game_version = resolved.launch;

    // Perf audit 1.5: the probe spawns real JVM subprocesses (100-500 ms
    // each) — run it on the blocking pool, not an async worker.
    let java_install = {
        let platform = core.platform.clone();
        let instance = instance.clone();
        let game_version = game_version.clone();
        tokio::task::spawn_blocking(move || detect_launch_java(&platform, &instance, &game_version))
            .await
            .map_err(|e| RmcccError::Other(format!("java detect join error: {e}")))?
    }?;
    tracing::info!(
        instance = %instance.summary.key,
        java_version = java_install.major_version,
        java_path = %java_install.path.display(),
        "Launching Minecraft"
    );
    let (username, uuid_str, access_token, user_type) = resolve_launch_auth(core).await?;
    let auth = rmccc_launch::AuthInfo::new(username, uuid_str, access_token, user_type);
    let options = rmccc_launch::LaunchOptions::from_instance(
        instance,
        java_install.clone(),
        core.platform.clone(),
    );

    // 计时的起点是进程成功 spawn（`launch()` 返回），因此包装监听器后才交给
    // launcher；退出时把本次运行时长累加到实例设置（GUI/CLI/MCP 同一入口）。
    let playtime = Arc::new(PlaytimeListener {
        inner: listener,
        instances: core.instances.clone(),
        key: instance.summary.key.clone(),
        started: std::sync::Mutex::new(None),
    });
    let listener: Arc<dyn ProcessListener> = playtime.clone();

    let launcher = rmccc_launch::DefaultLauncher::new(
        instance.summary.id.clone(),
        game_version,
        auth,
        options,
        Some(listener),
    );

    emit(
        progress,
        ProgressMsg::Stage(format!(
            "🚀 启动 Minecraft {} (Java {})",
            instance.summary.game_version, java_install.major_version
        )),
    );

    let managed = launcher
        .launch()
        .await
        .map_err(|e| RmcccError::Other(e.to_string()))?;
    playtime.mark_started();
    emit(progress, ProgressMsg::StageDone("游戏已启动。".into()));

    // last_played 在启动成功处更新。
    if let Ok(mut instances) = core.instances.lock() {
        if let Err(e) = instances.touch(&instance.summary.key) {
            tracing::warn!(instance = %instance.summary.key, error = %e, "Failed to update last_played");
        }
    }

    Ok(managed)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 游玩时长：on_exit 把开始计时后的实际秒数累加进实例设置，并继续转发
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

        let (dir, core) = test_env("playtime");
        seed_version(&core, "1.20.4");
        {
            let mut instances = core.instances.lock().unwrap();
            instances.refresh(Some("default"), true).unwrap();
            let detail = instances.ensure_instance("default", "1.20.4").unwrap();
            let mut settings = detail.settings;
            settings.play_time_seconds = 30;
            settings
                .save(&detail.paths.instance_dir, core.platform.fs.as_ref())
                .unwrap();
        }

        let inner = Arc::new(RecordingListener::default());
        let listener = PlaytimeListener {
            inner: inner.clone(),
            instances: core.instances.clone(),
            key: "default/1.20.4".into(),
            started: std::sync::Mutex::new(Some(
                std::time::Instant::now() - std::time::Duration::from_secs(90),
            )),
        };
        listener.on_exit(0, rmccc_launch::ExitType::Normal);

        let detail = core
            .instances
            .lock()
            .unwrap()
            .get("default/1.20.4")
            .unwrap();
        assert_eq!(
            detail.settings.play_time_seconds, 120,
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

        write_if_absent(&path, r#"{"id":"patched"}"#).expect("second write");
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            r#"{"id":"1.20.4"}"#
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    // ── 编排测试（无网络）：覆盖 GUI / CLI 共用的三条链路 ──

    const MINIMAL_VERSION_JSON: &str = r#"{
        "id": "1.20.4",
        "type": "release",
        "mainClass": "net.minecraft.client.main.Main",
        "assetIndex": { "id": "", "url": "", "sha1": "", "size": 0, "totalSize": 0 },
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

    fn game_root(core: &crate::CoreContext) -> std::path::PathBuf {
        core.instances
            .lock()
            .unwrap()
            .selected_directory()
            .path
            .clone()
    }

    fn seed_version(core: &crate::CoreContext, version_id: &str) {
        let dir = game_root(core).join("versions").join(version_id);
        std::fs::create_dir_all(&dir).expect("version dir");
        let json = MINIMAL_VERSION_JSON.replace("\"1.20.4\"", &format!("\"{version_id}\""));
        std::fs::write(dir.join(format!("{version_id}.json")), json).expect("version json");
    }

    /// 外置目录（HMCL）必须能通过实例服务解析，而不是回落到主游戏根。
    #[test]
    fn test_external_directory_resolves_from_its_own_root() {
        let (dir, core) = test_env("external-resolve");
        let external_root = dir.join("hmcl/.minecraft");
        let version_id = "1.20.4-Forge";
        let version_dir = external_root.join("versions").join(version_id);
        std::fs::create_dir_all(&version_dir).expect("external version dir");
        let json = MINIMAL_VERSION_JSON.replace("\"1.20.4\"", "\"1.20.4-Forge\"");
        std::fs::write(version_dir.join(format!("{version_id}.json")), json).expect("json");

        let mut instances = core.instances.lock().unwrap();
        let directory = instances
            .add_directory(
                "HMCL",
                &external_root,
                rmccc_instance::GameDirectoryKind::External,
            )
            .expect("add directory");
        let resolved = instances
            .resolved_in_root(&external_root, version_id)
            .expect("external resolve");
        assert_eq!(resolved.launch.id, version_id);
        assert!(instances
            .list(Some(directory.id.as_str()), false, false)
            .unwrap()
            .iter()
            .any(|i| i.id == version_id));
        drop(instances);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn test_install_game_noop_when_version_complete() {
        let (dir, core) = test_env("install-noop");
        seed_version(&core, "1.20.4");
        let root = game_root(&core);

        let report = install_game(&core.handle(), &root, "1.20.4", None)
            .await
            .expect("install_game");

        assert!(report.is_complete(), "no failed files expected");
        assert!(report.is_noop(), "complete version needs no downloads");
        let json = root.join("versions").join("1.20.4").join("1.20.4.json");
        assert!(json.exists());

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn test_repair_instance_noop_when_nothing_downloadable() {
        let (dir, core) = test_env("repair-noop");
        seed_version(&core, "1.20.4");
        let root = game_root(&core);
        refresh_repository(&core.handle(), "default").expect("refresh");

        let report = repair_instance(
            &core.handle(),
            &root,
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
        refresh_repository(&core.handle(), "default").expect("refresh");
        let detail = core
            .instances
            .lock()
            .unwrap()
            .ensure_instance("default", "1.20.4")
            .expect("ensure");

        let report = ensure_launch_ready(&core.handle(), &detail, None)
            .await
            .expect("ensure_launch_ready");

        assert!(report.is_noop());

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_commit_install_creates_instance_and_refreshes_index() {
        let (dir, core) = test_env("commit-install");
        seed_version(&core, "1.20.4");

        let detail = commit_install(&core.handle(), "default", "1.20.4").expect("commit_install");
        assert_eq!(detail.summary.id, "1.20.4");
        assert_eq!(detail.summary.key, "default/1.20.4");
        assert!(detail.settings_exists);

        let mut instances = core.instances.lock().unwrap();
        assert!(instances
            .list(None, false, false)
            .unwrap()
            .iter()
            .any(|i| i.id == "1.20.4"));
        drop(instances);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_commit_install_rejects_unknown_directory() {
        let (dir, core) = test_env("commit-missing");
        let err = commit_install(&core.handle(), "nope", "1.20.4").expect_err("must fail");
        assert!(matches!(err, RmcccError::Validation(_)));
        let _ = std::fs::remove_dir_all(&dir);
    }

    fn uuid_like() -> String {
        use std::time::{SystemTime, UNIX_EPOCH};
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        format!("{}-{}", std::process::id(), nanos)
    }
}
