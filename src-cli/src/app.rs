//! CLI argument definitions (clap derive).
//!
//! This file is the single parsing source of truth. Every command maps 1:1 to a
//! backend capability exposed on [`crate::context::CliContext`]. Command
//! *metadata* for help / completion / REPL lives in [`crate::registry`], which is
//! kept in sync with this enum (a unit test asserts the invariant).

use clap::{Parser, Subcommand};

/// RMCCC — Rust Minecraft core client launcher CLI.
#[derive(Debug, Parser)]
#[command(name = "rmcli")]
#[command(version, about = "RMCCC Minecraft launcher command-line interface", long_about = None)]
// 自定义 `help` 子命令（见 `Commands::Help`）取代 clap 内建的 help 子命令，
// 否则 clap debug 断言会以 "command name `help` is duplicated" panic。
#[command(disable_help_subcommand = true)]
pub struct Cli {
    /// Override the data directory (defaults to the platform RMCL directory).
    #[arg(short = 'd', long)]
    pub data_dir: Option<String>,

    /// Force interactive (REPL) mode.
    #[arg(short = 'i', long)]
    pub interactive: bool,

    /// Output mode: auto (JSON when not a TTY) | plain | json.
    #[arg(short = 'o', long, default_value = "auto")]
    pub output: String,

    /// Emit structured NDJSON events (progress / game output) in JSON mode.
    #[arg(long)]
    pub event_stream: bool,

    #[command(subcommand)]
    pub command: Option<Commands>,
}

/// Top-level command groups.
#[derive(Debug, Subcommand)]
pub enum Commands {
    /// Authentication: Microsoft OAuth2 / offline accounts.
    Auth {
        #[command(subcommand)]
        action: AuthAction,
    },
    /// Launcher settings, runtime state and user preferences.
    Config {
        #[command(subcommand)]
        action: ConfigAction,
    },
    /// Game instance (profile) management.
    Profile {
        #[command(subcommand)]
        action: ProfileAction,
    },
    /// Version management (remote manifest + local repository).
    Version {
        #[command(subcommand)]
        action: VersionAction,
    },
    /// Game resource install / check / repair / verify.
    Resource {
        #[command(subcommand)]
        action: ResourceAction,
    },
    /// Mod loader discovery, install and analysis.
    Modloader {
        #[command(subcommand)]
        action: ModloaderAction,
    },
    /// Mod management (Modrinth search / install / local files).
    Mod {
        #[command(subcommand)]
        action: ModAction,
    },
    /// Declarative plugin management (Phase 2, no third-party code).
    Plugin {
        #[command(subcommand)]
        action: PluginAction,
    },
    /// Launch games, observe running processes, dry-run commands.
    Launch {
        #[command(subcommand)]
        action: LaunchAction,
    },
    /// Launcher-wide diagnostics.
    System {
        #[command(subcommand)]
        action: SystemAction,
    },
    /// MCP stdio bridge: forward MCP JSON-RPC from stdin/stdout to the
    /// running launcher (ADR-013 S5). The launcher must be running.
    Mcp {
        /// Launch the GUI first and wait until the MCP endpoint is ready.
        #[arg(long)]
        launch: bool,
    },
    /// Show help for a command (REPL / script friendly).
    Help {
        /// Command path, e.g. `profile create`.
        command: Vec<String>,
    },
    /// Generate shell completions.
    Completions {
        /// Target shell.
        /// Target shell (`bash` | `zsh` | `fish` | `powershell` | `elvish`);
        /// unknown values are reported as a note rather than a usage error.
        #[arg(long)]
        shell: String,
    },
}

// ── Auth ──

#[derive(Debug, Subcommand)]
pub enum AuthAction {
    /// Log in with Microsoft OAuth2 (device flow) or an offline account.
    Login {
        /// Login type: microsoft | offline.
        #[arg(long, short, default_value = "microsoft")]
        ty: String,
        /// Username for offline login.
        #[arg(long, short)]
        username: Option<String>,
    },
    /// Log out the active account (or a specific one with --account).
    Logout {
        /// Account id to log out (default: active account).
        #[arg(long)]
        account: Option<String>,
    },
    /// Refresh an expired token.
    Refresh {
        /// Account id to refresh (default: active account).
        #[arg(long)]
        account: Option<String>,
    },
    /// Show the active session (username / provider / expiry).
    Status,
    /// List all persisted accounts (public metadata only).
    List,
    /// Select an account as active.
    Select {
        /// Account id (from `auth list`).
        #[arg(long)]
        account: String,
    },
}

// ── Config ──

#[derive(Debug, Subcommand)]
pub enum ConfigAction {
    /// Read launcher settings (`config/launcher-settings.json`).
    Get {
        /// Settings key; omit for the full document.
        key: Option<String>,
    },
    /// Update a structured settings field (whitelist + validation).
    Set {
        /// Field: data_dir | game_root_dir | default_java_path |
        /// default_min_memory_mb | default_max_memory_mb |
        /// download_concurrency | mirror_url | event_throttle_ms.
        field: String,
        /// Value (empty string clears optional fields).
        value: String,
    },
    /// Show resolved data dir / game root / paths.
    Path,
    /// Runtime state (`state/launcher-state.json`, loss-tolerant).
    State {
        #[command(subcommand)]
        action: StateAction,
    },
    /// User preferences (`user-settings.json`).
    User {
        #[command(subcommand)]
        action: UserAction,
    },
}

#[derive(Debug, Subcommand)]
pub enum StateAction {
    /// Read a state field (omit key for the full document).
    Get { key: Option<String> },
    /// Update a state field (only `selected_profile_id`).
    Set { field: String, value: String },
}

#[derive(Debug, Subcommand)]
pub enum UserAction {
    /// Read a user preference.
    Get { key: Option<String> },
    /// Update a user preference (`locale` | `check_updates`).
    Set { field: String, value: String },
}

// ── Profile ──

#[derive(Debug, Subcommand)]
pub enum ProfileAction {
    /// List all game instances.
    List,
    /// Show one instance.
    Get {
        #[arg(long)]
        id: String,
    },
    /// Create a new game instance.
    Create {
        #[arg(long, short)]
        name: String,
        /// Minecraft version (e.g. 1.20.4).
        #[arg(long, short, default_value = "1.20.4")]
        version: String,
        /// Mod loader type: fabric | forge | neoforge | quilt | ...
        #[arg(long, short)]
        loader: Option<String>,
        /// Heap memory in MB.
        #[arg(long)]
        memory: Option<u32>,
    },
    /// Update instance fields (only provided ones).
    Update {
        #[arg(long)]
        id: String,
        #[arg(long)]
        name: Option<String>,
        #[arg(long)]
        version: Option<String>,
        #[arg(long)]
        java_path: Option<String>,
        #[arg(long)]
        memory: Option<u32>,
        #[arg(long)]
        icon: Option<String>,
        #[arg(long, num_args = 1..)]
        jvm_args: Option<Vec<String>>,
        #[arg(long, num_args = 1..)]
        game_args: Option<Vec<String>>,
    },
    /// Delete an instance (optionally its game directory).
    Delete {
        #[arg(long)]
        id: String,
        /// Also delete the game directory on disk.
        #[arg(long)]
        cleanup: bool,
    },
    /// Select an instance as the active one.
    Select {
        #[arg(long)]
        id: String,
    },
    /// Show the active instance.
    Active,
}

// ── Version ──

#[derive(Debug, Subcommand)]
pub enum VersionAction {
    /// List remote Minecraft versions (Mojang manifest + offline cache).
    List {
        /// Force a fresh network fetch (ignore cache on failure).
        #[arg(long)]
        refresh: bool,
    },
    /// Show metadata for a remote version.
    Get { id: String },
    /// Install a version into a profile.
    Install {
        #[arg(long)]
        profile: String,
        /// Version id to install (defaults to the profile's game version).
        #[arg(long, short)]
        version: String,
    },
    /// Operations on locally installed versions (local repository).
    Local {
        #[command(subcommand)]
        action: LocalVersionAction,
    },
}

#[derive(Debug, Subcommand)]
pub enum LocalVersionAction {
    /// List locally installed versions.
    List,
    /// Show one local version's metadata.
    Get { id: String },
    /// Remove a local version from disk.
    Delete { id: String },
}

// ── Resource ──

#[derive(Debug, Subcommand)]
pub enum ResourceAction {
    /// Install game resources (client jar + libraries + assets + natives).
    Install {
        #[arg(long)]
        profile: String,
        /// Version id to install.
        #[arg(long, short)]
        version: String,
    },
    /// Check completeness of a profile's installed files.
    Check {
        #[arg(long)]
        profile: String,
    },
    /// Repair missing / corrupted files (auto-download).
    Repair {
        #[arg(long)]
        profile: String,
    },
    /// Verify SHA1 integrity of installed files.
    Verify {
        #[arg(long)]
        profile: String,
    },
    /// Show download engine status.
    Status,
}

// ── Modloader ──

#[derive(Debug, Subcommand)]
pub enum ModloaderAction {
    /// List available loader versions for a Minecraft version.
    List {
        /// Loader type: fabric | forge | neoforge | quilt | liteloader | optifine | cleanroom | legacyfabric.
        #[arg(long, short)]
        ty: String,
        /// Minecraft version (e.g. 1.20.4).
        #[arg(long, short = 'v')]
        version: String,
    },
    /// Install a mod loader into a profile.
    Install {
        #[arg(long, short = 'p')]
        profile: String,
        /// Loader type.
        #[arg(long, short)]
        ty: String,
        /// Loader version.
        #[arg(long, short = 'v')]
        version: String,
    },
    /// Remove the installed loader (reset to base Minecraft version).
    Remove {
        #[arg(long, short = 'p')]
        profile: String,
    },
    /// Analyze which loaders are installed in a local version.
    Analyze {
        #[arg(long, short = 'v')]
        version: String,
    },
}

// ── Mod ──

#[derive(Debug, Subcommand)]
pub enum ModAction {
    /// Search Modrinth for mods.
    Search {
        #[arg(long, short)]
        query: String,
        #[arg(long)]
        game_version: Option<String>,
        #[arg(long)]
        loader: Option<String>,
        #[arg(long)]
        page: Option<u32>,
        #[arg(long)]
        limit: Option<u32>,
    },
    /// List versions of a mod.
    Versions {
        /// Mod id / slug.
        #[arg(long)]
        id: String,
    },
    /// Install a mod into a profile (remote version or local file).
    Install {
        #[arg(long)]
        profile: String,
        /// Mod id/slug (from `mod search`).
        #[arg(long)]
        mod_id: Option<String>,
        /// Remote version id (from `mod versions`).
        #[arg(long)]
        version_id: Option<String>,
        /// Local jar file to copy in.
        #[arg(long)]
        file: Option<String>,
    },
    /// List installed mods (active + disabled).
    List {
        #[arg(long)]
        profile: String,
    },
    /// Remove an installed mod.
    Remove {
        #[arg(long)]
        profile: String,
        #[arg(long)]
        file: String,
    },
    /// Enable / disable an installed mod.
    Toggle {
        #[arg(long)]
        profile: String,
        #[arg(long)]
        file: String,
        /// `--enabled true|false` (omit to disable, matching `--enabled <bool>`).
        #[arg(long, action = clap::ArgAction::Set, default_value_t = false)]
        enabled: bool,
    },
    /// Resolve the download URL for a version.
    DownloadUrl {
        #[arg(long)]
        version_id: String,
    },
}

// ── Plugin ──

#[derive(Debug, Subcommand)]
pub enum PluginAction {
    /// List installed plugins (add --all to include removed tombstones).
    List {
        #[arg(long)]
        all: bool,
    },
    /// Show one plugin: manifest, permissions, dependencies, config.
    Inspect {
        #[arg(long)]
        id: String,
    },
    /// Install a `.rmcpkg` package (stays disabled unless --enable).
    Install {
        #[arg(long)]
        path: String,
        /// Enable immediately when no permission review is pending.
        #[arg(long)]
        enable: bool,
    },
    /// Enable an installed plugin after permission review.
    Enable {
        #[arg(long)]
        id: String,
    },
    /// Disable an enabled plugin.
    Disable {
        #[arg(long)]
        id: String,
    },
    /// Uninstall a plugin (keeps user config unless --delete-data).
    Remove {
        #[arg(long)]
        id: String,
        #[arg(long)]
        delete_data: bool,
    },
    /// Roll back to the previously installed version.
    Rollback {
        #[arg(long)]
        id: String,
    },
    /// Grant a declared capability to a plugin.
    Grant {
        #[arg(long)]
        id: String,
        #[arg(long)]
        capability: String,
    },
    /// Revoke a granted capability (effective immediately).
    Revoke {
        #[arg(long)]
        id: String,
        #[arg(long)]
        capability: String,
    },
    /// Plugin user configuration (`plugin-data/<id>/config.json`).
    Config {
        #[command(subcommand)]
        action: PluginConfigAction,
    },
    /// Show the capability catalog with risk levels.
    Capabilities,
    /// Pack a directory into a `.rmcpkg` (development helper).
    Pack {
        #[arg(long)]
        dir: String,
        #[arg(long)]
        out: String,
    },
}

#[derive(Debug, Subcommand)]
pub enum PluginConfigAction {
    /// Read the effective config (validated against config.schema.json).
    Get {
        #[arg(long)]
        id: String,
    },
    /// Write config values from a JSON object string.
    Set {
        #[arg(long)]
        id: String,
        #[arg(long)]
        values: String,
    },
    /// Reset config to schema defaults.
    Reset {
        #[arg(long)]
        id: String,
    },
    /// Show the resolved config file path.
    Path {
        #[arg(long)]
        id: String,
    },
}

// ── Launch ──

#[derive(Debug, Subcommand)]
pub enum LaunchAction {
    /// Launch a game. Blocks until exit unless --no-wait.
    Run {
        #[arg(long, short = 'p')]
        profile: String,
        /// Return immediately after spawning (use `launch wait` to block).
        #[arg(long)]
        no_wait: bool,
        /// Skip the pre-launch completeness check & repair.
        #[arg(long)]
        no_repair: bool,
    },
    /// List running game processes.
    Status,
    /// Terminate a running game.
    Stop {
        #[arg(long, short = 'p')]
        profile: String,
    },
    /// Block until a game exits.
    Wait {
        #[arg(long, short = 'p')]
        profile: String,
    },
    /// Detect Java installations.
    Java,
    /// Dry-run: print the exact command line a launch would generate.
    Command {
        #[arg(long, short = 'p')]
        profile: String,
    },
}

// ── System ──

#[derive(Debug, Subcommand)]
pub enum SystemAction {
    /// Launcher-wide status snapshot (paths / accounts / profiles / java).
    Info,
    /// Resolved paths (data dir, game root).
    Paths,
}
