//! Portable / standard data-root selection.
//!
//! The active layout is resolved before settings are loaded (see
//! `rmccc_config::paths`). Switching therefore writes a mode marker and asks
//! the process to restart instead of trying to hot-swap every open path.

use std::path::Path;

use rmccc_config::paths::{self, DataMode};
use rmccc_error::{ErrorCode, IpcError};
use serde::Serialize;
use ts_rs::TS;

use crate::state::AppState;

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export_to = "types.ts")]
pub struct DataLocationView {
    pub mode: String,
    pub executable_dir: String,
    pub data_dir: String,
    pub game_root: String,
    pub background_dir: String,
    pub portable_root: String,
    pub standard_root: String,
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export_to = "types.ts")]
pub struct DataModeSwitchResult {
    pub mode: String,
    pub migrated: bool,
    pub restart_required: bool,
}

fn view_for(mode: DataMode) -> DataLocationView {
    let data_dir = paths::data_dir_for_mode(mode);
    DataLocationView {
        mode: mode.as_str().to_string(),
        executable_dir: paths::executable_dir().display().to_string(),
        data_dir: data_dir.display().to_string(),
        game_root: paths::game_root_for_mode(mode).display().to_string(),
        background_dir: paths::background_dir(&data_dir).display().to_string(),
        portable_root: paths::data_dir_for_mode(DataMode::Portable)
            .display()
            .to_string(),
        standard_root: paths::standard_data_dir().display().to_string(),
    }
}

#[tauri::command]
#[tracing::instrument]
pub fn data_mode_get() -> DataLocationView {
    view_for(paths::AppPaths::current().mode())
}

/// Switch to `standard` or `portable`, optionally moving the existing data.
///
/// Migration is a cut, not a copy: after a byte-count and file-size
/// verification pass succeeds, the old data tree is emptied. This is what
/// users expect when they switch modes to reclaim disk space.
#[tauri::command]
#[tracing::instrument(skip(app, state))]
pub async fn data_mode_switch(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    mode: String,
    migrate: bool,
) -> Result<DataModeSwitchResult, IpcError> {
    let target_mode = DataMode::parse(&mode).ok_or_else(|| IpcError {
        code: ErrorCode::ValidationError,
        message: format!("unknown data mode: {mode}"),
        retryable: false,
    })?;
    let source = paths::AppPaths::current();
    if source.mode() == target_mode {
        return Ok(DataModeSwitchResult {
            mode: target_mode.as_str().to_string(),
            migrated: false,
            restart_required: false,
        });
    }

    let source_data = source.data_dir().to_path_buf();
    let source_game = source.game_root().to_path_buf();
    let target_data = paths::data_dir_for_mode(target_mode);
    let target_game = paths::game_root_for_mode(target_mode);

    if source_data == target_data {
        return Err(IpcError {
            code: ErrorCode::ValidationError,
            message: "source and target data directories are identical".into(),
            retryable: false,
        });
    }
    if migrate && has_user_entries(&target_data) {
        return Err(IpcError {
            code: ErrorCode::ValidationError,
            message: format!(
                "target data directory already contains files: {}; move it away before migrating",
                target_data.display()
            ),
            retryable: false,
        });
    }

    // Flush the debounced settings writer before copying, otherwise the target
    // can miss the user's latest preference edits.
    {
        let mut core = state.core.write().await;
        core.flush_pending_writes();
        let content = core.settings_json().map_err(|error| IpcError {
            code: ErrorCode::Unknown,
            message: error.to_string(),
            retryable: false,
        })?;
        let settings = paths::launcher_settings_path(&source_data);
        if let Some(parent) = settings.parent() {
            std::fs::create_dir_all(parent).map_err(io_error)?;
        }
        std::fs::write(&settings, content).map_err(io_error)?;
    }

    let mut migrated = false;
    if migrate {
        let source_data_copy = source_data.clone();
        let source_game_copy = source_game.clone();
        let target_data_copy = target_data.clone();
        let target_game_copy = target_game.clone();
        tauri::async_runtime::spawn_blocking(move || {
            migrate_layout(
                &source_data_copy,
                &source_game_copy,
                &target_data_copy,
                &target_game_copy,
            )
        })
        .await
        .map_err(|error| IpcError {
            code: ErrorCode::Unknown,
            message: format!("data migration task failed: {error}"),
            retryable: true,
        })??;
        migrated = true;
    }

    paths::AppPaths::write_mode_marker(target_mode).map_err(io_error)?;

    // The process still holds the old AppPaths once-cell and open repositories.
    // Restart after the IPC response has a chance to reach the UI.
    let restart_handle = app.clone();
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(700));
        restart_handle.restart();
    });

    Ok(DataModeSwitchResult {
        mode: target_mode.as_str().to_string(),
        migrated,
        restart_required: true,
    })
}

fn io_error(error: std::io::Error) -> IpcError {
    IpcError {
        code: ErrorCode::Unknown,
        message: error.to_string(),
        retryable: false,
    }
}

#[derive(Default)]
struct CopyStats {
    files: u64,
    bytes: u64,
}

fn migrate_layout(
    source_data: &Path,
    source_game: &Path,
    target_data: &Path,
    target_game: &Path,
) -> Result<(), IpcError> {
    if !source_data.is_dir() {
        return Err(IpcError {
            code: ErrorCode::ValidationError,
            message: format!(
                "source data directory does not exist: {}",
                source_data.display()
            ),
            retryable: false,
        });
    }
    if target_data.starts_with(source_data) || source_data.starts_with(target_data) {
        return Err(IpcError {
            code: ErrorCode::ValidationError,
            message: "source and target data directories overlap".into(),
            retryable: false,
        });
    }

    // If the game root is inside the data root (standard layout), copy it
    // through the dedicated game-root mapping instead of duplicating it under
    // SNGL/instances.
    let game_inside_data = source_game
        .strip_prefix(source_data)
        .ok()
        .map(Path::to_path_buf);
    let mut data_stats = CopyStats::default();
    copy_tree(
        source_data,
        target_data,
        &["cache", ".portable", ".standard"],
        game_inside_data.as_deref(),
        &mut data_stats,
    )?;

    if source_game != target_game && source_game.is_dir() {
        let mut game_stats = CopyStats::default();
        copy_tree(source_game, target_game, &[], None, &mut game_stats)?;
        verify_tree(source_game, target_game, &[], None)?;
    }

    verify_tree(
        source_data,
        target_data,
        &["cache", ".portable", ".standard"],
        game_inside_data.as_deref(),
    )?;

    rewrite_migrated_paths(
        target_data,
        source_data,
        target_data,
        source_game,
        target_game,
    )?;
    normalize_migrated_settings(target_data);

    // Cut only after every copied file has been verified. Keep the directory
    // itself so the caller can write the new marker into it.
    clear_dir_contents(source_data)?;
    if !game_inside_data.is_some() && source_game != target_game && source_game.is_dir() {
        clear_dir_contents(source_game)?;
    }
    tracing::info!(
        files = data_stats.files,
        bytes = data_stats.bytes,
        source = %source_data.display(),
        target = %target_data.display(),
        "Migrated SNGL data root"
    );
    Ok(())
}

fn copy_tree(
    source: &Path,
    target: &Path,
    skip_names: &[&str],
    skip_path: Option<&Path>,
    stats: &mut CopyStats,
) -> Result<(), IpcError> {
    if !source.exists() {
        return Ok(());
    }
    std::fs::create_dir_all(target).map_err(io_error)?;
    for entry in std::fs::read_dir(source).map_err(io_error)? {
        let entry = entry.map_err(io_error)?;
        let path = entry.path();
        if skip_path.is_some_and(|skip| path == skip) {
            continue;
        }
        let name = entry.file_name();
        if skip_names.iter().any(|skip| name == *skip) {
            continue;
        }
        // Never follow symlinks while moving a user directory: a link can point
        // outside the source tree and duplicate arbitrary files.
        let metadata = std::fs::symlink_metadata(&path).map_err(io_error)?;
        let destination = target.join(&name);
        if metadata.file_type().is_symlink() {
            tracing::warn!(path = %path.display(), "Skipping symlink during data migration");
            continue;
        }
        if metadata.is_dir() {
            copy_tree(&path, &destination, skip_names, skip_path, stats)?;
        } else if metadata.is_file() {
            if let Some(parent) = destination.parent() {
                std::fs::create_dir_all(parent).map_err(io_error)?;
            }
            std::fs::copy(&path, &destination).map_err(io_error)?;
            stats.files += 1;
            stats.bytes += metadata.len();
        }
    }
    Ok(())
}

fn verify_tree(
    source: &Path,
    target: &Path,
    skip_names: &[&str],
    skip_path: Option<&Path>,
) -> Result<(), IpcError> {
    if !source.exists() {
        return Ok(());
    }
    for entry in std::fs::read_dir(source).map_err(io_error)? {
        let entry = entry.map_err(io_error)?;
        let path = entry.path();
        if skip_path.is_some_and(|skip| path == skip) {
            continue;
        }
        let name = entry.file_name();
        if skip_names.iter().any(|skip| name == *skip) {
            continue;
        }
        let metadata = std::fs::symlink_metadata(&path).map_err(io_error)?;
        if metadata.file_type().is_symlink() {
            continue;
        }
        let destination = target.join(&name);
        if metadata.is_dir() {
            verify_tree(&path, &destination, skip_names, skip_path)?;
        } else if metadata.is_file() {
            if !files_equal(&path, &destination)? {
                return Err(IpcError {
                    code: ErrorCode::Unknown,
                    message: format!(
                        "migration verification failed: {} does not match {}",
                        destination.display(),
                        path.display()
                    ),
                    retryable: false,
                });
            }
        }
    }
    Ok(())
}

fn has_user_entries(path: &Path) -> bool {
    let Ok(entries) = std::fs::read_dir(path) else {
        return false;
    };
    entries.flatten().any(|entry| {
        let name = entry.file_name();
        name != ".portable" && name != ".standard"
    })
}

/// Compare two files byte-for-byte without loading either one fully into RAM.
fn files_equal(left: &Path, right: &Path) -> Result<bool, IpcError> {
    use std::io::Read;

    let mut left = std::fs::File::open(left).map_err(io_error)?;
    let mut right = std::fs::File::open(right).map_err(io_error)?;
    let mut left_buf = [0u8; 64 * 1024];
    let mut right_buf = [0u8; 64 * 1024];
    loop {
        let left_len = left.read(&mut left_buf).map_err(io_error)?;
        let right_len = right.read(&mut right_buf).map_err(io_error)?;
        if left_len != right_len || left_buf[..left_len] != right_buf[..right_len] {
            return Ok(false);
        }
        if left_len == 0 {
            return Ok(true);
        }
    }
}

fn clear_dir_contents(path: &Path) -> Result<(), IpcError> {
    for entry in std::fs::read_dir(path).map_err(io_error)? {
        let entry = entry.map_err(io_error)?;
        let target = entry.path();
        let metadata = std::fs::symlink_metadata(&target).map_err(io_error)?;
        if metadata.is_dir() {
            std::fs::remove_dir_all(&target).map_err(io_error)?;
        } else {
            std::fs::remove_file(&target).map_err(io_error)?;
        }
    }
    Ok(())
}

/// Rewrite absolute references to the old data/game roots inside the copied
/// JSON documents. Without this, the game-directory registry and per-profile
/// `game_dir` fields keep pointing at a directory that the cut migration is
/// about to empty.
fn rewrite_migrated_paths(
    target_data: &Path,
    source_data: &Path,
    target_data_path: &Path,
    source_game: &Path,
    target_game: &Path,
) -> Result<(), IpcError> {
    let mut files = vec![target_data.join("settings.json")];
    for subdir in ["config", "profiles", "dashboard"] {
        collect_json_files(&target_data.join(subdir), &mut files)?;
    }
    for file in files {
        if !file.is_file() {
            continue;
        }
        rewrite_json_file(
            &file,
            source_data,
            target_data_path,
            source_game,
            target_game,
        )?;
    }
    Ok(())
}

fn collect_json_files(dir: &Path, output: &mut Vec<std::path::PathBuf>) -> Result<(), IpcError> {
    if !dir.is_dir() {
        return Ok(());
    }
    for entry in std::fs::read_dir(dir).map_err(io_error)? {
        let path = entry.map_err(io_error)?.path();
        if path.is_dir() {
            collect_json_files(&path, output)?;
        } else if path.extension().and_then(|ext| ext.to_str()) == Some("json") {
            output.push(path);
        }
    }
    Ok(())
}

fn rewrite_json_file(
    path: &Path,
    source_data: &Path,
    target_data: &Path,
    source_game: &Path,
    target_game: &Path,
) -> Result<(), IpcError> {
    let raw = std::fs::read_to_string(path).map_err(io_error)?;
    let mut value: serde_json::Value = match serde_json::from_str(&raw) {
        Ok(value) => value,
        Err(_) => return Ok(()),
    };
    rewrite_json_value(
        &mut value,
        source_data,
        target_data,
        source_game,
        target_game,
    );
    let rendered = serde_json::to_string_pretty(&value).map_err(|error| IpcError {
        code: ErrorCode::Unknown,
        message: format!("could not serialise migrated {}: {error}", path.display()),
        retryable: false,
    })?;
    std::fs::write(path, rendered).map_err(io_error)
}

fn rewrite_json_value(
    value: &mut serde_json::Value,
    source_data: &Path,
    target_data: &Path,
    source_game: &Path,
    target_game: &Path,
) {
    match value {
        serde_json::Value::String(text) => {
            if let Some(path) =
                rewrite_path_string(text, source_data, target_data, source_game, target_game)
            {
                *text = path;
            }
        }
        serde_json::Value::Array(items) => {
            for item in items {
                rewrite_json_value(item, source_data, target_data, source_game, target_game);
            }
        }
        serde_json::Value::Object(object) => {
            for item in object.values_mut() {
                rewrite_json_value(item, source_data, target_data, source_game, target_game);
            }
        }
        _ => {}
    }
}

fn rewrite_path_string(
    value: &str,
    source_data: &Path,
    target_data: &Path,
    source_game: &Path,
    target_game: &Path,
) -> Option<String> {
    let path = Path::new(value);
    if path == source_game {
        return Some(target_game.display().to_string());
    }
    if let Ok(relative) = path.strip_prefix(source_game) {
        if !relative.as_os_str().is_empty() {
            return Some(target_game.join(relative).display().to_string());
        }
    }
    if path == source_data {
        return Some(target_data.display().to_string());
    }
    if let Ok(relative) = path.strip_prefix(source_data) {
        if !relative.as_os_str().is_empty() {
            return Some(target_data.join(relative).display().to_string());
        }
    }
    None
}

/// The copied settings file can contain absolute paths to the old root. They
/// are runtime-derived fields; clearing them lets the new layout win.
fn normalize_migrated_settings(target_data: &Path) {
    let path = paths::launcher_settings_path(target_data);
    let Ok(raw) = std::fs::read_to_string(&path) else {
        return;
    };
    let Ok(mut value) = serde_json::from_str::<serde_json::Value>(&raw) else {
        return;
    };
    if let Some(object) = value.as_object_mut() {
        object.insert("data_dir".into(), serde_json::Value::Null);
        object.insert("game_root_dir".into(), serde_json::Value::Null);
    }
    if let Ok(rendered) = serde_json::to_string_pretty(&value) {
        if let Err(error) = std::fs::write(&path, rendered) {
            tracing::warn!(%error, "Could not normalize migrated settings");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_root(tag: &str) -> std::path::PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!(
            "rmccc-data-paths-{tag}-{}-{nanos}",
            std::process::id()
        ))
    }

    #[test]
    fn migration_copies_verifies_cuts_and_rewrites_paths() {
        let root = temp_root("migrate");
        let source_data = root.join("source/SNGL");
        let source_game = source_data.join("instances");
        let target_data = root.join("target/SNGL");
        let target_game = root.join("target/.minecraft");
        std::fs::create_dir_all(source_game.join("pack")).unwrap();
        std::fs::create_dir_all(source_data.join("profiles")).unwrap();
        std::fs::create_dir_all(source_data.join("config")).unwrap();
        std::fs::create_dir_all(source_data.join("cache")).unwrap();
        std::fs::write(source_game.join("pack/level.dat"), b"level").unwrap();
        std::fs::write(source_data.join("cache/old.bin"), b"cache").unwrap();
        std::fs::write(
            source_data.join("settings.json"),
            serde_json::json!({
                "data_dir": source_data,
                "game_root_dir": source_game,
                "extra_game_roots": [source_game.join("pack")],
            })
            .to_string(),
        )
        .unwrap();
        std::fs::write(
            source_data.join("config/game-directories.json"),
            serde_json::json!({
                "directories": [{"id": "default", "path": source_game}]
            })
            .to_string(),
        )
        .unwrap();
        std::fs::write(
            source_data.join("profiles/profile.json"),
            serde_json::json!({"id": "p", "game_dir": source_game}).to_string(),
        )
        .unwrap();

        migrate_layout(&source_data, &source_game, &target_data, &target_game).unwrap();

        assert_eq!(
            std::fs::read(target_game.join("pack/level.dat")).unwrap(),
            b"level"
        );
        assert!(!target_data.join("cache").exists());
        assert!(!source_game.join("pack/level.dat").exists());
        let settings: serde_json::Value = serde_json::from_str(
            &std::fs::read_to_string(target_data.join("settings.json")).unwrap(),
        )
        .unwrap();
        assert!(settings["data_dir"].is_null());
        assert!(settings["game_root_dir"].is_null());
        assert_eq!(
            settings["extra_game_roots"][0],
            target_game.join("pack").display().to_string()
        );
        let registry: serde_json::Value = serde_json::from_str(
            &std::fs::read_to_string(target_data.join("config/game-directories.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(
            registry["directories"][0]["path"],
            target_game.display().to_string()
        );
        let profile: serde_json::Value = serde_json::from_str(
            &std::fs::read_to_string(target_data.join("profiles/profile.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(profile["game_dir"], target_game.display().to_string());
        assert!(std::fs::read_dir(&source_data).unwrap().next().is_none());
        let _ = std::fs::remove_dir_all(&root);
    }
}
