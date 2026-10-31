//! Windows native-resolution HEVC transcoding through WinRT MediaTranscoder.
//!
//! Hardware acceleration is mandatory for this preparation path. Codec, GPU and
//! driver support are checked by Media Foundation; any failure is returned to the
//! UI. Native Windows compilation and WebView2 GPU verification are deferred to
//! a native Windows runner under the project's platform validation policy.

use super::{VideoPrepareRequest, VideoTranscoder};
use rmccc_error::{Result, RmcccError};
use std::path::Path;
use windows::core::HSTRING;
use windows::Media::MediaProperties::{MediaEncodingProfile, VideoEncodingQuality};
use windows::Media::Transcoding::MediaTranscoder;
use windows::Storage::StorageFile;
use windows::Win32::System::WinRT::{RoInitialize, RO_INIT_MULTITHREADED};

pub struct WindowsVideoTranscoder;

impl VideoTranscoder for WindowsVideoTranscoder {
    fn backend_name(&self) -> &'static str {
        "windows-media-transcoder-hevc-native"
    }

    fn transcode(&self, request: &VideoPrepareRequest, output: &Path) -> Result<()> {
        let started = std::time::Instant::now();
        if let Err(error) = unsafe { RoInitialize(RO_INIT_MULTITHREADED) } {
            tracing::debug!(error = ?error, "WinRT apartment was already initialized");
        }

        let profile = map(MediaEncodingProfile::CreateHevc(
            VideoEncodingQuality::Uhd2160p,
        ))?;
        let video = map(profile.Video())?;
        map(video.SetWidth(request.metadata.width))?;
        map(video.SetHeight(request.metadata.height))?;
        let frame_rate = map(video.FrameRate())?;
        map(frame_rate.SetNumerator(request.metadata.frame_rate_num.max(1)))?;
        map(frame_rate.SetDenominator(request.metadata.frame_rate_den.max(1)))?;
        map(profile.SetVideo(&video))?;

        let transcoder = map(MediaTranscoder::new())?;
        map(transcoder.SetHardwareAccelerationEnabled(true))?;
        map(transcoder.SetAlwaysReencode(true))?;

        std::fs::File::create(output)?;
        let source = map(StorageFile::GetFileFromPathAsync(&HSTRING::from(
            request.source.to_string_lossy().as_ref(),
        )))?
        .get()
        .map_err(win_error)?;
        let destination = map(StorageFile::GetFileFromPathAsync(&HSTRING::from(
            output.to_string_lossy().as_ref(),
        )))?
        .get()
        .map_err(win_error)?;

        let prepared = map(transcoder.PrepareFileTranscodeAsync(&source, &destination, &profile))?
            .get()
            .map_err(win_error)?;
        if !map(prepared.CanTranscode())? {
            let reason = prepared.FailureReason().unwrap_or_default();
            return Err(RmcccError::Other(format!(
                "native HEVC hardware transcode is unavailable: {reason:?}; check the Windows HEVC codec and GPU driver"
            )));
        }
        map(prepared.TranscodeAsync())?.get().map_err(win_error)?;

        tracing::info!(
            source = ?request.source.file_name(),
            output = %output.display(),
            width = request.metadata.width,
            height = request.metadata.height,
            fps = %request.metadata.fps_label(),
            frames = request.metadata.frame_count,
            elapsed_ms = started.elapsed().as_millis(),
            "Native-resolution Windows wallpaper encoded"
        );
        Ok(())
    }
}

fn map<T>(result: windows::core::Result<T>) -> Result<T> {
    result.map_err(win_error)
}

fn win_error(error: windows::core::Error) -> RmcccError {
    RmcccError::Other(format!("Windows MediaTranscoder: {error}"))
}
