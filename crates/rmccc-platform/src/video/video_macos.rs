//! macOS native-resolution HEVC transcoding through the system `avconvert`.

use super::{VideoPrepareRequest, VideoTranscoder};
use rmccc_error::{Result, RmcccError};
use std::path::Path;
use std::process::Command;

pub struct MacosVideoTranscoder;

impl VideoTranscoder for MacosVideoTranscoder {
    fn backend_name(&self) -> &'static str {
        "macos-avconvert-hevc-native"
    }

    fn transcode(&self, request: &VideoPrepareRequest, output: &Path) -> Result<()> {
        let started = std::time::Instant::now();
        let result = Command::new("/usr/bin/avconvert")
            .arg("-s")
            .arg(&request.source)
            .arg("-p")
            .arg("PresetHEVCHighestQuality")
            .arg("-o")
            .arg(output)
            .arg("--replace")
            .arg("--disableMetadataFilter")
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::piped())
            .output()
            .map_err(|error| RmcccError::Other(format!("could not start avconvert: {error}")))?;

        if !result.status.success() {
            let stderr = String::from_utf8_lossy(&result.stderr);
            return Err(RmcccError::Other(format!(
                "avconvert exited with {}: {}",
                result.status,
                stderr.chars().take(400).collect::<String>()
            )));
        }

        tracing::info!(
            source = ?request.source.file_name(),
            output = %output.display(),
            width = request.metadata.width,
            height = request.metadata.height,
            fps = %request.metadata.fps_label(),
            frames = request.metadata.frame_count,
            elapsed_ms = started.elapsed().as_millis(),
            "Native-resolution macOS wallpaper encoded"
        );
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::video::{probe_mp4, VideoCodec};
    use std::path::PathBuf;

    #[test]
    #[ignore = "requires RMCCC_TEST_VIDEO to reference an oversized real MP4"]
    fn transcodes_native_hevc_with_exact_geometry_and_timing() {
        let source = std::env::var("RMCCC_TEST_VIDEO").expect("RMCCC_TEST_VIDEO");
        let metadata = probe_mp4(Path::new(&source)).expect("probe source");
        let dir = std::env::temp_dir().join(format!("rmccc-avconvert-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("temp dir");
        let output = dir.join("native.mp4");
        let request = VideoPrepareRequest {
            source: PathBuf::from(&source),
            cache_dir: dir.clone(),
            metadata: metadata.clone(),
        };
        MacosVideoTranscoder
            .transcode(&request, &output)
            .expect("transcode");
        let converted = probe_mp4(&output).expect("probe output");
        assert_eq!(converted.codec, VideoCodec::Hevc);
        assert_eq!(
            (converted.width, converted.height),
            (metadata.width, metadata.height)
        );
        assert_eq!(
            (converted.frame_rate_num, converted.frame_rate_den),
            (metadata.frame_rate_num, metadata.frame_rate_den)
        );
        assert_eq!(converted.frame_count, metadata.frame_count);
        assert!(converted.duration_ms.abs_diff(metadata.duration_ms) <= 200);
        let _ = std::fs::remove_dir_all(dir);
    }
}
