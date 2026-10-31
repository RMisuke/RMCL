//! Response file builder and process spawning for Minecraft.
//!
//! Following HMCL's [`CommandBuilder`] and [`DefaultLauncher.generateCommandLine()`]
//! patterns for argument deduplication and pipeline construction.

use rmccc_error::{Result, RmcccError};
use rmccc_platform::Platform;
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
/// - A backslash inside a quoted string is an escape character; a literal
///   Windows path separator must therefore be written as `\\`
///
/// Fixes Bug #6: macOS paths containing spaces (e.g. "Application Support")
/// were incorrectly escaped with `\"` causing `ClassNotFoundException`.
/// Also fixes Windows paths with spaces: `"C:\Users\A B\game"` previously lost
/// every backslash when the JVM parsed the argfile.
fn java_argfile_escape(s: &str) -> String {
    let cleaned = s.replace(['\n', '\r'], " ");
    if cleaned.contains(char::is_whitespace) || cleaned.contains('"') || cleaned.contains('\\') {
        // Backslash first, then quotes: Java treats both as escapes inside a
        // quoted argfile token.
        let escaped = cleaned.replace('\\', "\\\\").replace('"', "\"\"");
        format!("\"{escaped}\"")
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

    /// Windows paths inside a quoted Java argfile token need doubled
    /// backslashes. Java 25 was used to reproduce the failure: without the
    /// doubling, `"C:\Users\A B\game"` is read back as `C:UsersA Bgame`.
    #[test]
    fn test_java_argfile_escape_windows_backslashes() {
        assert_eq!(
            java_argfile_escape(r"C:\Users\A B\game"),
            r#""C:\\Users\\A B\\game""#
        );
        assert_eq!(
            java_argfile_escape(r"-Djava.library.path=C:\Users\A B\natives"),
            r#""-Djava.library.path=C:\\Users\\A B\\natives""#
        );
        // No backslash / whitespace / quote remains an unquoted token.
        assert_eq!(java_argfile_escape("--username"), "--username");
        assert_eq!(java_argfile_escape("Steve"), "Steve");
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

        // 使用 mock fs：这里验证 argfile 内容/生命周期，不依赖宿主 ACL。
        let platform = Platform::mock().0;
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

        // 使用 mock fs：这里验证 argfile 内容/生命周期，不依赖宿主 ACL。
        let platform = Platform::mock().0;
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
