//! Output abstraction — the single place that decides how command results,
//! progress and events are rendered.
//!
//! Two stable output contracts:
//! - **JSON mode** (`--output json`): the final result is emitted as one
//!   [`CommandResult`] envelope `{"ok":true,"data":...}` on stdout; progress /
//!   auxiliary text goes to stderr; when `--event-stream` is set, structured
//!   NDJSON events are emitted to stdout as they occur. This is the contract
//!   the future frontend / automation layer consumes.
//! - **Plain mode** (default / `auto`): human-friendly rendering on stdout.
//!
//! `Output` is cheaply clonable and its methods take `&self`, so progress
//! callbacks (auth device-code, launch output) can share a sink.

use crate::error::CliError;
use rmccc_core::pipeline::ProgressMsg;
use serde::Serialize;
use serde_json::Value;
use std::io::Write;
use std::sync::{Arc, Mutex};

/// Output rendering mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum OutputMode {
    /// Auto-detect: JSON when stdout is not a TTY, plain otherwise.
    #[default]
    Auto,
    Plain,
    Json,
}

impl OutputMode {
    pub fn parse(s: &str) -> Option<Self> {
        match s.to_ascii_lowercase().as_str() {
            "auto" => Some(Self::Auto),
            "plain" | "text" | "human" => Some(Self::Plain),
            "json" => Some(Self::Json),
            _ => None,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Plain => "plain",
            Self::Json => "json",
        }
    }

    /// Resolve auto to a concrete mode based on whether stdout is a TTY.
    pub fn resolve(self, stdout_is_tty: bool) -> Self {
        match self {
            Self::Auto => {
                if stdout_is_tty {
                    Self::Plain
                } else {
                    Self::Json
                }
            }
            other => other,
        }
    }
}

/// The unified success envelope emitted in JSON mode.
#[derive(Debug, Serialize)]
pub struct CommandResult<T> {
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<T>,
}

/// The unified error envelope emitted in JSON mode.
#[derive(Debug, Serialize)]
pub struct CommandErrorResult {
    pub ok: bool,
    pub error: CliError,
}

/// Event envelope emitted for state-sync / progress streams.
#[derive(Debug, Serialize)]
pub struct EventEnvelope {
    pub event: String,
    pub data: Value,
}

type SharedWriter = Arc<Mutex<Box<dyn Write + Send>>>;

/// Output sink (cheaply clonable; methods take `&self`).
#[derive(Clone)]
pub struct Output {
    mode: OutputMode,
    event_stream: bool,
    stdout: SharedWriter,
    stderr: SharedWriter,
}

fn wrap(w: Box<dyn Write + Send>) -> SharedWriter {
    Arc::new(Mutex::new(w))
}

impl Output {
    /// Create an output sink targeting real stdout/stderr.
    pub fn new(mode: OutputMode, event_stream: bool, stdout_is_tty: bool) -> Self {
        let mode = mode.resolve(stdout_is_tty);
        Self {
            mode,
            event_stream,
            stdout: wrap(Box::new(std::io::stdout())),
            stderr: wrap(Box::new(std::io::stderr())),
        }
    }

    /// Create an output sink writing into provided writers (for tests).
    pub fn with_writers(
        mode: OutputMode,
        event_stream: bool,
        stdout: Box<dyn Write + Send>,
        stderr: Box<dyn Write + Send>,
    ) -> Self {
        Self {
            mode,
            event_stream,
            stdout: wrap(stdout),
            stderr: wrap(stderr),
        }
    }

    pub fn is_json(&self) -> bool {
        self.mode == OutputMode::Json
    }

    pub fn mode(&self) -> OutputMode {
        self.mode
    }

    /// Whether structured events are requested (`--event-stream`).
    pub fn event_stream(&self) -> bool {
        self.event_stream
    }

    fn writeln_out(&self, s: &str) {
        if let Ok(mut w) = self.stdout.lock() {
            let _ = writeln!(w, "{s}");
            let _ = w.flush();
        }
    }

    fn writeln_err(&self, s: &str) {
        if let Ok(mut w) = self.stderr.lock() {
            let _ = writeln!(w, "{s}");
            let _ = w.flush();
        }
    }

    /// Auxiliary human text (progress bar, status notes).
    /// Plain mode → stdout; JSON mode → stderr (keeps stdout machine-clean).
    pub fn note(&self, msg: &str) {
        if self.is_json() {
            self.writeln_err(msg);
        } else {
            self.writeln_out(msg);
        }
    }

    /// Emit a structured event (only active in JSON + event-stream mode).
    pub fn event(&self, event: &str, data: Value) {
        if self.is_json() && self.event_stream {
            if let Ok(line) = serde_json::to_string(&EventEnvelope {
                event: event.to_string(),
                data,
            }) {
                self.writeln_out(&line);
            }
        }
    }

    /// Forward a pipeline progress message to the current sink.
    pub fn progress(&self, msg: &ProgressMsg) {
        match msg {
            ProgressMsg::Stage(s) => {
                self.note(s);
            }
            ProgressMsg::Download { label, progress } => {
                let pct = progress
                    .completed_files
                    .checked_mul(100)
                    .and_then(|n| n.checked_div(progress.total_files.max(1)))
                    .unwrap_or(0);
                self.note(&format!(
                    "  {label}: {}/{} ({}%)",
                    progress.completed_files, progress.total_files, pct
                ));
                self.event(
                    "progress",
                    serde_json::json!({
                        "label": label,
                        "completed_files": progress.completed_files,
                        "total_files": progress.total_files,
                        "downloaded_bytes": progress.downloaded_bytes,
                        "total_bytes": progress.total_bytes,
                        "failed_files": progress.failed_files,
                    }),
                );
            }
            ProgressMsg::StageDone(s) => {
                self.note(s);
                self.event("stage_done", serde_json::json!({ "message": s }));
            }
        }
    }

    /// Finish a successful command: emit the unified result envelope.
    pub fn finish<T: Serialize>(&self, value: &T) {
        if self.is_json() {
            let envelope = CommandResult {
                ok: true,
                data: Some(value),
            };
            if let Ok(line) = serde_json::to_string(&envelope) {
                self.writeln_out(&line);
            }
        } else {
            let v = serde_json::to_value(value).unwrap_or(Value::Null);
            if let Ok(mut w) = self.stdout.lock() {
                render_value_plain(&mut **w, &v, 0);
                let _ = w.flush();
            }
        }
    }

    /// Render a CLI error to the sink.
    pub fn render_error(&self, err: &CliError) {
        if self.is_json() {
            let envelope = CommandErrorResult {
                ok: false,
                error: err.clone(),
            };
            if let Ok(line) = serde_json::to_string(&envelope) {
                self.writeln_out(&line);
            }
        } else {
            self.writeln_err(&format!(
                "error[{}]: {}",
                exit_code_label(err.exit_code),
                err.message
            ));
        }
    }

    /// Render a clap parse error (usage errors).
    pub fn render_clap_error(&self, err: &clap::Error) {
        let text = err.to_string();
        if self.is_json() {
            self.writeln_out(
                &serde_json::json!({
                    "ok": false,
                    "error": {
                        "code": "cli_usage_error",
                        "message": text.lines().next().unwrap_or("invalid usage"),
                        "retryable": false,
                        "exit_code": 2,
                    }
                })
                .to_string(),
            );
        } else {
            self.writeln_err(&text);
        }
    }

    /// Print a free-form string (used for help text) — always stdout.
    pub fn print(&self, text: &str) {
        self.writeln_out(text.trim_end_matches('\n'));
    }
}

/// A short label for an exit code in human output.
fn exit_code_label(code: i32) -> String {
    match code {
        3 => "validation".to_string(),
        4 => "not_found".to_string(),
        5 => "auth".to_string(),
        6 => "network".to_string(),
        7 => "java".to_string(),
        8 => "io".to_string(),
        9 => "already_running".to_string(),
        10 => "conflict".to_string(),
        _ => format!("{code}"),
    }
}

/// Heuristic plain-text renderer for structured command results.
pub fn render_value_plain(out: &mut dyn Write, value: &Value, indent: usize) {
    match value {
        Value::Null => {}
        Value::Bool(b) => {
            let _ = writeln!(out, "{}", if *b { "true" } else { "false" });
        }
        Value::Number(n) => {
            let _ = writeln!(out, "{n}");
        }
        Value::String(s) => {
            let _ = writeln!(out, "{s}");
        }
        Value::Array(items) => {
            if items.is_empty() {
                return;
            }
            if items.iter().all(|i| i.is_object()) {
                render_table(out, items);
            } else {
                for item in items {
                    render_value_plain(out, item, indent);
                }
            }
        }
        Value::Object(map) => {
            for (k, v) in map {
                match v {
                    Value::Object(_) | Value::Array(_) => {
                        let _ = writeln!(out, "{}{}:", pad(indent), k);
                        render_value_plain(out, v, indent + 2);
                    }
                    Value::Null => {
                        let _ = writeln!(out, "{}{}: -", pad(indent), k);
                    }
                    _ => {
                        let _ = writeln!(out, "{}{}: {}", pad(indent), k, scalar(v));
                    }
                }
            }
        }
    }
}

fn pad(indent: usize) -> String {
    " ".repeat(indent)
}

fn scalar(v: &Value) -> String {
    match v {
        Value::Bool(b) => b.to_string(),
        Value::Number(n) => n.to_string(),
        Value::String(s) => s.clone(),
        _ => v.to_string(),
    }
}

/// Render an array of objects as an aligned column table.
fn render_table(out: &mut dyn Write, rows: &[Value]) {
    let mut keys: Vec<String> = Vec::new();
    for row in rows {
        if let Some(map) = row.as_object() {
            for k in map.keys() {
                if !keys.iter().any(|k2| k2 == k) {
                    keys.push(k.clone());
                }
            }
        }
    }
    if keys.is_empty() {
        return;
    }

    let mut widths: Vec<usize> = keys.iter().map(|k| k.len()).collect();
    let mut cells: Vec<Vec<String>> = Vec::new();
    for row in rows {
        let map = row.as_object().cloned().unwrap_or_default();
        let mut line = Vec::new();
        for (i, k) in keys.iter().enumerate() {
            let cell = map.get(k).map(scalar).unwrap_or_default();
            widths[i] = widths[i].max(cell.chars().count());
            line.push(cell);
        }
        cells.push(line);
    }

    print_row(out, &keys, &widths);
    let total: usize = widths.iter().sum::<usize>() + (widths.len().saturating_sub(1) * 2);
    let _ = writeln!(out, "{}", "-".repeat(total));
    for line in cells {
        print_row(out, &line, &widths);
    }
}

fn print_row(out: &mut dyn Write, cells: &[String], widths: &[usize]) {
    let mut s = String::new();
    for (i, cell) in cells.iter().enumerate() {
        if i > 0 {
            s.push_str("  ");
        }
        let width = widths.get(i).copied().unwrap_or(cell.chars().count());
        s.push_str(&format!("{cell:<width$}"));
    }
    let _ = writeln!(out, "{}", s.trim_end());
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex as StdMutex;

    /// A `Write` sink backed by a shared buffer so tests can inspect output.
    #[derive(Clone, Default)]
    struct SharedBuf(Arc<StdMutex<Vec<u8>>>);

    impl Write for SharedBuf {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            self.0.lock().unwrap().extend_from_slice(buf);
            Ok(buf.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    impl SharedBuf {
        fn text(&self) -> String {
            String::from_utf8(self.0.lock().unwrap().clone()).unwrap()
        }
    }

    fn capture(mode: OutputMode, event_stream: bool) -> (Output, SharedBuf, SharedBuf) {
        let stdout = SharedBuf::default();
        let stderr = SharedBuf::default();
        let out = Output::with_writers(
            mode,
            event_stream,
            Box::new(stdout.clone()),
            Box::new(stderr.clone()),
        );
        (out, stdout, stderr)
    }

    #[test]
    fn json_finish_emits_envelope() {
        let (out, stdout, _stderr) = capture(OutputMode::Json, false);
        out.finish(&serde_json::json!({ "id": "p1", "name": "Test" }));
        let parsed: serde_json::Value = serde_json::from_str(&stdout.text()).unwrap();
        assert_eq!(parsed["ok"], true);
        assert_eq!(parsed["data"]["id"], "p1");
    }

    #[test]
    fn json_error_emits_error_envelope() {
        let (out, stdout, _stderr) = capture(OutputMode::Json, false);
        out.render_error(&CliError::new(
            rmccc_error::ErrorCode::ProfileNotFound,
            "no profile",
        ));
        let parsed: serde_json::Value = serde_json::from_str(&stdout.text()).unwrap();
        assert_eq!(parsed["ok"], false);
        assert_eq!(parsed["error"]["exit_code"], 4);
    }

    #[test]
    fn json_events_only_in_event_stream() {
        let (out, stdout, _stderr) = capture(OutputMode::Json, true);
        out.event("progress", serde_json::json!({ "n": 1 }));
        assert!(stdout.text().contains("\"event\":\"progress\""));

        let (out2, stdout2, _stderr2) = capture(OutputMode::Json, false);
        out2.event("progress", serde_json::json!({ "n": 1 }));
        assert!(stdout2.text().is_empty());
    }

    #[test]
    fn plain_note_goes_to_stdout_json_to_stderr() {
        let (out, stdout, _stderr) = capture(OutputMode::Plain, false);
        out.note("hello");
        assert!(stdout.text().contains("hello"));

        let (out2, stdout2, stderr2) = capture(OutputMode::Json, false);
        out2.note("hello");
        assert!(stdout2.text().is_empty());
        assert!(stderr2.text().contains("hello"));
    }

    #[test]
    fn auto_resolves_to_json_when_not_tty() {
        let out = Output::new(OutputMode::Auto, false, false);
        assert!(out.is_json());
        let out = Output::new(OutputMode::Auto, false, true);
        assert!(!out.is_json());
    }
}
