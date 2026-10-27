//! Read models returned by the instance service — the only shapes IPC/UI need.

use crate::directory::GameDirectory;
use crate::id::InstanceKey;
use crate::settings::{
    EffectiveGameSettings, InstanceIcon, InstanceSettings, IsolationPolicy, ProcessPriority,
};
use serde::Serialize;
use std::path::PathBuf;
use ts_rs::TS;

/// Detected mod loader.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export_to = "types.ts")]
pub struct LoaderInfo {
    /// Backend id: `forge` / `neo_forge` / `fabric` / `quilt` / …
    pub kind: String,
    /// Loader version, when it can be determined from the manifest.
    pub version: Option<String>,
}

/// One row in the instance list.
///
/// Deliberately does **not** run a completion scan: listing 50 instances would
/// otherwise stat thousands of libraries/assets on every page load. Completeness
/// is a per-instance action (`instance_check_completion`).
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export_to = "types.ts")]
pub struct InstanceSummary {
    /// `{directory_id}/{instance_id}`.
    pub key: String,
    pub directory_id: String,
    pub directory_name: String,
    pub id: String,
    /// Display name (falls back to `id`).
    pub display_name: String,
    pub icon: InstanceIcon,

    /// Base Minecraft version (root of the inheritance chain).
    pub game_version: String,
    pub release_type: String,
    pub loader: Option<LoaderInfo>,
    /// Whether the instance carries loader patches / inherits a parent.
    pub modded: bool,
    pub is_modpack: bool,
    pub hidden: bool,

    pub has_jar: bool,
    pub has_json: bool,
    /// `None` = not checked (list view); `Some` from detail/completion commands.
    pub missing_count: Option<usize>,

    /// `versions/{id}`.
    pub path: PathBuf,
    /// Effective `--gameDir` (depends on isolation).
    pub run_dir: PathBuf,
    pub mods_dir: PathBuf,
    pub isolation: IsolationPolicy,
    pub isolated: bool,

    pub created_at: Option<chrono::DateTime<chrono::Utc>>,
    pub last_played: Option<chrono::DateTime<chrono::Utc>>,
    pub play_time_seconds: u64,
    pub favorite: bool,
    /// Settings file exists but cannot be written (read-only external root).
    pub settings_read_only: bool,
}

/// All on-disk locations relevant to an instance.
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export_to = "types.ts")]
pub struct InstancePaths {
    /// Registered game directory (shared versions/libraries/assets).
    pub game_dir: PathBuf,
    /// `{game_dir}/versions/{id}`.
    pub instance_dir: PathBuf,
    /// `--gameDir` / process working directory.
    pub run_dir: PathBuf,
    pub mods_dir: PathBuf,
    pub saves_dir: PathBuf,
    pub resourcepacks_dir: PathBuf,
    pub shaderpacks_dir: PathBuf,
    pub screenshots_dir: PathBuf,
    pub logs_dir: PathBuf,
    pub config_dir: PathBuf,
    pub json_file: PathBuf,
    pub jar_file: PathBuf,
    pub settings_file: PathBuf,
    /// Natives directory for this platform.
    pub natives_dir: PathBuf,
}

/// Diagnostics extracted from the version manifest.
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export_to = "types.ts")]
pub struct InstanceManifestInfo {
    pub id: String,
    pub main_class: String,
    pub release_type: String,
    pub asset_index: String,
    pub java_major: Option<u32>,
    pub library_count: usize,
    pub patch_count: usize,
    pub inherits_from: Option<String>,
    pub root: bool,
    pub hidden: bool,
    /// True when `inheritsFrom` or `patches` was folded in.
    pub has_inheritance: bool,
}

/// Full detail payload used by the instance detail page.
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export_to = "types.ts")]
pub struct InstanceDetail {
    pub summary: InstanceSummary,
    pub directory: GameDirectory,
    pub settings: InstanceSettings,
    pub effective: EffectiveGameSettings,
    pub paths: InstancePaths,
    pub manifest: InstanceManifestInfo,
    /// Whether `instance.json` existed before this load.
    pub settings_exists: bool,
    /// Non-fatal setting/manifest diagnostic.
    pub warning: Option<String>,
}

/// Effective Java-related values, resolved enough for the UI to show what will
/// be used without duplicating inheritance logic in TypeScript.
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export_to = "types.ts")]
pub struct ResolvedJavaSettings {
    pub java_type: crate::settings::JavaType,
    pub custom_java_path: String,
    pub custom_java_version: String,
    pub detected_java_version: String,
    pub detected_java_path_hash: String,
}

/// Compact resolved view for list rows / quick launch.
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export_to = "types.ts")]
pub struct ResolvedInstanceSettings {
    pub isolation: IsolationPolicy,
    pub java_type: crate::settings::JavaType,
    pub custom_java_path: String,
    pub custom_java_version: String,
    pub detected_java_version: String,
    pub detected_java_path_hash: String,
    pub jvm_args: Vec<String>,
    pub game_args: Vec<String>,
    pub java_agents: Vec<String>,
    pub no_generated_jvm_args: bool,
    pub no_generated_optimizing_jvm_args: bool,
    pub auto_memory: bool,
    pub max_memory_mb: u32,
    pub min_memory_mb: Option<u32>,
    pub metaspace_mb: Option<u32>,
    pub process_priority: ProcessPriority,
    pub window_type: crate::settings::WindowType,
    pub width: u32,
    pub height: u32,
    pub running_directory: Option<PathBuf>,
    pub launcher_visibility: crate::settings::LauncherVisibility,
    pub allow_auto_agent: bool,
    pub disable_auto_game_options: bool,
    pub graphics_backend: crate::settings::GraphicsBackend,
    pub opengl_renderer: String,
    pub vulkan_renderer: String,
    pub env_vars: std::collections::BTreeMap<String, String>,
    pub wrapper: String,
    pub pre_launch_command: String,
    pub post_exit_command: String,
    pub quick_play: crate::settings::QuickPlayType,
    pub quick_play_multiplayer: String,
    pub quick_play_singleplayer: String,
    pub quick_play_realms: String,
    pub show_logs: bool,
    pub enable_debug_log_output: bool,
    pub not_check_jvm: bool,
    pub not_check_game: bool,
    pub not_patch_natives: bool,
    pub use_custom_natives: bool,
    pub natives_directory: Option<PathBuf>,
    pub use_native_glfw: bool,
    pub use_native_openal: bool,
}

impl From<&EffectiveGameSettings> for ResolvedInstanceSettings {
    fn from(v: &EffectiveGameSettings) -> Self {
        Self {
            isolation: v.isolation,
            java_type: v.java_type,
            custom_java_path: v.custom_java_path.clone(),
            custom_java_version: v.custom_java_version.clone(),
            detected_java_version: v.detected_java_version.clone(),
            detected_java_path_hash: v.detected_java_path_hash.clone(),
            jvm_args: v.jvm_args.clone(),
            game_args: v.game_args.clone(),
            java_agents: v.java_agents.clone(),
            no_generated_jvm_args: v.no_generated_jvm_args,
            no_generated_optimizing_jvm_args: v.no_generated_optimizing_jvm_args,
            auto_memory: v.auto_memory,
            max_memory_mb: v.max_memory_mb,
            min_memory_mb: v.min_memory_mb,
            metaspace_mb: v.metaspace_mb,
            process_priority: v.process_priority,
            window_type: v.window_type,
            width: v.width,
            height: v.height,
            running_directory: v.running_directory.clone(),
            launcher_visibility: v.launcher_visibility,
            allow_auto_agent: v.allow_auto_agent,
            disable_auto_game_options: v.disable_auto_game_options,
            graphics_backend: v.graphics_backend,
            opengl_renderer: v.opengl_renderer.clone(),
            vulkan_renderer: v.vulkan_renderer.clone(),
            env_vars: v.env_vars.clone(),
            wrapper: v.wrapper.clone(),
            pre_launch_command: v.pre_launch_command.clone(),
            post_exit_command: v.post_exit_command.clone(),
            quick_play: v.quick_play,
            quick_play_multiplayer: v.quick_play_multiplayer.clone(),
            quick_play_singleplayer: v.quick_play_singleplayer.clone(),
            quick_play_realms: v.quick_play_realms.clone(),
            show_logs: v.show_logs,
            enable_debug_log_output: v.enable_debug_log_output,
            not_check_jvm: v.not_check_jvm,
            not_check_game: v.not_check_game,
            not_patch_natives: v.not_patch_natives,
            use_custom_natives: v.use_custom_natives,
            natives_directory: v.natives_directory.clone(),
            use_native_glfw: v.use_native_glfw,
            use_native_openal: v.use_native_openal,
        }
    }
}

impl InstanceSummary {
    /// Parse the summary key back into its typed parts.
    pub fn instance_key(&self) -> rmccc_error::Result<InstanceKey> {
        InstanceKey::parse(&self.key)
    }
}
