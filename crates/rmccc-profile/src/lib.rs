//! Profile (game instance) management — JSON file-based storage.

use rmccc_error::Result;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// A game instance / profile.
#[derive(Debug, Clone, Serialize, Deserialize)]
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
}

/// Mod loader configuration within a profile.
/// Mod loader configuration within a profile.
///
/// The `loader_type` is a string matching [`rmccc_modloader::ModLoaderType`]
/// variants (`"fabric"`, `"forge"`, `"neoforge"`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModLoaderConfig {
    pub loader_type: String,
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
                        tracing::warn!(
                            "Skipping invalid profile file {}: {e}",
                            path.display()
                        );
                    }
                }
            }
        }

        profiles.sort_by(|a, b| b.created_at.cmp(&a.created_at));
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

    /// Save a profile to disk.
    pub fn save(&self, profile: &Profile) -> Result<()> {
        std::fs::create_dir_all(&self.profiles_dir)?;
        let path = self.profile_path(&profile.id);
        let json = serde_json::to_string_pretty(profile)?;
        std::fs::write(&path, json)?;
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
        let profile = self.get(id)?.ok_or_else(|| {
            rmccc_error::RmcccError::ProfileNotFound(id.to_string())
        })?;

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
