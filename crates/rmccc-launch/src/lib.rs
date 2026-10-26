//! Minecraft game launcher: JVM detection, classpath resolution,
//! argument building (response file), process spawning, native extraction,
//! log4j configuration and managed lifecycle.
//!
//! Mirrors HMCL's launch module layout:
//! - [`Launcher`] (abstract) → [`DefaultLauncher`] (implementation)
//! - [`LaunchOptions`] value object (Builder)
//! - [`ManagedProcess`] + [`ProcessListener`] / [`ExitType`]
//! - [`natives`] decompression, [`log4j`] configuration

pub mod command;
pub mod default_launcher;
pub mod java;
pub mod launcher;
pub mod log4j;
pub mod managed;
pub mod natives;
pub mod options;
pub mod process;
pub mod process_listener;

pub use command::Command;
pub use default_launcher::{new_default_launcher, DefaultLauncher};
pub use java::{detect_java_installations, find_java, JavaInstallation};
pub use launcher::{LaunchError, Launcher};
pub use log4j::{extract_log4j_configuration_file, log4j_config_file, mc_version_before};
pub use managed::ManagedProcess;
pub use natives::{clean_directory_quietly, decompress_natives, is_native};
pub use options::{
    instance_game_root, instance_mods_dir, instance_run_dir, LaunchOptions, ProcessPriority,
};
pub use process::{build_response_file, spawn_game, CommandBuilder};
pub use process_listener::{AuthInfo, ExitType, ProcessListener};
