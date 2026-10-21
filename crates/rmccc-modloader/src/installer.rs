//! 官方安装器（installer JAR）运行器。
//!
//! Forge / NeoForge 的安装方式与 Fabric/Quilt 等"打 patch"的 loader 不同：
//! 它们发布一个 **installer JAR**，必须用 Java 运行
//! `java -jar <installer> --installClient <game_dir>`，由安装器自己写下
//! `versions/<loader 版本>/<同名>.json` 与所需库文件。
//!
//! 这段逻辑此前在 `ModLoaderProvider` 的三个实现里各写一遍
//! （`ForgeInstaller::install` / `NeoForgeInstaller::install`），并被 Tauri 与
//! CLI 各自 `match` 分流（审计 P1-4）。现在只有这一份实现，
//! 由 [`DependencyManager::install_loader`] 统一路由。

use rmccc_error::{Result, RmcccError};
use std::path::{Path, PathBuf};

use crate::ModLoaderType;

/// NeoForge Maven 根。
const NEOFORGE_MAVEN: &str = "https://maven.neoforged.net/releases/net/neoforged";
/// MinecraftForge Maven 根。
const FORGE_MAVEN: &str = "https://maven.minecraftforge.net/net/minecraftforge/forge";

/// 用官方 installer JAR 安装 loader 所需的三样东西。
pub struct InstallerJarRunner<'a> {
    pub client: &'a reqwest::Client,
    pub java_path: &'a Path,
    pub game_dir: &'a Path,
}

impl<'a> InstallerJarRunner<'a> {
    /// 下载并运行该 loader 的 installer，返回安装器创建出的**版本 id**。
    ///
    /// 返回的 id 以磁盘上实际出现的 `versions/<id>/<id>.json` 为准（安装器有
    /// 时会把 MC 版本号写成 `1.20.1-forge-47.2.0`，有时只写 `1.20.1`），
    /// 而不是我们猜测的字符串。
    pub async fn run(
        &self,
        loader_type: &ModLoaderType,
        mc_version: &str,
        loader_version: &str,
    ) -> Result<String> {
        let before = self.version_ids()?;
        let (bytes, used_url) = self
            .download_installer(loader_type, mc_version, loader_version)
            .await?;
        tracing::info!(loader = %loader_type.display_name(), url = %used_url, "Loader installer downloaded");

        // 安装器要求 game_dir 下存在 launcher_profiles.json（Mojang 启动器痕迹）。
        let profiles_path = self.game_dir.join("launcher_profiles.json");
        if !profiles_path.exists() {
            std::fs::create_dir_all(self.game_dir)?;
            std::fs::write(&profiles_path, "{\"profiles\":{}}")?;
        }

        let installer_path = self
            .game_dir
            .join(format!("{}-installer.jar", loader_type.library_id()));
        std::fs::write(&installer_path, &bytes)?;

        tracing::info!(
            java = %self.java_path.display(),
            installer = %installer_path.display(),
            "Running loader installer JAR"
        );
        let output = tokio::process::Command::new(self.java_path)
            .arg("-jar")
            .arg(&installer_path)
            .arg("--installClient")
            .arg(self.game_dir)
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .output()
            .await
            .map_err(|e| {
                RmcccError::Other(format!(
                    "Failed to run {} installer: {e}",
                    loader_type.display_name()
                ))
            })?;

        // 安装器 JAR 不再需要（它已经把库与版本 JSON 铺好）。
        let _ = std::fs::remove_file(&installer_path);

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            let exit_code = output
                .status
                .code()
                .map(|c| c.to_string())
                .unwrap_or_else(|| "unknown".into());
            return Err(RmcccError::Other(format!(
                "{} installer failed (exit {exit_code}): {stderr}",
                loader_type.display_name()
            )));
        }

        // 以"新增了哪个版本目录"为准，而不是猜 id。
        let after = self.version_ids()?;
        let created: Vec<&String> = after.iter().filter(|id| !before.contains(id)).collect();
        if let Some(id) = created
            .iter()
            .find(|id| version_json_exists(self.game_dir, id))
            .or_else(|| created.last())
        {
            tracing::info!(version_id = %id, "Loader installer created version");
            return Ok((*id).clone());
        }

        // 安装器可能原地更新了已有目录（重复安装同一版本）；退回到约定 id。
        let fallback = format!("{mc_version}-{}-{loader_version}", loader_type.library_id());
        if version_json_exists(self.game_dir, &fallback) {
            tracing::info!(version_id = %fallback, "Loader installer reused existing version id");
            return Ok(fallback);
        }

        Err(RmcccError::Other(format!(
            "{} installer finished but no version JSON appeared under {}",
            loader_type.display_name(),
            self.game_dir.join("versions").display()
        )))
    }

    /// 按 loader 类型下载 installer JAR（NeoForge 有 `neoforge/` 与旧 `forge/` 两种 artifact）。
    async fn download_installer(
        &self,
        loader_type: &ModLoaderType,
        mc_version: &str,
        loader_version: &str,
    ) -> Result<(Vec<u8>, String)> {
        let candidates = installer_urls(loader_type, mc_version, loader_version);
        let mut last_error = String::new();

        for url in &candidates {
            match self.client.get(url).send().await {
                Ok(resp) if resp.status().is_success() => {
                    let bytes = resp
                        .bytes()
                        .await
                        .map_err(|e| RmcccError::Network(e.to_string()))?;
                    // 早失败：不是合法 ZIP 就不要交给 Java。
                    if zip::ZipArchive::new(std::io::Cursor::new(&bytes)).is_err() {
                        last_error = format!("{url} — not a valid ZIP/JAR");
                        continue;
                    }
                    return Ok((bytes.to_vec(), url.clone()));
                }
                Ok(resp) => last_error = format!("{url} — HTTP {}", resp.status()),
                Err(e) => last_error = format!("{url} — {e}"),
            }
        }

        Err(RmcccError::DownloadFailed {
            url: candidates.first().cloned().unwrap_or_default(),
            reason: format!("All installer URLs failed. Last error: {last_error}"),
        })
    }

    /// `versions/` 下现有目录名。
    fn version_ids(&self) -> Result<Vec<String>> {
        let dir = self.game_dir.join("versions");
        let mut ids = Vec::new();
        if let Ok(entries) = std::fs::read_dir(&dir) {
            for entry in entries.flatten() {
                if entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
                    ids.push(entry.file_name().to_string_lossy().to_string());
                }
            }
        }
        Ok(ids)
    }
}

/// 该 loader 是否需要"运行官方安装器"（而不是打 patch）。
pub fn needs_installer_jar(loader_type: &ModLoaderType) -> bool {
    matches!(loader_type, ModLoaderType::Forge | ModLoaderType::NeoForge)
}

/// 安装器 JAR 的候选 URL（纯函数，便于单测）。
///
/// NeoForge 在 MC 1.20.1 时代仍发布在 `forge/` artifact 下，因此两种都试。
pub fn installer_urls(
    loader_type: &ModLoaderType,
    mc_version: &str,
    loader_version: &str,
) -> Vec<String> {
    match loader_type {
        ModLoaderType::Forge => vec![format!(
            "{FORGE_MAVEN}/{mc_version}-{loader_version}/forge-{mc_version}-{loader_version}-installer.jar"
        )],
        ModLoaderType::NeoForge => vec![
            format!("{NEOFORGE_MAVEN}/neoforge/{loader_version}/neoforge-{loader_version}-installer.jar"),
            format!("{NEOFORGE_MAVEN}/forge/{loader_version}/forge-{loader_version}-installer.jar"),
        ],
        _ => Vec::new(),
    }
}

fn version_json_exists(game_dir: &Path, id: &str) -> bool {
    version_json_path(game_dir, id).exists()
}

fn version_json_path(game_dir: &Path, id: &str) -> PathBuf {
    game_dir
        .join("versions")
        .join(id)
        .join(format!("{id}.json"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_forge_and_neoforge_need_installer_jar() {
        assert!(needs_installer_jar(&ModLoaderType::Forge));
        assert!(needs_installer_jar(&ModLoaderType::NeoForge));
        for other in [
            ModLoaderType::Fabric,
            ModLoaderType::Quilt,
            ModLoaderType::LiteLoader,
            ModLoaderType::OptiFine,
            ModLoaderType::Cleanroom,
            ModLoaderType::LegacyFabric,
        ] {
            assert!(!needs_installer_jar(&other), "{other:?} 应走 patch 路径");
        }
    }

    #[test]
    fn forge_installer_url_matches_maven_layout() {
        let urls = installer_urls(&ModLoaderType::Forge, "1.20.1", "47.2.0");
        assert_eq!(
            urls,
            vec![
                "https://maven.minecraftforge.net/net/minecraftforge/forge/1.20.1-47.2.0/forge-1.20.1-47.2.0-installer.jar".to_string()
            ]
        );
    }

    #[test]
    fn neoforge_tries_both_artifact_names() {
        let urls = installer_urls(&ModLoaderType::NeoForge, "1.20.1", "47.1.106");
        assert_eq!(urls.len(), 2);
        assert!(urls[0].contains("/neoforge/"));
        assert!(urls[1].contains("/forge/"));
    }

    #[test]
    fn non_installer_loaders_have_no_installer_urls() {
        assert!(installer_urls(&ModLoaderType::Fabric, "1.20.4", "0.15.0").is_empty());
    }
}
