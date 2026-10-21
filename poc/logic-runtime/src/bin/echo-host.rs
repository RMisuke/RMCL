//! Minimal "plugin host process" used to measure spawn + stdio round-trip cost.

use std::io::{BufRead, Write};

fn main() {
    let stdin = std::io::stdin();
    let mut stdout = std::io::stdout();
    for line in stdin.lock().lines() {
        let Ok(line) = line else { break };
        writeln!(stdout, "{}", line.trim()).ok();
        stdout.flush().ok();
    }
}
