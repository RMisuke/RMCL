//! RMCCC CLI binary entry point.
//!
//! All logic lives in the library crate (`rmccc_cli`) so integration / e2e
//! tests can drive the full pipeline in-process; this file only collects the
//! environment (args + terminal state) and maps the exit code.

use std::io::IsTerminal;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let stdin_is_terminal = std::io::stdin().is_terminal();
    let stdout_is_terminal = std::io::stdout().is_terminal();
    let code = rmccc_cli::run(args, stdin_is_terminal, stdout_is_terminal);
    std::process::exit(code);
}
