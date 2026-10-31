//! Modpack install orchestration — the Rust counterpart of HMCL's
//! `ModpackHelper` + per-provider install tasks.
//!
//! `rmccc-modpack` parses the archive and describes *what* to install; this
//! module installs the base game, the loader, override files, pack downloads,
//! metadata (`modpack.cfg` + provider manifest), and instance settings.
//!
//! Install order mirrors HMCL:
//! 1. parse manifest + create the version-backed instance;
//! 2. base game (client/libs/assets);
//! 3. loader (Forge/NeoForge installer or patch loader);
//! 4. extra manifest libraries (MCBBS `libraries`);
//! 5. overrides (`overrides/`, `client-overrides/`, `.minecraft/`, …);
//! 6. manifest-declared files (Curse/Modrinth/FileApi);
//! 7. `modpack.cfg`, provider manifest, icon and launch settings.

use crate::pipeline::{self, ProgressCb, ProgressMsg};
use crate::CoreHandle;
use rmccc_error::{Result, RmcccError};
use rmccc_instance::{
    fields as instance_fields, GameDirectoryKind, InstanceDetail, InstanceIcon,
    InstanceSettingsPatch, IsolationPolicy, JavaType, WindowType,
};
use rmccc_modloader::ModLoaderType;
use rmccc_modpack::{
    extract, read_modpack, safe_join, suggest_instance_id, write_modpack_metadata, CurseApi,
    LoaderKind, Modpack, ModpackInfo, ModpackKind, PackSettings,
};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;

/// Progress reported by the modpack pipeline.
#[derive(Debug, Clone)]
pub enum ModpackProgress {
    /// A named stage with a creeping overall percentage.
    Stage {
        phase: &'static str,
        percent: u8,
        message: String,
    },
    /// Override extraction counters.
    Overrides { completed: u64, total: u64 },
    /// Manifest file download counters.
    Downloads {
        completed: u64,
        total: u64,
        current: Option<String>,
    },
}

/// Callback used by the Tauri layer to emit `modpack://progress`.
pub type ModpackProgressCb = Arc<dyn Fn(ModpackProgress) + Send + Sync>;

/// Request to install a modpack archive.
#[derive(Debug, Clone)]
pub struct ModpackInstallRequest {
    pub archive: PathBuf,
    pub directory_id: String,
    /// User-chosen instance name/id. `None` derives it from the pack name.
    pub instance_name: Option<String>,
}

/// Result of a successful install.
#[derive(Debug, Clone)]
pub struct InstalledModpack {
    /// `{directory}/{id}` of the new instance (manual packs may have none).
    pub key: Option<String>,
    pub instance_id: Option<String>,
    pub directory_id: String,
    pub name: String,
    pub kind: ModpackKind,
    /// Set for manually-created packs: the newly registered game directory.
    pub manual_directory: Option<String>,
    pub warnings: Vec<String>,
}

/// Inspect a local archive without network or disk changes.
pub fn inspect_modpack(path: &std::path::Path) -> Result<ModpackInfo> {
    rmccc_modpack::inspect_modpack(path)
}

/// Suggested instance id/name from an inspect result.
pub fn suggested_instance_name(info: &ModpackInfo, archive: &std::path::Path) -> String {
    let fallback = archive
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or("Modpack");
    suggest_instance_id(&info.name, fallback)
}

fn emit(progress: Option<&ModpackProgressCb>, event: ModpackProgress) {
    if let Some(cb) = progress {
        cb(event);
    }
}

fn cancelled(cancel: &Option<Arc<AtomicBool>>) -> bool {
    cancel
        .as_ref()
        .is_some_and(|flag| flag.load(Ordering::Relaxed))
}

fn cancel_error() -> RmcccError {
    RmcccError::Other("modpack installation cancelled".to_string())
}

fn lock_error() -> RmcccError {
    RmcccError::Other("instance service lock poisoned".to_string())
}

fn join_error(error: tokio::task::JoinError) -> RmcccError {
    RmcccError::Other(format!("background task failed: {error}"))
}

/// Install a modpack into a registered game directory.
///
/// `cancel` is polled between stages; an in-flight download batch is cancelled
/// by a watcher task.
pub async fn install_modpack(
    core: &CoreHandle,
    request: ModpackInstallRequest,
    progress: Option<ModpackProgressCb>,
    cancel: Option<Arc<AtomicBool>>,
) -> Result<InstalledModpack> {
    emit(
        progress.as_ref(),
        ModpackProgress::Stage {
            phase: "prepare",
            percent: 1,
            message: "读取整合包清单...".to_string(),
        },
    );
    let archive = request.archive.clone();
    let mut pack = tokio::task::spawn_blocking(move || read_modpack(&archive))
        .await
        .map_err(join_error)??;
    if cancelled(&cancel) {
        return Err(cancel_error());
    }
    if pack.kind == ModpackKind::Manual {
        return install_manual_modpack(core, request, pack, progress, cancel).await;
    }

    let mc_version = pack
        .game_version
        .clone()
        .ok_or_else(|| RmcccError::Validation("modpack has no Minecraft version".into()))?;
    let fallback = request
        .archive
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or("Modpack");
    let instance_name = request
        .instance_name
        .as_deref()
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .unwrap_or(pack.name.as_str());
    let mut instance_id = suggest_instance_id(instance_name, fallback);
    if pack.primary_loader().is_some() && instance_id == mc_version {
        // A loader instance must live in its own version folder; keep the
        // display name but make the id distinct from the base game.
        instance_id = suggest_instance_id(&format!("{instance_id}-pack"), fallback);
    }
    rmccc_instance::validate_instance_id(&instance_id)?;

    let game_dir = {
        let service = core.instances.lock().map_err(|_| lock_error())?;
        service.game_dir(&request.directory_id)?
    };
    let target_json = rmccc_version::file_layout::instance_json(&game_dir, &instance_id);
    let target_root = rmccc_version::file_layout::instance_root(&game_dir, &instance_id);
    if target_json.exists() || target_root.exists() {
        return Err(RmcccError::Validation(format!(
            "instance '{instance_id}' already exists in this directory"
        )));
    }

    let display_name = instance_name.to_string();
    let result = install_normal(
        core,
        &mut pack,
        &request.directory_id,
        &game_dir,
        &mc_version,
        &instance_id,
        &display_name,
        progress.clone(),
        cancel.clone(),
    )
    .await;

    match result {
        Ok(installed) => Ok(installed),
        Err(error) => {
            // HMCL removes the instance it was creating unless the failure is
            // a tolerable "pack completion" error. We only clean the target and
            // leave the shared base version in place.
            cleanup_instance(core, &request.directory_id, &instance_id);
            Err(error)
        }
    }
}

#[allow(clippy::too_many_arguments)]
async fn install_normal(
    core: &CoreHandle,
    pack: &mut Modpack,
    directory_id: &str,
    game_dir: &std::path::Path,
    mc_version: &str,
    instance_id: &str,
    display_name: &str,
    progress: Option<ModpackProgressCb>,
    cancel: Option<Arc<AtomicBool>>,
) -> Result<InstalledModpack> {
    let primary = pack.primary_loader().cloned();
    let base_id = if primary.is_some() {
        mc_version.to_string()
    } else {
        instance_id.to_string()
    };

    // ── 1. Vanilla game ─────────────────────────────────────────────
    emit(
        progress.as_ref(),
        ModpackProgress::Stage {
            phase: "game",
            percent: 4,
            message: format!("安装 Minecraft {mc_version}..."),
        },
    );
    let game_progress = make_pipeline_progress(progress.clone(), "game", 4, 36);
    let report =
        pipeline::install_game_into(core, game_dir, mc_version, &base_id, Some(&game_progress))
            .await?;
    if !report.is_complete() {
        return Err(RmcccError::DownloadFailed {
            url: "Minecraft game files".to_string(),
            reason: format!(
                "{} of {} files failed",
                report.failed_files, report.total_files
            ),
        });
    }
    if cancelled(&cancel) {
        return Err(cancel_error());
    }
    let mut detail = pipeline::commit_install(core, directory_id, &base_id)?;
    emit(
        progress.as_ref(),
        ModpackProgress::Stage {
            phase: "game",
            percent: 36,
            message: "游戏文件完成".to_string(),
        },
    );

    // ── 2. Loader ───────────────────────────────────────────────────
    if let Some(loader) = primary {
        let loader_type = ModLoaderType::from_str(loader.kind.as_str()).ok_or_else(|| {
            RmcccError::Validation(format!("unsupported loader: {}", loader.kind.as_str()))
        })?;
        if !loader_type.is_installable() {
            return Err(RmcccError::UnsupportedPlatform(format!(
                "{} 安装器当前不可用",
                loader_type.display_name()
            )));
        }
        emit(
            progress.as_ref(),
            ModpackProgress::Stage {
                phase: "loader",
                percent: 40,
                message: format!("安装 {} {}...", loader_type.display_name(), loader.version),
            },
        );
        let loader_progress = make_loader_progress(progress.clone());
        let installed = crate::modloader::install_loader(
            core,
            &detail,
            &loader_type,
            &loader.version,
            Some(loader_progress),
        )
        .await?;
        if cancelled(&cancel) {
            return Err(cancel_error());
        }
        let mut service = core.instances.lock().map_err(|_| lock_error())?;
        detail = if installed.version_id != instance_id {
            service.rename(&installed.instance_key, instance_id)?
        } else {
            service.get(&installed.instance_key)?
        };
        drop(service);
        emit(
            progress.as_ref(),
            ModpackProgress::Stage {
                phase: "loader",
                percent: 60,
                message: format!("{} 安装完成", loader_type.display_name()),
            },
        );
    }
    let key = detail.summary.key.clone();
    let skipped_warnings: Vec<String> = pack
        .skipped_loaders()
        .into_iter()
        .map(|skipped| {
            format!(
                "整合包还声明了 {} {}；RMCL 当前一次只安装一个主加载器",
                skipped.kind.display_name(),
                skipped.version
            )
        })
        .collect();
    pack.warnings.extend(skipped_warnings);

    // ── 3. Extra manifest libraries (MCBBS) ─────────────────────────
    if !pack.extra_libraries.is_empty() {
        emit(
            progress.as_ref(),
            ModpackProgress::Stage {
                phase: "libraries",
                percent: 61,
                message: "补充整合包依赖库...".to_string(),
            },
        );
        append_extra_libraries(&detail.paths.json_file, &pack.extra_libraries)?;
        {
            let mut service = core.instances.lock().map_err(|_| lock_error())?;
            service.refresh(Some(directory_id), true)?;
            detail = service.get(&key)?;
        }
        let repair_progress = make_pipeline_progress(progress.clone(), "libraries", 61, 65);
        let repaired = pipeline::repair_instance(
            core,
            game_dir,
            instance_id,
            Some(&repair_progress),
            rmccc_version::completion::CompletionVerify::Hash,
        )
        .await?;
        if !repaired.is_complete() {
            pack.warnings
                .push(format!("{} 个整合包依赖库下载失败", repaired.failed_files));
        }
    }

    // ── 4. Instance settings + icon ─────────────────────────────────
    let run_dir = {
        let mut service = core.instances.lock().map_err(|_| lock_error())?;
        service.paths(&key)?.run_dir
    };
    apply_instance_settings(core, pack, &key, &detail, display_name)?;
    let detail = {
        let mut service = core.instances.lock().map_err(|_| lock_error())?;
        service.get(&key)?
    };
    let run_dir = if detail.paths.run_dir != run_dir {
        detail.paths.run_dir.clone()
    } else {
        run_dir
    };

    // ── 5. Overrides ────────────────────────────────────────────────
    emit(
        progress.as_ref(),
        ModpackProgress::Stage {
            phase: "overrides",
            percent: 66,
            message: "解压整合包覆盖文件...".to_string(),
        },
    );
    let overrides = extract_pack_overrides(pack, &run_dir, progress.clone()).await?;
    if cancelled(&cancel) {
        return Err(cancel_error());
    }

    // ── 6. Manifest files ───────────────────────────────────────────
    let warnings =
        resolve_and_download_files(core, pack, &run_dir, progress.clone(), &cancel).await?;
    pack.warnings.extend(warnings);

    // ── 7. Metadata + config ────────────────────────────────────────
    emit(
        progress.as_ref(),
        ModpackProgress::Stage {
            phase: "metadata",
            percent: 98,
            message: "写入整合包配置...".to_string(),
        },
    );
    let run_for_meta = run_dir.clone();
    let pack_for_meta = pack.clone();
    tokio::task::spawn_blocking(move || {
        write_modpack_metadata(&run_for_meta, &pack_for_meta, overrides)
    })
    .await
    .map_err(join_error)??;

    {
        let mut service = core.instances.lock().map_err(|_| lock_error())?;
        service.select_instance(Some(&key))?;
        service.refresh(Some(directory_id), true)?;
    }

    let warnings = pack.warnings.clone();
    emit(
        progress.as_ref(),
        ModpackProgress::Stage {
            phase: "finalize",
            percent: 100,
            message: "整合包安装完成".to_string(),
        },
    );
    tracing::info!(
        key = %key,
        kind = pack.kind.as_str(),
        files = pack.files.len(),
        "Modpack install complete"
    );
    Ok(InstalledModpack {
        key: Some(key),
        instance_id: Some(instance_id.to_string()),
        directory_id: directory_id.to_string(),
        name: display_name.to_string(),
        kind: pack.kind,
        manual_directory: None,
        warnings,
    })
}

fn make_pipeline_progress(
    progress: Option<ModpackProgressCb>,
    phase: &'static str,
    start: u8,
    end: u8,
) -> ProgressCb {
    Arc::new(move |message: &ProgressMsg| match message {
        ProgressMsg::Download { progress: dl, .. } => {
            let ratio = if dl.total_files > 0 {
                dl.completed_files as f64 / dl.total_files as f64
            } else {
                0.0
            };
            let percent = start as f64 + (end.saturating_sub(start) as f64 * ratio);
            emit(
                progress.as_ref(),
                ModpackProgress::Stage {
                    phase,
                    percent: percent.round().clamp(0.0, 100.0) as u8,
                    message: dl.current_file.clone().unwrap_or_else(|| {
                        format!("{}/{} 文件", dl.completed_files, dl.total_files)
                    }),
                },
            );
        }
        ProgressMsg::Stage(stage) | ProgressMsg::StageDone(stage) => emit(
            progress.as_ref(),
            ModpackProgress::Stage {
                phase,
                percent: start,
                message: stage.clone(),
            },
        ),
    })
}

fn make_loader_progress(
    progress: Option<ModpackProgressCb>,
) -> rmccc_modloader::InstallProgressCallback {
    Arc::new(move |event: rmccc_modloader::InstallProgressEvent| {
        if event.stage == "detail" {
            return;
        }
        let percent = 40u16 + (event.percent as u16 * 20 / 100);
        emit(
            progress.as_ref(),
            ModpackProgress::Stage {
                phase: "loader",
                percent: percent.min(60) as u8,
                message: event.message,
            },
        );
    })
}

async fn extract_pack_overrides(
    pack: &Modpack,
    run_dir: &std::path::Path,
    progress: Option<ModpackProgressCb>,
) -> Result<Vec<rmccc_modpack::OverrideEntry>> {
    if pack.override_dirs.is_empty() {
        return Ok(Vec::new());
    }
    let archive = pack.archive.clone();
    let dirs = pack.override_dirs.clone();
    let kind = pack.kind;
    let run = run_dir.to_path_buf();
    let previous = rmccc_modpack::read_modpack_configuration(run_dir)
        .map(|config| config.overrides)
        .unwrap_or_default();
    let progress_for_extract = progress.clone();
    let cb = Arc::new(move |completed: usize, total: usize| {
        emit(
            progress_for_extract.as_ref(),
            ModpackProgress::Overrides {
                completed: completed as u64,
                total: total as u64,
            },
        );
    });
    tokio::task::spawn_blocking(move || {
        let exclude = |relative: &str| kind == ModpackKind::Hmcl && relative == "pack.json";
        extract::extract_overrides(
            &archive,
            &run,
            &dirs,
            &previous,
            &exclude,
            Some(cb.as_ref()),
        )
    })
    .await
    .map_err(join_error)?
}

async fn resolve_and_download_files(
    core: &CoreHandle,
    pack: &mut Modpack,
    run_dir: &std::path::Path,
    progress: Option<ModpackProgressCb>,
    cancel: &Option<Arc<AtomicBool>>,
) -> Result<Vec<String>> {
    let mut warnings = rmccc_modpack::resolve_files(
        pack,
        &core.http_client,
        core.download_provider.as_ref(),
        CurseApi::api_key_from_env(),
    )
    .await;
    if cancelled(cancel) {
        return Err(cancel_error());
    }

    let run = run_dir.to_path_buf();
    let files = pack.files.clone();
    let prepared = tokio::task::spawn_blocking(move || prepare_downloads(&run, &files))
        .await
        .map_err(join_error)?;
    let (items, skipped, prepare_warnings) = prepared;
    warnings.extend(prepare_warnings);
    if items.is_empty() {
        return Ok(warnings);
    }

    emit(
        progress.as_ref(),
        ModpackProgress::Stage {
            phase: "files",
            percent: 76,
            message: format!(
                "下载 {} 个整合包文件（已存在 {skipped} 个）...",
                items.len()
            ),
        },
    );
    let total_items = items.len() as u64;
    let tally = Arc::new(AtomicU64::new(0));
    let tally_cb = tally.clone();
    let progress_cb = progress.clone();
    let callback: rmccc_download::ProgressCallback = Arc::new(move |p| {
        if p.current_file.is_none() {
            tally_cb.store(p.failed_files, Ordering::Relaxed);
        }
        let ratio = if p.total_files > 0 {
            p.completed_files as f64 / p.total_files as f64
        } else {
            0.0
        };
        emit(
            progress_cb.as_ref(),
            ModpackProgress::Stage {
                phase: "files",
                percent: (76.0 + 21.0 * ratio).round().clamp(0.0, 100.0) as u8,
                message: p
                    .current_file
                    .clone()
                    .unwrap_or_else(|| format!("{}/{} 文件", p.completed_files, p.total_files)),
            },
        );
    });
    let handle = core
        .download_manager
        .download_batch(items, callback)
        .await?;
    let watcher = cancel.as_ref().map(|flag| {
        let flag = flag.clone();
        let manager = core.download_manager.clone();
        let batch = handle.id.clone();
        tokio::spawn(async move {
            loop {
                if flag.load(Ordering::Relaxed) {
                    manager.cancel_batch(&batch);
                    break;
                }
                tokio::time::sleep(std::time::Duration::from_millis(200)).await;
            }
        })
    });
    core.download_manager.wait_for_batch(&handle.id).await;
    if let Some(watcher) = watcher {
        watcher.abort();
    }
    if cancelled(cancel) {
        return Err(cancel_error());
    }
    let failed = tally.load(Ordering::Relaxed);
    if failed > 0 {
        warnings.push(format!("{failed} 个整合包文件下载失败（可稍后修复）"));
    }
    tracing::info!(
        total = total_items,
        failed,
        skipped,
        "Modpack manifest files processed"
    );
    Ok(warnings)
}

fn prepare_downloads(
    run_dir: &std::path::Path,
    files: &[rmccc_modpack::PackFile],
) -> (Vec<rmccc_download::DownloadItem>, usize, Vec<String>) {
    let mut items = Vec::new();
    let mut skipped = 0usize;
    let mut warnings = Vec::new();
    for file in files {
        let Some(relative) = &file.path else {
            if file.required {
                warnings.push(format!("无法确定整合包文件路径（{}），已跳过", file.origin));
            }
            continue;
        };
        let Ok(dest) = safe_join(run_dir, relative) else {
            warnings.push(format!("跳过不安全的整合包路径: {relative}"));
            continue;
        };
        let existing_ok = dest.is_file()
            && match &file.sha1 {
                Some(hash) => extract::file_sha1(&dest)
                    .map(|actual| actual.eq_ignore_ascii_case(hash))
                    .unwrap_or(false),
                // HMCL skips any existing file when the manifest has no hash.
                None => true,
            };
        if existing_ok {
            skipped += 1;
            continue;
        }
        if file.urls.is_empty() {
            if file.required {
                warnings.push(format!("整合包文件没有可用下载地址: {relative}"));
            }
            continue;
        }
        let mut urls = file.urls.clone();
        if urls.len() > 1 {
            let primary = urls.remove(0);
            items.push(rmccc_download::DownloadItem {
                url: primary,
                dest,
                expected_sha1: file.sha1.clone(),
                size: file.size,
                use_cache: file.sha1.is_some(),
                candidates: urls,
            });
        } else {
            items.push(rmccc_download::DownloadItem {
                url: urls.remove(0),
                dest,
                expected_sha1: file.sha1.clone(),
                size: file.size,
                use_cache: file.sha1.is_some(),
                candidates: Vec::new(),
            });
        }
    }
    (items, skipped, warnings)
}

fn apply_instance_settings(
    core: &CoreHandle,
    pack: &Modpack,
    key: &str,
    detail: &InstanceDetail,
    display_name: &str,
) -> Result<()> {
    let mut patch = InstanceSettingsPatch {
        display_name: Some(display_name.to_string()),
        isolation: Some(IsolationPolicy::Always.as_str().to_string()),
        ..Default::default()
    };
    let mut overrides = detail.settings.override_properties.clone();

    if let Some(icon) = &pack.icon {
        let path = detail.paths.instance_dir.join(rmccc_modpack::INSTANCE_ICON);
        std::fs::write(&path, &icon.bytes)?;
        patch.icon = Some(InstanceIcon::File(path));
    }
    apply_pack_settings(
        &detail.effective,
        &pack.settings,
        &mut patch,
        &mut overrides,
    );
    patch.override_properties = Some(overrides.into_iter().collect());
    let mut service = core.instances.lock().map_err(|_| lock_error())?;
    service.update_settings(key, patch)?;
    Ok(())
}

fn apply_pack_settings(
    effective: &rmccc_instance::EffectiveGameSettings,
    settings: &PackSettings,
    patch: &mut InstanceSettingsPatch,
    overrides: &mut std::collections::BTreeSet<String>,
) {
    use instance_fields as f;

    if let Some(memory) = settings.memory {
        if let Some(max) = memory.max_mb {
            let apply = if memory.require_at_least {
                max > effective.max_memory_mb
            } else {
                true
            };
            if apply {
                patch.auto_memory = Some(false);
                patch.max_memory_mb = Some(max.max(effective.min_memory_mb.unwrap_or(0)));
                overrides.insert(f::AUTO_MEMORY.to_string());
                overrides.insert(f::MAX_MEMORY_MB.to_string());
            }
        }
        if let Some(min) = memory.min_mb {
            patch.min_memory_mb = Some(min);
            overrides.insert(f::MIN_MEMORY_MB.to_string());
        }
        if let Some(perm) = memory.perm_gen_mb {
            patch.metaspace_mb = Some(perm);
            overrides.insert(f::METASPACE_MB.to_string());
        }
    }
    if let Some(path) = &settings.java_path {
        patch.java_type = Some(JavaType::Custom);
        patch.custom_java_path = Some(path.clone());
        overrides.insert(f::JAVA_TYPE.to_string());
        overrides.insert(f::CUSTOM_JAVA_PATH.to_string());
    }
    if let Some(args) = &settings.jvm_args {
        let mut merged = effective.jvm_args.clone();
        for arg in args {
            if !merged.contains(arg) {
                merged.push(arg.clone());
            }
        }
        patch.jvm_args = Some(merged);
        overrides.insert(f::JVM_ARGS.to_string());
    }
    if !settings.game_args.is_empty() {
        let mut merged = effective.game_args.clone();
        for arg in &settings.game_args {
            if !merged.contains(arg) {
                merged.push(arg.clone());
            }
        }
        patch.game_args = Some(merged);
        overrides.insert(f::GAME_ARGS.to_string());
    }
    if let Some(wrapper) = &settings.wrapper {
        patch.wrapper = Some(wrapper.clone());
        overrides.insert(f::WRAPPER.to_string());
    }
    if let Some(command) = &settings.pre_launch_command {
        patch.pre_launch_command = Some(command.clone());
        overrides.insert(f::PRE_LAUNCH_COMMAND.to_string());
    }
    if let Some(command) = &settings.post_exit_command {
        patch.post_exit_command = Some(command.clone());
        overrides.insert(f::POST_EXIT_COMMAND.to_string());
    }
    if let Some(window) = &settings.window {
        patch.window_type = Some(if window.fullscreen {
            WindowType::Fullscreen
        } else {
            WindowType::Windowed
        });
        if let Some(width) = window.width {
            patch.width = Some(width);
        }
        if let Some(height) = window.height {
            patch.height = Some(height);
        }
        overrides.insert(f::WINDOW_TYPE.to_string());
        if window.width.is_some() {
            overrides.insert(f::WIDTH.to_string());
        }
        if window.height.is_some() {
            overrides.insert(f::HEIGHT.to_string());
        }
    }
    if let Some(show) = settings.show_logs {
        patch.show_logs = Some(show);
        overrides.insert(f::SHOW_LOGS.to_string());
    }
}

/// Append an extra-libraries patch to the instance version JSON (MCBBS).
fn append_extra_libraries(
    json_file: &std::path::Path,
    libraries: &[rmccc_version::Library],
) -> Result<()> {
    let text = std::fs::read_to_string(json_file)?;
    let mut value: serde_json::Value = serde_json::from_str(&text)?;
    let object = value
        .as_object_mut()
        .ok_or_else(|| RmcccError::Validation("instance JSON is not an object".into()))?;
    let patch = serde_json::json!({
        "id": "mcbbs",
        "version": "1",
        "libraries": libraries,
    });
    match object.get_mut("patches").and_then(|v| v.as_array_mut()) {
        Some(patches) => {
            if !patches
                .iter()
                .any(|entry| entry.get("id") == Some(&serde_json::json!("mcbbs")))
            {
                patches.push(patch);
            }
        }
        None => {
            object.insert("patches".to_string(), serde_json::json!([patch]));
        }
    }
    std::fs::write(json_file, serde_json::to_string_pretty(&value)?)?;
    Ok(())
}

fn cleanup_instance(core: &CoreHandle, directory_id: &str, instance_id: &str) {
    let key = match rmccc_instance::InstanceService::key(directory_id, instance_id) {
        Ok(key) => key,
        Err(_) => return,
    };
    if let Ok(mut service) = core.instances.lock() {
        let _ = service.remove(&key, true);
        let _ = service.refresh(Some(directory_id), true);
    }
}

async fn install_manual_modpack(
    core: &CoreHandle,
    request: ModpackInstallRequest,
    pack: Modpack,
    progress: Option<ModpackProgressCb>,
    cancel: Option<Arc<AtomicBool>>,
) -> Result<InstalledModpack> {
    let fallback = request
        .archive
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or("Modpack");
    let requested_name = request
        .instance_name
        .as_deref()
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .unwrap_or(pack.name.as_str());
    let name = suggest_instance_id(requested_name, fallback);
    let root = pack.manual_root.clone().unwrap_or_default();
    let dest = core.data_dir.join("externalgames").join(&name);
    if dest.exists() {
        return Err(RmcccError::Validation(format!(
            "external game directory already exists: {}",
            dest.display()
        )));
    }
    emit(
        progress.as_ref(),
        ModpackProgress::Stage {
            phase: "manual",
            percent: 5,
            message: format!("解压整合包到 {}...", dest.display()),
        },
    );
    let archive = pack.archive.clone();
    let dest_for_extract = dest.clone();
    let progress_for_extract = progress.clone();
    let cb = Arc::new(move |completed: usize, total: usize| {
        let percent = if total > 0 {
            (5.0 + 90.0 * completed as f64 / total as f64) as u8
        } else {
            50
        };
        emit(
            progress_for_extract.as_ref(),
            ModpackProgress::Stage {
                phase: "manual",
                percent,
                message: format!("解压 {completed}/{total} 个文件..."),
            },
        );
    });
    let extracted = tokio::task::spawn_blocking(move || {
        extract::extract_subdirectory(&archive, &dest_for_extract, &root, Some(cb.as_ref()))
    })
    .await
    .map_err(join_error)?;
    let extracted = match extracted {
        Ok(count) => count,
        Err(error) => {
            let _ = std::fs::remove_dir_all(&dest);
            return Err(error);
        }
    };
    if cancelled(&cancel) {
        let _ = std::fs::remove_dir_all(&dest);
        return Err(cancel_error());
    }

    let directory = {
        let mut service = core.instances.lock().map_err(|_| lock_error())?;
        let directory = match service.add_directory(&name, &dest, GameDirectoryKind::External) {
            Ok(directory) => directory,
            Err(error) => {
                drop(service);
                let _ = std::fs::remove_dir_all(&dest);
                return Err(error);
            }
        };
        let id = directory.id.as_str().to_string();
        service.select_directory(&id)?;
        service.refresh(Some(&id), true)?;
        directory
    };
    let directory_id = directory.id.as_str().to_string();
    let key = {
        let mut service = core.instances.lock().map_err(|_| lock_error())?;
        service
            .list(Some(&directory_id), false, false)?
            .first()
            .map(|summary| summary.key.clone())
    };
    emit(
        progress.as_ref(),
        ModpackProgress::Stage {
            phase: "finalize",
            percent: 100,
            message: format!("已导入 {extracted} 个文件"),
        },
    );
    Ok(InstalledModpack {
        key,
        instance_id: None,
        directory_id: directory_id.clone(),
        name,
        kind: ModpackKind::Manual,
        manual_directory: Some(directory_id),
        warnings: pack.warnings,
    })
}

/// Map a manifest loader kind onto the actual installer type.
pub fn mod_loader_type(kind: LoaderKind) -> Option<ModLoaderType> {
    ModLoaderType::from_str(kind.as_str())
}

#[cfg(test)]
mod tests {
    use super::*;
    use rmccc_modpack::{PackFile, PackFileSource};

    fn temp_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "rmccc-modpack-core-{tag}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn extra_libraries_patch_is_appended_once() {
        let dir = temp_dir("patch");
        let json = dir.join("pack.json");
        std::fs::write(
            &json,
            r#"{"id":"pack","type":"release","mainClass":"net.minecraft.client.main.Main","assetIndex":{"id":"1","url":"u","sha1":"s","size":1,"totalSize":1},"downloads":{},"libraries":[]}"#,
        )
        .unwrap();
        let library = rmccc_version::Library {
            name: "example:lib:1.0".to_string(),
            ..Default::default()
        };
        append_extra_libraries(&json, std::slice::from_ref(&library)).unwrap();
        append_extra_libraries(&json, &[library]).unwrap();
        let value: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&json).unwrap()).unwrap();
        let patches = value["patches"].as_array().unwrap();
        assert_eq!(patches.len(), 1);
        assert_eq!(patches[0]["id"], "mcbbs");
        assert_eq!(patches[0]["libraries"][0]["name"], "example:lib:1.0");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn existing_verified_file_is_not_redownloaded() {
        let dir = temp_dir("downloads");
        std::fs::create_dir_all(dir.join("mods")).unwrap();
        std::fs::write(dir.join("mods/a.jar"), b"jar").unwrap();
        let hash = rmccc_modpack::file_sha1(&dir.join("mods/a.jar")).unwrap();
        let files = vec![PackFile {
            path: Some("mods/a.jar".to_string()),
            urls: vec!["https://example.com/a.jar".to_string()],
            sha1: Some(hash),
            size: Some(3),
            required: true,
            origin: "test".to_string(),
            source: PackFileSource::Http,
        }];
        let (items, skipped, warnings) = prepare_downloads(&dir, &files);
        assert!(items.is_empty());
        assert_eq!(skipped, 1);
        assert!(warnings.is_empty());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn mcbbs_memory_and_jvm_settings_are_overridden() {
        let effective = rmccc_instance::EffectiveGameSettings {
            max_memory_mb: 2048,
            jvm_args: vec!["-XX:+UseG1GC".to_string()],
            ..Default::default()
        };
        let settings = PackSettings {
            memory: Some(rmccc_modpack::MemorySettings {
                min_mb: None,
                max_mb: Some(4096),
                perm_gen_mb: None,
                require_at_least: false,
            }),
            jvm_args: Some(vec!["-XX:+UseG1GC".to_string(), "-Dfoo=bar".to_string()]),
            game_args: vec!["--demo".to_string()],
            ..Default::default()
        };
        let mut patch = InstanceSettingsPatch::default();
        let mut overrides = std::collections::BTreeSet::new();
        apply_pack_settings(&effective, &settings, &mut patch, &mut overrides);
        assert_eq!(patch.max_memory_mb, Some(4096));
        assert_eq!(patch.auto_memory, Some(false));
        assert!(patch
            .jvm_args
            .as_ref()
            .unwrap()
            .contains(&"-Dfoo=bar".to_string()));
        assert_eq!(patch.game_args, Some(vec!["--demo".to_string()]));
        assert!(overrides.contains(instance_fields::MAX_MEMORY_MB));
        assert!(overrides.contains(instance_fields::JVM_ARGS));
    }
}

#[cfg(test)]
mod end_to_end_tests {
    use super::*;
    use std::io::Write;

    fn temp_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "rmccc-modpack-e2e-{tag}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[tokio::test]
    async fn installs_curse_pack_offline_from_local_version() {
        let data_dir = temp_dir("curse");
        let core = crate::CoreContext::new(&data_dir).unwrap();
        let handle = core.handle();
        let game_dir = {
            let service = handle.instances.lock().unwrap();
            service.selected_directory().path.clone()
        };
        // A minimal local version manifest: no client jar, no libraries, no
        // asset index, so install_game_into performs zero network requests.
        let version_dir = game_dir.join("versions").join("1.20.1");
        std::fs::create_dir_all(&version_dir).unwrap();
        std::fs::write(
            version_dir.join("1.20.1.json"),
            serde_json::json!({
                "id": "1.20.1",
                "type": "release",
                "mainClass": "net.minecraft.client.main.Main",
                "assetIndex": {"id": "", "url": "", "sha1": "", "size": 0, "totalSize": 0},
                "downloads": {},
                "libraries": []
            })
            .to_string(),
        )
        .unwrap();

        let archive = data_dir.join("pack.zip");
        {
            let file = std::fs::File::create(&archive).unwrap();
            let mut zip = zip::ZipWriter::new(file);
            let options = zip::write::SimpleFileOptions::default();
            zip.start_file("manifest.json", options).unwrap();
            let manifest = serde_json::json!({
                "manifestType": "minecraftModpack",
                "manifestVersion": 1,
                "name": "Offline Pack",
                "version": "1.0",
                "author": "tester",
                "overrides": "overrides",
                "minecraft": {"version": "1.20.1", "modLoaders": []},
                "files": []
            });
            zip.write_all(manifest.to_string().as_bytes()).unwrap();
            zip.start_file("overrides/config/test.txt", options)
                .unwrap();
            zip.write_all(b"hello").unwrap();
            zip.start_file("icon.png", options).unwrap();
            zip.write_all(b"PNG").unwrap();
            zip.finish().unwrap();
        }

        let directory_id = {
            let service = handle.instances.lock().unwrap();
            service.selected_directory().id.as_str().to_string()
        };
        let installed = install_modpack(
            &handle,
            ModpackInstallRequest {
                archive,
                directory_id: directory_id.clone(),
                instance_name: None,
            },
            None,
            None,
        )
        .await
        .unwrap();
        let key = installed.key.expect("instance key");
        assert_eq!(installed.name, "Offline Pack");
        let instance_root = game_dir.join("versions").join("Offline Pack");
        assert_eq!(
            std::fs::read(instance_root.join("config/test.txt")).unwrap(),
            b"hello"
        );
        assert!(instance_root.join("modpack.cfg").exists());
        assert!(instance_root.join("manifest.json").exists());
        assert!(instance_root.join("icon.png").exists());
        let detail = {
            let mut service = handle.instances.lock().unwrap();
            service.get(&key).unwrap()
        };
        assert_eq!(detail.summary.display_name, "Offline Pack");
        assert!(detail.summary.is_modpack);
        assert_eq!(detail.paths.run_dir, instance_root);
        let _ = std::fs::remove_dir_all(data_dir);
    }
}
