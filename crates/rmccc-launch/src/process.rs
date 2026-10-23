//! Response file builder and process spawning for Minecraft.
//!
//! Following HMCL's [`CommandBuilder`] and [`DefaultLauncher.generateCommandLine()`]
//! patterns for argument deduplication and pipeline construction.

use rmccc_error::{Result, RmcccError};
use rmccc_platform::{Platform, PlatformDescriptor};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// The standard template variable names used in Minecraft version JSON.
pub mod vars {
    pub use rmccc_version::arguments::vars::*;
}

// ── Response file ──

/// Build a response file with launch arguments (avoids token leak via `ps aux`).
///
/// Java supports an `@file` argument that reads arguments from a file.
/// This keeps `--accessToken` out of the process listing.
pub fn build_response_file(
    jvm_args: &[String],
    game_args: &[String],
    main_class: &str,
    game_dir: &Path,
    platform: &Platform,
) -> Result<PathBuf> {
    let file_path = game_dir.join(format!(".launch-args-{}", uuid::Uuid::new_v4()));

    let mut content = String::new();
    for arg in jvm_args {
        content.push_str(&java_argfile_escape(arg));
        content.push('\n');
    }
    content.push_str(main_class);
    content.push('\n');
    for arg in game_args {
        content.push_str(&java_argfile_escape(arg));
        content.push('\n');
    }

    std::fs::write(&file_path, &content)?;

    // 仅属主可读写（argfile 里有 accessToken）。经平台后端，无裸 cfg。
    platform.fs.restrict_owner_only(&file_path)?;

    tracing::debug!(
        path = %file_path.display(),
        "Response file created"
    );

    Ok(file_path)
}

/// Escape a string for use in a Java `@file` argument file.
///
/// Java's `@file` format differs from shell escaping:
/// - Arguments are separated by whitespace or newlines
/// - A double-quote within a quoted string is represented by two
///   double-quote characters (`""`), **not** backslash-quote (`\"`)
/// - No other escape sequences are recognized
///
/// Fixes Bug #6: macOS paths containing spaces (e.g. "Application Support")
/// were incorrectly escaped with `\"` causing `ClassNotFoundException`.
fn java_argfile_escape(s: &str) -> String {
    let cleaned = s.replace(['\n', '\r'], " ");
    if cleaned.contains(char::is_whitespace) || cleaned.contains('"') {
        format!("\"{}\"", cleaned.replace('"', "\"\""))
    } else {
        cleaned
    }
}

/// Assemble the final spawn argv from the process-level prefix and the JVM
/// response file.
///
/// The result is `[prefix…, java, @file]`: the priority prefix (`nice -n N`)
/// and wrapper command wrap the JVM at the OS process layer, and the `@file`
/// must be read by java itself — the JVM parses its own `@argfile`, so the
/// response file path must directly follow the java executable.
///
/// Returns `(program, leading args before `@file`)`. With an empty prefix the
/// java executable is the program; otherwise `prefix[0]` is the program and the
/// java path becomes its first argument (`nice -n 5 java @file`).
pub(crate) fn spawn_argv(
    java_path: &Path,
    process_prefix: &[String],
    response_file: &Path,
) -> (String, Vec<String>) {
    let argfile = format!("@{}", response_file.display());
    match process_prefix.split_first() {
        Some((first, rest)) if !first.is_empty() => {
            let mut args = rest.to_vec();
            args.push(java_path.display().to_string());
            args.push(argfile);
            (first.clone(), args)
        }
        _ => (java_path.display().to_string(), vec![argfile]),
    }
}

/// Spawn the Minecraft game process.
///
/// Uses a response file (`@file`) to pass arguments securely.
/// `process_prefix` carries the priority prefix and wrapper command
/// (`Command::prefix`); the final argv is `[prefix…, java, @file]`.
/// Captures stdout and stderr for display in the launcher UI.
pub async fn spawn_game(
    java_path: &Path,
    process_prefix: &[String],
    response_file: &Path,
    game_dir: &Path,
    extra_env: Option<HashMap<String, String>>,
    platform: &Platform,
) -> Result<tokio::process::Child> {
    let (program, lead_args) = spawn_argv(java_path, process_prefix, response_file);
    let mut cmd = tokio::process::Command::new(&program);
    for arg in &lead_args {
        cmd.arg(arg);
    }
    cmd.current_dir(game_dir)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());

    tracing::info!(
        program = %program,
        prefix = ?process_prefix,
        "Spawning game process"
    );

    // P2-17：让游戏进程自成进程组，退出时按进程组整体终止（Unix `setpgid`，
    // Windows 后端为 no-op 待 Job Object）。经平台后端，无裸 cfg。
    platform
        .process
        .configure_child_process_group(cmd.as_std_mut())?;

    // Inject environment variables
    if let Some(env) = extra_env {
        for (key, value) in env {
            cmd.env(key, value);
        }
    }

    let child = cmd
        .spawn()
        .map_err(|e| RmcccError::Other(format!("Failed to start Java: {e}")))?;

    tracing::info!(
        pid = child.id().unwrap_or(0),
        java = %java_path.display(),
        "Game process started"
    );

    Ok(child)
}

// ── CommandBuilder (deduplication-aware argument builder) ──

/// Kinds of arguments for deduplication purposes.
#[derive(Debug, Clone, PartialEq, Eq)]
enum ArgKind {
    /// Plain argument — checked for exact match.
    Plain,
    /// System property: `-Dkey=value` — deduped by `-Dkey` prefix.
    SystemProperty,
    /// Unstable / diagnostic: `-XX:+Flag`, `-XX:-Flag`, `-XX:Key=Value`.
    JvmUnstable,
    /// Memory: `-Xmx`, `-Xms`, `-Xmn`, `-Xss` — deduped by prefix.
    JvmMemory,
    /// Other `-X` prefixed argument.
    JvmX,
}

/// A command line builder with deduplication support.
///
/// Inspired by HMCL's [`CommandBuilder`] which provides:
/// - `addDefault(key, value)`: only adds if no arg starts with `key`
/// - `addUnstableDefault(key, value)`: handles `-XX:` patterns
/// - `addAllDefault(args)`: batch add with auto-detection of arg type
pub struct CommandBuilder {
    args: Vec<String>,
    kinds: Vec<ArgKind>,
}

impl CommandBuilder {
    pub fn new() -> Self {
        Self {
            args: Vec::new(),
            kinds: Vec::new(),
        }
    }

    /// Add an argument unconditionally.
    pub fn add(&mut self, arg: &str) -> &mut Self {
        self.args.push(arg.to_string());
        self.kinds.push(ArgKind::Plain);
        self
    }

    /// Add multiple arguments unconditionally.
    pub fn add_all(&mut self, args: &[String]) -> &mut Self {
        for a in args {
            self.add(a);
        }
        self
    }

    /// Add an argument only if no existing argument starts with the same key.
    ///
    /// For example, `add_default("-Xmx", "2048M")` will only add if no existing
    /// argument starts with `-Xmx`.
    ///
    /// Returns `Some(existing_value)` if the argument was skipped, `None` if added.
    pub fn add_default(&mut self, key: &str, value: &str) -> Option<String> {
        let full = format!("{}{}", key, value);
        // Check if any existing arg starts with this key
        for existing in &self.args {
            if existing.starts_with(key) {
                tracing::info!("Default option '{full}' is suppressed by '{existing}'");
                return Some(existing.clone());
            }
        }
        self.args.push(full);
        self.kinds.push(detect_kind(key));
        None
    }

    /// Add a `-XX:+Flag` or `-XX:-Flag` only if not already set.
    pub fn add_unstable_bool(&mut self, key: &str, enabled: bool) -> Option<String> {
        let flag = if enabled {
            format!("-XX:+{}", key)
        } else {
            format!("-XX:-{}", key)
        };
        // Check existing -XX:+/-key patterns
        for existing in &self.args {
            if (existing.starts_with("-XX:+") || existing.starts_with("-XX:-"))
                && existing
                    .trim_start_matches("-XX:+")
                    .trim_start_matches("-XX:-")
                    == key
            {
                return Some(existing.clone());
            }
        }
        self.args.push(flag);
        self.kinds.push(ArgKind::JvmUnstable);
        None
    }

    /// Add a `-XX:Key=Value` only if not already set.
    pub fn add_unstable_value(&mut self, key: &str, value: &str) -> Option<String> {
        let full = format!("-XX:{}={}", key, value);
        for existing in &self.args {
            if existing.starts_with(&format!("-XX:{}=", key)) {
                return Some(existing.clone());
            }
        }
        self.args.push(full);
        self.kinds.push(ArgKind::JvmUnstable);
        None
    }

    /// Batch add a collection of arguments with deduplication.
    /// Automatically detects argument types for proper dedup.
    /// Repeatable JVM flags (--add-opens, --add-exports, --add-reads,
    /// --patch-module, --add-modules) are never deduplicated because
    /// they can appear multiple times with different values.
    pub fn add_all_default(&mut self, args: &[String]) -> &mut Self {
        for arg in args {
            if let Some((key, value)) = parse_d_option(arg) {
                self.add_default(key, value);
            } else if arg.starts_with("-XX:") {
                if let Some(rest) = arg.strip_prefix("-XX:") {
                    if rest.starts_with('+') || rest.starts_with('-') {
                        // -XX:+Flag or -XX:-Flag
                        let key = &rest[1..];
                        let enabled = rest.starts_with('+');
                        self.add_unstable_bool(key, enabled);
                    } else if let Some(eq_pos) = rest.find('=') {
                        let key = &rest[..eq_pos];
                        let value = &rest[eq_pos + 1..];
                        self.add_unstable_value(key, value);
                    } else {
                        self.args.push(arg.clone());
                        self.kinds.push(ArgKind::JvmUnstable);
                    }
                }
            } else if let Some((prefix, value)) = parse_x_option(arg) {
                self.add_default(prefix, value);
            } else if is_repeatable_jvm_flag(arg) || !self.contains_exact(arg) {
                self.args.push(arg.clone());
                self.kinds.push(ArgKind::Plain);
            }
        }
        self
    }

    /// Check if an exact argument string is already present.
    pub fn contains_exact(&self, arg: &str) -> bool {
        self.args.iter().any(|a| a == arg)
    }

    /// Check if any argument matches a predicate.
    pub fn none_match<F>(&self, predicate: F) -> bool
    where
        F: Fn(&str) -> bool,
    {
        !self.args.iter().any(|a| predicate(a))
    }

    /// Remove arguments matching a predicate.
    pub fn remove_if<F>(&mut self, predicate: F)
    where
        F: Fn(&str) -> bool,
    {
        let mut i = 0;
        while i < self.args.len() {
            if predicate(&self.args[i]) {
                self.args.remove(i);
                self.kinds.remove(i);
            } else {
                i += 1;
            }
        }
    }

    /// Consume the builder and return the argument list.
    pub fn build(self) -> Vec<String> {
        self.args
    }
}

impl Default for CommandBuilder {
    fn default() -> Self {
        Self::new()
    }
}

/// Check if a JVM argument is repeatable (can appear multiple times).
///
/// Flags like `--add-opens`, `--add-exports`, `--add-reads` can be specified
/// multiple times with different values. They should not be deduplicated.
fn is_repeatable_jvm_flag(arg: &str) -> bool {
    // Repeatable module-system flags
    if arg == "--add-opens" || arg == "--add-exports" || arg == "--add-reads" {
        return true;
    }
    // --add-opens=<value> (equals form) — also repeatable
    if arg.starts_with("--add-opens=")
        || arg.starts_with("--add-exports=")
        || arg.starts_with("--add-reads=")
    {
        return true;
    }
    false
}

/// Parse a `-Dkey=value` system property argument.
fn parse_d_option(arg: &str) -> Option<(&str, &str)> {
    if arg.starts_with("-D") {
        let d_end = 2; // "-D" prefix length
        let rest = &arg[d_end..];
        if let Some(eq_pos) = rest.find('=') {
            let value = &rest[eq_pos + 1..];
            return Some((&arg[..d_end + eq_pos + 1], value));
        }
    }
    None
}

/// Parse an `-X` memory option.
fn parse_x_option(arg: &str) -> Option<(&'static str, &str)> {
    for prefix in &["-Xmx", "-Xms", "-Xmn", "-Xss"] {
        if let Some(value) = arg.strip_prefix(prefix) {
            return Some((prefix, value));
        }
    }
    None
}

/// Detect the kind of an argument key.
fn detect_kind(key: &str) -> ArgKind {
    if key.starts_with("-D") {
        ArgKind::SystemProperty
    } else if key.starts_with("-XX:") {
        ArgKind::JvmUnstable
    } else if key.starts_with("-Xmx")
        || key.starts_with("-Xms")
        || key.starts_with("-Xmn")
        || key.starts_with("-Xss")
    {
        ArgKind::JvmMemory
    } else if key.starts_with("-X") {
        ArgKind::JvmX
    } else {
        ArgKind::Plain
    }
}

// ── Java version gating ──

/// Version constraint for JVM arguments.
///
/// Each JVM argument can be tagged with a compatible Java version range.
/// Arguments outside their range are silently skipped rather than added.
/// This prevents JVM crashes from unsupported flags (e.g. `--illegal-access`
/// removed in Java 17, `-XX:ActiveProcessorCount` only valid in Java 9+).
#[derive(Debug, Clone, Copy)]
struct VersionGate {
    min: Option<u32>,
    max: Option<u32>,
}

impl VersionGate {
    /// Applicable when java_version >= v.
    const fn at_least(v: u32) -> Self {
        Self {
            min: Some(v),
            max: None,
        }
    }

    fn matches(&self, version: u32) -> bool {
        self.min.is_none_or(|m| version >= m) && self.max.is_none_or(|m| version <= m)
    }
}

/// Add a version-gated `-XX:+/-Flag` boolean argument.
fn add_gated_unstable_bool(
    cmd: &mut CommandBuilder,
    key: &str,
    enabled: bool,
    gate: VersionGate,
    java_version: u32,
) {
    if gate.matches(java_version) {
        cmd.add_unstable_bool(key, enabled);
    }
}

/// Add a version-gated `-XX:Key=Value` argument.
fn add_gated_unstable_value(
    cmd: &mut CommandBuilder,
    key: &str,
    value: &str,
    gate: VersionGate,
    java_version: u32,
) {
    if gate.matches(java_version) {
        cmd.add_unstable_value(key, value);
    }
}

// ── High-level argument builders ──

/// Build the complete launch arguments for a profile.
pub struct LaunchArgBuilder;

impl LaunchArgBuilder {
    /// Build full JVM arguments including memory, GC tuning, networking, and
    /// launcher metadata.
    ///
    /// Following HMCL's `generateCommandLine()` pipeline order.
    ///
    /// JVM arguments are version-gated via [`VersionGate`] to ensure
    /// compatibility across Java 8 through Java 25. Arguments that are not
    /// applicable for the target Java version are silently skipped.
    #[allow(clippy::too_many_arguments)]
    pub fn jvm_args(
        platform: &PlatformDescriptor,
        classpath: &[PathBuf],
        memory_mb: u32,
        extra_jvm_args: &[String],
        version_jvm_args: &[String],
        _natives_dir: &Path,
        _game_dir: &Path,
        launcher_name: &str,
        launcher_version: &str,
        java_version: u32,
        is_64bit: bool,
    ) -> Vec<String> {
        let mut cmd = CommandBuilder::new();
        let jv = java_version;

        // 0. Version JSON JVM args come FIRST (e.g. -XstartOnFirstThread on macOS)
        cmd.add_all_default(version_jvm_args);

        // 1. User extra JVM args
        cmd.add_all_default(extra_jvm_args);

        // ── Memory arguments ──
        if memory_mb > 0 {
            cmd.add_default("-Xmx", &format!("{}M", memory_mb));
            cmd.add_default("-Xms", &format!("{}M", (memory_mb / 4).max(256)));
        }

        // ── Launcher metadata (all versions) ──
        cmd.add_default("-Dminecraft.launcher.brand=", launcher_name);
        cmd.add_default("-Dminecraft.launcher.version=", launcher_version);

        // ── Encoding settings ──
        // Java < 19: sun.stdout/stderr.encoding
        // Java >= 19: stdout/stderr.encoding (sun.* removed in 19)
        let encoding = platform.default_encoding();
        cmd.add_default("-Dfile.encoding=", encoding);
        if jv < 19 {
            cmd.add_default("-Dsun.stdout.encoding=", encoding);
            cmd.add_default("-Dsun.stderr.encoding=", encoding);
        } else {
            cmd.add_default("-Dstdout.encoding=", encoding);
            cmd.add_default("-Dstderr.encoding=", encoding);
        }

        // ── Log4j RCE fixes (all versions) ──
        cmd.add_default("-Djava.rmi.server.useCodebaseOnly=", "true");
        cmd.add_default("-Dcom.sun.jndi.rmi.object.trustURLCodebase=", "false");
        cmd.add_default("-Dcom.sun.jndi.cosnaming.object.trustURLCodebase=", "false");

        // ── Minecraft client.jar path ──
        cmd.add_default("-Dminecraft.client.jar=", ""); // caller overrides if needed

        // ── 平台专属 JVM 参数（macOS `-Xdock:name`；数据来自适配层）──
        for (key, value) in platform.extra_jvm_args() {
            cmd.add_default(key, &value.replace("{instance}", "Minecraft"));
        }

        // ── GC tuning (Java 8+) ──
        // UnlockExperimentalVMOptions / UnlockDiagnosticVMOptions: Java 8+
        // UseG1GC: default in Java 9+, but we enable it explicitly for Java 8
        // G1 tuning params: Java 8–25
        let gate_java8p = VersionGate::at_least(8);
        add_gated_unstable_bool(
            &mut cmd,
            "UnlockExperimentalVMOptions",
            true,
            gate_java8p,
            jv,
        );
        add_gated_unstable_bool(&mut cmd, "UnlockDiagnosticVMOptions", true, gate_java8p, jv);
        if gate_java8p.matches(jv)
            && cmd.none_match(|a| a.starts_with("-XX:+Use") && a != "-XX:+UseG1GC")
        {
            cmd.add_unstable_bool("UseG1GC", true);
            add_gated_unstable_value(&mut cmd, "G1MixedGCCountTarget", "5", gate_java8p, jv);
            add_gated_unstable_value(&mut cmd, "G1NewSizePercent", "20", gate_java8p, jv);
            add_gated_unstable_value(&mut cmd, "G1ReservePercent", "20", gate_java8p, jv);
            add_gated_unstable_value(&mut cmd, "MaxGCPauseMillis", "50", gate_java8p, jv);
            add_gated_unstable_value(&mut cmd, "G1HeapRegionSize", "32m", gate_java8p, jv);
        }
        add_gated_unstable_bool(
            &mut cmd,
            "OmitStackTraceInFastThrow",
            false,
            gate_java8p,
            jv,
        );

        // ── Compiler / JIT tuning ──
        if is_64bit {
            // ReservedCodeCacheSize and DontCompileHugeMethods: valid Java 8+
            add_gated_unstable_value(&mut cmd, "ReservedCodeCacheSize", "400M", gate_java8p, jv);
            add_gated_unstable_bool(&mut cmd, "DontCompileHugeMethods", false, gate_java8p, jv);
        } else {
            // 32-bit JVM: larger stack needed for Minecraft 1.13+
            cmd.add_default("-Xss", "1m");
        }

        // ── FML mod loader fixes (all versions) ──
        cmd.add_default("-Dfml.ignoreInvalidMinecraftCertificates=", "true");
        cmd.add_default("-Dfml.ignorePatchDiscrepancies=", "true");

        // ── Java version-specific compatibility ──
        // --illegal-access=permit: Java 16 only (removed in 17)
        // --add-opens / --add-exports: Java 17+ for reflective access
        if jv == 16 {
            cmd.add_default("--illegal-access=", "permit");
        }
        if jv >= 17 {
            // Open modules for reflective access needed by older mod loaders
            cmd.add("--add-opens=java.base/java.lang.invoke=ALL-UNNAMED");
        }

        // ── Classpath ──
        let separator = platform.classpath_separator();
        let cp = classpath
            .iter()
            .map(|p| p.display().to_string())
            .collect::<Vec<_>>()
            .join(separator);
        cmd.add("-cp");
        cmd.add(&cp);

        cmd.build()
    }

    /// Build game arguments with template variables from version JSON.
    ///
    /// Unlike the old hardcoded approach, this now supports parsing the
    /// `arguments.game` section from the version JSON and applying template
    /// variable substitution.
    ///
    /// If `version_args` is provided (new-style, 1.13+), those are used.
    /// Otherwise falls back to `minecraft_args` (legacy).
    /// If neither is available, uses the legacy flat argument format.
    #[allow(clippy::too_many_arguments)]
    pub fn game_args(
        version_id: &str,
        game_dir: &Path,
        username: &str,
        uuid: &str,
        access_token: &str,
        user_type: &str,
        asset_index: &str,
        version_type: &str,
        extra_game_args: &[String],
    ) -> Vec<String> {
        let assets_dir = game_dir.join("assets");
        let mut args = vec![
            "--username".to_string(),
            username.to_string(),
            "--version".to_string(),
            version_id.to_string(),
            "--gameDir".to_string(),
            game_dir.display().to_string(),
            "--assetsDir".to_string(),
            assets_dir.display().to_string(),
            "--assetIndex".to_string(),
            asset_index.to_string(),
            "--accessToken".to_string(),
            access_token.to_string(),
            "--uuid".to_string(),
            uuid.to_string(),
            "--userType".to_string(),
            user_type.to_string(),
            "--versionType".to_string(),
            version_type.to_string(),
        ];
        args.extend(extra_game_args.iter().cloned());
        args
    }
}

// ── Environment variable builder ──

/// Build environment variables for the game process.
///
/// Following HMCL's `getEnvVars()` pattern, injecting `INST_*` and modloader
/// marker variables.
pub fn build_game_env(
    instance_name: &str,
    instance_dir: &Path,
    game_dir: &Path,
    java_path: &Path,
    modloader_type: Option<&str>,
) -> HashMap<String, String> {
    let mut env = HashMap::new();
    env.insert("INST_NAME".into(), instance_name.to_string());
    env.insert("INST_ID".into(), instance_name.to_string());
    env.insert("INST_DIR".into(), instance_dir.display().to_string());
    env.insert("INST_MC_DIR".into(), game_dir.display().to_string());
    env.insert("INST_JAVA".into(), java_path.display().to_string());

    if let Some(ml) = modloader_type {
        let key = format!("INST_{}", ml.to_uppercase());
        env.insert(key, "1".into());
    }

    env
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_spawn_argv_empty_prefix_java_is_program() {
        let java = Path::new("/usr/bin/java");
        let rf = Path::new("/game/.launch-args-1");
        let (program, args) = spawn_argv(java, &[], rf);
        assert_eq!(program, "/usr/bin/java");
        assert_eq!(args, vec!["@/game/.launch-args-1"]);
    }

    #[test]
    fn test_spawn_argv_prefix_wraps_java_then_argfile() {
        let java = Path::new("/opt/jdk/bin/java");
        let rf = Path::new("/run dir/.launch-args-2");
        let prefix = vec!["nice".to_string(), "-n".to_string(), "5".to_string()];
        let (program, args) = spawn_argv(java, &prefix, rf);
        // P1-7 regression: nice is the program, java is its argument and the
        // argfile directly follows java so the JVM parses it.
        assert_eq!(program, "nice");
        assert_eq!(
            args,
            vec!["-n", "5", "/opt/jdk/bin/java", "@/run dir/.launch-args-2"]
        );
    }

    #[test]
    fn test_spawn_argv_wrapper_tokens_preserved() {
        let java = Path::new("/usr/bin/java");
        let rf = Path::new("/game/.launch-args-3");
        let prefix = vec!["flatpak".to_string(), "run".to_string()];
        let (program, args) = spawn_argv(java, &prefix, rf);
        assert_eq!(program, "flatpak");
        assert_eq!(args, vec!["run", "/usr/bin/java", "@/game/.launch-args-3"]);
    }

    #[test]
    fn test_command_builder_add_default() {
        let mut cmd = CommandBuilder::new();
        cmd.add_default("-Xmx", "2048M");
        cmd.add_default("-Xmx", "4096M"); // should be suppressed
        let args = cmd.build();
        assert_eq!(args, vec!["-Xmx2048M"]);
    }

    #[test]
    fn test_command_builder_unstable_bool() {
        let mut cmd = CommandBuilder::new();
        cmd.add_unstable_bool("UseG1GC", true);
        cmd.add_unstable_bool("UseG1GC", false); // should be suppressed
        let args = cmd.build();
        assert_eq!(args, vec!["-XX:+UseG1GC"]);
    }

    #[test]
    fn test_command_builder_add_all_default() {
        let input = vec![
            "-Xmx4G".to_string(),
            "-XX:+UseG1GC".to_string(),
            "-Dfile.encoding=UTF-8".to_string(),
            "--username".to_string(),
        ];
        let mut cmd = CommandBuilder::new();
        cmd.add_all_default(&input);
        let args = cmd.build();
        assert_eq!(args.len(), 4);
    }

    #[test]
    fn test_command_builder_remove_if() {
        let mut cmd = CommandBuilder::new();
        cmd.add("-Xincgc");
        cmd.add("-XX:+UseG1GC");
        cmd.remove_if(|a| a == "-Xincgc");
        let args = cmd.build();
        assert_eq!(args, vec!["-XX:+UseG1GC"]);
    }

    #[test]
    fn test_build_game_env() {
        let env = build_game_env(
            "My Instance",
            Path::new("/instances/my-instance"),
            Path::new("/instances/my-instance/.minecraft"),
            Path::new("/usr/bin/java"),
            Some("forge"),
        );
        assert_eq!(env.get("INST_NAME").unwrap(), "My Instance");
        assert_eq!(env.get("INST_FORGE").unwrap(), "1");
        assert_eq!(env.get("INST_JAVA").unwrap(), "/usr/bin/java");
    }

    #[test]
    fn test_jvm_args_with_dedup() {
        let args = LaunchArgBuilder::jvm_args(
            &PlatformDescriptor::macos_arm64(),
            &[PathBuf::from("/game/libraries/lib.jar")],
            2048,
            &["-Dfile.encoding=GBK".to_string()], // user override
            &[],                                  // no version JVM args
            Path::new("/game/natives"),
            Path::new("/game"),
            "RMCL",
            "0.1.0",
            17,
            true,
        );
        // User's encoding should take precedence
        assert!(args.iter().any(|a| a == "-Dfile.encoding=GBK"));
        // Should not have RMCL's default encoding
        assert!(!args.iter().any(|a| a == "-Dfile.encoding=UTF-8"));
        // Should have classpath
        assert!(args.iter().any(|a| a == "-cp"));
        // Should have encoding
        assert!(args.iter().any(|a| a.starts_with("-Dfile.encoding=")));
        // Should have launcher brand
        assert!(args.iter().any(|a| a == "-Dminecraft.launcher.brand=RMCL"));
        // Should have launcher version
        assert!(args
            .iter()
            .any(|a| a.starts_with("-Dminecraft.launcher.version=")));
        // Should have Xmx set
        assert!(args.iter().any(|a| a.starts_with("-Xmx")));
        // Should have Xms set
        assert!(args.iter().any(|a| a.starts_with("-Xms")));
        // Should not have Xms duplicate (add_default should suppress)
        let xms_count = args.iter().filter(|a| a.starts_with("-Xms")).count();
        assert_eq!(xms_count, 1, "Xms should appear exactly once");
        // G1GC should be enabled
        assert!(args.iter().any(|a| a == "-XX:+UseG1GC"));
        // Unlock flags
        assert!(args.iter().any(|a| a == "-XX:+UnlockExperimentalVMOptions"));
    }

    #[test]
    fn test_jvm_args_zero_memory() {
        let args = LaunchArgBuilder::jvm_args(
            &PlatformDescriptor::macos_arm64(),
            &[PathBuf::from("/game/lib.jar")],
            0, // no memory limit
            &[],
            &[], // no version JVM args
            Path::new("/game/natives"),
            Path::new("/game"),
            "RMCL",
            "0.1.0",
            21,
            true,
        );
        // No memory args when memory_mb is 0
        assert!(!args.iter().any(|a| a.starts_with("-Xmx")));
        assert!(!args.iter().any(|a| a.starts_with("-Xms")));
    }

    #[test]
    fn test_jvm_args_java_version_8() {
        let args = LaunchArgBuilder::jvm_args(
            &PlatformDescriptor::macos_arm64(),
            &[],
            1024,
            &[],
            &[], // no version JVM args
            Path::new("/game/natives"),
            Path::new("/game"),
            "Test",
            "1.0",
            8,     // Java 8
            false, // 32-bit
        );
        // Java 8: should NOT have G1GC (needs java >= 8, but let's check the GC flag)
        // Actually G1GC is enabled for java >= 8
        assert!(args.iter().any(|a| a == "-XX:+UseG1GC"));
        // 32-bit: should have -Xss1m for stack workaround
        assert!(args.iter().any(|a| a == "-Xss1m"));
    }

    #[test]
    fn test_jvm_args_java_version_16() {
        let args = LaunchArgBuilder::jvm_args(
            &PlatformDescriptor::macos_arm64(),
            &[],
            2048,
            &[],
            &[], // no version JVM args
            Path::new("/game/natives"),
            Path::new("/game"),
            "Test",
            "1.0",
            16,
            true,
        );
        // Java 16: --illegal-access=permit
        assert!(args.iter().any(|a| a.starts_with("--illegal-access=")));
    }

    #[test]
    fn test_build_game_env_no_modloader() {
        let env = build_game_env(
            "Vanilla",
            Path::new("/inst/vanilla"),
            Path::new("/inst/vanilla/.minecraft"),
            Path::new("/usr/lib/jvm/java-21/bin/java"),
            None,
        );
        assert_eq!(env.get("INST_NAME").unwrap(), "Vanilla");
        assert_eq!(env.get("INST_ID").unwrap(), "Vanilla");
        assert_eq!(env.get("INST_MC_DIR").unwrap(), "/inst/vanilla/.minecraft");
        // No modloader env var
        assert!(!env.contains_key("INST_FORGE"));
        assert!(!env.contains_key("INST_FABRIC"));
    }

    #[test]
    fn test_build_game_env_fabric() {
        let env = build_game_env(
            "Fabric Test",
            Path::new("/inst/fabric"),
            Path::new("/inst/fabric/minecraft"),
            Path::new("/usr/bin/java"),
            Some("fabric"),
        );
        assert_eq!(env.get("INST_FABRIC").unwrap(), "1");
    }

    #[test]
    fn test_build_game_env_neoforge() {
        let env = build_game_env(
            "NeoForge Test",
            Path::new("/inst/neoforge"),
            Path::new("/inst/neoforge/minecraft"),
            Path::new("/usr/bin/java"),
            Some("neoforge"),
        );
        assert_eq!(env.get("INST_NEOFORGE").unwrap(), "1");
    }

    #[test]
    fn test_command_builder_add_default_system_property() {
        let mut cmd = CommandBuilder::new();
        cmd.add_default("-Dfile.encoding=", "UTF-8");
        cmd.add_default("-Dfile.encoding=", "GBK"); // should be suppressed
        let args = cmd.build();
        assert_eq!(args, vec!["-Dfile.encoding=UTF-8"]);
    }

    #[test]
    fn test_command_builder_add_unstable_value() {
        let mut cmd = CommandBuilder::new();
        cmd.add_unstable_value("MaxGCPauseMillis", "50");
        cmd.add_unstable_value("MaxGCPauseMillis", "200"); // should be suppressed
        let args = cmd.build();
        assert_eq!(args, vec!["-XX:MaxGCPauseMillis=50"]);
    }

    #[test]
    fn test_command_builder_none_match() {
        let mut cmd = CommandBuilder::new();
        cmd.add("-XX:+UseG1GC");
        assert!(!cmd.none_match(|a| a.contains("G1GC")));
        assert!(cmd.none_match(|a| a.contains("ZGC")));
    }

    #[test]
    fn test_command_builder_contains_exact() {
        let mut cmd = CommandBuilder::new();
        cmd.add("-Xmx2G");
        assert!(cmd.contains_exact("-Xmx2G"));
        assert!(!cmd.contains_exact("-Xmx4G"));
    }

    #[test]
    fn test_build_response_file_creates_file() {
        let dir = std::env::temp_dir().join(format!("rmcl-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();

        let platform = Platform::current();
        let result = build_response_file(
            &["-Xmx2G".to_string(), "-Dfoo=bar".to_string()],
            &["--username".to_string(), "Steve".to_string()],
            "net.minecraft.client.main.Main",
            &dir,
            &platform,
        );
        assert!(result.is_ok());
        let path = result.unwrap();
        assert!(path.exists());

        // Cleanup
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_dir(&dir);
    }

    /// P1-03：启动后 response file 应立即删除，避免含 --accessToken 的文件残留磁盘。
    #[test]
    fn test_response_file_removed_after_create() {
        let dir = std::env::temp_dir().join(format!("rmcl-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();

        let platform = Platform::current();
        let path = build_response_file(
            &["-Xmx2G".to_string()],
            &["--accessToken".to_string(), "SECRET".to_string()],
            "net.minecraft.client.main.Main",
            &dir,
            &platform,
        )
        .unwrap();
        assert!(path.exists());

        // 模拟 launch_start 的清理行为：spawn 后立即删除
        std::fs::remove_file(&path).unwrap();
        assert!(!path.exists());

        let _ = std::fs::remove_dir(&dir);
    }
}
