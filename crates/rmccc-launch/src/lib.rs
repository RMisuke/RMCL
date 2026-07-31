//! Minecraft game launcher: JVM detection, classpath resolution,
//! argument building (response file), process spawning, native extraction,
//! and managed lifecycle.

pub mod java;
pub mod managed;
pub mod natives;
pub mod process;

pub use java::{detect_java_installations, find_java, JavaInstallation};
pub use managed::{spawn_managed, ManagedProcess};
pub use natives::{decompress_natives, is_native};
pub use process::{
    build_game_env, build_response_file, spawn_game, CommandBuilder, LaunchArgBuilder,
};
