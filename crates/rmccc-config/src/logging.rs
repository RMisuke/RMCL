//! Logging initialization using `tracing`.
//!
//! Features:
//! - Console output (colored, INFO level)
//! - File output (JSON structured, daily rolling, max 10 files, max 5MB each)

use std::path::Path;
use tracing_appender::rolling::Rotation;
use tracing_subscriber::{fmt, prelude::*, EnvFilter};

/// Initialize the global tracing subscriber.
///
/// The file writer guard is stored in a static to prevent it from being
/// dropped when this function returns, which would stop log output.
pub fn init_logging(log_dir: &Path) -> rmccc_error::Result<()> {
    use std::sync::OnceLock;

    static LOG_GUARD: OnceLock<tracing_appender::non_blocking::WorkerGuard> = OnceLock::new();

    std::fs::create_dir_all(log_dir)?;

    // Daily rolling with max 10 log files retained
    let file_appender = tracing_appender::rolling::Builder::new()
        .rotation(Rotation::DAILY)
        .max_log_files(10)
        .filename_prefix("rmccc")
        .filename_suffix("log")
        .build(log_dir)
        .map_err(|e| rmccc_error::RmcccError::Other(format!("Failed to create log appender: {e}")))?;

    let (non_blocking, guard) = tracing_appender::non_blocking(file_appender);
    LOG_GUARD.set(guard).ok();

    tracing_subscriber::registry()
        .with(fmt::layer().with_target(false))
        .with(fmt::layer().json().with_writer(non_blocking))
        .with(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| EnvFilter::new("rmccc=info")),
        )
        .init();

    tracing::info!(log_dir = %log_dir.display(), "Logging initialized");
    Ok(())
}
