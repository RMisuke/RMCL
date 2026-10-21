//! RMCCC CLI — library entry point.
//!
//! Exposing the CLI as a library (in addition to the `rmcli` binary) lets
//! integration and end-to-end tests drive the full command pipeline in-process
//! with captured output, and lets the frontend automation reuse the exact same
//! dispatch logic.
//!
//! # Architecture
//!
//! - [`app`] — clap parsing (the command surface source of truth).
//! - [`registry`] — data-driven command metadata (help / completions / REPL).
//! - [`context`] — backend facade over `rmccc_core::CoreContext` (1:1 mapping).
//! - [`commands`] — handlers; each returns a structured `serde_json::Value`.
//! - [`output`] — unified `CommandResult` envelope / plain rendering / events.
//! - [`error`] — shared `ErrorCode` + stable process exit codes.

pub mod app;
pub mod commands;
pub mod context;
pub mod error;
pub mod output;
pub mod registry;
pub mod repl;

use app::{Cli, Commands};
use clap::Parser as _;
use context::CliContext;
use error::CliResult;
use output::{Output, OutputMode};
use serde_json::Value;
use std::path::PathBuf;

pub const CLI_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Execute the CLI with the given raw arguments (e.g. `["rmcli", "profile", "list"]`).
///
/// Returns the process exit code (see [`crate::error::exit_code_for`]).
/// `stdin_is_terminal` / `stdout_is_terminal` drive interactive / output-mode
/// auto-detection and are injectable for tests.
pub fn run(raw_args: Vec<String>, stdin_is_terminal: bool, stdout_is_terminal: bool) -> i32 {
    let cli = match Cli::try_parse_from(&raw_args) {
        Ok(cli) => cli,
        Err(e) => {
            // `--help` / `--version` are not errors: clap reports them as
            // `DisplayHelp` / `DisplayVersion` with exit code 0. Render them as
            // plain text on stdout instead of wrapping them in the error
            // envelope (which would make `rmcli --version` unusable in scripts).
            if render_display_request(&e) {
                return 0;
            }
            // Respect an explicit `-o/--output` even when parsing failed.
            let mode = requested_output_mode(&raw_args).unwrap_or(OutputMode::Auto);
            let out = Output::new(mode, false, stdout_is_terminal);
            out.render_clap_error(&e);
            return e.exit_code();
        }
    };

    let output_mode = match OutputMode::parse(&cli.output) {
        Some(m) => m,
        None => {
            let out = Output::new(OutputMode::Plain, false, stdout_is_terminal);
            out.render_clap_error(&clap::Error::raw(
                clap::error::ErrorKind::InvalidValue,
                format!(
                    "invalid --output value '{}' (auto | plain | json)",
                    cli.output
                ),
            ));
            return 3;
        }
    };

    let out = Output::new(output_mode, cli.event_stream, stdout_is_terminal);

    // Resolve the data directory + initialize structured logging (shared with
    // the Tauri layer: `rmccc_config::logging`).
    let data_dir = cli
        .data_dir
        .as_deref()
        .map(PathBuf::from)
        .unwrap_or_else(rmccc_config::paths::default_data_dir);
    if let Err(e) = std::fs::create_dir_all(&data_dir) {
        out.render_error(&error::CliError::from(e));
        return 8;
    }
    let log_dir = data_dir.join("logs");
    let _ = rmccc_config::logging::init_logging(&log_dir);

    // Build the backend facade.
    let ctx = match CliContext::new(&data_dir) {
        Ok(ctx) => ctx,
        Err(e) => {
            let ce = error::CliError::from(e);
            out.render_error(&ce);
            return ce.exit_code();
        }
    };

    // Boot the tokio runtime for the command execution.
    let rt = match tokio::runtime::Runtime::new() {
        Ok(rt) => rt,
        Err(e) => {
            out.render_error(&error::CliError::from(e));
            return 8;
        }
    };

    rt.block_on(async move {
        let should_repl = cli.interactive || (cli.command.is_none() && stdin_is_terminal);
        if should_repl {
            let mut repl = match repl::Repl::new(ctx, out.clone(), data_dir) {
                Ok(r) => r,
                Err(e) => {
                    let ce = error::CliError::new(
                        rmccc_error::ErrorCode::InternalError,
                        format!("REPL initialization failed: {e}"),
                    );
                    out.render_error(&ce);
                    return ce.exit_code();
                }
            };
            repl.run().await
        } else {
            match cli.command {
                Some(cmd) => dispatch(&ctx, &out, cmd).await,
                None => {
                    out.print(&registry::render_help(&[]));
                    0
                }
            }
        }
    })
}

/// Parse one command line (used by the REPL) and dispatch it.
///
/// Global options in the line are ignored (the context / output sink are fixed
/// for the session). Returns the process exit code.
pub async fn execute_line(ctx: &CliContext, out: &Output, line_args: &[String]) -> i32 {
    let full: Vec<String> = std::iter::once("rmcli".to_string())
        .chain(line_args.iter().cloned())
        .collect();
    match Cli::try_parse_from(&full) {
        Ok(cli) => match cli.command {
            Some(cmd) => dispatch(ctx, out, cmd).await,
            None => {
                out.print(&registry::render_help(&[]));
                0
            }
        },
        Err(e) => {
            // Same as `run`: `--help` / `--version` inside a REPL line are not errors.
            if render_display_request(&e) {
                return 0;
            }
            out.render_clap_error(&e);
            e.exit_code()
        }
    }
}

/// Render clap's `--help` / `--version` requests as plain text on stdout.
///
/// Returns `true` when `err` was such a request (already rendered), `false`
/// when it is a genuine usage error that must go through the error envelope.
fn render_display_request(err: &clap::Error) -> bool {
    use clap::error::ErrorKind;
    if matches!(
        err.kind(),
        ErrorKind::DisplayHelp
            | ErrorKind::DisplayVersion
            | ErrorKind::DisplayHelpOnMissingArgumentOrSubcommand
    ) {
        print!("{err}");
        true
    } else {
        false
    }
}

/// Extract an explicit `-o/--output` value from raw args so the error path can
/// honour it even when clap parsing failed.
fn requested_output_mode(raw_args: &[String]) -> Option<OutputMode> {
    let mut iter = raw_args.iter().skip(1);
    while let Some(arg) = iter.next() {
        if let Some(value) = arg.strip_prefix("--output=") {
            return OutputMode::parse(value);
        }
        if arg == "-o" || arg == "--output" {
            return iter.next().and_then(|v| OutputMode::parse(v));
        }
    }
    None
}

/// Dispatch a parsed command, render the unified result/error envelope and
/// return the process exit code.
pub async fn dispatch(ctx: &CliContext, out: &Output, command: Commands) -> i32 {
    // Help / completions are rendered directly (no structured result).
    match &command {
        Commands::Help { command } => {
            out.print(&registry::render_help(command));
            return 0;
        }
        Commands::Completions { shell } => {
            out.print(&registry::render_completions(shell));
            return 0;
        }
        _ => {}
    }

    match run_command(ctx, out, command).await {
        Ok(value) => {
            out.finish(&value);
            0
        }
        Err(e) => {
            tracing::warn!(code = ?e.code, message = %e.message, "Command failed");
            out.render_error(&e);
            e.exit_code()
        }
    }
}

/// Route a parsed command to its handler module (the extensibility point: add
/// a new `Commands` variant + a `commands::*` handler here).
async fn run_command(ctx: &CliContext, out: &Output, command: Commands) -> CliResult<Value> {
    match command {
        Commands::Auth { action } => commands::auth::handle(ctx, out, action).await,
        Commands::Config { action } => commands::config::handle(ctx, out, action).await,
        Commands::Profile { action } => commands::profile::handle(ctx, out, action).await,
        Commands::Version { action } => commands::version::handle(ctx, out, action).await,
        Commands::Resource { action } => commands::resource::handle(ctx, out, action).await,
        Commands::Modloader { action } => commands::modloader::handle(ctx, out, action).await,
        Commands::Mod { action } => commands::mods::handle(ctx, out, action).await,
        Commands::Plugin { action } => commands::plugin::handle(ctx, out, action).await,
        Commands::Launch { action } => commands::launch::handle(ctx, out, action).await,
        Commands::System { action } => commands::system::handle(ctx, out, action).await,
        Commands::Help { .. } | Commands::Completions { .. } => {
            Ok(Value::Null) // handled by `dispatch` directly
        }
    }
}
