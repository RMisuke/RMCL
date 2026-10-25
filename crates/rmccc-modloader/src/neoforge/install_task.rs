//! NeoForge install task — routes to the Forge processors pipeline.
//!
//! # HMCL alignment（`download/neoforge/NeoForgeInstallTask`）
//!
//! NeoForge 安装器有两种形态，处理方式都是**复用 Forge 新式 processors 管线**
//! （[`ForgeNewInstallTask`]），只是产物 patch 的 id/version 需要改名：
//!
//! - **Path A（1.20.1 世代）**：`install_profile.json.profile == "forge"` 且带
//!   `META-INF/NEOFORGE.RSA` 签名（或正文含 "neoforge"）——伪装成 Forge 格式；
//!   HMCL 直接交给 `ForgeNewInstallTask`，之后 `withId(neoforge)` 改名。
//! - **Path B（1.20.2+ 世代，原生 neoforge profile）**：HMCL 的
//!   `NeoForgeOldInstallTask` 本质也是同一套 processors 循环（差异只在
//!   DOWNLOAD_MOJMAPS 处理器的镜像加速补丁，见下方注释），这里同样直接复用。
//!
//! 此前实现只从安装器提取库、从不运行 processors——现代 NeoForge 的
//! universal/binpatched 产物全部依赖 processors 生成，装出来必然不可启动。
//!
//! # 与 HMCL 的已知差异（后续可补）
//!
//! HMCL 把 `DOWNLOAD_MOJMAPS` 处理器替换为"从版本清单直下 client mappings"，
//! 目的只是让 mappings 走 BMCLAPI 镜像候选；我们的处理器按原样执行（Java 进程
//! 自行下载），功能等价、不走镜像。

use crate::analyzer::LibraryType;
use crate::forge::new_install::ForgeNewInstallTask;
use crate::install_task::InstallTask;
use crate::patch::GameInstancePatch;
use rmccc_error::{Result, RmcccError};
use std::io::Read;
use std::path::PathBuf;

pub struct NeoForgeInstallTask {
    pub game_version: String,
    pub neo_version: String,
    /// 下载候选 = [BMCLAPI 镜像, maven.neoforged.net 官方回退]（与下载源
    /// 回退机制一致，403/失败时切官方源）。
    pub installer_urls: Vec<String>,
    /// Path to the Java binary（processors 需要真实 JVM）.
    pub java_path: PathBuf,
    /// Game directory（libraries 落盘与模板变量解析）.
    pub game_dir: PathBuf,
    /// 平台描述符：processors 的 `java -cp` 分隔符。
    pub platform: rmccc_platform::PlatformDescriptor,
    /// Shared HTTP client（下载安装器）.
    pub client: reqwest::Client,
}

#[async_trait::async_trait]
impl InstallTask for NeoForgeInstallTask {
    async fn execute(&mut self) -> Result<GameInstancePatch> {
        tracing::info!(
            game_version = %self.game_version,
            neo_version = %self.neo_version,
            "Executing NeoForge install task"
        );

        let (installer_path, _used_url) =
            crate::installer::download_installer_file(&self.client, &self.installer_urls, None)
                .await?;
        let result = self.execute_with_installer(&installer_path).await;
        let _ = std::fs::remove_file(&installer_path);
        result
    }

    fn description(&self) -> &str {
        "Installing NeoForge"
    }
}

impl NeoForgeInstallTask {
    async fn execute_with_installer(
        &self,
        installer_path: &std::path::Path,
    ) -> Result<GameInstancePatch> {
        let profile_json = read_install_profile(installer_path)?;
        let parsed: serde_json::Value = serde_json::from_str(&profile_json)
            .map_err(|e| RmcccError::Other(format!("Invalid install_profile.json: {e}")))?;

        let profile_type = parsed.get("profile").and_then(|v| v.as_str()).unwrap_or("");
        let profile_minecraft = parsed
            .get("minecraft")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        // HMCL `NeoForgeInstallTask.install` 的 Path A 判据：forge profile +
        // NEOFORGE.RSA 签名（或正文含 "neoforge"）。
        let has_neoforge_rsa = {
            let file = std::fs::File::open(installer_path)?;
            zip::ZipArchive::new(file)
                .map(|mut a| a.by_name("META-INF/NEOFORGE.RSA").is_ok())
                .unwrap_or(false)
        };

        if profile_type == "forge" && (has_neoforge_rsa || profile_json.contains("neoforge")) {
            // ── Path A：1.20.1 世代伪装 Forge 格式 ──
            tracing::info!("Detected old NeoForge installer (forge profile + NEOFORGE.RSA)");
            self.check_version_mismatch(&profile_minecraft)?;

            let profile_version = parsed
                .get("version")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let forge_version = modify_neoforge_old_version(&self.game_version, &profile_version);

            let mut task = ForgeNewInstallTask {
                game_version: self.game_version.clone(),
                forge_version,
                installer_path: installer_path.to_path_buf(),
                java_path: self.java_path.clone(),
                game_dir: self.game_dir.clone(),
                client: self.client.clone(),
                platform: self.platform,
            };
            let mut patch = task.execute().await?;
            // HMCL：`withId(neoforge)` + 版本名剥 "forge" 段（1.20.1 世代
            // 版本号已由 modify_neoforge_old_version 归一，replace 是幂等兜底）。
            patch.id = LibraryType::NeoForge.patch_id().to_string();
            patch.version = patch
                .version
                .replace("forge", "")
                .trim_start_matches('-')
                .to_string();
            Ok(patch)
        } else if profile_type == "neoforge" || profile_type == "NeoForge" {
            // ── Path B：1.20.2+ 原生 neoforge profile（processors 形态同 Forge）──
            tracing::info!("Detected native NeoForge installer (neoforge profile)");
            self.check_version_mismatch(&profile_minecraft)?;

            let profile_version = parsed
                .get("version")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let neo_self = modify_neoforge_new_version(&profile_version);

            let mut task = ForgeNewInstallTask {
                game_version: self.game_version.clone(),
                // ForgeNewInstallTask 只把 forge_version 填进 patch.version；
                // 传归一后的 NeoForge 版本号即可。
                forge_version: neo_self,
                installer_path: installer_path.to_path_buf(),
                java_path: self.java_path.clone(),
                game_dir: self.game_dir.clone(),
                client: self.client.clone(),
                platform: self.platform,
            };
            let mut patch = task.execute().await?;
            patch.id = LibraryType::NeoForge.patch_id().to_string();
            Ok(patch)
        } else {
            Err(RmcccError::Other(format!(
                "Unsupported NeoForge installer profile type: {profile_type} (NEOFORGE.RSA: {has_neoforge_rsa})"
            )))
        }
    }

    /// HMCL `VersionMismatchException` 对齐：安装器目标 MC 版本必须与实例一致。
    fn check_version_mismatch(&self, profile_minecraft: &str) -> Result<()> {
        if !profile_minecraft.is_empty() && profile_minecraft != self.game_version {
            return Err(RmcccError::Other(format!(
                "NeoForge installer targets MC {} but instance is {}",
                profile_minecraft, self.game_version
            )));
        }
        Ok(())
    }
}

/// HMCL `modifyNeoForgeOldVersion`：剥 MC 版本前缀与首尾 `-_`。
///
/// `"1.20.1-47.1.105"` → `"47.1.105"`。
fn modify_neoforge_old_version(game_version: &str, version: &str) -> String {
    let x = version.replace(game_version, "").trim().to_string();
    let x = x.trim_start_matches('-').to_string();
    let x = x.trim_end_matches('-').to_string();
    let x = x.trim_end_matches('_').to_string();
    x.trim_start_matches('_').to_string()
}

/// HMCL `modifyNeoForgeNewVersion`：剥 "neoforge" 段与前导 `-`。
fn modify_neoforge_new_version(version: &str) -> String {
    version
        .replace("neoforge", "")
        .trim_start_matches('-')
        .to_string()
}

fn read_install_profile(path: &std::path::Path) -> Result<String> {
    let file = std::fs::File::open(path)?;
    let mut archive = zip::ZipArchive::new(file)
        .map_err(|e| RmcccError::Other(format!("Not a valid ZIP: {e}")))?;
    let mut json = String::new();
    archive
        .by_name("install_profile.json")
        .map_err(|e| RmcccError::Other(format!("install_profile.json not found: {e}")))?
        .read_to_string(&mut json)?;
    Ok(json)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_modify_neoforge_old_version() {
        // HMCL 语义：replace(gameVersion) → trim → 剥首尾 '-'/'_'
        assert_eq!(
            modify_neoforge_old_version("1.20.1", "1.20.1-47.1.105"),
            "47.1.105"
        );
        assert_eq!(
            modify_neoforge_old_version("1.20.1", "1.20.1-20.4.233"),
            "20.4.233"
        );
        // 已是裸版本号 → 原样
        assert_eq!(
            modify_neoforge_old_version("1.20.1", "47.1.105"),
            "47.1.105"
        );
        // 尾部下划线形态（HMCL removeSuffix "_" 覆盖的输入）
        assert_eq!(
            modify_neoforge_old_version("1.20.1", "1.20.1-47.1.105_"),
            "47.1.105"
        );
    }

    #[test]
    fn test_modify_neoforge_new_version() {
        assert_eq!(modify_neoforge_new_version("neoforge-20.4.233"), "20.4.233");
        assert_eq!(modify_neoforge_new_version("20.4.233"), "20.4.233");
        assert_eq!(modify_neoforge_new_version("21.1.77"), "21.1.77");
    }
}
