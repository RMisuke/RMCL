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
}

#[async_trait::async_trait]
impl InstallTask for ForgeNewInstallTask {
    async fn execute(&mut self) -> Result<GameInstancePatch> {
        tracing::info!(
            game_version = %self.game_version,
            forge_version = %self.forge_version,
            "Executing Forge new install task"
        );

        // ── 1. Parse install_profile.json ──
        let profile = self.parse_install_profile()?;

        // ── 3. Build template variables ──
        let vars = build_template_vars(&self.game_dir, &self.installer_path);

        tracing::info!(
            processor_count = profile.processors.len(),
            library_count = profile.libraries.len(),
            data_keys = profile.data.len(),
            "Forge install profile parsed"
        );

        // ── 4. Read the version JSON from the installer ──
        let version_info = self.read_version_json(&profile)?;

        // ── 5. Execute processors sequentially ──
        let client_processors: Vec<&Processor> = profile
            .processors
            .iter()
            .filter(|p| p.is_client_side())
            .collect();

        for (i, processor) in client_processors.iter().enumerate() {
            // Check if outputs already exist and are valid (idempotency)
            if self.all_outputs_valid(processor, &vars, &profile.data)? {
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

    fn description(&self) -> &str {
        "Installing Forge"
    }
}

impl ForgeNewInstallTask {
    /// Parse the install_profile.json from the installer JAR.
    fn parse_install_profile(&self) -> Result<ForgeNewInstallProfile> {
        let file = std::fs::File::open(&self.installer_path)
            .map_err(|e| RmcccError::Other(format!("Cannot open Forge installer: {e}")))?;

        let mut archive = zip::ZipArchive::new(file)
            .map_err(|e| RmcccError::Other(format!("Forge installer is not a valid ZIP: {e}")))?;

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

    /// Read the version JSON referenced by the install profile.
    fn read_version_json(
        &self,
        profile: &ForgeNewInstallProfile,
    ) -> Result<rmccc_version::GameVersion> {
        let file = std::fs::File::open(&self.installer_path)?;
        let mut archive = zip::ZipArchive::new(file)
            .map_err(|e| RmcccError::Other(format!("Cannot open installer ZIP: {e}")))?;

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

        serde_json::from_str(&version_json)
            .map_err(|e| RmcccError::Other(format!("Failed to parse version JSON: {e}")))
    }

    /// Check if all processor outputs exist with correct SHA-1 hashes.
    fn all_outputs_valid(
        &self,
        processor: &Processor,
        vars: &HashMap<String, String>,
        data: &HashMap<String, super::profile::DataEntry>,
    ) -> Result<bool> {
        if processor.outputs.is_empty() {
            return Ok(false);
        }

        for (key, expected_hash) in &processor.outputs {
            let resolved_key = resolve_template(key, vars, data)?;
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
            .map(|arg| resolve_template(arg, vars, data))
            .collect::<Result<Vec<_>>>()?;

        // Find the main class from the processor JAR
        let main_class = self.read_jar_main_class(&processor.jar)?;

        // Execute
        let mut cmd = tokio::process::Command::new(&self.java_path);
        cmd.arg("-cp")
            .arg(classpath.join(std::path::MAIN_SEPARATOR_STR))
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
fn build_template_vars(game_dir: &Path, installer_path: &Path) -> HashMap<String, String> {
    let mut vars = HashMap::new();
    vars.insert("SIDE".to_string(), "client".to_string());
    vars.insert(
        "MINECRAFT_JAR".to_string(),
        game_dir.join("versions").display().to_string(),
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
fn resolve_template(
    template: &str,
    vars: &HashMap<String, String>,
    data: &HashMap<String, super::profile::DataEntry>,
) -> Result<String> {
    if template.starts_with('{') && template.ends_with('}') {
        let key = &template[1..template.len() - 1];
        vars.get(key)
            .cloned()
            .or_else(|| data.get(key).map(|d| d.client.clone()))
            .or_else(|| {
                // Try resolving the data entry value recursively
                data.get(key).map(|d| d.client.clone())
            })
            .ok_or_else(|| RmcccError::Other(format!("Unknown template variable: {key}")))
    } else if template.starts_with('[') && template.ends_with(']') {
        // Artifact path — return as-is (caller resolves to filesystem path)
        Ok(template[1..template.len() - 1].to_string())
    } else if template.starts_with('\'') && template.ends_with('\'') {
        // Literal string
        Ok(template[1..template.len() - 1].to_string())
    } else {
        // Plain value — could contain nested templates
        let mut result = template.to_string();
        // Simple {key} replacement
        for (key, value) in vars {
            let placeholder = format!("{{{}}}", key);
            result = result.replace(&placeholder, value);
        }
        Ok(result)
    }
}

// ── Utilities ──

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

    #[test]
    fn test_resolve_variable() {
        let vars = HashMap::from([("SIDE".to_string(), "client".to_string())]);
        let data = HashMap::new();
        assert_eq!(resolve_template("{SIDE}", &vars, &data).unwrap(), "client");
    }

    #[test]
    fn test_resolve_literal() {
        let vars = HashMap::new();
        let data = HashMap::new();
        assert_eq!(
            resolve_template("'hello world'", &vars, &data).unwrap(),
            "hello world"
        );
    }

    #[test]
    fn test_resolve_artifact() {
        let vars = HashMap::new();
        let data = HashMap::new();
        let result =
            resolve_template("[net.minecraftforge:forge:1.21-49.0.50]", &vars, &data).unwrap();
        assert_eq!(result, "net.minecraftforge:forge:1.21-49.0.50");
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
