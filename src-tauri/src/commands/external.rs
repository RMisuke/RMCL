//! External URL opening — wraps `tauri-plugin-opener` with validation.
//!
//! Also owns the native wallpaper pickers used by the appearance settings: the
//! chosen MP4 or still image is validated once, then whitelisted for the
//! duration of this session in the asset-protocol scope so the WebView can
//! stream it locally without exposing any other file on disk.

use std::path::{Path, PathBuf};

use rmccc_error::{ErrorCode, IpcError};
use tauri::Manager;

use crate::state::AppState;

/// Open an external URL in the system default browser (via tauri-plugin-opener).
///
/// Only `http`/`https` schemes are accepted; arbitrary protocol handlers
/// (`file:`, `javascript:`, custom schemes) are rejected. Failures are
/// returned as `IpcError` and logged — never panics.
#[tauri::command]
#[tracing::instrument(skip(url))]
pub fn open_external_url(url: String) -> Result<(), IpcError> {
    let trimmed = url.trim();
    if !(trimmed.starts_with("https://") || trimmed.starts_with("http://")) {
        tracing::warn!(url = %trimmed, "Rejected non-http(s) URL open request");
        return Err(IpcError {
            code: rmccc_error::ErrorCode::ValidationError,
            message: "Only http/https URLs are allowed".into(),
            retryable: false,
        });
    }

    match tauri_plugin_opener::open_url(trimmed, None::<&str>) {
        Ok(()) => {
            tracing::info!(url = %trimmed, "Opened external URL");
            Ok(())
        }
        Err(e) => {
            tracing::warn!(url = %trimmed, error = ?e, "Failed to open external URL");
            Err(IpcError {
                code: rmccc_error::ErrorCode::Unknown,
                message: format!("Failed to open URL: {e}"),
                retryable: false,
            })
        }
    }
}

/// Native directory picker (tauri-plugin-dialog). Returns `None` on cancel.
///
/// Read-only browsing target for extra game roots — the picked path is only
/// stored when the user confirms adding it in the UI.
#[tauri::command]
#[tracing::instrument(skip(app))]
pub async fn dialog_pick_directory(app: tauri::AppHandle) -> Result<Option<String>, IpcError> {
    use tauri_plugin_dialog::DialogExt;

    let picked =
        tauri::async_runtime::spawn_blocking(move || app.dialog().file().blocking_pick_folder())
            .await
            .map_err(|e| IpcError {
                code: rmccc_error::ErrorCode::Unknown,
                message: format!("directory dialog failed: {e}"),
                retryable: false,
            })?;

    match picked {
        Some(path) => {
            let path = path.into_path().map_err(|e| IpcError {
                code: rmccc_error::ErrorCode::ValidationError,
                message: format!("invalid path: {e}"),
                retryable: false,
            })?;
            Ok(Some(path.display().to_string()))
        }
        None => Ok(None),
    }
}

/// Resolve and validate a user-picked background video.
///
/// Only existing regular `.mp4` files are accepted; symlinks are resolved so
/// the asset-protocol scope matches the real file the WebView opens.
fn checked_background_video(path: &Path) -> Result<PathBuf, IpcError> {
    let resolved = std::fs::canonicalize(path).map_err(|e| IpcError {
        code: ErrorCode::ValidationError,
        message: format!("video file is not accessible: {e}"),
        retryable: false,
    })?;
    let metadata = std::fs::metadata(&resolved).map_err(|e| IpcError {
        code: ErrorCode::ValidationError,
        message: format!("video file is not accessible: {e}"),
        retryable: false,
    })?;
    if !metadata.is_file() {
        return Err(IpcError {
            code: ErrorCode::ValidationError,
            message: "background video must be a file".into(),
            retryable: false,
        });
    }
    let is_mp4 = resolved
        .extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| ext.eq_ignore_ascii_case("mp4"));
    if !is_mp4 {
        return Err(IpcError {
            code: ErrorCode::ValidationError,
            message: "only local MP4 videos are supported".into(),
            retryable: false,
        });
    }
    Ok(resolved)
}

/// Still-image wallpaper formats the system WebViews can decode without an
/// extra image runtime. SVG is deliberately excluded because it can carry
/// active content; the asset is only ever rendered through an `<img>`.
const BACKGROUND_IMAGE_EXTENSIONS: &[&str] = &["png", "jpg", "jpeg", "webp", "gif", "bmp", "avif"];
/// Decoded image memory is far larger than the file. A conservative file cap
/// keeps a wrongly-chosen 100 MP asset from freezing the launcher process.
const MAX_BACKGROUND_IMAGE_BYTES: u64 = 32 * 1024 * 1024;

/// Resolve and validate a user-picked still-image wallpaper.
///
/// Only an existing regular file with a WebView-decodable extension is
/// accepted; symlinks are resolved so the asset-protocol scope matches the real
/// file the WebView opens.
fn checked_background_image(path: &Path) -> Result<PathBuf, IpcError> {
    let resolved = std::fs::canonicalize(path).map_err(|e| IpcError {
        code: ErrorCode::ValidationError,
        message: format!("image file is not accessible: {e}"),
        retryable: false,
    })?;
    let metadata = std::fs::metadata(&resolved).map_err(|e| IpcError {
        code: ErrorCode::ValidationError,
        message: format!("image file is not accessible: {e}"),
        retryable: false,
    })?;
    if !metadata.is_file() {
        return Err(IpcError {
            code: ErrorCode::ValidationError,
            message: "background image must be a file".into(),
            retryable: false,
        });
    }
    let supported = resolved
        .extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| {
            BACKGROUND_IMAGE_EXTENSIONS
                .iter()
                .any(|allowed| ext.eq_ignore_ascii_case(allowed))
        });
    if !supported {
        return Err(IpcError {
            code: ErrorCode::ValidationError,
            message: "only PNG, JPEG, WebP, GIF, BMP or AVIF images are supported".into(),
            retryable: false,
        });
    }
    if metadata.len() == 0 {
        return Err(IpcError {
            code: ErrorCode::ValidationError,
            message: "background image is empty".into(),
            retryable: false,
        });
    }
    if metadata.len() > MAX_BACKGROUND_IMAGE_BYTES {
        return Err(IpcError {
            code: ErrorCode::ValidationError,
            message: "background image must be 32 MB or smaller".into(),
            retryable: false,
        });
    }
    Ok(resolved)
}

fn allow_asset_read(app: &tauri::AppHandle, path: &Path) -> Result<(), IpcError> {
    app.asset_protocol_scope()
        .allow_file(path)
        .map_err(|e| IpcError {
            code: ErrorCode::ValidationError,
            message: format!("could not grant wallpaper access: {e}"),
            retryable: false,
        })
}

/// Native MP4 picker. Preparation and asset access are handled by one later IPC.
#[tauri::command]
#[tracing::instrument(skip(app))]
pub async fn dialog_pick_background_video(
    app: tauri::AppHandle,
) -> Result<Option<String>, IpcError> {
    use tauri_plugin_dialog::DialogExt;

    let picked = tauri::async_runtime::spawn_blocking(move || {
        app.dialog()
            .file()
            .add_filter("MP4 video", &["mp4"])
            .blocking_pick_file()
    })
    .await
    .map_err(|e| IpcError {
        code: ErrorCode::Unknown,
        message: format!("video dialog failed: {e}"),
        retryable: false,
    })?;

    let Some(file) = picked else {
        return Ok(None);
    };
    let path = file.into_path().map_err(|e| IpcError {
        code: ErrorCode::ValidationError,
        message: format!("invalid path: {e}"),
        retryable: false,
    })?;
    let resolved = checked_background_video(&path)?;
    tracing::info!(file = ?resolved.file_name(), "Local background video picked");
    Ok(Some(resolved.display().to_string()))
}

/// Native still-image picker. Asset access is granted by one later prepare IPC.
#[tauri::command]
#[tracing::instrument(skip(app))]
pub async fn dialog_pick_background_image(
    app: tauri::AppHandle,
) -> Result<Option<String>, IpcError> {
    use tauri_plugin_dialog::DialogExt;

    let picked = tauri::async_runtime::spawn_blocking(move || {
        app.dialog()
            .file()
            .add_filter("Image", BACKGROUND_IMAGE_EXTENSIONS)
            .blocking_pick_file()
    })
    .await
    .map_err(|e| IpcError {
        code: ErrorCode::Unknown,
        message: format!("image dialog failed: {e}"),
        retryable: false,
    })?;

    let Some(file) = picked else {
        return Ok(None);
    };
    let path = file.into_path().map_err(|e| IpcError {
        code: ErrorCode::ValidationError,
        message: format!("invalid path: {e}"),
        retryable: false,
    })?;
    let resolved = checked_background_image(&path)?;
    tracing::info!(file = ?resolved.file_name(), "Local background image picked");
    Ok(Some(resolved.display().to_string()))
}

/// Prepared wallpaper asset returned after its exact file has been whitelisted.
#[derive(Debug, serde::Serialize, ts_rs::TS)]
#[ts(export)]
pub struct BackgroundVideoAsset {
    pub path: String,
    pub width: u32,
    pub height: u32,
    pub codec: String,
    pub mode: String,
    pub backend: String,
}

impl From<rmccc_core::rmccc_platform::video::PreparedVideo> for BackgroundVideoAsset {
    fn from(prepared: rmccc_core::rmccc_platform::video::PreparedVideo) -> Self {
        let mode = match prepared.mode {
            rmccc_core::rmccc_platform::video::VideoPrepareMode::Direct => "direct",
            rmccc_core::rmccc_platform::video::VideoPrepareMode::Transcoded => "transcoded",
        };
        Self {
            path: prepared.path.display().to_string(),
            width: prepared.metadata.width,
            height: prepared.metadata.height,
            codec: prepared.metadata.codec.label().to_string(),
            mode: mode.to_string(),
            backend: prepared.backend.to_string(),
        }
    }
}

/// Prepare one hardware-safe, native-resolution wallpaper and grant its access.
///
/// Safe H.264/HEVC sources return directly. Oversized H.264 is converted once to
/// native-resolution HEVC by the injected platform backend. Any probe, transcode,
/// validation or asset-scope failure is returned instead of silently falling back
/// to a source that may require software decoding.
#[tauri::command]
#[tracing::instrument(skip(app, state, path))]
pub async fn background_video_prepare(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    path: String,
) -> Result<BackgroundVideoAsset, IpcError> {
    let resolved = checked_background_video(Path::new(&path))?;
    let metadata = rmccc_core::rmccc_platform::video::probe_mp4(&resolved)?;
    let cache_dir = {
        let core = state.core.read().await;
        core.data_dir.join("cache").join("background-video")
    };
    let request = rmccc_core::rmccc_platform::video::VideoPrepareRequest {
        source: resolved.clone(),
        cache_dir,
        metadata: metadata.clone(),
    };
    let platform = state.platform.clone();
    let started = std::time::Instant::now();
    let prepared = tauri::async_runtime::spawn_blocking(move || platform.video.prepare(&request))
        .await
        .map_err(|error| IpcError {
            code: ErrorCode::Unknown,
            message: format!("video preparation worker failed: {error}"),
            retryable: true,
        })??;

    allow_asset_read(&app, &prepared.path)?;
    tracing::info!(
        file = ?resolved.file_name(),
        prepared = %prepared.path.display(),
        backend = prepared.backend,
        mode = ?prepared.mode,
        width = metadata.width,
        height = metadata.height,
        fps = %metadata.fps_label(),
        frames = metadata.frame_count,
        elapsed_ms = started.elapsed().as_millis(),
        "Native-resolution wallpaper ready"
    );
    Ok(prepared.into())
}

/// Validate one still-image wallpaper and grant the asset protocol access.
///
/// The frontend only needs the canonical path; image decoding and scaling stay
/// in the WebView, so no new image codec dependency enters the binary.
#[tauri::command]
#[tracing::instrument(skip(app, path))]
pub fn background_image_prepare(app: tauri::AppHandle, path: String) -> Result<String, IpcError> {
    let resolved = checked_background_image(Path::new(&path))?;
    allow_asset_read(&app, &resolved)?;
    tracing::info!(
        file = ?resolved.file_name(),
        bytes = std::fs::metadata(&resolved).map(|m| m.len()).unwrap_or(0),
        "Image wallpaper ready"
    );
    Ok(resolved.display().to_string())
}

#[cfg(test)]
mod tests {
    use super::{checked_background_image, MAX_BACKGROUND_IMAGE_BYTES};
    use std::path::{Path, PathBuf};

    fn scratch(extension: &str, bytes: &[u8]) -> (PathBuf, PathBuf) {
        let dir = std::env::temp_dir().join(format!(
            "rmccc-background-image-test-{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&dir).expect("create scratch directory");
        let file = dir.join(format!("wallpaper.{extension}"));
        std::fs::write(&file, bytes).expect("write scratch image");
        (dir, file)
    }

    fn cleanup(dir: &Path) {
        std::fs::remove_dir_all(dir).expect("remove scratch directory");
    }

    #[test]
    fn accepts_supported_extensions_case_insensitively() {
        for extension in ["png", "JPG", "jpeg", "WebP", "gif", "bmp", "avif"] {
            let (dir, file) = scratch(extension, b"not-decoded-here");
            let resolved = checked_background_image(&file).expect("supported extension");
            assert_eq!(resolved, std::fs::canonicalize(&file).unwrap());
            cleanup(&dir);
        }
    }

    #[test]
    fn rejects_unsupported_or_active_content_extensions() {
        for extension in ["svg", "html", "txt", "heic"] {
            let (dir, file) = scratch(extension, b"nope");
            let error = checked_background_image(&file).expect_err("unsupported extension");
            assert_eq!(error.code, rmccc_error::ErrorCode::ValidationError);
            assert!(error.message.contains("PNG, JPEG, WebP, GIF, BMP or AVIF"));
            cleanup(&dir);
        }
    }

    #[test]
    fn rejects_empty_files() {
        let (dir, file) = scratch("png", b"");
        let error = checked_background_image(&file).expect_err("empty image");
        assert!(error.message.contains("empty"));
        cleanup(&dir);
    }

    #[test]
    fn rejects_files_above_the_decode_budget() {
        let (dir, file) = scratch("png", b"");
        let handle = std::fs::OpenOptions::new()
            .write(true)
            .open(&file)
            .expect("open scratch image");
        handle
            .set_len(MAX_BACKGROUND_IMAGE_BYTES + 1)
            .expect("grow scratch image");
        let error = checked_background_image(&file).expect_err("oversized image");
        assert!(error.message.contains("32 MB"));
        cleanup(&dir);
    }
}
