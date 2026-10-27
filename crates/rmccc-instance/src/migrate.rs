//! One-shot migration from the legacy `profiles/{uuid}.json` model.
//!
//! The old profile UUID is deliberately discarded: the version id becomes the
//! instance id (ADR-016). Legacy files are **never modified or deleted**, so a
//! user can roll back by removing the migration marker and the new stores.

use crate::directory::{GameDirectoryKind, GameDirectoryStore};
use crate::id::{GameDirectoryId, InstanceId, InstanceKey};
use crate::settings::{
    fields, InstanceIcon, InstanceSettings, IsolationPolicy, JavaType, ProcessPriority, WindowType,
};
use rmccc_error::Result;
use rmccc_platform::PlatformFs;
use serde::Deserialize;
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// Marker written after a successful migration pass.
pub const MIGRATION_MARKER: &str = "instance-migration-v2.json";

/// Legacy profile shape (all fields optional/defaulted for forward tolerance).
#[derive(Debug, Clone, Deserialize, Default)]
pub struct LegacyProfile {
    pub id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub game_version: String,
    #[serde(default)]
    pub isolation: Option<String>,
    #[serde(default)]
    pub override_properties: BTreeSet<String>,
    #[serde(default)]
    pub java_path: Option<String>,
    #[serde(default)]
    pub jvm_args: Vec<String>,
    #[serde(default)]
    pub game_args: Vec<String>,
    #[serde(default)]
    pub memory_mb: u32,
    #[serde(default)]
    pub game_dir: PathBuf,
    #[serde(default)]
    pub created_at: Option<chrono::DateTime<chrono::Utc>>,
    #[serde(default)]
    pub last_played: Option<chrono::DateTime<chrono::Utc>>,
    #[serde(default)]
    pub play_time_seconds: u64,
    #[serde(default)]
    pub external_root: bool,
    #[serde(default)]
    pub icon: Option<String>,
    #[serde(default)]
    pub min_memory_mb: u32,
    #[serde(default)]
    pub metaspace_mb: u32,
    #[serde(default)]
    pub process_priority: String,
    #[serde(default)]
    pub width: Option<u32>,
    #[serde(default)]
    pub height: Option<u32>,
    #[serde(default)]
    pub fullscreen: bool,
    #[serde(default)]
    pub use_native_glfw: bool,
    #[serde(default)]
    pub use_native_openal: bool,
    #[serde(default)]
    pub use_custom_natives: bool,
    #[serde(default)]
    pub no_generated_jvm_args: bool,
    #[serde(default)]
    pub no_generated_optimizing_jvm_args: bool,
    #[serde(default)]
    pub enable_debug_log_output: bool,
    #[serde(default)]
    pub wrapper: Option<String>,
    #[serde(default)]
    pub java_agents: Vec<String>,
    #[serde(default)]
    pub override_jvm_args: Vec<String>,
    #[serde(default)]
    pub pre_launch_command: Option<String>,
    #[serde(default)]
    pub post_exit_command: Option<String>,
    #[serde(default)]
    pub env_vars: BTreeMap<String, String>,
}

/// Summary of one migration pass.
#[derive(Debug, Default, Clone, serde::Serialize, serde::Deserialize)]
pub struct MigrationReport {
    pub migrated: usize,
    pub skipped: usize,
    pub diagnostics: Vec<String>,
}

/// Run the one-shot migration if it has not already completed.
///
/// `selected_legacy_profile` is the old `LauncherState.selected_profile_id`;
/// when its instance is migrated it becomes the selected instance key.
pub fn migrate_legacy_profiles(
    data_dir: &Path,
    directories: &mut GameDirectoryStore,
    selected_legacy_profile: Option<&str>,
    fs: Arc<dyn PlatformFs>,
) -> Result<MigrationReport> {
    let marker = rmccc_config::paths::state_dir(data_dir).join(MIGRATION_MARKER);
    if marker.exists() {
        return Ok(MigrationReport::default());
    }

    let profiles_dir = data_dir.join("profiles");
    let mut report = MigrationReport::default();
    let mut selected_key: Option<String> = None;

    if profiles_dir.is_dir() {
        let mut entries: Vec<_> = std::fs::read_dir(&profiles_dir)?
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|e| e == "json"))
            .collect();
        entries.sort();

        let default_dir = directories
            .selected()
            .cloned()
            .or_else(|| directories.list().first().cloned());

        for path in entries {
            let profile = match std::fs::read_to_string(&path)
                .ok()
                .and_then(|text| serde_json::from_str::<LegacyProfile>(&text).ok())
            {
                Some(profile) => profile,
                None => {
                    report.skipped += 1;
                    report
                        .diagnostics
                        .push(format!("skipped unreadable profile: {}", path.display()));
                    continue;
                }
            };

            let Some(directory_id) = ensure_directory(
                directories,
                &profile,
                default_dir.as_ref().map(|d| d.id.clone()),
            ) else {
                report.skipped += 1;
                report.diagnostics.push(format!(
                    "skipped profile {}: game directory missing",
                    profile.id
                ));
                continue;
            };

            let Some(directory) = directories.get(&directory_id) else {
                report.skipped += 1;
                continue;
            };

            let Ok(instance_id) = InstanceId::parse(&profile.game_version) else {
                report.skipped += 1;
                report.diagnostics.push(format!(
                    "skipped profile {}: unsafe version id {:?}",
                    profile.id, profile.game_version
                ));
                continue;
            };

            let instance_root = directory.path.join("versions").join(instance_id.as_str());
            let manifest = instance_root.join(format!("{instance_id}.json"));
            if !manifest.exists() {
                report.skipped += 1;
                report.diagnostics.push(format!(
                    "skipped profile {}: version '{}' is not installed",
                    profile.id, instance_id
                ));
                continue;
            }

            let settings_path = InstanceSettings::file_path(&instance_root);
            if settings_path.exists() {
                report.skipped += 1;
                continue;
            }

            let settings = settings_from_legacy(&profile, &instance_id);
            if let Err(e) = settings.save(&instance_root, fs.as_ref()) {
                report.skipped += 1;
                report
                    .diagnostics
                    .push(format!("failed to write settings for {}: {e}", instance_id));
                continue;
            }

            if selected_legacy_profile == Some(profile.id.as_str()) {
                selected_key =
                    Some(InstanceKey::new(directory_id.clone(), instance_id.clone()).as_key());
            }

            report.migrated += 1;
            tracing::info!(
                profile_id = %profile.id,
                instance = %instance_id,
                directory = %directory_id,
                "Migrated legacy profile to instance settings"
            );
        }
    }

    if selected_key.is_some() {
        directories.select_instance(selected_key)?;
    }

    if let Some(parent) = marker.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let json = serde_json::to_string_pretty(&report)?;
    std::fs::write(&marker, json)?;
    tracing::info!(
        migrated = report.migrated,
        skipped = report.skipped,
        "Legacy profile migration finished"
    );
    Ok(report)
}

fn ensure_directory(
    directories: &mut GameDirectoryStore,
    profile: &LegacyProfile,
    default_id: Option<GameDirectoryId>,
) -> Option<GameDirectoryId> {
    let root = if profile.external_root && !profile.game_dir.as_os_str().is_empty() {
        profile.game_dir.clone()
    } else if profile.game_dir.is_dir()
        && profile.game_dir.join("versions").is_dir()
        && !profile.external_root
    {
        // Pre-shared-root layouts or a user-chosen root: register it rather
        // than moving files around. The old migration code moved them; keeping
        // the directory is lossless and makes rollback trivial.
        profile.game_dir.clone()
    } else {
        directories.get(&default_id.clone()?)?.path.clone()
    };

    if !root.is_dir() {
        return None;
    }

    if let Some(existing) = directories.find_by_path(&root) {
        return Some(existing.id.clone());
    }

    let name = root
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("Migrated")
        .to_string();
    directories
        .add(name, &root, GameDirectoryKind::External)
        .ok()
        .map(|d| d.id)
}

/// Convert legacy launch options into the new instance settings.
fn settings_from_legacy(profile: &LegacyProfile, instance_id: &InstanceId) -> InstanceSettings {
    let mut settings = InstanceSettings {
        display_name: Some(profile.name.clone())
            .filter(|name| !name.trim().is_empty() && name.as_str() != instance_id.as_str()),
        icon: icon_from_legacy(profile.icon.as_deref()),
        isolation: profile
            .isolation
            .as_deref()
            .and_then(IsolationPolicy::parse),
        created_at: profile.created_at.or_else(|| Some(chrono::Utc::now())),
        last_played: profile.last_played,
        play_time_seconds: profile.play_time_seconds,
        ..Default::default()
    };

    // Java: the old model had one path field; a configured path becomes CUSTOM.
    settings.java_type = if profile.java_path.is_some() {
        JavaType::Custom
    } else {
        JavaType::Auto
    };
    settings.custom_java_path = profile.java_path.clone().unwrap_or_default();

    // JVM args: legacy had both normal and "override" args. The new model has
    // one list; override args keep their old priority by appearing first.
    settings.jvm_args = profile
        .override_jvm_args
        .iter()
        .chain(profile.jvm_args.iter())
        .cloned()
        .collect();
    settings.game_args = profile.game_args.clone();
    settings.java_agents = profile.java_agents.clone();
    settings.no_generated_jvm_args = profile.no_generated_jvm_args;
    settings.no_generated_optimizing_jvm_args = profile.no_generated_optimizing_jvm_args;

    settings.max_memory_mb = if profile.memory_mb > 0 {
        profile.memory_mb
    } else {
        4096
    };
    settings.min_memory_mb = (profile.min_memory_mb > 0).then_some(profile.min_memory_mb);
    settings.metaspace_mb = (profile.metaspace_mb > 0).then_some(profile.metaspace_mb);

    settings.process_priority =
        ProcessPriority::parse(&profile.process_priority).unwrap_or_default();
    settings.window_type = if profile.fullscreen {
        WindowType::Fullscreen
    } else {
        WindowType::Windowed
    };
    settings.width = profile.width.unwrap_or(854);
    settings.height = profile.height.unwrap_or(480);

    settings.use_native_glfw = profile.use_native_glfw;
    settings.use_native_openal = profile.use_native_openal;
    settings.use_custom_natives = profile.use_custom_natives;
    settings.enable_debug_log_output = profile.enable_debug_log_output;
    settings.wrapper = profile.wrapper.clone().unwrap_or_default();
    settings.pre_launch_command = profile.pre_launch_command.clone().unwrap_or_default();
    settings.post_exit_command = profile.post_exit_command.clone().unwrap_or_default();
    settings.env_vars = profile.env_vars.clone();

    settings.override_properties = migrate_overrides(&profile.override_properties);
    settings
}

fn migrate_overrides(old: &BTreeSet<String>) -> BTreeSet<String> {
    let mut new = BTreeSet::new();
    for name in old {
        let mapped: &[&str] = match name.as_str() {
            "java_path" => &[fields::JAVA_TYPE, fields::CUSTOM_JAVA_PATH],
            "memory_mb" => &[fields::MAX_MEMORY_MB],
            "fullscreen" => &[fields::WINDOW_TYPE],
            "override_jvm_args" => &[fields::JVM_ARGS],
            "jvm_args" => &[fields::JVM_ARGS],
            "game_args" => &[fields::GAME_ARGS],
            "min_memory_mb" => &[fields::MIN_MEMORY_MB],
            "metaspace_mb" => &[fields::METASPACE_MB],
            "process_priority" => &[fields::PROCESS_PRIORITY],
            "width" => &[fields::WIDTH],
            "height" => &[fields::HEIGHT],
            "use_native_glfw" => &[fields::USE_NATIVE_GLFW],
            "use_native_openal" => &[fields::USE_NATIVE_OPENAL],
            "use_custom_natives" => &[fields::USE_CUSTOM_NATIVES],
            "no_generated_jvm_args" => &[fields::NO_GENERATED_JVM_ARGS],
            "no_generated_optimizing_jvm_args" => &[fields::NO_GENERATED_OPTIMIZING_JVM_ARGS],
            "enable_debug_log_output" => &[fields::ENABLE_DEBUG_LOG_OUTPUT],
            "wrapper" => &[fields::WRAPPER],
            "java_agents" => &[fields::JAVA_AGENTS],
            "pre_launch_command" => &[fields::PRE_LAUNCH_COMMAND],
            "post_exit_command" => &[fields::POST_EXIT_COMMAND],
            "env_vars" => &[fields::ENV_VARS],
            other if fields::ALL.contains(&other) => &[],
            _ => &[],
        };
        if mapped.is_empty() {
            if fields::ALL.contains(&name.as_str()) {
                new.insert(name.clone());
            }
        } else {
            new.extend(mapped.iter().map(|s| s.to_string()));
        }
    }
    new
}

fn icon_from_legacy(icon: Option<&str>) -> InstanceIcon {
    match icon.map(str::trim).filter(|s| !s.is_empty()) {
        None => InstanceIcon::Auto,
        Some(value) if value.starts_with("data:") => InstanceIcon::DataUrl(value.to_string()),
        Some(value) if value.contains('/') || value.contains('\\') => {
            InstanceIcon::File(PathBuf::from(value))
        }
        Some(value) => InstanceIcon::Builtin(value.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn temp(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "rmccc-migrate-{tag}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn fs() -> Arc<dyn PlatformFs> {
        rmccc_platform::Platform::current().fs
    }

    #[test]
    fn migrates_installed_profile_and_keeps_uninstalled_out() {
        let data = temp("basic");
        let config = rmccc_config::AppConfig {
            data_dir: Some(data.clone()),
            ..Default::default()
        };
        let mut dirs = GameDirectoryStore::load(&data, &config, fs()).unwrap();
        let default = dirs.selected().unwrap().path.clone();

        let version_dir = default.join("versions/1.20.4");
        std::fs::create_dir_all(&version_dir).unwrap();
        std::fs::write(version_dir.join("1.20.4.json"), b"{}").unwrap();

        let profiles = data.join("profiles");
        std::fs::create_dir_all(&profiles).unwrap();
        let legacy = serde_json::json!({
            "id": "profile-uuid",
            "name": "Survival",
            "game_version": "1.20.4",
            "java_path": "/jdk21/bin/java",
            "jvm_args": ["-Xss4m"],
            "memory_mb": 8192,
            "game_dir": default,
            "override_properties": ["java_path", "memory_mb"],
            "created_at": "2026-01-01T00:00:00Z",
            "play_time_seconds": 3600
        });
        std::fs::write(
            profiles.join("profile-uuid.json"),
            serde_json::to_string_pretty(&legacy).unwrap(),
        )
        .unwrap();
        // A profile whose version was deleted must not create a ghost instance.
        std::fs::write(
            profiles.join("ghost.json"),
            r#"{"id":"ghost","name":"Ghost","game_version":"9.9.9","game_dir":"/nope"}"#,
        )
        .unwrap();

        let report = migrate_legacy_profiles(&data, &mut dirs, Some("profile-uuid"), fs()).unwrap();
        assert_eq!(report.migrated, 1);
        assert_eq!(report.skipped, 1);

        let settings = InstanceSettings::load(&version_dir).settings;
        assert_eq!(settings.display_name.as_deref(), Some("Survival"));
        assert_eq!(settings.max_memory_mb, 8192);
        assert_eq!(settings.java_type, JavaType::Custom);
        assert!(settings.is_overridden(fields::MAX_MEMORY_MB));
        assert_eq!(settings.play_time_seconds, 3600);
        assert_eq!(dirs.selected_instance(), Some("default/1.20.4"));

        // Marker makes the migration idempotent.
        let report2 = migrate_legacy_profiles(&data, &mut dirs, None, fs()).unwrap();
        assert_eq!(report2.migrated, 0);

        let _ = std::fs::remove_dir_all(&data);
    }
}
