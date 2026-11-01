//! Windows native-resolution wallpaper transcoding through WinRT MediaTranscoder.
//!
//! Hardware acceleration is mandatory for this preparation path. Codec, GPU and
//! driver support are checked by Media Foundation; any failure is returned to the
//! UI. Native Windows compilation and WebView2 GPU verification are deferred to
//! a native Windows runner under the project's platform validation policy.

use super::{VideoCodec, VideoPrepareRequest, VideoTranscoder};
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
        "windows-media-transcoder-h264-native"
    }

    /// WebView2 用系统媒体基础解码：H.264 在任何干净安装的 Windows 上都硬解，
    /// HEVC 则要用户另装"HEVC 视频扩展"（商店付费/厂商版），否则 `<video>`
    /// 直接黑屏。所以 Windows 输出 H.264——宁可码率高一些，也不让壁纸在
    /// 默认系统上播不出来。
    fn target_codec(&self) -> VideoCodec {
        VideoCodec::H264
    }

    fn transcode(&self, request: &VideoPrepareRequest, output: &Path) -> Result<()> {
        let started = std::time::Instant::now();
        if let Err(error) = unsafe { RoInitialize(RO_INIT_MULTITHREADED) } {
            tracing::debug!(error = ?error, "WinRT apartment was already initialized");
        }

        let profile = map(MediaEncodingProfile::CreateMp4(
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

        // MediaTranscoder 自己创建目标文件。预建一个空文件（旧实现）会让
        // PrepareFileTranscodeAsync 在"目标已存在"上失败——而那正是每次
        // 转码都会走到的路径。
        if output.exists() {
            std::fs::remove_file(output)?;
        }

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
                "native H.264 hardware transcode is unavailable: {reason:?}; check the GPU driver and Media Foundation codecs"
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
