//! Interactive REPL mode.
//!
//! Reuses the exact same parsing ([`crate::app`]) and dispatch
//! ([`crate::execute_line`]) as single-command mode, so behaviour is identical
//! inside and outside the shell. Tab completion is driven by the command
//! registry ([`crate::registry`]) — new commands are auto-completable with no
//! extra work.

use crate::context::CliContext;
use crate::output::Output;
use crate::registry;

use rustyline::completion::{Completer, Pair};
use rustyline::highlight::Highlighter;
use rustyline::hint::Hinter;
use rustyline::validate::Validator;
use rustyline::Helper;
use std::path::PathBuf;

struct ReplHelper;

impl Highlighter for ReplHelper {}
impl Hinter for ReplHelper {
    type Hint = String;
}
impl Validator for ReplHelper {}

impl Completer for ReplHelper {
    type Candidate = Pair;

    fn complete(
        &self,
        line: &str,
        pos: usize,
        _ctx: &rustyline::Context<'_>,
    ) -> rustyline::Result<(usize, Vec<Pair>)> {
        let partial = &line[..pos];
        let words: Vec<&str> = partial.split_whitespace().collect();
        let top = registry::command_names();

        let candidates: Vec<Pair> = if words.len() <= 1 {
            let prefix = words.first().copied().unwrap_or("");
            top.iter()
                .filter(|c| c.starts_with(prefix))
                .map(|c| Pair {
                    display: c.to_string(),
                    replacement: c.to_string(),
                })
                .collect()
        } else {
            let group = words[0];
            let prefix = words.last().copied().unwrap_or("");
            registry::subcommand_names(group)
                .iter()
                .filter(|s| s.starts_with(prefix))
                .map(|s| Pair {
                    display: s.to_string(),
                    replacement: s.to_string(),
                })
                .collect()
        };

        let start = partial
            .rfind(|c: char| c.is_whitespace())
            .map(|i| i + 1)
            .unwrap_or(0);

        Ok((start, candidates))
    }
}

impl Helper for ReplHelper {}

pub struct Repl {
    ctx: CliContext,
    out: Output,
    rl: rustyline::Editor<ReplHelper, rustyline::history::FileHistory>,
    history_path: PathBuf,
}

impl Repl {
    /// Create a REPL bound to an already-initialized context and output sink.
    pub fn new(ctx: CliContext, out: Output, data_dir: PathBuf) -> rustyline::Result<Self> {
        let mut rl = rustyline::Editor::new()?;
        rl.set_helper(Some(ReplHelper));

        let history_path = data_dir.join(".rmcli_history");
        if history_path.exists() {
            let _ = rl.load_history(&history_path);
        }

        Ok(Self {
            ctx,
            out,
            rl,
            history_path,
        })
    }

    /// Run the REPL loop. Returns a process exit code (0 on clean exit).
    pub async fn run(&mut self) -> i32 {
        self.print_welcome();

        loop {
            let prompt = "\x1b[1;36mrmcli>\x1b[0m ";
            match self.rl.readline(prompt) {
                Ok(line) => {
                    let line = line.trim().to_string();
                    if line.is_empty() {
                        continue;
                    }
                    let _ = self.rl.add_history_entry(&line);
                    if matches!(line.as_str(), "exit" | "quit" | "q") {
                        self.out.note("bye");
                        break;
                    }
                    self.handle_input(&line).await;
                }
                Err(rustyline::error::ReadlineError::Interrupted) => {
                    self.out.note("^C");
                    continue;
                }
                Err(rustyline::error::ReadlineError::Eof) => {
                    self.out.note("bye");
                    break;
                }
                Err(e) => {
                    eprintln!("read error: {e}");
                    break;
                }
            }
        }

        if let Some(parent) = self.history_path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let _ = self.rl.save_history(&self.history_path);
        0
    }

    fn print_welcome(&self) {
        let groups: Vec<String> = registry::command_names()
            .iter()
            .filter(|n| **n != "help" && **n != "completions")
            .map(|n| n.to_string())
            .collect();
        self.out.print("RMCLI — interactive mode");
        self.out.print(&format!("commands: {}", groups.join(", ")));
        self.out.print("help, exit / quit");
    }

    async fn handle_input(&self, line: &str) {
        let args = match shlex::split(line) {
            Some(args) if !args.is_empty() => args,
            _ => return,
        };

        // `help` / `?` shortcuts.
        if args.len() == 1 && matches!(args[0].as_str(), "help" | "?" | "h" | "帮助") {
            self.out.print(&registry::render_help(&[]));
            return;
        }
        if args.first().map(String::as_str) == Some("help") {
            let path: Vec<String> = args[1..].to_vec();
            self.out.print(&registry::render_help(&path));
            return;
        }

        let code = crate::execute_line(&self.ctx, &self.out, &args).await;
        let _ = code;
    }
}
