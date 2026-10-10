//! Launcher abstraction — mirrors HMCL's `Launcher` abstract base class.
//!
//! A [`Launcher`] turns a resolved manifest + auth + [`LaunchOptions`] into a
//! launch command or a launch script on disk. The concrete
//! [`crate::default_launcher::DefaultLauncher`] implements the full command-line
//! pipeline, native extraction, log4j configuration and (async) process
//! spawning — the async `launch()` lives on the concrete type.

use crate::command::Command;
use rmccc_error::RmcccError;
use std::path::Path;

/// Result alias for launch operations.
pub type LaunchResult<T> = std::result::Result<T, LaunchError>;

/// Launch-time errors (HMCL `ProcessCreationException` / `NotDecompressingNativesException`).
#[derive(Debug, thiserror::Error)]
pub enum LaunchError {
    /// The generated command line contained blank arguments.
    #[error("Illegal command line: {0:?}")]
    IllegalCommandLine(Vec<String>),
    /// The main class is missing.
    #[error("Main class is null for instance {0}")]
    MissingMainClass(String),
    /// Native library decompression failed.
    #[error("Failed to decompress native libraries: {0}")]
    Natives(#[from] std::io::Error),
    /// The client JAR is missing.
    #[error("Minecraft client JAR does not exist: {0}")]
    MissingClientJar(String),
    /// The launch script exceeds the platform command-line limit.
    #[error("Command line is too long ({0} chars > 32767)")]
    CommandTooLong(usize),
    /// The launch script could not be made executable.
    #[error("Cannot make launch script executable: {0}")]
    Permission(String),
    /// Process creation failed.
    #[error("Failed to start Java process: {0}")]
    ProcessCreation(RmcccError),
    /// Other launch-time failures.
    #[error("Launch failed: {0}")]
    Other(String),
}

impl From<LaunchError> for RmcccError {
    fn from(e: LaunchError) -> Self {
        RmcccError::Other(e.to_string())
    }
}

impl From<RmcccError> for LaunchError {
    fn from(e: RmcccError) -> Self {
        LaunchError::Other(e.to_string())
    }
}

/// The launch entry point contract (HMCL `Launcher`).
pub trait Launcher: Send + Sync {
    /// Generate the full command line for this launch.
    fn generate_command_line(&self) -> Result<Command, LaunchError>;

    /// Write a platform launch script (`.bat` / `.ps1` / `.sh`) to `path`.
    fn make_launch_script(&self, path: &Path) -> Result<(), LaunchError>;
}
