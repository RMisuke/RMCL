//! The generated launch command (HMCL `Command` record).
//!
//! Produced by [`crate::default_launcher::DefaultLauncher::generate_command_line`]
//! and consumed by both [`crate::default_launcher::DefaultLauncher::launch`] and
//! [`crate::default_launcher::DefaultLauncher::make_launch_script`] — the two
//! output paths share the exact same command-line generation.

use std::path::PathBuf;

/// A fully generated launch command.
#[derive(Debug, Clone)]
pub struct Command {
    /// Process-level prefix that wraps the JVM at the OS process layer:
    /// the priority prefix (Unix `nice -n N`, empty on other platforms)
    /// followed by the wrapper command tokens (`flatpak run ...`).
    ///
    /// These must never enter `jvm_args`: with the `@argfile` spawn model the
    /// JVM reads `jvm_args` from the response file, and a leading non-`-` token
    /// there is parsed as the main class (`could not find or load main class
    /// nice`). The final argv is `[prefix…, java, @argfile]`.
    pub prefix: Vec<String>,
    /// JVM arguments (including `-cp`, the classpath and `-javaagent` entries).
    pub jvm_args: Vec<String>,
    /// Game arguments (`--username ... --gameDir ...` etc.).
    pub game_args: Vec<String>,
    /// The main class.
    pub main_class: String,
    /// The directory native libraries are extracted into (HMCL `javaNativeFolder`).
    pub java_native_folder: PathBuf,
    /// Optional temporary natives path (symlinked to `java_native_folder`) used
    /// when the real path contains non-ASCII characters on Linux/macOS.
    pub temp_native_folder: Option<PathBuf>,
}
