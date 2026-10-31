//! Explicit unsupported backend for platforms without a native transcoder.

use super::{VideoPrepareRequest, VideoTranscoder};
use rmccc_error::{Result, RmcccError};
use std::path::Path;

pub struct UnsupportedVideoTranscoder;

impl VideoTranscoder for UnsupportedVideoTranscoder {
    fn backend_name(&self) -> &'static str {
        "unsupported"
    }

    fn transcode(&self, _request: &VideoPrepareRequest, _output: &Path) -> Result<()> {
        Err(RmcccError::UnsupportedPlatform(format!(
            "native wallpaper transcoding is not implemented for {}",
            std::env::consts::OS
        )))
    }
}
