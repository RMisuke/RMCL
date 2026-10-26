//! Forge new install task — for Forge 1.13+.
//!
//! Parses the `install_profile.json` from the installer JAR, extracts
//! embedded libraries, downloads processor dependencies, and executes
//! processor steps with template variable substitution.
//!
//! # HMCL alignment
//!
//! This implements the processor-based installation pipeline from HMCL's
//! `ForgeNewInstallTask`, including:
//!
//! - Template variable resolution (`{key}`, `[artifact]`, `'literal'`)
//! - Processor output SHA-1 verification with idempotency
//! - DOWNLOAD_MOJMAPS task special handling

use crate::analyzer::LibraryType;
use crate::install_task::InstallTask;
use crate::patch::{GameInstancePatch, PRIORITY_LOADER};
use rmccc_error::{Result, RmcccError};
use std::collections::HashMap;
use std::io::Read;
use std::path::{Path, PathBuf};

use super::profile::{ForgeNewInstallProfile, Processor};

/// A single open handle on the installer ZIP (perf audit M4: the ~180 MB
/// installer was previously opened and its central directory parsed 4-6 times
/// across the Forge/NeoForge pipelines; one handle now flows through).
pub(crate) type InstallerArchive = zip::ZipArchive<std::fs::File>;

/// Open the installer ZIP once for the whole install flow.
pub(crate) fn open_installer_zip(path: &Path) -> Result<InstallerArchive> {
    let file = std::fs::File::open(path)
        .map_err(|e| RmcccError::Other(format!("Cannot open installer: {e}")))?;
    zip::ZipArchive::new(file).map_err(|e| RmcccError::Other(format!("Installer is not a valid ZIP: {e}")))
}

/// Installs Forge using the new (1.13+) processor-based method.
#[derive(Debug)]
pub struct ForgeNewInstallTask {
    pub game_version: String,
    pub forge_version: String,
    pub installer_path: PathBuf,
    /// Path to the Java binary (for running processor JARs).
    pub java_path: PathBuf,
    /// Game directory (for resolving library paths).
    pub game_dir: PathBuf,
    /// 共享 HTTP client（批次 4）：下载 processor 依赖库，不再各任务 `Client::new()`。
    pub client: reqwest::Client,
    /// 平台描述符（批次 4）：processor 的 `java -cp` 分隔符按 JVM 平台取，
    /// 不再误用文件路径分隔符 `MAIN_SEPARATOR`（Unix `/` ≠ classpath `:`）。
    pub platform: rmccc_platform::PlatformDescriptor,
}

#[async_trait::async_trait]
impl InstallTask for ForgeNewInstallTask {
    async fn execute(&mut self) -> Result<GameInstancePatch> {
        let mut archive = open_installer_zip(&self.installer_path)?;
        self.execute_with_archive(&mut archive).await
    }

    fn description(&self) -> &str {
        "Installing Forge"
    }
}

impl ForgeNewInstallTask {
    /// Run the whole install flow over one open installer archive (perf audit
    /// M4) — NeoForge reuses this with the archive it already opened for
    /// profile classification.
    pub(crate) async fn execute_with_archive(
        &self,
        archive: &mut InstallerArchive,
    ) -> Result<GameInstancePatch> {
        tracing::info!(
            game_version = %self.game_version,
            forge_version = %self.forge_version,
            "Executing Forge new install task"
        );

        // ── 1. Parse install_profile.json ──
        let profile = Self::parse_install_profile(archive)?;

        // ── 2. 铺平 processor 的运行环境（批次 4 补齐，对齐 HMCL）──
        // a) 安装器内嵌 `maven/**` 提取到 libraries/；
        // b) install_profile.libraries 里未内嵌的依赖下载落盘。
        // 此前两步都缺失：processor JAR 根本不在 classpath 路径上，
        // `--installClient` 式整体运行之外的 processors 路径必然失败。
        let libs_dir = self.game_dir.join("libraries");
        let embedded = extract_embedded_maven(archive, &self.game_dir)?;
        download_processor_libraries(&self.client, &profile.libraries, &libs_dir).await?;
        tracing::info!(
            processor_count = profile.processors.len(),
            profile_library_count = profile.libraries.len(),
            embedded_maven_entries = embedded,
            data_keys = profile.data.len(),
            "Forge install profile parsed"
        );

        // ── 3. Build template variables ──
        let mut vars =
            build_template_vars(&self.game_dir, &self.installer_path, &self.game_version);

        // ── 3b. data 表中指向**安装器内部文件**的条目解压到临时文件（HMCL
        // `NeoForgeOldInstallTask::execute` 同款）──
        // NeoForge 的 `{BINPATCH}` = `/data/client.lzma` 等是 ZIP 内路径，直接
        // 传给处理器会拿去开文件系统绝对路径 → FileNotFoundException。Forge
        // 47.4.5 走 installer-JAR 整体运行所以从未暴露；processors 自处理路径
        // 必须补齐。
        let data_temp_dir = std::env::temp_dir().join(format!(
            "rmccc-loader-data-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        std::fs::create_dir_all(&data_temp_dir)?;
        {
            for (key, entry) in &profile.data {
                let resolved = resolve_template(&entry.client, &vars, &profile.data, &libs_dir)?;
                if !resolved.starts_with('/') {
                    continue;
                }
                let inner = resolved.trim_start_matches('/');
                let Ok(mut zip_entry) = archive.by_name(inner) else {
                    continue;
                };
                let dest = data_temp_dir.join(sanitize_data_temp_name(key));
                let mut out = std::fs::File::create(&dest)?;
                std::io::copy(&mut zip_entry, &mut out)?;
                vars.insert(key.clone(), dest.display().to_string());
                tracing::debug!(data_key = %key, zip_path = %inner, "extracted installer data entry");
            }
        }

        // ── 4. Read the version JSON from the installer ──
        let version_info = Self::read_version_json(archive, &profile)?;

        // ── 5. Execute processors sequentially ──
        let client_processors: Vec<&Processor> = profile
            .processors
            .iter()
            .filter(|p| p.is_client_side())
            .collect();

        for (i, processor) in client_processors.iter().enumerate() {
            // Check if outputs already exist and are valid (idempotency)
            if self.all_outputs_valid(processor, &vars, &profile.data, &libs_dir)? {
                tracing::info!(processor = i, "Skipping (outputs already valid)");
                continue;
            }

            tracing::info!(processor = i, jar = %processor.jar, "Executing processor");

            self.execute_processor(processor, &vars, &profile.data)
                .await?;
        }

        // ── 6. Build patch from the version JSON ──
        Ok(GameInstancePatch::with_loader(
            LibraryType::Forge.patch_id(),
            &self.forge_version,
            PRIORITY_LOADER,
            Some(version_info.main_class),
            version_info.arguments,
            version_info.libraries,
        ))
    }

    /// Parse the install_profile.json from the already-open installer archive.
    fn parse_install_profile(archive: &mut InstallerArchive) -> Result<ForgeNewInstallProfile> {
        let mut profile_json = String::new();
        {
            let mut entry = archive
                .by_name("install_profile.json")
                .map_err(|e| RmcccError::Other(format!("install_profile.json not found: {e}")))?;
            entry.read_to_string(&mut profile_json)?;
        }

        serde_json::from_str(&profile_json)
            .map_err(|e| RmcccError::Other(format!("Failed to parse install_profile.json: {e}")))
    }

    /// Read the version JSON referenced by the install profile (same archive).
    fn read_version_json(
        archive: &mut InstallerArchive,
        profile: &ForgeNewInstallProfile,
    ) -> Result<rmccc_version::GameVersion> {
        // The json path may have a leading slash
        let json_path = profile.json.trim_start_matches('/');
        let mut version_json = String::new();
        {
            let mut entry = archive.by_name(json_path).map_err(|e| {
                RmcccError::Other(format!(
                    "Version JSON '{}' not found in installer: {e}",
                    profile.json
                ))
            })?;
            entry.read_to_string(&mut version_json)?;
        }

        // 安装器内的子 version JSON 常缺 `assetIndex` / `downloads` 等字段
        //（NeoForge 26.1 实测缺 assetIndex）——typed 整体解析会失败。patch 只
        // 需要 mainClass / arguments / libraries，走宽松的部分解析即可
        //（与 pipeline::resolve_version 的合并语义同源）。
        let value: serde_json::Value = serde_json::from_str(&version_json)
            .map_err(|e| RmcccError::Other(format!("Failed to parse version JSON: {e}")))?;
        let mut partial = rmccc_version::GameVersion::default();
        rmccc_version::merge_partial_json_into_parent(&mut partial, &value);
        if partial.main_class.is_empty() {
            return Err(RmcccError::Other(
                "Installer version JSON has no mainClass".to_string(),
            ));
        }
        Ok(partial)
    }

    /// Check if all processor outputs exist with correct SHA-1 hashes.
    fn all_outputs_valid(
        &self,
        processor: &Processor,
        vars: &HashMap<String, String>,
        data: &HashMap<String, super::profile::DataEntry>,
        libs_dir: &Path,
    ) -> Result<bool> {
        if processor.outputs.is_empty() {
            return Ok(false);
        }

        for (key, expected_hash) in &processor.outputs {
            let resolved_key = resolve_template(key, vars, data, libs_dir)?;
            let path = Path::new(&resolved_key);

            if !path.exists() {
                return Ok(false);
            }

            // Verify SHA-1
            let actual = sha1_file(path)?;
            if !actual.eq_ignore_ascii_case(expected_hash) {
                return Ok(false);
            }
        }

        Ok(true)
    }

    /// Execute a single processor step by running its JAR.
    async fn execute_processor(
        &self,
        processor: &Processor,
        vars: &HashMap<String, String>,
        data: &HashMap<String, super::profile::DataEntry>,
    ) -> Result<()> {
        // Build classpath
        let classpath = self.resolve_classpath(processor)?;

        // Build args
        let args: Vec<String> = processor
            .args
            .iter()
            .map(|arg| resolve_template(arg, vars, data, &self.game_dir.join("libraries")))
            .collect::<Result<Vec<_>>>()?;

        // Find the main class from the processor JAR
        let main_class = self.read_jar_main_class(&processor.jar)?;

        // Execute（classpath 分隔符经平台描述符，批次 4 修复：
        // Unix 需 `:`、Windows 需 `;`，`MAIN_SEPARATOR` 两者皆错）。
        let mut cmd = tokio::process::Command::new(&self.java_path);
        cmd.arg("-cp")
            .arg(classpath.join(self.platform.classpath_separator()))
            .arg(&main_class)
            .args(&args)
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped());

        tracing::debug!(classpath = ?classpath, args = ?args, "Running processor");

        let output = cmd
            .output()
            .await
            .map_err(|e| RmcccError::Other(format!("Failed to run processor: {e}")))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(RmcccError::Other(format!(
                "Processor {} failed: {stderr}",
                processor.jar
            )));
        }

        Ok(())
    }

    /// Resolve the classpath for a processor.
    fn resolve_classpath(&self, processor: &Processor) -> Result<Vec<String>> {
        let libs_dir = self.game_dir.join("libraries");
        let mut cp = Vec::new();

        for artifact in &processor.classpath {
            let path = maven_to_path(artifact, &libs_dir);
            cp.push(path.display().to_string());
        }

        // Add the processor JAR itself
        let jar_path = maven_to_path(&processor.jar, &libs_dir);
        cp.push(jar_path.display().to_string());

        Ok(cp)
    }

    /// Read the Main-Class attribute from a JAR's manifest.
    fn read_jar_main_class(&self, maven_coord: &str) -> Result<String> {
        let libs_dir = self.game_dir.join("libraries");
        let jar_path = maven_to_path(maven_coord, &libs_dir);

        let file = std::fs::File::open(&jar_path).map_err(|e| {
            RmcccError::Other(format!(
                "Processor JAR not found at {}: {e}",
                jar_path.display()
            ))
        })?;

        let mut archive = zip::ZipArchive::new(file)
            .map_err(|e| RmcccError::Other(format!("Cannot read processor JAR: {e}")))?;

        let mut manifest_entry = archive.by_name("META-INF/MANIFEST.MF").map_err(|e| {
            RmcccError::Other(format!(
                "META-INF/MANIFEST.MF not found in processor JAR: {e}"
            ))
        })?;

        let mut content = String::new();
        manifest_entry.read_to_string(&mut content)?;

        for line in content.lines() {
            if let Some(value) = line.strip_prefix("Main-Class: ") {
                return Ok(value.trim().to_string());
            }
        }

        Err(RmcccError::Other(format!(
            "No Main-Class found in processor JAR manifest: {maven_coord}"
        )))
    }
}

// ── Template variable resolution ──

/// Build the initial template variable map.
fn build_template_vars(
    game_dir: &Path,
    installer_path: &Path,
    game_version: &str,
) -> HashMap<String, String> {
    let mut vars = HashMap::new();
    vars.insert("SIDE".to_string(), "client".to_string());
    // MINECRAFT_JAR 指向 client jar **文件**（批次 4 修复：此前是 `versions/`
    // 目录——processors 拿到目录当 jar 用，DEOBFUSCATE 类步骤必然失败）。
    vars.insert(
        "MINECRAFT_JAR".to_string(),
        game_dir
            .join("versions")
            .join(game_version)
            .join(format!("{game_version}.jar"))
            .display()
            .to_string(),
    );
    vars.insert("ROOT".to_string(), game_dir.display().to_string());
    vars.insert(
        "INSTALLER".to_string(),
        installer_path.display().to_string(),
    );
    vars.insert(
        "LIBRARY_DIR".to_string(),
        game_dir.join("libraries").display().to_string(),
    );
    vars
}

/// Resolve a template string containing `{key}`, `[artifact]`, or `'literal'`.
///
/// 批次 4 修复：
/// - `[artifact]` 解析为 libraries 下的**实际 jar 路径**（此前原样返回
///   Maven 坐标，processor 参数拿到的是无效字符串）；
/// - `{key}` 的 data 值递归解析（data 值常为 `[artifact]` / `'literal'` /
///   嵌套 `{key}`），与 HMCL 的数据表语义一致。
fn resolve_template(
    template: &str,
    vars: &HashMap<String, String>,
    data: &HashMap<String, super::profile::DataEntry>,
    libs_dir: &Path,
) -> Result<String> {
    if template.starts_with('[') && template.ends_with(']') {
        let coord = &template[1..template.len() - 1];
        return Ok(maven_to_path(coord, libs_dir).display().to_string());
    }
    if template.starts_with('{') && template.ends_with('}') {
        let key = &template[1..template.len() - 1];
        if let Some(v) = vars.get(key) {
            return Ok(v.clone());
        }
        if let Some(entry) = data.get(key) {
            return resolve_template(&entry.client, vars, data, libs_dir);
        }
        return Err(RmcccError::Other(format!(
            "Unknown template variable: {key}"
        )));
    }
    if template.starts_with('\'') && template.ends_with('\'') {
        return Ok(template[1..template.len() - 1].to_string());
    }
    // Plain value — could contain nested templates
    let mut result = template.to_string();
    for (key, value) in vars {
        let placeholder = format!("{{{}}}", key);
        result = result.replace(&placeholder, value);
    }
    Ok(result)
}

/// 提取安装器内嵌的 `maven/**` 到 `{game_dir}/libraries/`（HMCL 同款）。
///
/// Forge 安装器把大部分 processor 依赖直接打进安装器 JAR 的 `maven/` 前缀下；
/// 不提取它们，processor classpath 就是空中楼阁。
fn extract_embedded_maven(archive: &mut InstallerArchive, game_dir: &Path) -> Result<usize> {
    let libs_dir = game_dir.join("libraries");
    let names: Vec<String> = archive
        .file_names()
        .filter(|n| n.starts_with("maven/") && !n.ends_with('/'))
        .map(|n| n.to_string())
        .collect();

    let mut extracted = 0usize;
    for name in names {
        let rel = name.trim_start_matches("maven/");
        // 防穿越：嵌入名必须落在 libraries/ 之内（与 plugin 包防护同思路）。
        if rel.is_empty() || rel.contains("..") || rel.starts_with('/') {
            tracing::warn!(entry = %name, "skipping suspicious embedded maven entry");
            continue;
        }
        let dest = libs_dir.join(rel);
        if dest.exists() {
            continue; // 幂等：上次安装/其他来源已铺好
        }
        let Ok(mut entry) = archive.by_name(&name) else {
            continue;
        };
        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let mut out = std::fs::File::create(&dest)?;
        std::io::copy(&mut entry, &mut out)?;
        extracted += 1;
    }
    Ok(extracted)
}

/// 下载 `install_profile.libraries` 中未由内嵌 `maven/` 覆盖的依赖。
///
/// 已存在的文件跳过（内嵌提取先行，或此前安装过）；带 sha1 的条目校验。
/// One processor dependency still missing after the embedded-maven pass.
struct PendingProcessorLib {
    name: String,
    url: String,
    sha1: String,
    dest: PathBuf,
}

/// Download one dependency through the mirror candidates, verifying SHA-1.
async fn fetch_processor_lib(
    client: reqwest::Client,
    item: PendingProcessorLib,
) -> Result<PendingProcessorLib> {
    let mut last_error: Option<RmcccError> = None;
    let mut downloaded: Option<Vec<u8>> = None;
    for url in mirror_candidates(&item.url) {
        match fetch_url(&client, &url).await {
            Ok(bytes) => {
                downloaded = Some(bytes);
                break;
            }
            Err(e) => {
                tracing::warn!(library = %item.name, url = %url, error = %e, "processor dependency candidate failed");
                last_error = Some(e);
            }
        }
    }
    let bytes = match downloaded {
        Some(bytes) => bytes,
        None => {
            return Err(last_error.unwrap_or_else(|| {
                RmcccError::Other(format!("no candidates for {}", item.name))
            }));
        }
    };
    if !item.sha1.is_empty() {
        use sha1::{Digest, Sha1};
        let mut hasher = Sha1::new();
        hasher.update(&bytes);
        let actual = format!("{:x}", hasher.finalize());
        if !actual.eq_ignore_ascii_case(&item.sha1) {
            return Err(RmcccError::Other(format!(
                "processor dependency {} SHA-1 mismatch (expected {}, got {})",
                item.name, item.sha1, actual
            )));
        }
    }
    if let Some(parent) = item.dest.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    tokio::fs::write(&item.dest, &bytes).await?;
    tracing::debug!(library = %item.name, "downloaded processor dependency");
    Ok(item)
}

/// 下载 `install_profile.libraries` 中未由内嵌 `maven/` 覆盖的依赖。
///
/// 已存在的文件跳过（内嵌提取先行，或此前安装过）；带 sha1 的条目校验。
/// Perf audit M1: 100+ processor deps used to download strictly serially —
/// fetch with bounded concurrency (8), one failing library fails the batch.
async fn download_processor_libraries(
    client: &reqwest::Client,
    libraries: &[rmccc_version::Library],
    libs_dir: &Path,
) -> Result<()> {
    let mut pending: Vec<PendingProcessorLib> = Vec::new();
    for lib in libraries {
        let Some(downloads) = &lib.downloads else {
            continue;
        };
        let Some(artifact) = &downloads.artifact else {
            continue;
        };
        let dest = match &artifact.path {
            Some(p) => libs_dir.join(p),
            None => maven_to_path(&lib.name, libs_dir),
        };
        if dest.exists() {
            continue;
        }
        if artifact.url.is_empty() {
            // 无 URL：期望来自内嵌提取；仍缺则 processor 启动时会给出
            // "Processor JAR not found"（带具体路径，可定位）。
            tracing::warn!(
                library = %lib.name,
                "processor dependency has no URL and was not embedded; leaving to processor startup error"
            );
            continue;
        }
        pending.push(PendingProcessorLib {
            name: lib.name.clone(),
            url: artifact.url.clone(),
            sha1: artifact.sha1.clone(),
            dest,
        });
    }
    if pending.is_empty() {
        return Ok(());
    }

    const CONCURRENCY: usize = 8;
    let shared_client = client.clone();
    let sem = std::sync::Arc::new(tokio::sync::Semaphore::new(CONCURRENCY));
    let mut handles = tokio::task::JoinSet::new();
    for item in pending {
        let permit = sem
            .clone()
            .acquire_owned()
            .await
            .map_err(|e| RmcccError::Other(format!("semaphore closed: {e}")))?;
        let client = shared_client.clone();
        handles.spawn(async move {
            let _permit = permit;
            fetch_processor_lib(client, item).await
        });
    }
    let mut first_error: Option<RmcccError> = None;
    while let Some(joined) = handles.join_next().await {
        match joined {
            Ok(Ok(_)) => {}
            Ok(Err(e)) => {
                // Keep going: the remaining downloads still finish, then the
                // first failure is reported (same all-or-nothing semantics as
                // the serial loop).
                first_error.get_or_insert(e);
            }
            Err(e) => {
                first_error
                    .get_or_insert_with(|| RmcccError::Other(format!("download join error: {e}")));
            }
        }
    }
    match first_error {
        Some(e) => Err(e),
        None => Ok(()),
    }
}

/// BMCLAPI maven 镜像根（`/maven/{maven path}`，无 `releases` 段）。
const BMCLAPI_MAVEN: &str = "https://bmclapi2.bangbang93.com/maven";

/// 原 URL 优先，紧跟 BMCLAPI maven 镜像候选（仅对已知两大 modloader maven）。
fn mirror_candidates(url: &str) -> Vec<String> {
    let mut v = vec![url.to_string()];
    if let Some(rest) = url.strip_prefix("https://maven.neoforged.net/releases/") {
        v.push(format!("{BMCLAPI_MAVEN}/{rest}"));
    } else if let Some(rest) = url.strip_prefix("https://maven.minecraftforge.net/maven/") {
        v.push(format!("{BMCLAPI_MAVEN}/{rest}"));
    }
    v
}

async fn fetch_url(client: &reqwest::Client, url: &str) -> Result<Vec<u8>> {
    let resp = client
        .get(url)
        .send()
        .await
        .map_err(|e| RmcccError::Network(e.to_string()))?;
    if !resp.status().is_success() {
        return Err(RmcccError::DownloadFailed {
            url: url.to_string(),
            reason: format!("HTTP {}", resp.status()),
        });
    }
    let bytes = resp
        .bytes()
        .await
        .map_err(|e| RmcccError::Network(e.to_string()))?;
    Ok(Vec::from(bytes))
}

// ── Utilities ──

/// data 键名 → 安全临时文件名（防穿越：只留字母数字与 `-_`）。
fn sanitize_data_temp_name(key: &str) -> String {
    let safe: String = key
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_')
        .collect();
    if safe.is_empty() {
        "data".to_string()
    } else {
        safe
    }
}

/// Convert a Maven coordinate to a filesystem path under `libs_dir`.
fn maven_to_path(maven: &str, libs_dir: &Path) -> PathBuf {
    let parts: Vec<&str> = maven.split(':').collect();
    if parts.len() < 3 {
        return libs_dir.join(maven);
    }
    let group = parts[0].replace('.', "/");
    let artifact = parts[1];
    let version = parts[2];
    let classifier = parts.get(3).copied();

    let jar_name = if let Some(cls) = classifier {
        format!("{}-{}-{}.jar", artifact, version, cls)
    } else {
        format!("{}-{}.jar", artifact, version)
    };

    libs_dir
        .join(group)
        .join(artifact)
        .join(version)
        .join(jar_name)
}

/// Compute the SHA-1 hash of a file.
fn sha1_file(path: &Path) -> Result<String> {
    use sha1::{Digest, Sha1};
    let data = std::fs::read(path)?;
    let mut hasher = Sha1::new();
    hasher.update(&data);
    Ok(format!("{:x}", hasher.finalize()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn libs() -> PathBuf {
        PathBuf::from("/game/libraries")
    }

    #[test]
    fn test_resolve_variable() {
        let vars = HashMap::from([("SIDE".to_string(), "client".to_string())]);
        let data = HashMap::new();
        assert_eq!(
            resolve_template("{SIDE}", &vars, &data, &libs()).unwrap(),
            "client"
        );
    }

    #[test]
    fn test_resolve_literal() {
        let vars = HashMap::new();
        let data = HashMap::new();
        assert_eq!(
            resolve_template("'hello world'", &vars, &data, &libs()).unwrap(),
            "hello world"
        );
    }

    /// 批次 4：`[artifact]` 必须解析成 libraries 下的实际 jar 路径
    /// （此前原样返回 Maven 坐标，processor 参数无效）。
    #[test]
    fn test_resolve_artifact_to_library_path() {
        let vars = HashMap::new();
        let data = HashMap::new();
        let result = resolve_template(
            "[net.minecraftforge:forge:1.21-49.0.50]",
            &vars,
            &data,
            &libs(),
        )
        .unwrap();
        assert_eq!(
            result,
            "/game/libraries/net/minecraftforge/forge/1.21-49.0.50/forge-1.21-49.0.50.jar"
        );
    }

    /// 批次 4：data 值（常为 `[artifact]` / 嵌套 `{key}`）必须递归解析。
    #[test]
    fn test_data_entry_resolves_recursively() {
        use super::super::profile::DataEntry;
        let vars = HashMap::from([("SIDE".to_string(), "client".to_string())]);
        let mut data = HashMap::new();
        data.insert(
            "MOJMAPS".to_string(),
            DataEntry {
                client: "[net.minecraftforge:forge:1.21-49.0.50:map_client]".to_string(),
            },
        );
        data.insert(
            "SIDE_REF".to_string(),
            DataEntry {
                client: "{SIDE}".to_string(),
            },
        );
        assert_eq!(
            resolve_template("{MOJMAPS}", &vars, &data, &libs()).unwrap(),
            "/game/libraries/net/minecraftforge/forge/1.21-49.0.50/forge-1.21-49.0.50-map_client.jar"
        );
        assert_eq!(
            resolve_template("{SIDE_REF}", &vars, &data, &libs()).unwrap(),
            "client"
        );
    }

    /// 批次 4：MINECRAFT_JAR 指向 client jar 文件（此前是 versions/ 目录）。
    #[test]
    fn test_minecraft_jar_points_at_client_jar_file() {
        let vars = build_template_vars(Path::new("/game"), Path::new("/tmp/installer.jar"), "1.21");
        assert_eq!(vars["MINECRAFT_JAR"], "/game/versions/1.21/1.21.jar");
    }

    #[test]
    fn test_maven_to_path() {
        let libs = PathBuf::from("/game/libraries");
        let path = maven_to_path("net.minecraftforge:forge:1.21-49.0.50", &libs);
        assert_eq!(
            path,
            PathBuf::from(
                "/game/libraries/net/minecraftforge/forge/1.21-49.0.50/forge-1.21-49.0.50.jar"
            )
        );
    }

    #[test]
    fn test_maven_to_path_with_classifier() {
        let libs = PathBuf::from("/game/libraries");
        let path = maven_to_path("net.minecraftforge:forge:1.21-49.0.50:installer", &libs);
        assert_eq!(
            path,
            PathBuf::from("/game/libraries/net/minecraftforge/forge/1.21-49.0.50/forge-1.21-49.0.50-installer.jar")
        );
    }
}
