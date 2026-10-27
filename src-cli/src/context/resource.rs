//! 资源域：安装 / 完整性检查 / 修复 / SHA1 校验。
//!
//! `impl CliContext` 的一个分片（P2-1：把 1,450 行的 God class 按域拆开）。
//! 方法名 / 签名与拆分前完全一致，命令层无需改动。

use super::*;

impl CliContext {
    /// Install game resources for a profile (canonical pipeline).
    pub async fn resource_install(
        &self,
        key: &str,
        version: &str,
        progress: Option<&ProgressCb>,
    ) -> Result<()> {
        let instance = self.get_instance(key).await?;
        let core = self.core.read().await;
        let report =
            pipeline::install_game(&core.handle(), &instance.paths.game_dir, version, progress)
                .await?;
        if !report.is_complete() {
            return Err(RmcccError::DownloadFailed {
                url: version.to_string(),
                reason: format!(
                    "{} of {} files failed to download",
                    report.failed_files, report.total_files
                ),
            });
        }
        // Refresh the local index so the new files are observed.
        pipeline::refresh_repository(&core.handle(), &instance.summary.directory_id)?;
        Ok(())
    }

    /// Completeness check against the local repository (offline).
    pub async fn resource_check(&self, key: &str) -> Result<Value> {
        let instance = self.get_instance(key).await?;
        let core = self.core.read().await;
        let (game_root, resolved) = {
            let mut service = core
                .instances
                .lock()
                .map_err(|_| RmcccError::Other("instance service lock poisoned".to_string()))?;
            let (detail, resolved) = service.resolved_version(key)?;
            (detail.paths.game_dir, resolved)
        };
        // 显式检查命令：全量 SHA1 复核（启动路径只用 Size 级）。
        let missing = rmccc_version::completion::check_completion(
            &game_root,
            &resolved,
            &core.platform.descriptor,
            rmccc_version::completion::CompletionVerify::Hash,
        );
        let items: Vec<Value> = missing
            .iter()
            .map(|m| {
                json!({
                    "kind": format!("{:?}", m.kind),
                    "description": m.description,
                    "path": m.path.display().to_string(),
                })
            })
            .collect();
        Ok(json!({
            "key": key,
            "game_version": instance.summary.id,
            "complete": items.is_empty(),
            "missing_count": items.len(),
            "missing": items,
        }))
    }

    /// Repair missing resources (HMCL checkGameCompletionAsync equivalent).
    /// Returns whether anything was repaired.
    pub async fn resource_repair(&self, key: &str, progress: Option<&ProgressCb>) -> Result<bool> {
        let instance = self.get_instance(key).await?;
        let core = self.core.read().await;
        // 显式修复命令：全量 SHA1 复核（ensure_launch_ready 的启动路径只用 Size 级）。
        let report = pipeline::repair_instance(
            &core.handle(),
            &instance.paths.game_dir,
            &instance.summary.id,
            progress,
            rmccc_version::completion::CompletionVerify::Hash,
        )
        .await?;
        if !report.is_complete() {
            return Err(RmcccError::DownloadFailed {
                url: instance.summary.id.clone(),
                reason: format!(
                    "{} of {} files failed to download",
                    report.failed_files, report.total_files
                ),
            });
        }
        Ok(!report.is_noop())
    }

    /// SHA1 integrity verification for a profile's game files.
    pub async fn resource_verify(&self, key: &str) -> Result<Value> {
        let instance = self.get_instance(key).await?;
        let game_dir = &instance.paths.game_dir;
        let version_id = &instance.summary.id;

        let provider: &dyn rmccc_version::VersionProvider =
            &rmccc_version::MojangVersionProvider::new(self.http_client.clone());
        let game_version = rmccc_version::resolve_version(version_id, provider).await?;
        let platform = self.core.read().await.platform.descriptor;

        let mut verified = 0u64;
        let mut failed: Vec<Value> = Vec::new();
        let mut missing: Vec<Value> = Vec::new();

        // Client JAR.
        if let Some(client_dl) = game_version.downloads.get("client") {
            let jar_path = game_dir
                .join("versions")
                .join(version_id)
                .join(format!("{version_id}.jar"));
            match verify_sha1(&jar_path, &client_dl.sha1) {
                Ok(true) => verified += 1,
                Ok(false) => failed.push(json!({
                    "file": jar_path.display().to_string(),
                    "reason": "sha1_mismatch",
                })),
                Err(_) => missing.push(json!({
                    "file": jar_path.display().to_string(),
                    "reason": "missing",
                })),
            }
        }

        // Libraries (sample check up to 50 to bound runtime).
        let libs_dir = game_dir.join("libraries");
        let mut lib_checked = 0u64;
        let mut lib_missing = 0u64;
        for lib in game_version
            .libraries
            .iter()
            .filter(|l| rmccc_version::libraries::library_applies(l, &platform))
        {
            if lib_checked >= 50 {
                break;
            }
            let lib_path = make_library_path(&lib.name, &libs_dir);
            if lib_path.exists() {
                lib_checked += 1;
            } else {
                lib_missing += 1;
            }
        }

        let assets_dir = game_dir.join("assets").join("objects");
        let asset_count = walk_dir_top(&assets_dir).len() as u64;

        let ok = failed.is_empty() && missing.is_empty();
        Ok(json!({
            "key": key,
            "game_version": version_id,
            "verified": verified + lib_checked,
            "verified_files": verified,
            "libraries_checked": lib_checked,
            "libraries_missing": lib_missing,
            "asset_objects": asset_count,
            "failed": failed,
            "missing": missing,
            "ok": ok,
        }))
    }
}
