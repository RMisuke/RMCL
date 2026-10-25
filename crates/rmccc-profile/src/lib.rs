//! Profile (game instance) management — JSON file-based storage.

use rmccc_error::Result;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeSet, HashMap};
use std::path::PathBuf;
use ts_rs::TS;

/// A game instance / profile.
///
/// Two layers, following HMCL: the launcher-wide defaults live in
/// `rmccc_config::AppConfig`, and this record holds the *instance* values plus
/// [`Profile::override_properties`] — the set of field names that actually take
/// the instance value instead of the global default.
///
/// Keeping the value and the "is overridden" flag apart is HMCL's
/// `overrideProperties` design, and it matters for the UI: toggling a field back
/// to "inherit" must not throw the user's value away, so it can be restored when
/// they toggle again. `Profile::resolve` does the merge in one place.
///
/// (HMCL stores the instance half in
/// `{instance_root}/.hmcl/config/instance-game-settings.json`. RMCCC keeps it in
/// `profiles/{id}.json` because its instance registry is the profile store, not
/// a scan of `versions/`; the settings still follow the instance on duplicate,
/// and are removed with it.)
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export_to = "types.ts")]
pub struct Profile {
    pub id: String,
    pub name: String,
    pub game_version: String,
    pub mod_loader: Option<ModLoaderConfig>,
    /// Isolation strategy (`never` | `modded` | `always`). `None` follows the
    /// global `default_isolation`. This is the "实例隔离 / 独立实例文件夹" switch:
    /// when it isolates, the run directory is the instance root instead of the
    /// shared game root.
    #[serde(default)]
    pub isolation: Option<String>,
    /// Field names that override the global defaults (HMCL `overrideProperties`).
    /// A field absent from this set uses the global value and its instance value
    /// here is ignored — but still preserved on disk.
    #[serde(default)]
    pub override_properties: BTreeSet<String>,
    pub java_path: Option<String>,
    pub jvm_args: Vec<String>,
    pub game_args: Vec<String>,
    pub memory_mb: u32,
    pub game_dir: PathBuf,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub last_played: Option<chrono::DateTime<chrono::Utc>>,
    pub icon: Option<String>,

    // ── Extended launch options (all serde-defaulted for backward compat) ──
    #[serde(default)]
    pub min_memory_mb: u32,
    #[serde(default)]
    pub metaspace_mb: u32,
    /// Process priority label: "low" | "below_normal" | "normal" | "above_normal" | "high".
    #[serde(default = "default_priority")]
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
    pub env_vars: HashMap<String, String>,
}

fn default_priority() -> String {
    "normal".to_string()
}

impl Default for Profile {
    fn default() -> Self {
        Self {
            id: String::new(),
            name: String::new(),
            game_version: String::new(),
            mod_loader: None,
            isolation: None,
            override_properties: BTreeSet::new(),
            java_path: None,
            jvm_args: Vec::new(),
            game_args: Vec::new(),
            memory_mb: 0,
            game_dir: PathBuf::new(),
            created_at: chrono::Utc::now(),
            last_played: None,
            icon: None,
            min_memory_mb: 0,
            metaspace_mb: 0,
            process_priority: "normal".to_string(),
            width: None,
            height: None,
            fullscreen: false,
            use_native_glfw: false,
            use_native_openal: false,
            use_custom_natives: false,
            no_generated_jvm_args: false,
            no_generated_optimizing_jvm_args: false,
            enable_debug_log_output: false,
            wrapper: None,
            java_agents: Vec::new(),
            override_jvm_args: Vec::new(),
            pre_launch_command: None,
            post_exit_command: None,
            env_vars: HashMap::new(),
        }
    }
}

/// Names used in [`Profile::override_properties`].
///
/// Constants rather than string literals so the UI, the resolution logic and the
/// tests cannot drift apart.
pub mod fields {
    pub const JAVA_PATH: &str = "java_path";
    pub const JVM_ARGS: &str = "jvm_args";
    pub const GAME_ARGS: &str = "game_args";
    pub const MEMORY_MB: &str = "memory_mb";
    pub const MIN_MEMORY_MB: &str = "min_memory_mb";
    pub const METASPACE_MB: &str = "metaspace_mb";
    pub const PROCESS_PRIORITY: &str = "process_priority";
    pub const WIDTH: &str = "width";
    pub const HEIGHT: &str = "height";
    pub const FULLSCREEN: &str = "fullscreen";
    pub const NO_GENERATED_JVM_ARGS: &str = "no_generated_jvm_args";
    pub const NO_GENERATED_OPTIMIZING_JVM_ARGS: &str = "no_generated_optimizing_jvm_args";
    pub const ENABLE_DEBUG_LOG_OUTPUT: &str = "enable_debug_log_output";
    pub const WRAPPER: &str = "wrapper";
    pub const JAVA_AGENTS: &str = "java_agents";
    pub const OVERRIDE_JVM_ARGS: &str = "override_jvm_args";
    pub const PRE_LAUNCH_COMMAND: &str = "pre_launch_command";
    pub const POST_EXIT_COMMAND: &str = "post_exit_command";
    pub const ENV_VARS: &str = "env_vars";
    pub const USE_CUSTOM_NATIVES: &str = "use_custom_natives";
    pub const USE_NATIVE_GLFW: &str = "use_native_glfw";
    pub const USE_NATIVE_OPENAL: &str = "use_native_openal";

    /// Every overridable field, in the order the settings UI shows them.
    pub const ALL: &[&str] = &[
        JAVA_PATH,
        JVM_ARGS,
        GAME_ARGS,
        MEMORY_MB,
        MIN_MEMORY_MB,
        METASPACE_MB,
        PROCESS_PRIORITY,
        WIDTH,
        HEIGHT,
        FULLSCREEN,
        NO_GENERATED_JVM_ARGS,
        NO_GENERATED_OPTIMIZING_JVM_ARGS,
        ENABLE_DEBUG_LOG_OUTPUT,
        WRAPPER,
        JAVA_AGENTS,
        OVERRIDE_JVM_ARGS,
        PRE_LAUNCH_COMMAND,
        POST_EXIT_COMMAND,
        ENV_VARS,
        USE_CUSTOM_NATIVES,
        USE_NATIVE_GLFW,
        USE_NATIVE_OPENAL,
    ];
}

impl Profile {
    /// Whether `field` takes the instance value instead of the global default.
    pub fn is_overridden(&self, field: &str) -> bool {
        self.override_properties.contains(field)
    }

    /// Marks `field` as overridden (or clears it, keeping the stored value).
    pub fn set_overridden(&mut self, field: &str, overridden: bool) {
        if overridden {
            self.override_properties.insert(field.to_string());
        } else {
            self.override_properties.remove(field);
        }
    }

    /// Effective isolation: the instance's own value, else the global default.
    pub fn effective_isolation(&self, config: &rmccc_config::AppConfig) -> String {
        rmccc_config::resolve_isolation(
            self.isolation
                .as_deref()
                .or(Some(config.default_isolation.as_str())),
        )
        .to_string()
    }
}

/// Concrete per-instance launch values after merging the instance's overrides
/// with the launcher-wide defaults (HMCL `GameSettings.Effective`).
///
/// The launcher only ever consumes these, never the raw profile.
///
/// 也经 IPC 暴露给前端（`instance_resolved_settings`），这样"某字段当前继承时
/// 生效值是多少"只有一个来源（审计 P2-8）。
#[derive(Debug, Clone, PartialEq, serde::Serialize, ts_rs::TS)]
#[ts(export_to = "types.ts")]
pub struct ResolvedInstanceSettings {
    /// Normalised isolation value (`never` | `modded` | `always`).
    pub isolation: String,
    pub java_path: Option<String>,
    pub jvm_args: Vec<String>,
    pub game_args: Vec<String>,
    pub max_memory_mb: u32,
    pub min_memory_mb: u32,
    pub metaspace_mb: u32,
    pub process_priority: String,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub fullscreen: bool,
    pub use_native_glfw: bool,
    pub use_native_openal: bool,
    pub use_custom_natives: bool,
    pub no_generated_jvm_args: bool,
    pub no_generated_optimizing_jvm_args: bool,
    pub enable_debug_log_output: bool,
    pub wrapper: Option<String>,
    pub java_agents: Vec<String>,
    pub override_jvm_args: Vec<String>,
    pub pre_launch_command: Option<String>,
    pub post_exit_command: Option<String>,
    pub env_vars: HashMap<String, String>,
}

impl Profile {
    /// Merges this instance with the global defaults, one field at a time.
    ///
    /// A field listed in [`Profile::override_properties`] wins; everything else
    /// falls back to `config` (HMCL `GameSettings.resolve`).
    pub fn resolve(&self, config: &rmccc_config::AppConfig) -> ResolvedInstanceSettings {
        use fields as f;
        let over = |field: &str| self.is_overridden(field);

        ResolvedInstanceSettings {
            isolation: self.effective_isolation(config),
            java_path: if over(f::JAVA_PATH) {
                self.java_path.clone()
            } else {
                config.default_java_path.clone()
            },
            jvm_args: if over(f::JVM_ARGS) {
                self.jvm_args.clone()
            } else {
                config.default_jvm_args.clone()
            },
            game_args: if over(f::GAME_ARGS) {
                self.game_args.clone()
            } else {
                config.default_game_args.clone()
            },
            max_memory_mb: if over(f::MEMORY_MB) && self.memory_mb > 0 {
                self.memory_mb
            } else {
                config.default_max_memory_mb
            },
            min_memory_mb: if over(f::MIN_MEMORY_MB) && self.min_memory_mb > 0 {
                self.min_memory_mb
            } else {
                config.default_min_memory_mb
            },
            metaspace_mb: if over(f::METASPACE_MB) {
                self.metaspace_mb
            } else {
                0
            },
            process_priority: if over(f::PROCESS_PRIORITY) && !self.process_priority.is_empty() {
                self.process_priority.clone()
            } else {
                config.default_process_priority.clone()
            },
            width: if over(f::WIDTH) {
                self.width
            } else {
                config.default_window_width
            },
            height: if over(f::HEIGHT) {
                self.height
            } else {
                config.default_window_height
            },
            fullscreen: if over(f::FULLSCREEN) {
                self.fullscreen
            } else {
                config.default_fullscreen
            },
            use_native_glfw: over(f::USE_NATIVE_GLFW) && self.use_native_glfw,
            use_native_openal: over(f::USE_NATIVE_OPENAL) && self.use_native_openal,
            use_custom_natives: over(f::USE_CUSTOM_NATIVES) && self.use_custom_natives,
            no_generated_jvm_args: over(f::NO_GENERATED_JVM_ARGS) && self.no_generated_jvm_args,
            no_generated_optimizing_jvm_args: over(f::NO_GENERATED_OPTIMIZING_JVM_ARGS)
                && self.no_generated_optimizing_jvm_args,
            enable_debug_log_output: over(f::ENABLE_DEBUG_LOG_OUTPUT)
                && self.enable_debug_log_output,
            wrapper: if over(f::WRAPPER) {
                self.wrapper.clone()
            } else {
                None
            },
            java_agents: if over(f::JAVA_AGENTS) {
                self.java_agents.clone()
            } else {
                Vec::new()
            },
            override_jvm_args: if over(f::OVERRIDE_JVM_ARGS) {
                self.override_jvm_args.clone()
            } else {
                Vec::new()
            },
            pre_launch_command: if over(f::PRE_LAUNCH_COMMAND) {
                self.pre_launch_command.clone()
            } else {
                None
            },
            post_exit_command: if over(f::POST_EXIT_COMMAND) {
                self.post_exit_command.clone()
            } else {
                None
            },
            env_vars: if over(f::ENV_VARS) {
                self.env_vars.clone()
            } else {
                HashMap::new()
            },
        }
    }
}

/// Mod loader configuration within a profile.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export_to = "types.ts")]
pub struct ModLoaderConfig {
    pub loader_type: rmccc_modloader::ModLoaderType,
    pub loader_version: String,
}

/// Profile storage manager.
pub struct ProfileStore {
    profiles_dir: PathBuf,
}

/// B2：profile id 直接进文件名拼接（`{id}.json`），来自内部生成但 store 是
/// pub API——拒绝空值、超长与路径注入字符（`/` `\` `..` 控制字符），
/// 对齐 agent/confirm 的 validate_id 思路（审计 2026-10-25）。
pub fn validate_profile_id(id: &str) -> Result<()> {
    let ok = !id.is_empty()
        && id.len() <= 64
        && id != ".."
        && id != "."
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_' || b == b'.')
        && !id.contains("..");
    if ok {
        Ok(())
    } else {
        Err(rmccc_error::RmcccError::Validation(format!(
            "invalid profile id: {id:?}"
        )))
    }
}

impl ProfileStore {
    pub fn new(profiles_dir: PathBuf) -> Self {
        Self { profiles_dir }
    }

    /// Scan the profiles directory and load all profiles.
    pub fn list(&self) -> Result<Vec<Profile>> {
        if !self.profiles_dir.exists() {
            std::fs::create_dir_all(&self.profiles_dir)?;
            return Ok(vec![]);
        }

        let mut profiles = Vec::new();
        for entry in std::fs::read_dir(&self.profiles_dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.extension().map(|e| e == "json").unwrap_or(false) {
                let data = std::fs::read_to_string(&path)?;
                match serde_json::from_str::<Profile>(&data) {
                    Ok(profile) => profiles.push(profile),
                    Err(e) => {
                        tracing::warn!("Skipping invalid profile file {}: {e}", path.display());
                    }
                }
            }
        }

        profiles.sort_by_key(|p| std::cmp::Reverse(p.created_at));
        Ok(profiles)
    }

    /// Load a single profile by ID.
    pub fn get(&self, id: &str) -> Result<Option<Profile>> {
        validate_profile_id(id)?;
        let path = self.profile_path(id);
        if !path.exists() {
            return Ok(None);
        }
        let data = std::fs::read_to_string(&path)?;
        Ok(Some(serde_json::from_str(&data)?))
    }

    /// Save a profile to disk atomically (write temp → rename).
    pub fn save(&self, profile: &Profile) -> Result<()> {
        validate_profile_id(&profile.id)?;
        std::fs::create_dir_all(&self.profiles_dir)?;
        let path = self.profile_path(&profile.id);
        let tmp = self.profiles_dir.join(format!("{}.json.tmp", profile.id));
        let json = serde_json::to_string_pretty(profile)?;
        std::fs::write(&tmp, json)?;
        std::fs::rename(&tmp, &path)?;
        tracing::info!(profile_id = %profile.id, name = %profile.name, "Profile saved");
        Ok(())
    }

    /// Delete a profile and optionally clean up its game directory.
    /// 删除 profile。
    ///
    /// `cleanup_game_dir` **只**会删除该实例自己的运行目录；两类目录被拒绝：
    /// 共享游戏根（`versions/` `libraries/` 的存放处，会连带删掉其它所有
    /// 实例的游戏文件，ADR-002 决策 3），以及**不像游戏目录的任意目录**
    /// （B10：profile JSON 可被手工编辑，`game_dir` 指向 `~/Documents` 之类
    /// 时绝不能整树删除——必须有游戏目录的磁盘特征才允许 remove_dir_all）。
    pub fn delete(&self, id: &str, cleanup_game_dir: bool) -> Result<()> {
        validate_profile_id(id)?;
        let path = self.profile_path(id);
        if !path.exists() {
            return Err(rmccc_error::RmcccError::ProfileNotFound(id.to_string()));
        }

        // Load profile to get game_dir before deleting
        let profile = self
            .get(id)?
            .ok_or_else(|| rmccc_error::RmcccError::ProfileNotFound(id.to_string()))?;

        std::fs::remove_file(&path)?;

        if cleanup_game_dir {
            if Self::is_shared_game_root(&profile.game_dir) {
                tracing::warn!(
                    profile_id = %id,
                    game_dir = %profile.game_dir.display(),
                    "Refusing to delete the shared game root; only the profile was removed"
                );
            } else if !Self::looks_like_game_dir(&profile.game_dir) {
                tracing::warn!(
                    profile_id = %id,
                    game_dir = %profile.game_dir.display(),
                    "Refusing to delete: game_dir has no game-directory markers; only the profile was removed"
                );
            } else if profile.game_dir.exists() {
                std::fs::remove_dir_all(&profile.game_dir)?;
                tracing::info!(profile_id = %id, "Instance directory cleaned up");
            }
        }

        tracing::info!(profile_id = %id, "Profile deleted");
        Ok(())
    }

    /// `game_dir` 是否就是共享游戏根（含 `versions/` 或 `libraries/` 的目录）。
    ///
    /// 用磁盘特征而不是配置判断，避免 `ProfileStore` 依赖 `AppConfig`。
    fn is_shared_game_root(game_dir: &std::path::Path) -> bool {
        game_dir.join("versions").exists() || game_dir.join("libraries").exists()
    }

    /// 目录是否具备游戏目录特征（B10 的 delete 守卫）。
    ///
    /// 共享根检查已经拒绝 versions/|libraries/；这里补上隔离实例的其余
    /// 特征（assets/ saves/ mods/ resourcepacks/ shaderpacks/ options.txt
    /// launcher_profiles.json）——都没有的目录（如被改写指向的
    /// `~/Documents`）一律拒绝整树删除。
    fn looks_like_game_dir(game_dir: &std::path::Path) -> bool {
        ["assets", "saves", "mods", "resourcepacks", "shaderpacks"]
            .iter()
            .any(|m| game_dir.join(m).is_dir())
            || ["options.txt", "launcher_profiles.json", "servers.dat"]
                .iter()
                .any(|m| game_dir.join(m).is_file())
    }

    /// Update the last_played timestamp.
    pub fn touch(&self, id: &str) -> Result<()> {
        let mut profile = self
            .get(id)?
            .ok_or_else(|| rmccc_error::RmcccError::ProfileNotFound(id.to_string()))?;
        profile.last_played = Some(chrono::Utc::now());
        self.save(&profile)?;
        Ok(())
    }

    fn profile_path(&self, id: &str) -> PathBuf {
        self.profiles_dir.join(format!("{}.json", id))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_profile(id: &str) -> Profile {
        Profile {
            id: id.to_string(),
            name: format!("Test {id}"),
            game_version: "1.20.4".into(),
            memory_mb: 2048,
            game_dir: std::env::temp_dir().join(format!("rmccc-profile-game-{id}")),
            ..Default::default()
        }
    }

    fn temp_profiles_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "rmccc-profile-{tag}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    /// P1-06：ProfileStore 保存/列出/读取。
    #[test]
    fn test_save_list_get() {
        let dir = temp_profiles_dir("crud");
        let store = ProfileStore::new(dir.clone());
        store.save(&make_profile("p1")).unwrap();

        let list = store.list().unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].id, "p1");

        let got = store.get("p1").unwrap();
        assert!(got.is_some());
        assert_eq!(got.unwrap().name, "Test p1");

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// P1-06：原子写（temp + rename）不残留 .tmp 文件。
    #[test]
    fn test_save_is_atomic_no_tmp_left() {
        let dir = temp_profiles_dir("atomic");
        let store = ProfileStore::new(dir.clone());
        store.save(&make_profile("p1")).unwrap();

        let leftovers: Vec<_> = std::fs::read_dir(&dir)
            .unwrap()
            .flatten()
            .filter(|e| e.file_name().to_string_lossy().ends_with(".tmp"))
            .collect();
        assert!(leftovers.is_empty());

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// P1-06：touch 更新 last_played；delete 删除文件；删除不存在报错。
    #[test]
    fn test_delete_and_touch() {
        let dir = temp_profiles_dir("del");
        let store = ProfileStore::new(dir.clone());
        store.save(&make_profile("p1")).unwrap();

        store.touch("p1").unwrap();
        let touched = store.get("p1").unwrap().unwrap();
        assert!(touched.last_played.is_some());

        store.delete("p1", false).unwrap();
        assert!(store.get("p1").unwrap().is_none());

        assert!(store.delete("nope", false).is_err());

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// ADR-002 决策 3：`cleanup=true` 绝不能删共享游戏根。
    #[test]
    fn test_delete_never_removes_shared_game_root() {
        let dir = temp_profiles_dir("del-shared");
        let store = ProfileStore::new(dir.clone());

        // 造一个"共享游戏根"：含 versions/ 与 libraries/，里面放标记文件。
        let game_root = dir.join("instances");
        std::fs::create_dir_all(game_root.join("versions/1.20.4")).unwrap();
        std::fs::create_dir_all(game_root.join("libraries")).unwrap();
        let marker = game_root.join("versions/1.20.4/1.20.4.json");
        std::fs::write(&marker, "{}").unwrap();

        let mut profile = make_profile("p-shared");
        profile.game_dir = game_root.clone();
        store.save(&profile).unwrap();

        // 即使要求清理，也只删 profile，不动共享库。
        store.delete("p-shared", true).unwrap();
        assert!(store.get("p-shared").unwrap().is_none());
        assert!(marker.exists(), "共享游戏根必须保留");
        assert!(game_root.join("libraries").exists(), "libraries 必须保留");

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// 隔离实例（game_dir 不是共享根）在 `cleanup=true` 时正常清理自己的目录。
    #[test]
    fn test_delete_cleans_isolated_instance_dir() {
        let dir = temp_profiles_dir("del-isolated");
        let store = ProfileStore::new(dir.clone());

        let instance_dir = dir.join("instances/isolated-1");
        std::fs::create_dir_all(instance_dir.join("mods")).unwrap();
        std::fs::write(instance_dir.join("options.txt"), "x").unwrap();

        let mut profile = make_profile("p-iso");
        profile.game_dir = instance_dir.clone();
        store.save(&profile).unwrap();

        store.delete("p-iso", true).unwrap();
        assert!(!instance_dir.exists(), "隔离实例目录应被清理");

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// B2：profile id 零校验曾是路径注入面——`../x` 之类不得落到文件名拼接。
    #[test]
    fn test_profile_id_validation_rejects_traversal() {
        for bad in ["../evil", "a/b", "a\\b", "..", ".", "", "a..b", "包含中文"] {
            assert!(
                validate_profile_id(bad).is_err(),
                "{bad:?} must be rejected"
            );
        }
        for good in ["11111111-1111-1111-1111-111111111111", "abc-DEF_012"] {
            assert!(
                validate_profile_id(good).is_ok(),
                "{good:?} must be accepted"
            );
        }
        // 非法 id 连读都不允许（get/save/delete 均走 profile_path 前置校验）。
        let dir = temp_profiles_dir("id-validate");
        let store = ProfileStore::new(dir.clone());
        assert!(store.get("../evil").is_err());
        assert!(store.delete("../evil", false).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// B10：game_dir 被手工改写为任意目录（无游戏特征）时，cleanup 决不能
    /// 整树删除它。
    #[test]
    fn test_delete_refuses_dirs_without_game_markers() {
        let dir = temp_profiles_dir("del-marker");
        let store = ProfileStore::new(dir.clone());

        let victim = dir.join("precious-docs");
        std::fs::create_dir_all(&victim).unwrap();
        std::fs::write(victim.join("resume.pdf"), b"do not delete").unwrap();

        let mut profile = make_profile("p-evil");
        profile.game_dir = victim.clone();
        store.save(&profile).unwrap();

        store.delete("p-evil", true).unwrap();
        assert!(store.get("p-evil").unwrap().is_none());
        assert!(victim.exists(), "无游戏特征的目录绝不能被整树删除");
        assert!(victim.join("resume.pdf").exists());

        let _ = std::fs::remove_dir_all(&dir);
    }
    fn sample_config() -> rmccc_config::AppConfig {
        rmccc_config::AppConfig {
            default_java_path: Some("/jdk21/bin/java".into()),
            default_jvm_args: vec!["-Dglobal=1".into()],
            default_game_args: vec!["--demo".into()],
            default_max_memory_mb: 2048,
            default_min_memory_mb: 512,
            default_isolation: "always".into(),
            default_window_width: Some(1280),
            default_process_priority: "high".into(),
            ..Default::default()
        }
    }

    /// Fields absent from `override_properties` must take the global value even
    /// when the instance carries a value of its own (HMCL `InheritableProperty`).
    #[test]
    fn test_resolve_inherits_unless_overridden() {
        let config = sample_config();
        let mut profile = Profile {
            memory_mb: 8192,
            jvm_args: vec!["-Xss4m".into()],
            process_priority: "low".into(),
            ..Default::default()
        };

        // Nothing marked as overridden → everything inherits the global value.
        let inherited = profile.resolve(&config);
        assert_eq!(inherited.max_memory_mb, 2048);
        assert_eq!(inherited.jvm_args, vec!["-Dglobal=1"]);
        assert_eq!(inherited.process_priority, "high");
        assert_eq!(inherited.game_args, vec!["--demo"]);
        assert_eq!(inherited.isolation, "always");

        // Marking a field overridden switches just that field to the instance value.
        profile.set_overridden(fields::MEMORY_MB, true);
        profile.set_overridden(fields::JVM_ARGS, true);
        let overridden = profile.resolve(&config);
        assert_eq!(overridden.max_memory_mb, 8192);
        assert_eq!(overridden.jvm_args, vec!["-Xss4m"]);
        // untouched fields still inherit
        assert_eq!(overridden.min_memory_mb, 512);
        assert_eq!(overridden.process_priority, "high");
    }

    /// Clearing an override must keep the stored value so the UI can restore it
    /// when the user toggles the field back on (the reason HMCL separates the
    /// value from the override list).
    #[test]
    fn test_unsetting_override_preserves_the_value() {
        let mut profile = Profile {
            memory_mb: 6144,
            ..Default::default()
        };
        profile.set_overridden(fields::MEMORY_MB, true);
        assert_eq!(profile.resolve(&sample_config()).max_memory_mb, 6144);

        profile.set_overridden(fields::MEMORY_MB, false);
        assert!(!profile.is_overridden(fields::MEMORY_MB));
        // Falls back to the global value…
        assert_eq!(profile.resolve(&sample_config()).max_memory_mb, 2048);
        // …but the instance value is still on disk.
        assert_eq!(profile.memory_mb, 6144);

        // Round-trips through JSON, so reopening the settings page keeps it.
        let json = serde_json::to_string(&profile).unwrap();
        let reloaded: Profile = serde_json::from_str(&json).unwrap();
        assert_eq!(reloaded.memory_mb, 6144);
        assert!(reloaded.override_properties.is_empty());
    }

    /// A profile JSON written before this model existed has no
    /// `override_properties`, so it inherits everything — and its concrete
    /// values are preserved for when the user opts in.
    #[test]
    fn test_legacy_profile_json_inherits_by_default() {
        let legacy = r#"{
            "id": "p1", "name": "Old", "game_version": "1.20.4", "mod_loader": null,
            "java_path": "/jdk8/bin/java", "jvm_args": ["-Xmx2G"], "game_args": [],
            "memory_mb": 4096, "game_dir": "/g",
            "created_at": "2026-01-01T00:00:00Z", "last_played": null, "icon": null,
            "min_memory_mb": 0, "metaspace_mb": 0, "process_priority": "normal",
            "fullscreen": false, "env_vars": {}
        }"#;
        let profile: Profile = serde_json::from_str(legacy).unwrap();
        assert!(profile.override_properties.is_empty());
        assert_eq!(profile.isolation, None);
        assert_eq!(profile.memory_mb, 4096);

        let resolved = profile.resolve(&sample_config());
        assert_eq!(resolved.max_memory_mb, 2048); // global wins while not overridden
        assert_eq!(resolved.java_path.as_deref(), Some("/jdk21/bin/java"));
    }

    /// Every constant in `fields::ALL` must be usable as an override name.
    #[test]
    fn test_all_override_names_are_distinct() {
        let unique: BTreeSet<&&str> = fields::ALL.iter().collect();
        assert_eq!(unique.len(), fields::ALL.len(), "duplicate override name");
        let mut profile = Profile::default();
        for field in fields::ALL {
            profile.set_overridden(field, true);
        }
        assert_eq!(profile.override_properties.len(), fields::ALL.len());
    }
}
