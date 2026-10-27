//! Launch options value object — mirrors HMCL's `LaunchOptions` (Builder).
//!
//! Carries every launch-time switch that is not part of the version manifest:
//! JVM / game arguments, memory, process priority, wrapper, java agents,
//! native handling switches and pre/post launch commands. The launcher reads
//! only from this struct, keeping the launch logic side-effect free w.r.t. the
//! persisted `Profile`.

use crate::java::JavaInstallation;
use rmccc_instance::{EffectiveGameSettings, InstanceDetail, QuickPlayType, WindowType};
use std::collections::HashMap;
use std::path::PathBuf;

/// OS process priority — 定义在平台适配层（`nice`/`SetPriorityClass` 的实现侧），
/// 这里 re-export 以保持 `rmccc_launch::ProcessPriority` 的历史路径可用。
pub use rmccc_platform::ProcessPriority;

/// Quick-play target (HMCL `QuickPlayOption`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum QuickPlay {
    Singleplayer(String),
    Multiplayer(String),
    Realms(String),
}

/// Launch options for a single game launch (HMCL `LaunchOptions`).
#[derive(Debug, Clone)]
pub struct LaunchOptions {
    /// The Java runtime used to run the game.
    pub java: JavaInstallation,
    /// Run directory (the `.minecraft` equivalent).
    pub game_dir: PathBuf,
    /// The instance's **run directory**: `--gameDir`, the process working
    /// directory, and where per-instance `mods/`, `saves/` and `options.txt`
    /// live. Equals `game_dir` unless the instance is isolated.
    pub run_dir: PathBuf,
    /// Display version name shown in-game (`${version_name}`).
    pub version_name: Option<String>,
    /// Version type label (`release` / `snapshot` / ...).
    pub version_type: Option<String>,
    /// Profile name (`${profile_name}`).
    pub profile_name: Option<String>,
    /// Max heap (`-Xmx`). 0 = unset.
    pub max_memory: u32,
    /// Min heap (`-Xms`). 0 = unset.
    pub min_memory: u32,
    /// Metaspace (`-XX:MetaspaceSize` / `-XX:PermSize`). 0 = unset.
    pub metaspace: u32,
    /// User JVM arguments (applied after override args, deduplicated).
    pub jvm_args: Vec<String>,
    /// Highest-priority JVM arguments (override the version setting).
    pub override_jvm_args: Vec<String>,
    /// User additional game arguments.
    pub game_args: Vec<String>,
    /// Java agents injected as `-javaagent:<path>`.
    pub java_agents: Vec<String>,
    /// Extra environment variables for the game process.
    pub env_vars: HashMap<String, String>,
    /// Process priority.
    pub process_priority: ProcessPriority,
    /// Optional wrapper command (e.g. `flatpak run ...`).
    pub wrapper: Option<String>,
    /// Custom window width (sets `has_custom_resolution` feature).
    pub width: Option<u32>,
    /// Custom window height.
    pub height: Option<u32>,
    /// Launch in fullscreen (`--fullscreen`).
    pub fullscreen: bool,
    /// Skip HMCL-generated default JVM arguments (memory, GC, etc.).
    pub no_generated_jvm_args: bool,
    /// Skip only the optimizing JVM arguments (GC / JIT tuning).
    pub no_generated_optimizing_jvm_args: bool,
    /// Use the OS-native GLFW instead of extracting the bundled one.
    pub use_native_glfw: bool,
    /// Use the OS-native OpenAL instead of extracting the bundled one.
    pub use_native_openal: bool,
    /// Skip native library decompression (user-provided natives dir).
    pub use_custom_natives: bool,
    /// Command run before the game process (in the run directory).
    pub pre_launch_command: Option<String>,
    /// Command run after the game exits.
    pub post_exit_command: Option<String>,
    /// Enable debug log output (affects log4j config selection).
    pub enable_debug_log_output: bool,
    /// Quick-play target, if configured.
    pub quick_play: Option<QuickPlay>,
    /// 平台适配层：描述符（classpath 分隔符 / natives 命名 / library 规则）+
    /// 执行后端（权限位、符号链接、进程组、终止进程树）。由组装层注入一次，
    /// 领域代码不裸写 `cfg!`（指南 §5.7.1）。
    pub platform: std::sync::Arc<rmccc_platform::Platform>,
}

impl LaunchOptions {
    /// Build launch options from a resolved instance + detected Java runtime.
    ///
    /// Every value comes from [`EffectiveGameSettings`] (global preset merged
    /// with the instance's overrides); the two directories come from
    /// [`InstanceDetail::paths`], which is the only run-directory derivation.
    pub fn from_instance(
        instance: &InstanceDetail,
        java: JavaInstallation,
        platform: std::sync::Arc<rmccc_platform::Platform>,
    ) -> Self {
        let effective = &instance.effective;
        let non_empty = |value: &str| {
            let value = value.trim();
            (!value.is_empty()).then(|| value.to_string())
        };

        Self {
            java,
            game_dir: instance.paths.game_dir.clone(),
            run_dir: instance.paths.run_dir.clone(),
            version_name: Some(instance.summary.display_name.clone()),
            profile_name: Some(instance.summary.display_name.clone()),
            version_type: Some(instance.summary.release_type.clone()),
            // `auto_memory` currently means "use the configured heap" (there is
            // no host-memory probe in the domain layer yet). Never emit no
            // `-Xmx` at all: that lets the JVM pick an unbounded default.
            max_memory: effective.max_memory_mb,
            min_memory: effective.min_memory_mb.unwrap_or(0),
            metaspace: effective.metaspace_mb.unwrap_or(0),
            jvm_args: effective.jvm_args.clone(),
            // The new model folds the legacy "override JVM args" into
            // `jvm_args` at migration time, so there is no second list.
            override_jvm_args: Vec::new(),
            game_args: effective.game_args.clone(),
            java_agents: effective.java_agents.clone(),
            env_vars: effective
                .env_vars
                .iter()
                .map(|(key, value)| (key.clone(), value.clone()))
                .collect(),
            process_priority: effective.process_priority.to_platform(),
            wrapper: non_empty(&effective.wrapper),
            width: (effective.width > 0).then_some(effective.width),
            height: (effective.height > 0).then_some(effective.height),
            fullscreen: effective.window_type == WindowType::Fullscreen,
            no_generated_jvm_args: effective.no_generated_jvm_args,
            no_generated_optimizing_jvm_args: effective.no_generated_optimizing_jvm_args,
            use_native_glfw: effective.use_native_glfw,
            use_native_openal: effective.use_native_openal,
            use_custom_natives: effective.use_custom_natives,
            pre_launch_command: non_empty(&effective.pre_launch_command),
            post_exit_command: non_empty(&effective.post_exit_command),
            enable_debug_log_output: effective.enable_debug_log_output,
            quick_play: quick_play(effective),
            platform,
        }
    }
}

fn quick_play(effective: &EffectiveGameSettings) -> Option<QuickPlay> {
    let target = |value: &str| {
        let value = value.trim();
        (!value.is_empty()).then(|| value.to_string())
    };
    match effective.quick_play {
        QuickPlayType::None => None,
        QuickPlayType::Singleplayer => {
            target(&effective.quick_play_singleplayer).map(QuickPlay::Singleplayer)
        }
        QuickPlayType::Multiplayer => {
            target(&effective.quick_play_multiplayer).map(QuickPlay::Multiplayer)
        }
        QuickPlayType::Realms => target(&effective.quick_play_realms).map(QuickPlay::Realms),
    }
}

impl Default for LaunchOptions {
    fn default() -> Self {
        Self {
            platform: std::sync::Arc::new(rmccc_platform::Platform::current()),
            java: JavaInstallation {
                path: PathBuf::from("java"),
                major_version: 17,
                vendor: String::new(),
                is_64bit: true,
            },
            game_dir: PathBuf::new(),
            run_dir: PathBuf::new(),
            version_name: None,
            version_type: None,
            profile_name: None,
            max_memory: 0,
            min_memory: 0,
            metaspace: 0,
            jvm_args: Vec::new(),
            override_jvm_args: Vec::new(),
            game_args: Vec::new(),
            java_agents: Vec::new(),
            env_vars: HashMap::new(),
            process_priority: ProcessPriority::Normal,
            wrapper: None,
            width: None,
            height: None,
            fullscreen: false,
            no_generated_jvm_args: false,
            no_generated_optimizing_jvm_args: false,
            use_native_glfw: false,
            use_native_openal: false,
            use_custom_natives: false,
            pre_launch_command: None,
            post_exit_command: None,
            enable_debug_log_output: false,
            quick_play: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_options() {
        let o = LaunchOptions::default();
        assert_eq!(o.max_memory, 0);
        assert!(o.jvm_args.is_empty());
        assert_eq!(o.process_priority, ProcessPriority::Normal);
        assert!(o.quick_play.is_none());
    }

    #[test]
    fn quick_play_mapping_requires_a_target() {
        let mut effective = EffectiveGameSettings {
            quick_play: QuickPlayType::Singleplayer,
            quick_play_singleplayer: "   ".to_string(),
            ..Default::default()
        };
        assert!(quick_play(&effective).is_none());
        effective.quick_play_singleplayer = "world".to_string();
        assert_eq!(
            quick_play(&effective),
            Some(QuickPlay::Singleplayer("world".to_string()))
        );
    }
}
