//! Profile (game instance) management — JSON file-based storage.

use rmccc_error::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use ts_rs::TS;

/// A game instance / profile.
///
/// Launch-related fields mirror [`rmccc_launch::LaunchOptions`]; every optional
/// field is `#[serde(default)]` so older profile JSON loads losslessly.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export_to = "types.ts")]
pub struct Profile {
    pub id: String,
    pub name: String,
    pub game_version: String,
    pub mod_loader: Option<ModLoaderConfig>,
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
        let path = self.profile_path(id);
        if !path.exists() {
            return Ok(None);
        }
        let data = std::fs::read_to_string(&path)?;
        Ok(Some(serde_json::from_str(&data)?))
    }

    /// Save a profile to disk atomically (write temp → rename).
    pub fn save(&self, profile: &Profile) -> Result<()> {
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
    pub fn delete(&self, id: &str, cleanup_game_dir: bool) -> Result<()> {
        let path = self.profile_path(id);
        if !path.exists() {
            return Err(rmccc_error::RmcccError::ProfileNotFound(id.to_string()));
        }

        // Load profile to get game_dir before deleting
        let profile = self
            .get(id)?
            .ok_or_else(|| rmccc_error::RmcccError::ProfileNotFound(id.to_string()))?;

        std::fs::remove_file(&path)?;

        if cleanup_game_dir && profile.game_dir.exists() {
            std::fs::remove_dir_all(&profile.game_dir)?;
            tracing::info!(profile_id = %id, "Game directory cleaned up");
        }

        tracing::info!(profile_id = %id, "Profile deleted");
        Ok(())
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
}
