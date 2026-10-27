//! Version-backed game instances, game-directory registry, and launch settings.
//!
//! This crate is the RMCCC counterpart of HMCL's `GameRepository` +
//! `GameSettings` + `GameDirectories` layer (ADR-016):
//!
//! - [`GameDirectoryStore`] registers `.minecraft`-equivalent roots and the
//!   selected directory/instance;
//! - an **instance is a version directory** (`versions/{id}/{id}.json`) — there
//!   is no separate profile UUID;
//! - [`InstanceSettings`] is a per-instance overlay stored at
//!   `{instance_root}/.rmccc/instance.json`, merged with the global
//!   [`GlobalGameSettings`] preset through `override_properties`;
//! - [`InstanceService`] is the single read/write facade used by IPC, CLI,
//!   launch, and mod management.

pub mod compat;
pub mod directory;
pub mod global;
pub mod id;
pub mod migrate;
pub mod service;
pub mod settings;
pub mod summary;

pub use compat::{
    count_instances, detect_layout, scan_candidates, scan_candidates_in, DirectoryCandidate,
    DirectoryLayout, DirectorySource, GameDirectoryOverview,
};
pub use directory::{directories_schema, GameDirectory, GameDirectoryKind, GameDirectoryStore};
pub use global::{game_settings_schema, GlobalGameSettingsStore, DEFAULT_PRESET_ID};
pub use id::{validate_instance_id, GameDirectoryId, InstanceId, InstanceKey, MAX_INSTANCE_ID_LEN};
pub use migrate::{migrate_legacy_profiles, LegacyProfile, MigrationReport, MIGRATION_MARKER};
pub use service::InstanceService;
pub use settings::{
    fields, EffectiveGameSettings, GlobalGameSettings, GraphicsBackend, InstanceIcon,
    InstanceSettings, InstanceSettingsPatch, IsolationPolicy, JavaType, LauncherVisibility,
    ProcessPriority, QuickPlayType, WindowType,
};
pub use summary::{
    InstanceDetail, InstanceManifestInfo, InstancePaths, InstanceSummary, LoaderInfo,
    ResolvedInstanceSettings, ResolvedJavaSettings,
};
