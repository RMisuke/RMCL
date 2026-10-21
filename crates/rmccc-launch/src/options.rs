//! Launch options value object — mirrors HMCL's `LaunchOptions` (Builder).
//!
//! Carries every launch-time switch that is not part of the version manifest:
//! JVM / game arguments, memory, process priority, wrapper, java agents,
//! native handling switches and pre/post launch commands. The launcher reads
//! only from this struct, keeping the launch logic side-effect free w.r.t. the
//! persisted `Profile`.

use crate::java::JavaInstallation;
use std::collections::HashMap;
use std::path::PathBuf;

/// OS process priority — 定义在平台适配层（`nice`/`SetPriorityClass` 的实现侧），
/// 这里 re-export 以保持 `rmccc_launch::ProcessPriority` 的历史路径可用。
pub use rmccc_platform::ProcessPriority;

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
    /// 平台适配层：描述符（classpath 分隔符 / natives 命名 / library 规则）+
    /// 执行后端（权限位、符号链接、进程组、终止进程树）。由组装层注入一次，
    /// 领域代码不裸写 `cfg!`（指南 §5.7.1）。
    pub platform: std::sync::Arc<rmccc_platform::Platform>,
}

impl LaunchOptions {
    /// Build launch options from a persisted profile + detected Java runtime.
    ///
    /// Maps every extended [`Profile`](rmccc_profile::Profile) field onto the
    /// corresponding [`LaunchOptions`] field so both the Tauri and CLI
    /// orchestration layers share a single conversion.
    /// Builds the launcher options from a profile plus the launcher-wide
    /// defaults.
    ///
    /// Both directories are derived here so every caller agrees on them:
    /// `game_dir` is the shared game root from the config (versions, libraries,
    /// assets), and `run_dir` is that root unless the instance's isolation
    /// setting gives it its own (HMCL `getRunDirectory`).
    pub fn from_profile(
        profile: &rmccc_profile::Profile,
        config: &rmccc_config::AppConfig,
        java: JavaInstallation,
        platform: std::sync::Arc<rmccc_platform::Platform>,
    ) -> Self {
        let resolved = profile.resolve(config);
        let game_dir = config.resolved_game_root();
        let run_dir = instance_run_dir(config, profile);

        Self {
            java,
            game_dir,
            run_dir,
            platform,
            version_name: Some(profile.name.clone()),
            profile_name: Some(profile.name.clone()),
            version_type: None,
            max_memory: resolved.max_memory_mb,
            min_memory: resolved.min_memory_mb,
            metaspace: resolved.metaspace_mb,
            jvm_args: resolved.jvm_args.clone(),
            override_jvm_args: resolved.override_jvm_args.clone(),
            game_args: resolved.game_args.clone(),
            java_agents: resolved.java_agents.clone(),
            env_vars: resolved.env_vars.clone(),
            process_priority: match resolved.process_priority.as_str() {
                "low" => ProcessPriority::Low,
                "below_normal" => ProcessPriority::BelowNormal,
                "above_normal" => ProcessPriority::AboveNormal,
                "high" => ProcessPriority::High,
                _ => ProcessPriority::Normal,
            },
            wrapper: resolved.wrapper.clone(),
            width: resolved.width,
            height: resolved.height,
            fullscreen: resolved.fullscreen,
            use_native_glfw: resolved.use_native_glfw,
            use_native_openal: resolved.use_native_openal,
            use_custom_natives: resolved.use_custom_natives,
            no_generated_jvm_args: resolved.no_generated_jvm_args,
            no_generated_optimizing_jvm_args: resolved.no_generated_optimizing_jvm_args,
            enable_debug_log_output: resolved.enable_debug_log_output,
            pre_launch_command: resolved.pre_launch_command.clone(),
            post_exit_command: resolved.post_exit_command.clone(),
        }
    }
}

/// The run directory (`--gameDir`) for an instance.
///
/// Equals the shared game root unless the instance's isolation setting gives it
/// its own directory (HMCL `HMCLGameRepository.getRunDirectory`): `always`
/// isolates every instance, `modded` only instances carrying a mod loader, and
/// `never` shares one `.minecraft`.
///
/// Every consumer of per-instance runtime files (launch, mods, saves) must use
/// this rather than `profile.game_dir`, which is the shared root.
pub fn instance_run_dir(
    config: &rmccc_config::AppConfig,
    profile: &rmccc_profile::Profile,
) -> std::path::PathBuf {
    let isolation = rmccc_version::IsolationType::parse(&profile.effective_isolation(config))
        .unwrap_or_default();
    rmccc_version::isolation::run_directory(
        &config.resolved_game_root(),
        &profile.game_version,
        isolation,
        profile.mod_loader.is_some(),
    )
}

/// The `mods/` directory for an instance, following its run directory.
pub fn instance_mods_dir(
    config: &rmccc_config::AppConfig,
    profile: &rmccc_profile::Profile,
) -> std::path::PathBuf {
    instance_run_dir(config, profile).join("mods")
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
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_priority_nice_values() {
        assert_eq!(ProcessPriority::Normal.nice_value(), None);
        assert_eq!(ProcessPriority::High.nice_value(), Some("-5"));
        assert_eq!(ProcessPriority::Low.nice_value(), Some("5"));
    }

    #[test]
    fn test_default_options() {
        let o = LaunchOptions::default();
        assert_eq!(o.max_memory, 0);
        assert!(o.jvm_args.is_empty());
        assert_eq!(o.process_priority, ProcessPriority::Normal);
    }
    fn isolation_config(default_isolation: &str) -> rmccc_config::AppConfig {
        rmccc_config::AppConfig {
            game_root_dir: Some(std::path::PathBuf::from("/g")),
            default_isolation: default_isolation.to_string(),
            ..Default::default()
        }
    }

    /// Built through serde so the test does not need a direct dependency on the
    /// modloader crate just to name its enum.
    fn loader() -> rmccc_profile::ModLoaderConfig {
        serde_json::from_value(serde_json::json!({
            "loader_type": "fabric",
            "loader_version": "0.16.9"
        }))
        .expect("valid ModLoaderConfig")
    }

    /// The run directory is what isolation actually controls: `always` isolates
    /// everything, `modded` only instances carrying a loader, `never` shares the
    /// game root — and an instance's own value beats the global default.
    #[test]
    fn test_instance_run_dir_follows_isolation() {
        let modded_default = isolation_config("modded");
        let always_default = isolation_config("always");
        let never_default = isolation_config("never");

        // modded + loader → own directory
        let with_loader = rmccc_profile::Profile {
            game_version: "1.20.4-fabric-0.16.9".into(),
            mod_loader: Some(loader()),
            ..Default::default()
        };
        assert_eq!(
            instance_run_dir(&modded_default, &with_loader),
            std::path::PathBuf::from("/g/versions/1.20.4-fabric-0.16.9")
        );
        assert_eq!(
            instance_mods_dir(&modded_default, &with_loader),
            std::path::PathBuf::from("/g/versions/1.20.4-fabric-0.16.9/mods")
        );

        // modded without a loader → shared root (libraries/assets stay shared)
        let vanilla = rmccc_profile::Profile {
            game_version: "1.20.4".into(),
            ..Default::default()
        };
        assert_eq!(
            instance_run_dir(&modded_default, &vanilla),
            std::path::PathBuf::from("/g")
        );

        // always → isolated even without a loader
        assert_eq!(
            instance_run_dir(&always_default, &vanilla),
            std::path::PathBuf::from("/g/versions/1.20.4")
        );

        // never → shared even with a loader
        assert_eq!(
            instance_run_dir(&never_default, &with_loader),
            std::path::PathBuf::from("/g")
        );

        // an instance-level value overrides the global default
        let pinned_never = rmccc_profile::Profile {
            isolation: Some("never".into()),
            ..with_loader.clone()
        };
        assert_eq!(
            instance_run_dir(&modded_default, &pinned_never),
            std::path::PathBuf::from("/g")
        );
        let pinned_always = rmccc_profile::Profile {
            isolation: Some("always".into()),
            ..vanilla.clone()
        };
        assert_eq!(
            instance_run_dir(&never_default, &pinned_always),
            std::path::PathBuf::from("/g/versions/1.20.4")
        );
    }

    /// `from_profile` must feed the resolved values (not the raw profile) and use
    /// the same two directories as `instance_run_dir`.
    #[test]
    fn test_from_profile_uses_resolved_values_and_dirs() {
        let config = rmccc_config::AppConfig {
            game_root_dir: Some(std::path::PathBuf::from("/g")),
            default_isolation: "always".into(),
            default_max_memory_mb: 3072,
            default_jvm_args: vec!["-Dglobal=1".into()],
            ..Default::default()
        };
        let profile = rmccc_profile::Profile {
            game_version: "1.20.4".into(),
            memory_mb: 8192, // ignored: not in override_properties
            ..Default::default()
        };

        let options = LaunchOptions::from_profile(
            &profile,
            &config,
            JavaInstallation {
                path: std::path::PathBuf::from("/java"),
                major_version: 21,
                vendor: String::new(),
                is_64bit: true,
            },
            std::sync::Arc::new(rmccc_platform::Platform::current()),
        );
        assert_eq!(options.max_memory, 3072, "inherits the global default");
        assert_eq!(options.jvm_args, vec!["-Dglobal=1"]);
        assert_eq!(options.game_dir, std::path::PathBuf::from("/g"));
        assert_eq!(
            options.run_dir,
            std::path::PathBuf::from("/g/versions/1.20.4")
        );
    }
}
