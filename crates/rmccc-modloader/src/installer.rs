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
        let candidates = installer_urls(loader_type, mc_version, loader_version);
        let (installer_path, used_url) = download_installer_file(self.client, &candidates).await?;
        tracing::info!(loader = %loader_type.display_name(), url = %used_url, "Loader installer downloaded");

        let result = self
            .run_with_snapshot(
                loader_type,
                &installer_path,
                mc_version,
                loader_version,
                before,
            )
            .await;
        // 安装器 JAR 不再需要（无论成败都清理，见 download_installer_file 注释）。
        let _ = std::fs::remove_file(&installer_path);
        result
    }

    /// 运行**已就位**的官方安装器 JAR，返回安装器创建出的版本 id。
    ///
    /// `run` 的无下载版；供本地安装器文件复用同一条执行/探测路径。
    pub async fn run_local(
        &self,
        loader_type: &ModLoaderType,
        installer_path: &Path,
        mc_version: &str,
        loader_version: &str,
    ) -> Result<String> {
        let before = self.version_ids()?;
        self.run_with_snapshot(
            loader_type,
            installer_path,
            mc_version,
            loader_version,
            before,
        )
        .await
    }

    /// 安装器要求 game_dir 下存在 launcher_profiles.json（Mojang 启动器痕迹）。
    async fn run_with_snapshot(
        &self,
        loader_type: &ModLoaderType,
        installer_path: &Path,
        mc_version: &str,
        loader_version: &str,
        before: Vec<String>,
    ) -> Result<String> {
        let profiles_path = self.game_dir.join("launcher_profiles.json");
        if !profiles_path.exists() {
            std::fs::create_dir_all(self.game_dir)?;
            std::fs::write(&profiles_path, "{\"profiles\":{}}")?;
        }

        tracing::info!(
            java = %self.java_path.display(),
            installer = %installer_path.display(),
            "Running loader installer JAR"
        );
        // 安装器要解压大量库 + 跑 processors，慢机上可达数分钟；完全无超时
        // 才是原状（此前挂起会永久卡住安装命令）。
        const INSTALLER_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(600);
        let output = tokio::time::timeout(
            INSTALLER_TIMEOUT,
            tokio::process::Command::new(self.java_path)
                .arg("-jar")
                .arg(installer_path)
                .arg("--installClient")
                .arg(self.game_dir)
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::piped())
                .output(),
        )
        .await
        .map_err(|_| {
            RmcccError::Other(format!(
                "{} installer timed out after {}s",
                loader_type.display_name(),
                INSTALLER_TIMEOUT.as_secs()
            ))
        })?
        .map_err(|e| {
            RmcccError::Other(format!(
                "Failed to run {} installer: {e}",
                loader_type.display_name()
            ))
        })?;

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

/// 下载 installer JAR 到**唯一**临时文件并校验（批次 4，三份副本合一）。
///
/// 此前 forge/neoforge 各自下载：installer.rs 全量入内存（~180MB 双倍拷贝）、
/// forge/install_task.rs 固定名 `/tmp/forge-installer-{pid}.jar` 且从不删除
/// （每次安装泄漏 ~180MB）。统一为：
/// - 流式写入唯一临时文件（`{loader}-installer-{pid}-{nanos}.jar`）；
/// - ZIP 完整性早失败（不是合法 ZIP 不交给 Java）；
/// - `{url}.sha1` 可取时强制校验（镜像劫持/损坏即失败）；取不到时警告放行
///   （部分镜像不带 .sha1）；
/// - **调用方负责在使用后删除**（成功与失败路径都要）。
pub async fn download_installer_file(
    client: &reqwest::Client,
    candidates: &[String],
) -> Result<(PathBuf, String)> {
    let mut last_error = String::new();

    for url in candidates {
        let fetched = fetch_installer_candidate(client, url).await;
        match fetched {
            Ok(path) => return Ok((path, url.clone())),
            Err(e) => last_error = format!("{url} — {e}"),
        }
    }

    Err(RmcccError::DownloadFailed {
        url: candidates.first().cloned().unwrap_or_default(),
        reason: format!("All installer URLs failed. Last error: {last_error}"),
    })
}

/// 单候选下载 + 校验；失败时清理半成品临时文件。
async fn fetch_installer_candidate(client: &reqwest::Client, url: &str) -> Result<PathBuf> {
    let resp = client
        .get(url)
        .send()
        .await
        .map_err(|e| RmcccError::Network(e.to_string()))?;
    if !resp.status().is_success() {
        return Err(RmcccError::Network(format!("HTTP {}", resp.status())));
    }

    let temp = unique_installer_temp_path(url);
    let result = async {
        stream_to_file(resp, &temp).await?;
        validate_and_verify(client, url, &temp).await
    }
    .await;
    match result {
        Ok(()) => {
            tracing::info!(url = %url, path = %temp.display(), "Installer downloaded and validated");
            Ok(temp)
        }
        Err(e) => {
            let _ = std::fs::remove_file(&temp);
            Err(e)
        }
    }
}

/// 流式写盘（chunk 循环，installer ~180MB 不再整份进内存）。
async fn stream_to_file(mut resp: reqwest::Response, dest: &Path) -> Result<()> {
    use tokio::io::AsyncWriteExt;
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut file = tokio::fs::File::create(dest)
        .await
        .map_err(RmcccError::Io)?;
    while let Some(chunk) = resp
        .chunk()
        .await
        .map_err(|e| RmcccError::Network(e.to_string()))?
    {
        file.write_all(&chunk).await.map_err(RmcccError::Io)?;
    }
    file.flush().await.map_err(RmcccError::Io)?;
    Ok(())
}

/// ZIP 完整性 + （可取时）`.sha1` 校验。
async fn validate_and_verify(client: &reqwest::Client, url: &str, path: &Path) -> Result<()> {
    let file = std::fs::File::open(path)
        .map_err(|e| RmcccError::Other(format!("installer temp file vanished: {e}")))?;
    zip::ZipArchive::new(file)
        .map_err(|e| RmcccError::Other(format!("installer is not a valid ZIP/JAR: {e}")))?;

    match client.get(format!("{url}.sha1")).send().await {
        Ok(resp) if resp.status().is_success() => {
            let expected = resp
                .text()
                .await
                .map_err(|e| RmcccError::Network(e.to_string()))?;
            // Maven .sha1 文件形如 "abc123  filename.jar"，取第一段。
            let expected = expected
                .split_whitespace()
                .next()
                .unwrap_or("")
                .to_lowercase();
            if expected.len() != 40 {
                tracing::warn!(url = %url, "malformed .sha1 sidecar; skipping verification");
                return Ok(());
            }
            let actual = sha1_of_file(path)?;
            if !actual.eq_ignore_ascii_case(&expected) {
                return Err(RmcccError::Other(format!(
                    "installer SHA-1 mismatch for {url}: expected {expected}, got {actual}"
                )));
            }
            tracing::debug!(url = %url, "installer SHA-1 verified");
            Ok(())
        }
        // 无 .sha1 sidecar（部分镜像）：警告放行——ZIP 结构校验仍然生效。
        Ok(_) => {
            tracing::warn!(url = %url, "no .sha1 sidecar available; skipping installer hash verification");
            Ok(())
        }
        Err(e) => {
            tracing::warn!(url = %url, error = %e, ".sha1 sidecar fetch failed; skipping verification");
            Ok(())
        }
    }
}

fn unique_installer_temp_path(url: &str) -> PathBuf {
    let tag = url.rsplit('/').next().unwrap_or("installer");
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    std::env::temp_dir().join(format!(
        "rmccc-installer-{}-{}-{nanos}.jar",
        tag,
        std::process::id()
    ))
}

/// 流式计算文件 SHA-1（不整份读入内存）。
fn sha1_of_file(path: &Path) -> Result<String> {
    use sha1::{Digest, Sha1};
    use std::io::Read;
    let mut file = std::fs::File::open(path)?;
    let mut hasher = Sha1::new();
    let mut buf = [0u8; 64 * 1024];
    loop {
        let n = file.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(format!("{:x}", hasher.finalize()))
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
