//! Global game-settings preset + per-instance settings overlay.
//!
//! This is the RMCCC counterpart of HMCL's `GameSettings.Preset` /
//! `GameSettings.Instance` pair:
//!
//! - one global preset lives in `config/game-settings.json`;
//! - each instance has `{instance_root}/.rmccc/instance.json`;
//! - `override_properties` names the fields that take the instance value;
//!   every other field inherits the global preset.
//!
//! Values and override flags are stored separately (HMCL
//! `InheritableProperty`): clearing an override keeps the user's value on disk
//! so it can be restored when they turn the lock back on.

use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use ts_rs::TS;

/// How the Java runtime is selected.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export_to = "types.ts")]
pub enum JavaType {
    /// Let the launcher pick a suitable runtime.
    #[default]
    Auto,
    /// Use `custom_java_path`.
    Custom,
    /// Pick a detected runtime with major version `custom_java_version`.
    Version,
    /// Use the detected runtime identified by version + path hash.
    Detected,
}

/// Initial game-window mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export_to = "types.ts")]
pub enum WindowType {
    #[default]
    Windowed,
    Fullscreen,
    Maximized,
}

/// What the launcher does after the game process starts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export_to = "types.ts")]
pub enum LauncherVisibility {
    /// Close the launcher as soon as the process is created.
    Close,
    /// Hide the launcher; show it again only if launching failed.
    #[default]
    Hide,
    /// Keep the launcher visible.
    Keep,
    /// Hide the launcher and reopen it when the game exits.
    HideAndReopen,
}

/// Graphics API requested from the game.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export_to = "types.ts")]
pub enum GraphicsBackend {
    #[default]
    Default,
    Opengl,
    Vulkan,
}

/// Quick-play target type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export_to = "types.ts")]
pub enum QuickPlayType {
    #[default]
    None,
    Multiplayer,
    Singleplayer,
    Realms,
}

/// Instance isolation strategy (HMCL `DefaultIsolationType` + `runningDirectory`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export_to = "types.ts")]
pub enum IsolationPolicy {
    /// Share the game root's run directory.
    Never,
    /// Isolate only instances carrying loader patches.
    #[default]
    Modded,
    /// Always run inside `versions/{id}`.
    Always,
}

impl IsolationPolicy {
    /// Stable wire name (matches the legacy `AppConfig.default_isolation`).
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Never => "never",
            Self::Modded => "modded",
            Self::Always => "always",
        }
    }

    /// Parse a legacy/directory value.
    pub fn parse(raw: &str) -> Option<Self> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "never" => Some(Self::Never),
            "modded" => Some(Self::Modded),
            "always" => Some(Self::Always),
            _ => None,
        }
    }

    /// Every variant, in picker order.
    pub const ALL: [Self; 3] = [Self::Never, Self::Modded, Self::Always];
}

impl From<IsolationPolicy> for rmccc_version::IsolationType {
    fn from(value: IsolationPolicy) -> Self {
        match value {
            IsolationPolicy::Never => Self::Never,
            IsolationPolicy::Modded => Self::Modded,
            IsolationPolicy::Always => Self::Always,
        }
    }
}

impl From<rmccc_version::IsolationType> for IsolationPolicy {
    fn from(value: rmccc_version::IsolationType) -> Self {
        match value {
            rmccc_version::IsolationType::Never => Self::Never,
            rmccc_version::IsolationType::Modded => Self::Modded,
            rmccc_version::IsolationType::Always => Self::Always,
        }
    }
}

/// OS process priority (HMCL `ProcessPriority`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export_to = "types.ts")]
pub enum ProcessPriority {
    Low,
    BelowNormal,
    #[default]
    Normal,
    AboveNormal,
    High,
}

impl ProcessPriority {
    /// Parse the legacy snake_case label.
    pub fn parse(raw: &str) -> Option<Self> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "low" => Some(Self::Low),
            "below_normal" => Some(Self::BelowNormal),
            "normal" => Some(Self::Normal),
            "above_normal" => Some(Self::AboveNormal),
            "high" => Some(Self::High),
            _ => None,
        }
    }

    /// Map to the platform contract.
    pub fn to_platform(self) -> rmccc_platform::ProcessPriority {
        match self {
            Self::Low => rmccc_platform::ProcessPriority::Low,
            Self::BelowNormal => rmccc_platform::ProcessPriority::BelowNormal,
            Self::Normal => rmccc_platform::ProcessPriority::Normal,
            Self::AboveNormal => rmccc_platform::ProcessPriority::AboveNormal,
            Self::High => rmccc_platform::ProcessPriority::High,
        }
    }
}

/// Instance icon source.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize, TS)]
#[serde(tag = "type", content = "value", rename_all = "snake_case")]
#[ts(export_to = "types.ts")]
pub enum InstanceIcon {
    /// Pick a built-in icon from the loader / game version.
    #[default]
    Auto,
    /// A named built-in icon (`grass`, `fabric`, `forge`, …).
    Builtin(String),
    /// A custom image file (usually `versions/{id}/icon.png`).
    File(PathBuf),
    /// A legacy `data:image/...;base64,...` value kept for migration.
    DataUrl(String),
}

/// Global game-settings preset (HMCL `GameSettings.Preset`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export_to = "types.ts")]
pub struct GlobalGameSettings {
    // Java
    #[serde(default)]
    pub java_type: JavaType,
    #[serde(default)]
    pub custom_java_path: String,
    #[serde(default)]
    pub custom_java_version: String,
    #[serde(default)]
    pub detected_java_version: String,
    #[serde(default)]
    pub detected_java_path_hash: String,

    // JVM / game arguments
    #[serde(default)]
    pub jvm_args: Vec<String>,
    #[serde(default)]
    pub game_args: Vec<String>,
    #[serde(default)]
    pub java_agents: Vec<String>,
    #[serde(default)]
    pub no_generated_jvm_args: bool,
    #[serde(default)]
    pub no_generated_optimizing_jvm_args: bool,

    // Memory
    #[serde(default)]
    pub auto_memory: bool,
    #[serde(default = "default_memory")]
    pub max_memory_mb: u32,
    #[serde(default)]
    pub min_memory_mb: Option<u32>,
    #[serde(default)]
    pub metaspace_mb: Option<u32>,

    // Window / process
    #[serde(default)]
    pub process_priority: ProcessPriority,
    #[serde(default)]
    pub window_type: WindowType,
    #[serde(default = "default_width")]
    pub width: u32,
    #[serde(default = "default_height")]
    pub height: u32,
    #[serde(default)]
    pub running_directory: Option<PathBuf>,
    #[serde(default)]
    pub launcher_visibility: LauncherVisibility,
    #[serde(default)]
    pub allow_auto_agent: bool,
    #[serde(default)]
    pub disable_auto_game_options: bool,

    // Graphics
    #[serde(default)]
    pub graphics_backend: GraphicsBackend,
    #[serde(default)]
    pub opengl_renderer: String,
    #[serde(default)]
    pub vulkan_renderer: String,

    // Environment / wrapper / hooks
    #[serde(default)]
    pub env_vars: BTreeMap<String, String>,
    #[serde(default)]
    pub wrapper: String,
    #[serde(default)]
    pub pre_launch_command: String,
    #[serde(default)]
    pub post_exit_command: String,

    // Quick play
    #[serde(default)]
    pub quick_play: QuickPlayType,
    #[serde(default)]
    pub quick_play_multiplayer: String,
    #[serde(default)]
    pub quick_play_singleplayer: String,
    #[serde(default)]
    pub quick_play_realms: String,

    // Logging / diagnostics
    #[serde(default)]
    pub show_logs: bool,
    #[serde(default)]
    pub enable_debug_log_output: bool,
    #[serde(default)]
    pub not_check_jvm: bool,
    #[serde(default)]
    pub not_check_game: bool,

    // Native libraries
    #[serde(default)]
    pub not_patch_natives: bool,
    #[serde(default)]
    pub use_custom_natives: bool,
    #[serde(default)]
    pub natives_directory: Option<PathBuf>,
    #[serde(default)]
    pub use_native_glfw: bool,
    #[serde(default)]
    pub use_native_openal: bool,

    // Default isolation for newly created instances.
    #[serde(default)]
    pub default_isolation: IsolationPolicy,
}

fn default_memory() -> u32 {
    4096
}
fn default_width() -> u32 {
    854
}
fn default_height() -> u32 {
    480
}

impl Default for GlobalGameSettings {
    fn default() -> Self {
        Self {
            java_type: JavaType::Auto,
            custom_java_path: String::new(),
            custom_java_version: String::new(),
            detected_java_version: String::new(),
            detected_java_path_hash: String::new(),
            jvm_args: Vec::new(),
            game_args: Vec::new(),
            java_agents: Vec::new(),
            no_generated_jvm_args: false,
            no_generated_optimizing_jvm_args: false,
            auto_memory: false,
            max_memory_mb: default_memory(),
            min_memory_mb: None,
            metaspace_mb: None,
            process_priority: ProcessPriority::Normal,
            window_type: WindowType::Windowed,
            width: default_width(),
            height: default_height(),
            running_directory: None,
            launcher_visibility: LauncherVisibility::Hide,
            allow_auto_agent: false,
            disable_auto_game_options: false,
            graphics_backend: GraphicsBackend::Default,
            opengl_renderer: String::new(),
            vulkan_renderer: String::new(),
            env_vars: BTreeMap::new(),
            wrapper: String::new(),
            pre_launch_command: String::new(),
            post_exit_command: String::new(),
            quick_play: QuickPlayType::None,
            quick_play_multiplayer: String::new(),
            quick_play_singleplayer: String::new(),
            quick_play_realms: String::new(),
            show_logs: false,
            enable_debug_log_output: false,
            not_check_jvm: false,
            not_check_game: false,
            not_patch_natives: false,
            use_custom_natives: false,
            natives_directory: None,
            use_native_glfw: false,
            use_native_openal: false,
            default_isolation: IsolationPolicy::Modded,
        }
    }
}

impl GlobalGameSettings {
    /// Seed the preset from the legacy `AppConfig` defaults on first run.
    pub fn from_app_config(config: &rmccc_config::AppConfig) -> Self {
        Self {
            java_type: if config.default_java_path.is_some() {
                JavaType::Custom
            } else {
                JavaType::Auto
            },
            custom_java_path: config.default_java_path.clone().unwrap_or_default(),
            jvm_args: config.default_jvm_args.clone(),
            game_args: config.default_game_args.clone(),
            max_memory_mb: config.default_max_memory_mb,
            min_memory_mb: (config.default_min_memory_mb > 0)
                .then_some(config.default_min_memory_mb),
            process_priority: ProcessPriority::parse(&config.default_process_priority)
                .unwrap_or_default(),
            window_type: if config.default_fullscreen {
                WindowType::Fullscreen
            } else {
                WindowType::Windowed
            },
            width: config.default_window_width.unwrap_or_else(default_width),
            height: config.default_window_height.unwrap_or_else(default_height),
            auto_memory: config.auto_memory,
            default_isolation: IsolationPolicy::parse(&config.default_isolation)
                .unwrap_or_default(),
            ..Self::default()
        }
    }
}

/// Field names accepted in `override_properties`.
pub mod fields {
    pub const JAVA_TYPE: &str = "java_type";
    pub const CUSTOM_JAVA_PATH: &str = "custom_java_path";
    pub const CUSTOM_JAVA_VERSION: &str = "custom_java_version";
    pub const DETECTED_JAVA_VERSION: &str = "detected_java_version";
    pub const DETECTED_JAVA_PATH_HASH: &str = "detected_java_path_hash";
    pub const JVM_ARGS: &str = "jvm_args";
    pub const GAME_ARGS: &str = "game_args";
    pub const JAVA_AGENTS: &str = "java_agents";
    pub const NO_GENERATED_JVM_ARGS: &str = "no_generated_jvm_args";
    pub const NO_GENERATED_OPTIMIZING_JVM_ARGS: &str = "no_generated_optimizing_jvm_args";
    pub const AUTO_MEMORY: &str = "auto_memory";
    pub const MAX_MEMORY_MB: &str = "max_memory_mb";
    pub const MIN_MEMORY_MB: &str = "min_memory_mb";
    pub const METASPACE_MB: &str = "metaspace_mb";
    pub const PROCESS_PRIORITY: &str = "process_priority";
    pub const WINDOW_TYPE: &str = "window_type";
    pub const WIDTH: &str = "width";
    pub const HEIGHT: &str = "height";
    pub const RUNNING_DIRECTORY: &str = "running_directory";
    pub const LAUNCHER_VISIBILITY: &str = "launcher_visibility";
    pub const ALLOW_AUTO_AGENT: &str = "allow_auto_agent";
    pub const DISABLE_AUTO_GAME_OPTIONS: &str = "disable_auto_game_options";
    pub const GRAPHICS_BACKEND: &str = "graphics_backend";
    pub const OPENGL_RENDERER: &str = "opengl_renderer";
    pub const VULKAN_RENDERER: &str = "vulkan_renderer";
    pub const ENV_VARS: &str = "env_vars";
    pub const WRAPPER: &str = "wrapper";
    pub const PRE_LAUNCH_COMMAND: &str = "pre_launch_command";
    pub const POST_EXIT_COMMAND: &str = "post_exit_command";
    pub const QUICK_PLAY: &str = "quick_play";
    pub const QUICK_PLAY_MULTIPLAYER: &str = "quick_play_multiplayer";
    pub const QUICK_PLAY_SINGLEPLAYER: &str = "quick_play_singleplayer";
    pub const QUICK_PLAY_REALMS: &str = "quick_play_realms";
    pub const SHOW_LOGS: &str = "show_logs";
    pub const ENABLE_DEBUG_LOG_OUTPUT: &str = "enable_debug_log_output";
    pub const NOT_CHECK_JVM: &str = "not_check_jvm";
    pub const NOT_CHECK_GAME: &str = "not_check_game";
    pub const NOT_PATCH_NATIVES: &str = "not_patch_natives";
    pub const USE_CUSTOM_NATIVES: &str = "use_custom_natives";
    pub const NATIVES_DIRECTORY: &str = "natives_directory";
    pub const USE_NATIVE_GLFW: &str = "use_native_glfw";
    pub const USE_NATIVE_OPENAL: &str = "use_native_openal";

    /// Every overridable field name, in UI order.
    pub const ALL: &[&str] = &[
        JAVA_TYPE,
        CUSTOM_JAVA_PATH,
        CUSTOM_JAVA_VERSION,
        DETECTED_JAVA_VERSION,
        DETECTED_JAVA_PATH_HASH,
        JVM_ARGS,
        GAME_ARGS,
        JAVA_AGENTS,
        NO_GENERATED_JVM_ARGS,
        NO_GENERATED_OPTIMIZING_JVM_ARGS,
        AUTO_MEMORY,
        MAX_MEMORY_MB,
        MIN_MEMORY_MB,
        METASPACE_MB,
        PROCESS_PRIORITY,
        WINDOW_TYPE,
        WIDTH,
        HEIGHT,
        RUNNING_DIRECTORY,
        LAUNCHER_VISIBILITY,
        ALLOW_AUTO_AGENT,
        DISABLE_AUTO_GAME_OPTIONS,
        GRAPHICS_BACKEND,
        OPENGL_RENDERER,
        VULKAN_RENDERER,
        ENV_VARS,
        WRAPPER,
        PRE_LAUNCH_COMMAND,
        POST_EXIT_COMMAND,
        QUICK_PLAY,
        QUICK_PLAY_MULTIPLAYER,
        QUICK_PLAY_SINGLEPLAYER,
        QUICK_PLAY_REALMS,
        SHOW_LOGS,
        ENABLE_DEBUG_LOG_OUTPUT,
        NOT_CHECK_JVM,
        NOT_CHECK_GAME,
        NOT_PATCH_NATIVES,
        USE_CUSTOM_NATIVES,
        NATIVES_DIRECTORY,
        USE_NATIVE_GLFW,
        USE_NATIVE_OPENAL,
    ];
}

/// Per-instance settings (HMCL `GameSettings.Instance`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export_to = "types.ts")]
pub struct InstanceSettings {
    /// Optional display name; defaults to the instance id.
    #[serde(default)]
    pub display_name: Option<String>,
    #[serde(default)]
    pub icon: InstanceIcon,
    /// `None` follows the global preset's `default_isolation`.
    #[serde(default)]
    pub isolation: Option<IsolationPolicy>,
    /// Reserved for multi-preset support; `Some("default")` or `None` today.
    #[serde(default)]
    pub parent: Option<String>,
    /// Fields that take this instance's value instead of the global preset.
    #[serde(default)]
    pub override_properties: BTreeSet<String>,

    // Java
    #[serde(default)]
    pub java_type: JavaType,
    #[serde(default)]
    pub custom_java_path: String,
    #[serde(default)]
    pub custom_java_version: String,
    #[serde(default)]
    pub detected_java_version: String,
    #[serde(default)]
    pub detected_java_path_hash: String,

    // JVM / game arguments
    #[serde(default)]
    pub jvm_args: Vec<String>,
    #[serde(default)]
    pub game_args: Vec<String>,
    #[serde(default)]
    pub java_agents: Vec<String>,
    #[serde(default)]
    pub no_generated_jvm_args: bool,
    #[serde(default)]
    pub no_generated_optimizing_jvm_args: bool,

    // Memory
    #[serde(default)]
    pub auto_memory: bool,
    #[serde(default = "default_memory")]
    pub max_memory_mb: u32,
    #[serde(default)]
    pub min_memory_mb: Option<u32>,
    #[serde(default)]
    pub metaspace_mb: Option<u32>,

    // Window / process
    #[serde(default)]
    pub process_priority: ProcessPriority,
    #[serde(default)]
    pub window_type: WindowType,
    #[serde(default = "default_width")]
    pub width: u32,
    #[serde(default = "default_height")]
    pub height: u32,
    #[serde(default)]
    pub running_directory: Option<PathBuf>,
    #[serde(default)]
    pub launcher_visibility: LauncherVisibility,
    #[serde(default)]
    pub allow_auto_agent: bool,
    #[serde(default)]
    pub disable_auto_game_options: bool,

    // Graphics
    #[serde(default)]
    pub graphics_backend: GraphicsBackend,
    #[serde(default)]
    pub opengl_renderer: String,
    #[serde(default)]
    pub vulkan_renderer: String,

    // Environment / wrapper / hooks
    #[serde(default)]
    pub env_vars: BTreeMap<String, String>,
    #[serde(default)]
    pub wrapper: String,
    #[serde(default)]
    pub pre_launch_command: String,
    #[serde(default)]
    pub post_exit_command: String,

    // Quick play
    #[serde(default)]
    pub quick_play: QuickPlayType,
    #[serde(default)]
    pub quick_play_multiplayer: String,
    #[serde(default)]
    pub quick_play_singleplayer: String,
    #[serde(default)]
    pub quick_play_realms: String,

    // Logging / diagnostics
    #[serde(default)]
    pub show_logs: bool,
    #[serde(default)]
    pub enable_debug_log_output: bool,
    #[serde(default)]
    pub not_check_jvm: bool,
    #[serde(default)]
    pub not_check_game: bool,

    // Native libraries
    #[serde(default)]
    pub not_patch_natives: bool,
    #[serde(default)]
    pub use_custom_natives: bool,
    #[serde(default)]
    pub natives_directory: Option<PathBuf>,
    #[serde(default)]
    pub use_native_glfw: bool,
    #[serde(default)]
    pub use_native_openal: bool,

    // Metadata (not overridable)
    #[serde(default)]
    pub created_at: Option<chrono::DateTime<chrono::Utc>>,
    #[serde(default)]
    pub last_played: Option<chrono::DateTime<chrono::Utc>>,
    #[serde(default)]
    pub play_time_seconds: u64,
    #[serde(default)]
    pub favorite: bool,
    /// Hidden from the instance list without deleting the version files
    /// (HMCL `hidden` manifest flag analogue).
    #[serde(default)]
    pub hidden: bool,
}

impl Default for InstanceSettings {
    fn default() -> Self {
        Self {
            display_name: None,
            icon: InstanceIcon::Auto,
            isolation: None,
            parent: None,
            override_properties: BTreeSet::new(),
            java_type: JavaType::Auto,
            custom_java_path: String::new(),
            custom_java_version: String::new(),
            detected_java_version: String::new(),
            detected_java_path_hash: String::new(),
            jvm_args: Vec::new(),
            game_args: Vec::new(),
            java_agents: Vec::new(),
            no_generated_jvm_args: false,
            no_generated_optimizing_jvm_args: false,
            auto_memory: false,
            max_memory_mb: default_memory(),
            min_memory_mb: None,
            metaspace_mb: None,
            process_priority: ProcessPriority::Normal,
            window_type: WindowType::Windowed,
            width: default_width(),
            height: default_height(),
            running_directory: None,
            launcher_visibility: LauncherVisibility::Hide,
            allow_auto_agent: false,
            disable_auto_game_options: false,
            graphics_backend: GraphicsBackend::Default,
            opengl_renderer: String::new(),
            vulkan_renderer: String::new(),
            env_vars: BTreeMap::new(),
            wrapper: String::new(),
            pre_launch_command: String::new(),
            post_exit_command: String::new(),
            quick_play: QuickPlayType::None,
            quick_play_multiplayer: String::new(),
            quick_play_singleplayer: String::new(),
            quick_play_realms: String::new(),
            show_logs: false,
            enable_debug_log_output: false,
            not_check_jvm: false,
            not_check_game: false,
            not_patch_natives: false,
            use_custom_natives: false,
            natives_directory: None,
            use_native_glfw: false,
            use_native_openal: false,
            created_at: None,
            last_played: None,
            play_time_seconds: 0,
            favorite: false,
            hidden: false,
        }
    }
}

/// Settings after merging the instance overlay with the global preset.
///
/// This is the **only** object launch/UI should consume; it contains concrete
/// values for every option.
#[derive(Debug, Clone, PartialEq, Default, Serialize, TS)]
#[ts(export_to = "types.ts")]
pub struct EffectiveGameSettings {
    pub java_type: JavaType,
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
    pub window_type: WindowType,
    pub width: u32,
    pub height: u32,
    pub running_directory: Option<PathBuf>,
    pub launcher_visibility: LauncherVisibility,
    pub allow_auto_agent: bool,
    pub disable_auto_game_options: bool,
    pub graphics_backend: GraphicsBackend,
    pub opengl_renderer: String,
    pub vulkan_renderer: String,
    pub env_vars: BTreeMap<String, String>,
    pub wrapper: String,
    pub pre_launch_command: String,
    pub post_exit_command: String,
    pub quick_play: QuickPlayType,
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
    pub isolation: IsolationPolicy,
}

impl InstanceSettings {
    /// Whether `field` currently overrides the global preset.
    pub fn is_overridden(&self, field: &str) -> bool {
        self.override_properties.contains(field)
    }

    /// Mark a field overridden (or clear the flag, keeping the stored value).
    pub fn set_overridden(&mut self, field: &str, overridden: bool) {
        if overridden {
            self.override_properties.insert(field.to_string());
        } else {
            self.override_properties.remove(field);
        }
    }

    /// Display name, falling back to the instance id.
    pub fn display_name_or<'a>(&'a self, id: &'a str) -> &'a str {
        self.display_name
            .as_deref()
            .map(str::trim)
            .filter(|name| !name.is_empty())
            .unwrap_or(id)
    }

    /// Resolve every field against the global preset.
    pub fn resolve(&self, global: &GlobalGameSettings) -> EffectiveGameSettings {
        use fields as f;
        let over = |field: &str| self.override_properties.contains(field);

        macro_rules! pick {
            ($field:expr, $instance:expr, $global:expr) => {
                if over($field) {
                    $instance
                } else {
                    $global
                }
            };
        }

        EffectiveGameSettings {
            java_type: pick!(f::JAVA_TYPE, self.java_type, global.java_type),
            custom_java_path: pick!(
                f::CUSTOM_JAVA_PATH,
                self.custom_java_path.clone(),
                global.custom_java_path.clone()
            ),
            custom_java_version: pick!(
                f::CUSTOM_JAVA_VERSION,
                self.custom_java_version.clone(),
                global.custom_java_version.clone()
            ),
            detected_java_version: pick!(
                f::DETECTED_JAVA_VERSION,
                self.detected_java_version.clone(),
                global.detected_java_version.clone()
            ),
            detected_java_path_hash: pick!(
                f::DETECTED_JAVA_PATH_HASH,
                self.detected_java_path_hash.clone(),
                global.detected_java_path_hash.clone()
            ),
            jvm_args: pick!(f::JVM_ARGS, self.jvm_args.clone(), global.jvm_args.clone()),
            game_args: pick!(
                f::GAME_ARGS,
                self.game_args.clone(),
                global.game_args.clone()
            ),
            java_agents: pick!(
                f::JAVA_AGENTS,
                self.java_agents.clone(),
                global.java_agents.clone()
            ),
            no_generated_jvm_args: pick!(
                f::NO_GENERATED_JVM_ARGS,
                self.no_generated_jvm_args,
                global.no_generated_jvm_args
            ),
            no_generated_optimizing_jvm_args: pick!(
                f::NO_GENERATED_OPTIMIZING_JVM_ARGS,
                self.no_generated_optimizing_jvm_args,
                global.no_generated_optimizing_jvm_args
            ),
            auto_memory: pick!(f::AUTO_MEMORY, self.auto_memory, global.auto_memory),
            max_memory_mb: pick!(f::MAX_MEMORY_MB, self.max_memory_mb, global.max_memory_mb),
            min_memory_mb: pick!(f::MIN_MEMORY_MB, self.min_memory_mb, global.min_memory_mb),
            metaspace_mb: pick!(f::METASPACE_MB, self.metaspace_mb, global.metaspace_mb),
            process_priority: pick!(
                f::PROCESS_PRIORITY,
                self.process_priority,
                global.process_priority
            ),
            window_type: pick!(f::WINDOW_TYPE, self.window_type, global.window_type),
            width: pick!(f::WIDTH, self.width, global.width),
            height: pick!(f::HEIGHT, self.height, global.height),
            running_directory: pick!(
                f::RUNNING_DIRECTORY,
                self.running_directory.clone(),
                global.running_directory.clone()
            ),
            launcher_visibility: pick!(
                f::LAUNCHER_VISIBILITY,
                self.launcher_visibility,
                global.launcher_visibility
            ),
            allow_auto_agent: pick!(
                f::ALLOW_AUTO_AGENT,
                self.allow_auto_agent,
                global.allow_auto_agent
            ),
            disable_auto_game_options: pick!(
                f::DISABLE_AUTO_GAME_OPTIONS,
                self.disable_auto_game_options,
                global.disable_auto_game_options
            ),
            graphics_backend: pick!(
                f::GRAPHICS_BACKEND,
                self.graphics_backend,
                global.graphics_backend
            ),
            opengl_renderer: pick!(
                f::OPENGL_RENDERER,
                self.opengl_renderer.clone(),
                global.opengl_renderer.clone()
            ),
            vulkan_renderer: pick!(
                f::VULKAN_RENDERER,
                self.vulkan_renderer.clone(),
                global.vulkan_renderer.clone()
            ),
            env_vars: pick!(f::ENV_VARS, self.env_vars.clone(), global.env_vars.clone()),
            wrapper: pick!(f::WRAPPER, self.wrapper.clone(), global.wrapper.clone()),
            pre_launch_command: pick!(
                f::PRE_LAUNCH_COMMAND,
                self.pre_launch_command.clone(),
                global.pre_launch_command.clone()
            ),
            post_exit_command: pick!(
                f::POST_EXIT_COMMAND,
                self.post_exit_command.clone(),
                global.post_exit_command.clone()
            ),
            quick_play: pick!(f::QUICK_PLAY, self.quick_play, global.quick_play),
            quick_play_multiplayer: pick!(
                f::QUICK_PLAY_MULTIPLAYER,
                self.quick_play_multiplayer.clone(),
                global.quick_play_multiplayer.clone()
            ),
            quick_play_singleplayer: pick!(
                f::QUICK_PLAY_SINGLEPLAYER,
                self.quick_play_singleplayer.clone(),
                global.quick_play_singleplayer.clone()
            ),
            quick_play_realms: pick!(
                f::QUICK_PLAY_REALMS,
                self.quick_play_realms.clone(),
                global.quick_play_realms.clone()
            ),
            show_logs: pick!(f::SHOW_LOGS, self.show_logs, global.show_logs),
            enable_debug_log_output: pick!(
                f::ENABLE_DEBUG_LOG_OUTPUT,
                self.enable_debug_log_output,
                global.enable_debug_log_output
            ),
            not_check_jvm: pick!(f::NOT_CHECK_JVM, self.not_check_jvm, global.not_check_jvm),
            not_check_game: pick!(
                f::NOT_CHECK_GAME,
                self.not_check_game,
                global.not_check_game
            ),
            not_patch_natives: pick!(
                f::NOT_PATCH_NATIVES,
                self.not_patch_natives,
                global.not_patch_natives
            ),
            use_custom_natives: pick!(
                f::USE_CUSTOM_NATIVES,
                self.use_custom_natives,
                global.use_custom_natives
            ),
            natives_directory: pick!(
                f::NATIVES_DIRECTORY,
                self.natives_directory.clone(),
                global.natives_directory.clone()
            ),
            use_native_glfw: pick!(
                f::USE_NATIVE_GLFW,
                self.use_native_glfw,
                global.use_native_glfw
            ),
            use_native_openal: pick!(
                f::USE_NATIVE_OPENAL,
                self.use_native_openal,
                global.use_native_openal
            ),
            isolation: self.isolation.unwrap_or(global.default_isolation),
        }
    }
}

/// A partial update for [`InstanceSettings`].
///
/// `None` means "leave unchanged"; empty strings / zero clear nullable values
/// (the legacy IPC convention, kept so the frontend does not need
/// `Option<Option<T>>` everywhere).
#[derive(Debug, Clone, Default, Deserialize, TS)]
#[ts(export_to = "types.ts")]
pub struct InstanceSettingsPatch {
    pub display_name: Option<String>,
    pub icon: Option<InstanceIcon>,
    /// Empty string clears the per-instance isolation (follow global).
    pub isolation: Option<String>,
    pub parent: Option<String>,
    pub override_properties: Option<Vec<String>>,

    pub java_type: Option<JavaType>,
    pub custom_java_path: Option<String>,
    pub custom_java_version: Option<String>,
    pub detected_java_version: Option<String>,
    pub detected_java_path_hash: Option<String>,

    pub jvm_args: Option<Vec<String>>,
    pub game_args: Option<Vec<String>>,
    pub java_agents: Option<Vec<String>>,
    pub no_generated_jvm_args: Option<bool>,
    pub no_generated_optimizing_jvm_args: Option<bool>,

    pub auto_memory: Option<bool>,
    pub max_memory_mb: Option<u32>,
    pub min_memory_mb: Option<u32>,
    pub metaspace_mb: Option<u32>,

    pub process_priority: Option<ProcessPriority>,
    pub window_type: Option<WindowType>,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub running_directory: Option<String>,
    pub launcher_visibility: Option<LauncherVisibility>,
    pub allow_auto_agent: Option<bool>,
    pub disable_auto_game_options: Option<bool>,

    pub graphics_backend: Option<GraphicsBackend>,
    pub opengl_renderer: Option<String>,
    pub vulkan_renderer: Option<String>,

    pub env_vars: Option<BTreeMap<String, String>>,
    pub wrapper: Option<String>,
    pub pre_launch_command: Option<String>,
    pub post_exit_command: Option<String>,

    pub quick_play: Option<QuickPlayType>,
    pub quick_play_multiplayer: Option<String>,
    pub quick_play_singleplayer: Option<String>,
    pub quick_play_realms: Option<String>,

    pub show_logs: Option<bool>,
    pub enable_debug_log_output: Option<bool>,
    pub not_check_jvm: Option<bool>,
    pub not_check_game: Option<bool>,

    pub not_patch_natives: Option<bool>,
    pub use_custom_natives: Option<bool>,
    pub natives_directory: Option<String>,
    pub use_native_glfw: Option<bool>,
    pub use_native_openal: Option<bool>,
    pub favorite: Option<bool>,
    pub hidden: Option<bool>,
}

impl InstanceSettings {
    /// Apply a patch, validating ranges and override names.
    pub fn apply(&mut self, patch: InstanceSettingsPatch) -> rmccc_error::Result<()> {
        use fields as f;

        if let Some(name) = patch.display_name {
            self.display_name = Some(name).filter(|n| !n.trim().is_empty());
        }
        if let Some(icon) = patch.icon {
            self.icon = icon;
        }
        if let Some(isolation) = patch.isolation {
            self.isolation = if isolation.trim().is_empty() {
                None
            } else {
                Some(IsolationPolicy::parse(&isolation).ok_or_else(|| {
                    rmccc_error::RmcccError::Validation(format!(
                        "invalid isolation policy: {isolation}"
                    ))
                })?)
            };
        }
        if let Some(parent) = patch.parent {
            self.parent = Some(parent).filter(|p| !p.trim().is_empty());
        }
        if let Some(overrides) = patch.override_properties {
            self.override_properties = overrides
                .into_iter()
                .filter(|name| f::ALL.contains(&name.as_str()))
                .collect();
        }

        if let Some(v) = patch.java_type {
            self.java_type = v;
        }
        if let Some(v) = patch.custom_java_path {
            self.custom_java_path = v;
        }
        if let Some(v) = patch.custom_java_version {
            self.custom_java_version = v;
        }
        if let Some(v) = patch.detected_java_version {
            self.detected_java_version = v;
        }
        if let Some(v) = patch.detected_java_path_hash {
            self.detected_java_path_hash = v;
        }

        if let Some(v) = patch.jvm_args {
            self.jvm_args = v;
        }
        if let Some(v) = patch.game_args {
            self.game_args = v;
        }
        if let Some(v) = patch.java_agents {
            self.java_agents = v;
        }
        if let Some(v) = patch.no_generated_jvm_args {
            self.no_generated_jvm_args = v;
        }
        if let Some(v) = patch.no_generated_optimizing_jvm_args {
            self.no_generated_optimizing_jvm_args = v;
        }

        if let Some(v) = patch.auto_memory {
            self.auto_memory = v;
        }
        if let Some(v) = patch.max_memory_mb {
            self.max_memory_mb = v;
        }
        if let Some(v) = patch.min_memory_mb {
            self.min_memory_mb = (v > 0).then_some(v);
        }
        if let Some(v) = patch.metaspace_mb {
            self.metaspace_mb = (v > 0).then_some(v);
        }

        if let Some(v) = patch.process_priority {
            self.process_priority = v;
        }
        if let Some(v) = patch.window_type {
            self.window_type = v;
        }
        if let Some(v) = patch.width {
            self.width = if v == 0 { default_width() } else { v };
        }
        if let Some(v) = patch.height {
            self.height = if v == 0 { default_height() } else { v };
        }
        if let Some(v) = patch.running_directory {
            self.running_directory = (!v.trim().is_empty()).then(|| PathBuf::from(v));
        }
        if let Some(v) = patch.launcher_visibility {
            self.launcher_visibility = v;
        }
        if let Some(v) = patch.allow_auto_agent {
            self.allow_auto_agent = v;
        }
        if let Some(v) = patch.disable_auto_game_options {
            self.disable_auto_game_options = v;
        }

        if let Some(v) = patch.graphics_backend {
            self.graphics_backend = v;
        }
        if let Some(v) = patch.opengl_renderer {
            self.opengl_renderer = v;
        }
        if let Some(v) = patch.vulkan_renderer {
            self.vulkan_renderer = v;
        }

        if let Some(v) = patch.env_vars {
            self.env_vars = v;
        }
        if let Some(v) = patch.wrapper {
            self.wrapper = v;
        }
        if let Some(v) = patch.pre_launch_command {
            self.pre_launch_command = v;
        }
        if let Some(v) = patch.post_exit_command {
            self.post_exit_command = v;
        }

        if let Some(v) = patch.quick_play {
            self.quick_play = v;
        }
        if let Some(v) = patch.quick_play_multiplayer {
            self.quick_play_multiplayer = v;
        }
        if let Some(v) = patch.quick_play_singleplayer {
            self.quick_play_singleplayer = v;
        }
        if let Some(v) = patch.quick_play_realms {
            self.quick_play_realms = v;
        }

        if let Some(v) = patch.show_logs {
            self.show_logs = v;
        }
        if let Some(v) = patch.enable_debug_log_output {
            self.enable_debug_log_output = v;
        }
        if let Some(v) = patch.not_check_jvm {
            self.not_check_jvm = v;
        }
        if let Some(v) = patch.not_check_game {
            self.not_check_game = v;
        }

        if let Some(v) = patch.not_patch_natives {
            self.not_patch_natives = v;
        }
        if let Some(v) = patch.use_custom_natives {
            self.use_custom_natives = v;
        }
        if let Some(v) = patch.natives_directory {
            self.natives_directory = (!v.trim().is_empty()).then(|| PathBuf::from(v));
        }
        if let Some(v) = patch.use_native_glfw {
            self.use_native_glfw = v;
        }
        if let Some(v) = patch.use_native_openal {
            self.use_native_openal = v;
        }
        if let Some(v) = patch.favorite {
            self.favorite = v;
        }
        if let Some(v) = patch.hidden {
            self.hidden = v;
        }

        self.validate()
    }

    /// Validate ranges that would produce a broken launch command.
    pub fn validate(&self) -> rmccc_error::Result<()> {
        if self.width < 320 || self.width > 16384 {
            return Err(rmccc_error::RmcccError::Validation(format!(
                "window width out of range: {}",
                self.width
            )));
        }
        if self.height < 240 || self.height > 16384 {
            return Err(rmccc_error::RmcccError::Validation(format!(
                "window height out of range: {}",
                self.height
            )));
        }
        if self.max_memory_mb > 0 && self.max_memory_mb < 256 {
            return Err(rmccc_error::RmcccError::Validation(
                "max memory must be at least 256 MiB (or 0 for auto)".to_string(),
            ));
        }
        if let Some(min) = self.min_memory_mb {
            if min < 128 {
                return Err(rmccc_error::RmcccError::Validation(
                    "min memory must be at least 128 MiB".to_string(),
                ));
            }
            if self.max_memory_mb > 0 && min > self.max_memory_mb {
                return Err(rmccc_error::RmcccError::Validation(
                    "min memory exceeds max memory".to_string(),
                ));
            }
        }
        Ok(())
    }
}

/// Current schema for per-instance settings.
pub fn instance_settings_schema() -> rmccc_config::schema::ConfigSchema {
    rmccc_config::schema::ConfigSchema::new(
        "instance-settings",
        rmccc_config::schema::SchemaVersion::new(1, 0, 0),
    )
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct InstanceSettingsFile {
    #[serde(rename = "$schema", default = "instance_settings_schema")]
    schema: rmccc_config::schema::ConfigSchema,
    #[serde(flatten)]
    settings: InstanceSettings,
}

/// Result of loading an instance's settings file.
#[derive(Debug, Clone)]
pub struct LoadedInstanceSettings {
    pub settings: InstanceSettings,
    /// Whether a settings file existed (vs. defaults generated in memory).
    pub exists: bool,
    /// Parse/IO warning; the instance still loads with defaults.
    pub warning: Option<String>,
}

impl InstanceSettings {
    /// `{instance_root}/.rmccc/instance.json`.
    pub fn file_path(instance_root: &Path) -> PathBuf {
        instance_root.join(".rmccc").join("instance.json")
    }

    /// Load settings from the instance root. Missing/corrupt files yield
    /// defaults plus a warning; they never make the instance disappear.
    pub fn load(instance_root: &Path) -> LoadedInstanceSettings {
        let path = Self::file_path(instance_root);
        if !path.exists() {
            return LoadedInstanceSettings {
                settings: Self::default(),
                exists: false,
                warning: None,
            };
        }
        match std::fs::read_to_string(&path) {
            Ok(text) => match serde_json::from_str::<InstanceSettingsFile>(&text) {
                Ok(file) => LoadedInstanceSettings {
                    settings: file.settings,
                    exists: true,
                    warning: None,
                },
                Err(e) => {
                    tracing::warn!(
                        path = %path.display(),
                        error = %e,
                        "Instance settings are malformed; using defaults"
                    );
                    LoadedInstanceSettings {
                        settings: Self::default(),
                        exists: true,
                        warning: Some(format!("instance settings are malformed: {e}")),
                    }
                }
            },
            Err(e) => LoadedInstanceSettings {
                settings: Self::default(),
                exists: false,
                warning: Some(format!("cannot read instance settings: {e}")),
            },
        }
    }

    /// Persist settings atomically into the instance directory.
    pub fn save(&self, instance_root: &Path, fs: &dyn PlatformFs) -> rmccc_error::Result<()> {
        self.validate()?;
        let path = Self::file_path(instance_root);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let file = InstanceSettingsFile {
            schema: instance_settings_schema(),
            settings: self.clone(),
        };
        let json = serde_json::to_string_pretty(&file)?;
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, json)?;
        fs.atomic_replace(&tmp, &path)?;
        Ok(())
    }
}

fn backup_invalid(path: &Path) {
    let backup = path.with_extension("json.invalid");
    let _ = std::fs::rename(path, &backup);
}

/// Remove a malformed settings file before re-writing it (diagnostics).
pub fn backup_instance_settings(instance_root: &Path) {
    let path = InstanceSettings::file_path(instance_root);
    if path.exists() {
        backup_invalid(&path);
    }
}

use rmccc_platform::PlatformFs;

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_global() -> GlobalGameSettings {
        GlobalGameSettings {
            jvm_args: vec!["-Dglobal=1".into()],
            max_memory_mb: 2048,
            min_memory_mb: Some(512),
            width: 1280,
            process_priority: ProcessPriority::High,
            default_isolation: IsolationPolicy::Always,
            ..Default::default()
        }
    }

    #[test]
    fn inherits_unless_overridden_and_keeps_values() {
        let global = sample_global();
        let mut settings = InstanceSettings {
            max_memory_mb: 8192,
            jvm_args: vec!["-Xss4m".into()],
            process_priority: ProcessPriority::Low,
            ..Default::default()
        };

        let inherited = settings.resolve(&global);
        assert_eq!(inherited.max_memory_mb, 2048);
        assert_eq!(inherited.jvm_args, vec!["-Dglobal=1"]);
        assert_eq!(inherited.process_priority, ProcessPriority::High);
        assert_eq!(inherited.isolation, IsolationPolicy::Always);

        settings.set_overridden(fields::MAX_MEMORY_MB, true);
        settings.set_overridden(fields::JVM_ARGS, true);
        let overridden = settings.resolve(&global);
        assert_eq!(overridden.max_memory_mb, 8192);
        assert_eq!(overridden.jvm_args, vec!["-Xss4m"]);
        assert_eq!(overridden.process_priority, ProcessPriority::High);

        settings.set_overridden(fields::MAX_MEMORY_MB, false);
        assert_eq!(settings.max_memory_mb, 8192);
        assert_eq!(settings.resolve(&global).max_memory_mb, 2048);
    }

    #[test]
    fn patch_validates_and_clears_nullable_fields() {
        let mut settings = InstanceSettings::default();
        settings
            .apply(InstanceSettingsPatch {
                width: Some(1920),
                height: Some(1080),
                min_memory_mb: Some(512),
                max_memory_mb: Some(1024),
                ..Default::default()
            })
            .unwrap();
        assert_eq!(settings.width, 1920);
        assert_eq!(settings.min_memory_mb, Some(512));

        settings
            .apply(InstanceSettingsPatch {
                min_memory_mb: Some(0),
                ..Default::default()
            })
            .unwrap();
        assert_eq!(settings.min_memory_mb, None);

        assert!(settings
            .apply(InstanceSettingsPatch {
                width: Some(100),
                ..Default::default()
            })
            .is_err());
        assert!(settings
            .apply(InstanceSettingsPatch {
                max_memory_mb: Some(512),
                min_memory_mb: Some(1024),
                ..Default::default()
            })
            .is_err());
    }

    #[test]
    fn overrides_only_accept_known_names() {
        let mut settings = InstanceSettings::default();
        settings
            .apply(InstanceSettingsPatch {
                override_properties: Some(vec![
                    fields::JVM_ARGS.to_string(),
                    "not_a_field".to_string(),
                ]),
                ..Default::default()
            })
            .unwrap();
        assert!(settings.is_overridden(fields::JVM_ARGS));
        assert_eq!(settings.override_properties.len(), 1);
    }

    #[test]
    fn settings_round_trip_through_instance_file() {
        let dir = std::env::temp_dir().join(format!(
            "rmccc-instance-settings-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let platform = rmccc_platform::Platform::current();
        let mut settings = InstanceSettings {
            display_name: Some("Survival".into()),
            ..Default::default()
        };
        settings.set_overridden(fields::MAX_MEMORY_MB, true);
        settings.save(&dir, platform.fs.as_ref()).unwrap();

        let loaded = InstanceSettings::load(&dir);
        assert!(loaded.exists);
        assert_eq!(loaded.settings.display_name.as_deref(), Some("Survival"));
        assert!(loaded.settings.is_overridden(fields::MAX_MEMORY_MB));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
