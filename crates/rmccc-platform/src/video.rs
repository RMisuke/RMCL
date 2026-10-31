//! Native-resolution video wallpaper preparation.
//!
//! The system WebView remains responsible for GPU decode and composition. This
//! module only selects a hardware-safe MP4, converts oversized H.264 to native
//! HEVC when necessary, and validates that conversion did not change the image
//! dimensions or playback timing. Platform modules contain only transcoder
//! execution; policy, cache and validation stay shared.

use rmccc_error::{Result, RmcccError};
use sha1::{Digest, Sha1};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

mod probe;
mod unsupported;
#[cfg(target_os = "macos")]
mod video_macos;
#[cfg(target_os = "windows")]
mod video_windows;

pub use probe::probe_mp4;
pub use unsupported::UnsupportedVideoTranscoder;
#[cfg(target_os = "macos")]
pub use video_macos::MacosVideoTranscoder;
#[cfg(target_os = "windows")]
pub use video_windows::WindowsVideoTranscoder;

/// Cache recipe. Changing codec, sizing or validation requires a new version.
pub const PREPARE_RECIPE: &str = "hevc-native-timing-v1";

/// Conservative H.264 single-axis limits for the system WebView GPU path.
pub const H264_HARDWARE_MAX_WIDTH: u32 = 4096;
pub const H264_HARDWARE_MAX_HEIGHT: u32 = 2304;

static STAGING_SEQUENCE: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VideoCodec {
    H264,
    Hevc,
    Other(String),
}

impl VideoCodec {
    pub fn label(&self) -> &str {
        match self {
            VideoCodec::H264 => "h264",
            VideoCodec::Hevc => "hevc",
            VideoCodec::Other(value) => value,
        }
    }
}

/// Metadata from the first video track in a non-fragmented MP4.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VideoMetadata {
    pub width: u32,
    pub height: u32,
    pub frame_rate_num: u32,
    pub frame_rate_den: u32,
    pub frame_count: u64,
    pub duration_ms: u64,
    pub codec: VideoCodec,
}

impl VideoMetadata {
    pub fn fps(&self) -> f64 {
        if self.frame_rate_den == 0 {
            return 0.0;
        }
        self.frame_rate_num as f64 / self.frame_rate_den as f64
    }

    pub fn fps_label(&self) -> String {
        format!("{}/{}", self.frame_rate_num, self.frame_rate_den)
    }
}

#[derive(Debug, Clone)]
pub struct VideoPrepareRequest {
    pub source: PathBuf,
    pub cache_dir: PathBuf,
    pub metadata: VideoMetadata,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VideoPrepareMode {
    Direct,
    Transcoded,
}

/// A validated local file ready for the WebView asset protocol.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreparedVideo {
    pub path: PathBuf,
    pub metadata: VideoMetadata,
    pub mode: VideoPrepareMode,
    pub backend: &'static str,
}

/// Platform transcoder seam. Implementations write native-resolution HEVC.
pub trait VideoTranscoder: Send + Sync {
    fn backend_name(&self) -> &'static str;

    fn transcode(&self, request: &VideoPrepareRequest, output: &Path) -> Result<()>;
}

/// Platform-neutral wallpaper engine injected into the application layer.
pub trait VideoEngine: Send + Sync {
    fn backend_name(&self) -> &'static str;

    fn prepare(&self, request: &VideoPrepareRequest) -> Result<PreparedVideo>;
}

/// Shared decision, cache and validation engine.
pub struct CachedVideoEngine<T: VideoTranscoder> {
    backend: T,
}

impl<T: VideoTranscoder> CachedVideoEngine<T> {
    pub fn new(backend: T) -> Self {
        Self { backend }
    }

    fn prepared(
        &self,
        path: PathBuf,
        metadata: VideoMetadata,
        mode: VideoPrepareMode,
    ) -> PreparedVideo {
        PreparedVideo {
            path,
            metadata,
            mode,
            backend: self.backend.backend_name(),
        }
    }
}

impl<T: VideoTranscoder + 'static> VideoEngine for CachedVideoEngine<T> {
    fn backend_name(&self) -> &'static str {
        self.backend.backend_name()
    }

    fn prepare(&self, request: &VideoPrepareRequest) -> Result<PreparedVideo> {
        ensure_supported_source(&request.metadata)?;
        if !needs_native_transcode(&request.metadata) {
            return Ok(self.prepared(
                request.source.clone(),
                request.metadata.clone(),
                VideoPrepareMode::Direct,
            ));
        }

        std::fs::create_dir_all(&request.cache_dir)?;
        let output = cache_path(request)?;
        if output.is_file() && validate_output(&output, &request.metadata).is_ok() {
            return Ok(self.prepared(
                output,
                request.metadata.clone(),
                VideoPrepareMode::Transcoded,
            ));
        }

        cleanup_stale_staging(&request.cache_dir);
        let temp = staging_path(&output);
        if let Err(error) = self.backend.transcode(request, &temp) {
            let _ = std::fs::remove_file(&temp);
            return Err(error);
        }
        if let Err(error) = validate_output(&temp, &request.metadata) {
            let _ = std::fs::remove_file(&temp);
            return Err(error);
        }

        // Another process may have completed the same immutable cache entry.
        if output.is_file() && validate_output(&output, &request.metadata).is_ok() {
            let _ = std::fs::remove_file(&temp);
            return Ok(self.prepared(
                output,
                request.metadata.clone(),
                VideoPrepareMode::Transcoded,
            ));
        }
        if output.exists() {
            std::fs::remove_file(&output)?;
        }
        if let Err(error) = std::fs::rename(&temp, &output) {
            let _ = std::fs::remove_file(&temp);
            return Err(error.into());
        }
        Ok(self.prepared(
            output,
            request.metadata.clone(),
            VideoPrepareMode::Transcoded,
        ))
    }
}

fn ensure_supported_source(metadata: &VideoMetadata) -> Result<()> {
    if matches!(metadata.codec, VideoCodec::Other(_)) {
        return Err(RmcccError::Validation(format!(
            "MP4 codec {} is not supported by the hardware wallpaper engine; use H.264 or HEVC",
            metadata.codec.label()
        )));
    }
    Ok(())
}

/// Only oversized H.264 needs conversion. HEVC already uses the target codec.
pub fn needs_native_transcode(metadata: &VideoMetadata) -> bool {
    metadata.codec == VideoCodec::H264
        && (metadata.width > H264_HARDWARE_MAX_WIDTH || metadata.height > H264_HARDWARE_MAX_HEIGHT)
}

fn cache_path(request: &VideoPrepareRequest) -> Result<PathBuf> {
    let file_meta = std::fs::metadata(&request.source)?;
    let modified = file_meta
        .modified()
        .ok()
        .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|duration| duration.as_nanos())
        .unwrap_or(0);
    let metadata = &request.metadata;

    let mut hasher = Sha1::new();
    hasher.update(PREPARE_RECIPE.as_bytes());
    hasher.update(request.source.to_string_lossy().as_bytes());
    hasher.update(file_meta.len().to_le_bytes());
    hasher.update(modified.to_le_bytes());
    hasher.update(metadata.width.to_le_bytes());
    hasher.update(metadata.height.to_le_bytes());
    hasher.update(metadata.frame_rate_num.to_le_bytes());
    hasher.update(metadata.frame_rate_den.to_le_bytes());
    hasher.update(metadata.frame_count.to_le_bytes());
    hasher.update(metadata.codec.label().as_bytes());
    let hash = format!("{:x}", hasher.finalize());
    Ok(request.cache_dir.join(format!("{hash}.mp4")))
}

fn output_stem(path: &Path) -> String {
    path.file_stem()
        .map(|value| value.to_string_lossy().into_owned())
        .unwrap_or_else(|| "wallpaper".to_string())
}

fn staging_path(output: &Path) -> PathBuf {
    let sequence = STAGING_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let parent = output.parent().unwrap_or_else(|| Path::new("."));
    parent.join(format!(
        "{}.{}.{}.tmp.mp4",
        output_stem(output),
        std::process::id(),
        sequence
    ))
}

/// Remove only stale staging files; active work in another process is untouched.
fn cleanup_stale_staging(cache_dir: &Path) {
    let Ok(entries) = std::fs::read_dir(cache_dir) else {
        return;
    };
    let stale_before = std::time::SystemTime::now()
        .checked_sub(std::time::Duration::from_secs(6 * 60 * 60))
        .unwrap_or(std::time::UNIX_EPOCH);
    for entry in entries.flatten() {
        if !entry.file_name().to_string_lossy().contains(".tmp.mp4") {
            continue;
        }
        let stale = entry
            .metadata()
            .ok()
            .and_then(|metadata| metadata.modified().ok())
            .map(|modified| modified < stale_before)
            .unwrap_or(false);
        if stale {
            let _ = std::fs::remove_file(entry.path());
        }
    }
}

fn validate_output(path: &Path, source: &VideoMetadata) -> Result<()> {
    let output = probe_mp4(path)?;
    validate_metadata(&output, source)
}

fn validate_metadata(output: &VideoMetadata, source: &VideoMetadata) -> Result<()> {
    if output.codec != VideoCodec::Hevc {
        return Err(RmcccError::Validation(format!(
            "wallpaper output codec is {}, expected hevc",
            output.codec.label()
        )));
    }
    if (output.width, output.height) != (source.width, source.height) {
        return Err(RmcccError::Validation(format!(
            "wallpaper output dimensions {}×{} differ from source {}×{}",
            output.width, output.height, source.width, source.height
        )));
    }
    if (output.frame_rate_num, output.frame_rate_den)
        != (source.frame_rate_num, source.frame_rate_den)
    {
        return Err(RmcccError::Validation(format!(
            "wallpaper output frame rate {} differs from source {}",
            output.fps_label(),
            source.fps_label()
        )));
    }
    if output.frame_count != source.frame_count {
        return Err(RmcccError::Validation(format!(
            "wallpaper output frame count {} differs from source {}",
            output.frame_count, source.frame_count
        )));
    }
    if output.duration_ms.abs_diff(source.duration_ms) > 200 {
        return Err(RmcccError::Validation(format!(
            "wallpaper output duration {}ms differs from source {}ms",
            output.duration_ms, source.duration_ms
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn metadata(width: u32, height: u32) -> VideoMetadata {
        VideoMetadata {
            width,
            height,
            frame_rate_num: 30_000,
            frame_rate_den: 1_001,
            frame_count: 3_840,
            duration_ms: 128_128,
            codec: VideoCodec::H264,
        }
    }

    #[test]
    fn safe_sources_play_directly_without_copies() {
        assert!(!needs_native_transcode(&metadata(4096, 2304)));
        assert!(!needs_native_transcode(&metadata(1920, 1080)));
        let mut hevc = metadata(7680, 4320);
        hevc.codec = VideoCodec::Hevc;
        assert!(!needs_native_transcode(&hevc));
    }

    #[test]
    fn only_oversized_h264_selects_native_hevc() {
        assert!(needs_native_transcode(&metadata(4126, 2596)));
        assert!(needs_native_transcode(&metadata(4096, 2305)));
        assert_eq!(PREPARE_RECIPE, "hevc-native-timing-v1");
    }

    #[test]
    fn unknown_codecs_are_not_claimed_as_hardware_decoded() {
        let mut source = metadata(1920, 1080);
        source.codec = VideoCodec::Other("vp09".into());
        assert!(ensure_supported_source(&source).is_err());
    }

    #[test]
    fn validation_preserves_native_dimensions_and_exact_timing() {
        let source = metadata(4126, 2596);
        let mut output = source.clone();
        output.codec = VideoCodec::Hevc;
        assert!(validate_metadata(&output, &source).is_ok());

        output.width = 1920;
        assert!(validate_metadata(&output, &source).is_err());
        output.width = source.width;
        output.frame_rate_num = 30;
        output.frame_rate_den = 1;
        assert!(validate_metadata(&output, &source).is_err());
        output.frame_rate_num = source.frame_rate_num;
        output.frame_rate_den = source.frame_rate_den;
        output.frame_count += 1;
        assert!(validate_metadata(&output, &source).is_err());
        output.frame_count = source.frame_count;
        output.duration_ms += 201;
        assert!(validate_metadata(&output, &source).is_err());
    }

    #[test]
    fn validation_rejects_h264_or_scaled_cache_entries() {
        let source = metadata(4126, 2596);
        assert!(validate_metadata(&source, &source).is_err());
        let mut scaled = source.clone();
        scaled.codec = VideoCodec::Hevc;
        scaled.width = 1920;
        scaled.height = 1208;
        assert!(validate_metadata(&scaled, &source).is_err());
    }

    #[test]
    fn staging_names_are_unique_within_one_process() {
        let output = Path::new("/tmp/cache/wallpaper.mp4");
        assert_ne!(staging_path(output), staging_path(output));
    }

    #[cfg(target_os = "macos")]
    #[test]
    #[ignore = "requires RMCCC_TEST_VIDEO to reference an oversized real MP4"]
    fn native_engine_round_trip_preserves_source_geometry_and_timing() {
        let source = std::env::var("RMCCC_TEST_VIDEO").expect("RMCCC_TEST_VIDEO");
        let metadata = probe_mp4(Path::new(&source)).expect("probe source");
        assert!(needs_native_transcode(&metadata));
        let dir = std::env::temp_dir().join(format!("rmccc-wallpaper-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("temp dir");
        let request = VideoPrepareRequest {
            source: PathBuf::from(&source),
            cache_dir: dir.clone(),
            metadata: metadata.clone(),
        };
        let prepared = CachedVideoEngine::new(MacosVideoTranscoder)
            .prepare(&request)
            .expect("prepare");
        assert_eq!(prepared.mode, VideoPrepareMode::Transcoded);
        assert_eq!(prepared.metadata, metadata);
        let output = probe_mp4(&prepared.path).expect("probe output");
        assert!(validate_metadata(&output, &metadata).is_ok());
        let _ = std::fs::remove_dir_all(dir);
    }
}
