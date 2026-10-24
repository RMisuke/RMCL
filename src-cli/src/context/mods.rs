//! Mod 域：Modrinth 搜索与版本、下载安装、本地文件启停 / 删除。
//!
//! `impl CliContext` 的一个分片（P2-1：把 1,450 行的 God class 按域拆开）。
//! 方法名 / 签名与拆分前完全一致，命令层无需改动。

use super::*;

impl CliContext {
    /// Build a fresh Modrinth source on the shared HTTP client. The DI
    /// container's `mod_sources` list is the canonical place where sources are
    /// registered; a fresh instance keeps the facade free of borrow problems
    /// while sharing the exact same HTTP client (connection pooling / proxy).
    /// `mods/` directory for a profile: follows the instance's run directory so
    /// isolated instances keep their own mods (HMCL behaviour).
    async fn mods_dir(&self, profile: &rmccc_profile::Profile) -> PathBuf {
        let core = self.core.read().await;
        rmccc_launch::instance_mods_dir(&core.config, profile)
    }

    fn mod_source(&self) -> Result<Box<dyn rmccc_mods::ModSource>> {
        let source: Box<dyn rmccc_mods::ModSource> = Box::new(
            rmccc_mods::modrinth::ModrinthSource::new(self.http_client.clone()),
        );
        Ok(source)
    }

    pub async fn mod_search(
        &self,
        query: &str,
        game_version: Option<&str>,
        loader: Option<&str>,
        page: u32,
        limit: u32,
    ) -> Result<rmccc_mods::ModSearchResult> {
        let source = self.mod_source()?;
        source
            .search(&rmccc_mods::ModSearchQuery {
                query: query.to_string(),
                game_version: game_version.map(str::to_string),
                loader: loader.map(str::to_string),
                page,
                limit: limit.min(100),
                index: None,
            })
            .await
    }

    pub async fn mod_versions(&self, mod_id: &str) -> Result<Vec<rmccc_mods::ModVersion>> {
        let source = self.mod_source()?;
        source.list_versions(mod_id).await
    }

    pub async fn mod_download_url(&self, version_id: &str) -> Result<String> {
        let source = self.mod_source()?;
        source.get_download_url(version_id).await
    }

    pub async fn mod_list(&self, profile_id: &str) -> Result<Value> {
        let profile = self.get_profile(profile_id).await?;
        let mods_dir = self.mods_dir(&profile).await;
        let mut active = Vec::new();
        let mut disabled = Vec::new();
        if let Ok(entries) = std::fs::read_dir(&mods_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_file() && path.extension().is_some_and(|e| e == "jar") {
                    active.push(
                        path.file_name()
                            .unwrap_or_default()
                            .to_string_lossy()
                            .to_string(),
                    );
                }
            }
        }
        let disabled_dir = mods_dir.join("disabled");
        if let Ok(entries) = std::fs::read_dir(&disabled_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_file() && path.extension().is_some_and(|e| e == "jar") {
                    disabled.push(
                        path.file_name()
                            .unwrap_or_default()
                            .to_string_lossy()
                            .to_string(),
                    );
                }
            }
        }
        active.sort();
        disabled.sort();
        Ok(json!({
            "profile_id": profile_id,
            "mods_dir": mods_dir.display().to_string(),
            "active": active,
            "disabled": disabled,
            "count": active.len() + disabled.len(),
        }))
    }

    pub async fn mod_install(
        &self,
        profile_id: &str,
        version: rmccc_mods::ModVersion,
    ) -> Result<String> {
        let profile = self.get_profile(profile_id).await?;
        let source = self.mod_source()?;
        let mods_dir = self.mods_dir(&profile).await;
        std::fs::create_dir_all(&mods_dir)?;

        let filename = version
            .filename
            .clone()
            .filter(|f| !f.trim().is_empty())
            .map(|f| sanitize_filename(&f))
            .unwrap_or_else(|| format!("{}.jar", sanitize_filename(&version.name)));
        let filename = if filename.ends_with(".jar") {
            filename
        } else {
            format!("{filename}.jar")
        };
        validate_mod_filename(&filename)?;

        let dest = mods_dir.join(&filename);
        source.install(&version, &dest).await?;
        tracing::info!(profile_id = %profile_id, filename = %filename, "Mod installed");
        Ok(filename)
    }

    /// Install a local jar file into a profile's mods directory (copies the
    /// file with a sanitized name derived from the source filename).
    pub async fn mod_install_local(&self, profile_id: &str, src_path: &str) -> Result<String> {
        let profile = self.get_profile(profile_id).await?;
        let src = Path::new(src_path);
        if !src.is_file() {
            return Err(RmcccError::Other(format!(
                "Local mod file not found: {src_path}"
            )));
        }
        let stem = src
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();
        let filename = sanitize_filename(&stem);
        let filename = if filename.ends_with(".jar") {
            filename
        } else {
            format!("{filename}.jar")
        };
        validate_mod_filename(&filename)?;

        let mods_dir = self.mods_dir(&profile).await;
        std::fs::create_dir_all(&mods_dir)?;
        let dest = mods_dir.join(&filename);
        std::fs::copy(src, &dest)?;
        tracing::info!(profile_id = %profile_id, filename = %filename, "Local mod installed");
        Ok(filename)
    }

    pub async fn mod_remove(&self, profile_id: &str, filename: &str) -> Result<()> {
        validate_mod_filename(filename)?;
        let profile = self.get_profile(profile_id).await?;
        let mods_dir = self.mods_dir(&profile).await;
        let active = mods_dir.join(filename);
        let disabled = mods_dir.join("disabled").join(filename);
        if active.exists() {
            std::fs::remove_file(&active)?;
        } else if disabled.exists() {
            std::fs::remove_file(&disabled)?;
        } else {
            return Err(RmcccError::Other(format!("Mod file not found: {filename}")));
        }
        Ok(())
    }

    pub async fn mod_toggle(&self, profile_id: &str, filename: &str, enabled: bool) -> Result<()> {
        validate_mod_filename(filename)?;
        let profile = self.get_profile(profile_id).await?;
        let mods_dir = self.mods_dir(&profile).await;
        let disabled_dir = mods_dir.join("disabled");
        let (src, dst) = if enabled {
            (disabled_dir.join(filename), mods_dir.join(filename))
        } else {
            (mods_dir.join(filename), disabled_dir.join(filename))
        };
        if !src.exists() {
            return Err(RmcccError::Other(format!("Mod file not found: {filename}")));
        }
        if let Some(parent) = dst.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::rename(&src, &dst)?;
        Ok(())
    }
}
