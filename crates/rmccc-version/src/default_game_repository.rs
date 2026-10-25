//! `DefaultGameRepository` — filesystem-backed instance index
//! (HMCL `DefaultGameRepository`).
//!
//! Responsibilities:
//! - `refresh()` scans `{game_dir}/versions/`, tolerates mismatched JSON
//!   filenames and malformed files, filters instances whose compatibility
//!   rules don't apply, drops instances with unresolvable parents, and
//!   ensures `launcher_profiles.json` exists (Mojang/HMCL compat).
//! - `resolve()` follows the `inheritsFrom` chain locally, merges fields, then
//!   applies `patches` sorted by priority, producing the dual-view
//!   [`ResolvedVersion`](super::resolved::ResolvedVersion).
//! - CRUD (rename / duplicate / remove) operates on the `versions/{id}`
//!   folders atomically.

use rmccc_error::{Result, RmcccError};
use rmccc_platform::PlatformDescriptor;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use super::compatibility;
use super::file_layout;
use super::game_repository::GameRepository;
use super::isolation::IsolationType;
use super::resolved::ResolvedVersion;
use super::types::{GamePatch, GameVersion};

/// The Mojang-compatible profile marker HMCL ensures on refresh.
fn launcher_profiles_content() -> String {
    let token = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    format!(
        "{{\"profiles\":{{}},\"selectedProfile\":\"(Default)\",\"clientToken\":\"{token:x}\",\"authenticationDatabase\":{{}}}}"
    )
}

/// A filesystem-backed game repository rooted at a `.minecraft` directory.
pub struct DefaultGameRepository {
    base_directory: PathBuf,
    manifests: HashMap<String, GameVersion>,
    /// 平台描述符：compatibility rules 与 natives 目录命名经它判断，
    /// 由组装层（`CoreContext`）注入一次（指南 §5.7.1 第 3/5 条）。
    platform: PlatformDescriptor,
}

impl DefaultGameRepository {
    pub fn new(base_directory: PathBuf) -> Self {
        Self::with_platform(base_directory, PlatformDescriptor::current())
    }

    /// 用显式平台描述符构造（组装层注入 / 测试模拟其他平台）。
    pub fn with_platform(base_directory: PathBuf, platform: PlatformDescriptor) -> Self {
        Self {
            base_directory,
            manifests: HashMap::new(),
            platform,
        }
    }

    /// 当前注入的平台描述符。
    pub fn platform(&self) -> &PlatformDescriptor {
        &self.platform
    }

    /// Change the base directory and refresh the index.
    pub fn set_base_directory(&mut self, dir: PathBuf) -> Result<()> {
        self.base_directory = dir;
        self.refresh()
    }

    fn ensure_launcher_profiles(&self) -> Result<()> {
        let path = file_layout::launcher_profiles_path(&self.base_directory);
        if path.exists() {
            return Ok(());
        }
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&path, launcher_profiles_content())?;
        tracing::info!(path = %path.display(), "Created launcher_profiles.json");
        Ok(())
    }

    /// Merge a child manifest on top of an already-resolved parent.
    ///
    /// 权威实现在 [`super::merge::merge_child_into_parent`]（审计批次 3 收敛，
    /// 三条解析链路共用一份语义）；此处保留薄委托以维持本模块内调用点可读。
    fn merge_child_into_parent(parent: &mut GameVersion, child: &GameVersion) {
        super::merge::merge_child_into_parent(parent, child);
    }

    /// Apply a patch to a resolved manifest (HMCL `GameInstanceManifest.merge(patch)`).
    fn apply_patch(version: &mut GameVersion, patch: &GamePatch) {
        if !patch.libraries.is_empty() {
            super::merge::append_libraries(version, &patch.libraries);
        }
        if let Some(mc) = &patch.main_class {
            version.main_class = mc.clone();
        }
        if let Some(ai) = &patch.asset_index {
            version.asset_index = ai.clone();
        }
        if let Some(jv) = &patch.java_version {
            version.java_version = Some(jv.clone());
        }
        if let Some(pa) = &patch.arguments {
            super::merge::append_arguments(version, pa);
        }
        if let Some(ma) = &patch.minecraft_arguments {
            version.minecraft_arguments = Some(ma.clone());
        }
        version
            .compatibility_rules
            .extend(patch.compatibility_rules.clone());
    }

    /// Recursively resolve an instance along its `inheritsFrom` chain.
    ///
    /// Patches from every manifest on the chain are accumulated into
    /// `patches_out` and applied by [`Self::resolve`] (sorted by priority).
    fn resolve_from(
        &self,
        id: &str,
        visited: &mut HashSet<String>,
        patches_out: &mut Vec<GamePatch>,
    ) -> Result<GameVersion> {
        if !visited.insert(id.to_string()) {
            return Err(RmcccError::Other(format!(
                "Version inheritance cycle detected at '{id}'"
            )));
        }
        let manifest = self
            .manifests
            .get(id)
            .cloned()
            .ok_or_else(|| RmcccError::Other(format!("Version not installed: {id}")))?;

        patches_out.extend(manifest.patches.clone());

        let mut merged = if let Some(parent) = &manifest.inherits_from {
            let mut parent_resolved = self.resolve_from(parent, visited, patches_out)?;
            Self::merge_child_into_parent(&mut parent_resolved, &manifest);
            parent_resolved
        } else {
            manifest.clone()
        };
        // Base version's own patches (root-level) applied too.
        let _ = &mut merged;
        Ok(merged)
    }

    /// Resolve an instance into the dual-view [`ResolvedVersion`].
    pub fn resolve(&self, id: &str) -> Result<ResolvedVersion> {
        let unresolved = self
            .manifests
            .get(id)
            .cloned()
            .ok_or_else(|| RmcccError::Other(format!("Version not installed: {id}")))?;

        let mut visited = HashSet::new();
        let mut patches = Vec::new();
        let merged = self.resolve_from(id, &mut visited, &mut patches)?;
        let root_id = self.get_game_version(id);
        patches.sort_by_key(|p| p.priority);

        let mut launch = merged.clone();
        let mut standalone = merged.clone();
        for p in &patches {
            Self::apply_patch(&mut launch, p);
            Self::apply_patch(&mut standalone, p);
        }
        launch.inherits_from = None;
        launch.patches = Vec::new();
        // standalone keeps the patches list (for editing / round-tripping).
        standalone.inherits_from = None;
        standalone.patches = patches;

        // `jar` falls back to the base version's id (HMCL): an inheriting
        // instance shares the parent's client JAR (e.g. `1.20.4-fabric` uses
        // `versions/1.20.4/1.20.4.jar`).
        if launch.jar.is_none() {
            launch.jar = Some(root_id.clone());
        }
        if standalone.jar.is_none() {
            standalone.jar = Some(root_id);
        }

        Ok(ResolvedVersion {
            unresolved,
            launch,
            standalone,
        })
    }

    /// Whether every `inheritsFrom` link on the chain resolves within the index.
    fn chain_resolvable(&self, id: &str, visited: &mut HashSet<String>) -> bool {
        if !visited.insert(id.to_string()) {
            return false;
        }
        match self.manifests.get(id).and_then(|m| m.inherits_from.clone()) {
            Some(parent) => self.chain_resolvable(&parent, visited),
            None => true,
        }
    }

    /// Read and parse a manifest file, normalizing the id to the folder name.
    fn read_instance_manifest(json_path: &Path, folder_id: &str) -> Result<GameVersion> {
        let data = std::fs::read_to_string(json_path)?;
        let mut manifest = super::manifest::parse_version_json(folder_id, &data)?;
        if manifest.id.is_empty() {
            manifest.id = folder_id.to_string();
        } else if manifest.id != folder_id {
            // HMCL tolerates / fixes id↔folder mismatches.
            tracing::warn!(
                folder = %folder_id,
                json_id = %manifest.id,
                "Version JSON id differs from folder name; normalizing to folder"
            );
            manifest.id = folder_id.to_string();
        }
        Ok(manifest)
    }

    /// Whether an instance is considered modded (has patches) — used by
    /// `Modded` isolation.
    pub fn is_modded(&self, id: &str) -> bool {
        self.manifests
            .get(id)
            .map(|m| !m.patches.is_empty() || m.inherits_from.is_some())
            .unwrap_or(false)
    }
}

impl GameRepository for DefaultGameRepository {
    fn game_dir(&self) -> &Path {
        &self.base_directory
    }

    fn refresh(&mut self) -> Result<()> {
        let versions_dir = file_layout::versions_dir(&self.base_directory);
        self.manifests.clear();
        self.ensure_launcher_profiles()?;

        if !versions_dir.exists() {
            std::fs::create_dir_all(&versions_dir)?;
            return Ok(());
        }

        for entry in std::fs::read_dir(&versions_dir)? {
            let entry = entry?;
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }
            let folder_id = match path.file_name().and_then(|n| n.to_str()) {
                Some(name) => name.to_string(),
                None => continue,
            };

            let json_path = path.join(format!("{folder_id}.json"));
            let manifest = if json_path.exists() {
                match Self::read_instance_manifest(&json_path, &folder_id) {
                    Ok(m) => m,
                    Err(e) => {
                        tracing::warn!(
                            folder = %folder_id,
                            error = %e,
                            "Skipping unparseable version JSON (HMCL GameJsonParseFailedEvent)"
                        );
                        continue;
                    }
                }
            } else {
                // Tolerate a mismatched JSON filename — but only when the file
                // actually looks like a version manifest. HMCL modpack folders
                // keep config JSONs (emi.json / modrinth.index.json / …) that
                // would otherwise parse into garbage instances (HMCL itself
                // requires `<folder>/<folder>.json` strictly).
                let fallback = std::fs::read_dir(&path).ok().and_then(|rd| {
                    rd.flatten()
                        .map(|e| e.path())
                        .find(|p| p.extension().and_then(|e| e.to_str()) == Some("json"))
                });
                match fallback {
                    Some(fb) if json_looks_like_version_manifest(&fb) => {
                        match Self::read_instance_manifest(&fb, &folder_id) {
                            Ok(m) => m,
                            Err(e) => {
                                tracing::warn!(folder = %folder_id, error = %e, "Skipping version");
                                continue;
                            }
                        }
                    }
                    _ => continue,
                }
            };

            // Compatibility filter (HMCL `CompatibilityRule.appliesToCurrentEnvironment`).
            if !compatibility::applies_to_environment(&manifest.compatibility_rules, &self.platform)
            {
                tracing::debug!(
                    folder = %folder_id,
                    "Instance filtered by compatibility rules (not applicable on this OS)"
                );
                continue;
            }

            self.manifests.insert(folder_id, manifest);
        }

        // Drop instances whose inheritance chain references a missing parent.
        let resolvable: Vec<String> = self
            .manifests
            .keys()
            .filter(|id| {
                let mut visited = HashSet::new();
                self.chain_resolvable(id, &mut visited)
            })
            .cloned()
            .collect();
        self.manifests.retain(|id, _| resolvable.contains(id));

        tracing::info!(
            game_dir = %self.base_directory.display(),
            instances = self.manifests.len(),
            "Game repository refreshed"
        );
        Ok(())
    }

    fn has_instance(&self, id: &str) -> bool {
        self.manifests.contains_key(id)
    }

    fn get_instance_ids(&self) -> Vec<String> {
        let mut ids: Vec<String> = self.manifests.keys().cloned().collect();
        ids.sort();
        ids
    }

    fn get_manifest(&self, id: &str) -> Option<&GameVersion> {
        self.manifests.get(id)
    }

    fn get_resolved(&self, id: &str) -> Result<ResolvedVersion> {
        self.resolve(id)
    }

    fn get_run_directory(&self, id: &str, isolation: IsolationType) -> PathBuf {
        super::isolation::run_directory(&self.base_directory, id, isolation, self.is_modded(id))
    }

    fn get_library_file(&self, _id: &str, name: &str) -> PathBuf {
        file_layout::library_file(&self.base_directory, name)
    }

    fn get_native_directory(&self, id: &str) -> PathBuf {
        file_layout::resolve_native_dir(&self.base_directory, id, &self.platform)
    }

    fn get_instance_jar(&self, id: &str) -> PathBuf {
        let jar_id = self
            .manifests
            .get(id)
            .and_then(|m| m.jar.as_deref())
            .map(|s| s.to_string())
            .unwrap_or_else(|| self.get_game_version(id));
        file_layout::instance_jar(&self.base_directory, &jar_id)
    }

    fn get_asset_index_file(&self, id: &str) -> PathBuf {
        let asset_id = self
            .manifests
            .get(id)
            .map(|m| m.asset_index.id.as_str())
            .unwrap_or("");
        if asset_id.is_empty() {
            file_layout::asset_index_file(&self.base_directory, id)
        } else {
            file_layout::asset_index_file(&self.base_directory, asset_id)
        }
    }

    fn get_asset_object(&self, hash: &str) -> Option<PathBuf> {
        file_layout::asset_object(&self.base_directory, hash)
    }

    fn get_mods_directory(&self, id: &str) -> PathBuf {
        // Mods always live in the run directory; default to shared mods dir.
        self.get_run_directory(id, IsolationType::Never)
            .join("mods")
    }

    fn get_game_version(&self, id: &str) -> String {
        let mut current = id.to_string();
        let mut visited = HashSet::new();
        while visited.insert(current.clone()) {
            match self
                .manifests
                .get(&current)
                .and_then(|m| m.inherits_from.clone())
            {
                Some(parent) => current = parent,
                None => break,
            }
        }
        current
    }

    fn rename_instance(&mut self, from: &str, to: &str) -> Result<()> {
        if from == to {
            return Ok(());
        }
        if self.manifests.contains_key(to) {
            return Err(RmcccError::Other(format!(
                "An instance named '{to}' already exists"
            )));
        }
        let from_root = file_layout::instance_root(&self.base_directory, from);
        let to_root = file_layout::instance_root(&self.base_directory, to);
        if !from_root.exists() {
            return Err(RmcccError::Other(format!(
                "Instance '{from}' not installed"
            )));
        }
        if let Some(parent) = to_root.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::rename(&from_root, &to_root)?;
        // Rename inner {from}.json / {from}.jar → {to}.json / {to}.jar.
        for ext in ["json", "jar"] {
            let old = to_root.join(format!("{from}.{ext}"));
            let new = to_root.join(format!("{to}.{ext}"));
            if old.exists() && !new.exists() {
                std::fs::rename(&old, &new)?;
            }
        }
        self.refresh()?;
        tracing::info!(from = %from, to = %to, "Instance renamed");
        Ok(())
    }

    fn duplicate_instance(&mut self, from: &str, to: &str) -> Result<()> {
        if from == to {
            return Ok(());
        }
        if self.manifests.contains_key(to) {
            return Err(RmcccError::Other(format!(
                "An instance named '{to}' already exists"
            )));
        }
        let from_root = file_layout::instance_root(&self.base_directory, from);
        let to_root = file_layout::instance_root(&self.base_directory, to);
        if !from_root.exists() {
            return Err(RmcccError::Other(format!(
                "Instance '{from}' not installed"
            )));
        }
        copy_dir_recursive(&from_root, &to_root)?;
        for ext in ["json", "jar"] {
            let old = to_root.join(format!("{from}.{ext}"));
            let new = to_root.join(format!("{to}.{ext}"));
            if old.exists() && !new.exists() {
                std::fs::rename(&old, &new)?;
            }
        }
        self.refresh()?;
        tracing::info!(from = %from, to = %to, "Instance duplicated");
        Ok(())
    }

    fn remove_instance(&mut self, id: &str) -> Result<()> {
        let root = file_layout::instance_root(&self.base_directory, id);
        if root.exists() {
            std::fs::remove_dir_all(&root)?;
        }
        self.manifests.remove(id);
        tracing::info!(id = %id, "Instance removed");
        Ok(())
    }
}

/// Recursively copy a directory tree.
fn copy_dir_recursive(from: &Path, to: &Path) -> Result<()> {
    std::fs::create_dir_all(to)?;
    for entry in std::fs::read_dir(from)? {
        let entry = entry?;
        let src = entry.path();
        let dst = to.join(entry.file_name());
        if src.is_dir() {
            copy_dir_recursive(&src, &dst)?;
        } else {
            std::fs::copy(&src, &dst)?;
        }
    }
    Ok(())
}

/// Heuristic: does this JSON actually describe a game version?
///
/// Used ONLY for the mismatched-filename fallback — any of the markers below
/// distinguishes a real manifest from mod/config files that merely share the
/// extension.
fn json_looks_like_version_manifest(path: &std::path::Path) -> bool {
    let Ok(text) = std::fs::read_to_string(path) else {
        return false;
    };
    let Ok(v) = serde_json::from_str::<serde_json::Value>(&text) else {
        return false;
    };
    v.get("assetIndex").is_some()
        || v.get("arguments").is_some()
        || v.get("minecraftArguments").is_some()
        || v.get("inheritsFrom").is_some()
        || v.get("downloads").is_some()
        || v.get("libraries")
            .and_then(|l| l.as_array())
            .map(|a| !a.is_empty())
            .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    // ── HMCL 兼容：额外目录扫描 + 整合包配置 json 不产生实例 ──

    #[test]
    fn hmcl_layout_scans_real_versions_and_skips_config_only_folders() {
        let dir = std::env::temp_dir().join(format!("rmccc-hmcl-{}", std::process::id()));
        let versions = dir.join("versions");
        // 真实实例：目录名一致的 json（含 assetIndex 标记）。
        let real = versions.join("1.21.1-NeoForge");
        std::fs::create_dir_all(&real).unwrap();
        std::fs::write(
            real.join("1.21.1-NeoForge.json"),
            r#"{"id":"1.21.1-NeoForge","assetIndex":{"id":"13"},"type":"release"}"#,
        )
        .unwrap();
        std::fs::write(real.join("1.21.1-NeoForge.jar"), b"jar").unwrap();
        // HMCL 整合包目录：只有配置 json（无版本标记）——不得成为实例。
        let pack = versions.join("SomeModpack");
        std::fs::create_dir_all(&pack).unwrap();
        std::fs::write(pack.join("emi.json"), r#"{"bind":{},"crafting":{}}"#).unwrap();
        let mut repo = super::DefaultGameRepository::new(dir.clone());
        repo.refresh().expect("refresh");
        let ids = repo.get_instance_ids();
        assert_eq!(ids, vec!["1.21.1-NeoForge".to_string()]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    use super::*;
    use crate::isolation::IsolationType;

    fn temp_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "rmccc-repo-{tag}-{}-{:x}",
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

    fn write_version(game_dir: &Path, id: &str, json: &str) {
        let dir = file_layout::instance_root(game_dir, id);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(file_layout::instance_json(game_dir, id), json).unwrap();
    }

    fn vanilla_json(id: &str) -> String {
        format!(
            r#"{{
                "id": "{id}",
                "type": "release",
                "mainClass": "net.minecraft.client.main.Main",
                "assetIndex": {{"id": "17", "url": "https://example.com/17.json", "sha1": "abc", "size": 1, "totalSize": 1}},
                "downloads": {{"client": {{"url": "https://example.com/{id}.jar", "sha1": "def", "size": 2}}}},
                "libraries": [{{"name": "com.mojang:test:1.0"}}],
                "javaVersion": {{"majorVersion": 21}}
            }}"#
        )
    }

    #[test]
    fn test_refresh_discovers_instances() {
        let dir = temp_dir("refresh");
        write_version(&dir, "1.20.4", &vanilla_json("1.20.4"));
        let mut repo = DefaultGameRepository::new(dir.clone());
        repo.refresh().unwrap();
        assert!(repo.has_instance("1.20.4"));
        assert_eq!(repo.get_instance_ids(), vec!["1.20.4".to_string()]);
        // launcher_profiles.json created (Mojang/HMCL compat)
        assert!(file_layout::launcher_profiles_path(&dir).exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_resolve_merges_inheritance_chain() {
        let dir = temp_dir("chain");
        write_version(&dir, "1.20.4", &vanilla_json("1.20.4"));
        // Child (modded) inheriting from 1.20.4
        write_version(
            &dir,
            "1.20.4-fabric",
            r#"{
                "id": "1.20.4-fabric",
                "type": "release",
                "inheritsFrom": "1.20.4",
                "patches": [{
                    "id": "fabric",
                    "version": "0.16.9",
                    "priority": 30000,
                    "mainClass": "net.fabricmc.loader.impl.launch.knot.KnotClient",
                    "libraries": [{"name": "net.fabricmc:fabric-loader:0.16.9"}]
                }]
            }"#,
        );
        let mut repo = DefaultGameRepository::new(dir.clone());
        repo.refresh().unwrap();

        let resolved = repo.get_resolved("1.20.4-fabric").unwrap();
        resolved.validate().unwrap();
        assert_eq!(
            resolved.launch.main_class,
            "net.fabricmc.loader.impl.launch.knot.KnotClient"
        );
        assert_eq!(resolved.launch.inherits_from, None);
        assert_eq!(resolved.launch.patches.len(), 0);
        // parent library + patch library both present
        let names: Vec<&str> = resolved
            .launch
            .libraries
            .iter()
            .map(|l| l.name.as_str())
            .collect();
        assert!(names.contains(&"com.mojang:test:1.0"));
        assert!(names.contains(&"net.fabricmc:fabric-loader:0.16.9"));
        // javaVersion inherited
        assert_eq!(
            resolved.launch.java_version.as_ref().unwrap().major_version,
            21
        );
        // standalone keeps the patch list
        assert_eq!(resolved.standalone.patches.len(), 1);
        // base MC version resolution
        assert_eq!(repo.get_game_version("1.20.4-fabric"), "1.20.4");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_resolve_cycle_detection() {
        let dir = temp_dir("cycle");
        write_version(
            &dir,
            "a",
            r#"{"id":"a","type":"release","inheritsFrom":"b"}"#,
        );
        write_version(
            &dir,
            "b",
            r#"{"id":"b","type":"release","inheritsFrom":"a"}"#,
        );
        let mut repo = DefaultGameRepository::new(dir.clone());
        repo.refresh().unwrap();
        // Cyclic pair is dropped from the index (chain_resolvable = false).
        assert!(!repo.has_instance("a"));
        assert!(!repo.has_instance("b"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_rename_and_duplicate() {
        let dir = temp_dir("crud");
        write_version(&dir, "1.20.4", &vanilla_json("1.20.4"));
        let mut repo = DefaultGameRepository::new(dir.clone());
        repo.refresh().unwrap();

        repo.rename_instance("1.20.4", "1.20.4-copy").unwrap();
        assert!(repo.has_instance("1.20.4-copy"));
        assert!(!repo.has_instance("1.20.4"));

        repo.duplicate_instance("1.20.4-copy", "1.20.4-dup")
            .unwrap();
        assert!(repo.has_instance("1.20.4-dup"));

        repo.remove_instance("1.20.4-dup").unwrap();
        assert!(!repo.has_instance("1.20.4-dup"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_isolation_run_directory() {
        let dir = temp_dir("iso");
        write_version(&dir, "1.20.4", &vanilla_json("1.20.4"));
        let mut repo = DefaultGameRepository::new(dir.clone());
        repo.refresh().unwrap();

        assert_eq!(repo.get_run_directory("1.20.4", IsolationType::Never), dir);
        assert_eq!(
            repo.get_run_directory("1.20.4", IsolationType::Always),
            dir.join("versions").join("1.20.4")
        );
        // vanilla (no patches) → Modded does not isolate
        assert_eq!(repo.get_run_directory("1.20.4", IsolationType::Modded), dir);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_patch_priority_ordering() {
        let dir = temp_dir("priority");
        write_version(&dir, "1.20.4", &vanilla_json("1.20.4"));
        write_version(
            &dir,
            "1.20.4-modded",
            r#"{
                "id": "1.20.4-modded",
                "type": "release",
                "inheritsFrom": "1.20.4",
                "patches": [
                    {"id": "high", "priority": 60000, "mainClass": "com.high.Main", "libraries": [{"name": "com.high:lib:1.0"}]},
                    {"id": "low", "priority": 10000, "mainClass": "com.low.Main", "libraries": [{"name": "com.low:lib:1.0"}]}
                ]
            }"#,
        );
        let mut repo = DefaultGameRepository::new(dir.clone());
        repo.refresh().unwrap();
        let resolved = repo.get_resolved("1.20.4-modded").unwrap();
        // highest priority wins for mainClass
        assert_eq!(resolved.launch.main_class, "com.high.Main");
        assert_eq!(resolved.launch.libraries.len(), 3); // vanilla + low + high
        let _ = std::fs::remove_dir_all(&dir);
    }
}
