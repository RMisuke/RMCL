//! Shared helpers for CLI integration tests.
//!
//! Drives the CLI in-process through the library entry points
//! ([`rmccc_cli::execute_line`]) with captured output buffers, so tests do not
//! need network or a real terminal.

#![allow(dead_code)]

use rmccc_cli::context::CliContext;
use rmccc_cli::output::Output;
use std::io::Write;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

/// A `Write` sink backed by a shared buffer so tests can inspect output.
#[derive(Clone, Default)]
pub struct SharedBuf(Arc<Mutex<Vec<u8>>>);

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
    pub fn text(&self) -> String {
        String::from_utf8(self.0.lock().unwrap().clone()).unwrap()
    }
    pub fn lines(&self) -> Vec<String> {
        self.text()
            .lines()
            .map(str::to_string)
            .filter(|l| !l.trim().is_empty())
            .collect()
    }
}

/// A fully wired CLI harness: real `CliContext` on a temp data dir + a JSON or
/// plain `Output` sink with captured stdout/stderr.
pub struct Harness {
    pub _dir: PathBuf,
    pub ctx: CliContext,
    pub out: Output,
    pub stdout: SharedBuf,
    pub stderr: SharedBuf,
}

impl Harness {
    pub fn json(data_dir_tag: &str) -> Self {
        Self::new(data_dir_tag, rmccc_cli::output::OutputMode::Json)
    }

    pub fn plain(data_dir_tag: &str) -> Self {
        Self::new(data_dir_tag, rmccc_cli::output::OutputMode::Plain)
    }

    pub fn new(data_dir_tag: &str, mode: rmccc_cli::output::OutputMode) -> Self {
        let dir = temp_dir(data_dir_tag);
        let ctx = CliContext::new(&dir).expect("CliContext init");
        let stdout = SharedBuf::default();
        let stderr = SharedBuf::default();
        let out = Output::with_writers(
            mode,
            false,
            Box::new(stdout.clone()),
            Box::new(stderr.clone()),
        );
        Self {
            _dir: dir,
            ctx,
            out,
            stdout,
            stderr,
        }
    }

    /// Seed a minimal installed version in the default game root. Instance
    /// creation only registers existing versions (ADR-016), so every create
    /// test needs this.
    pub fn seed_version(&self, version: &str) {
        let dir = self._dir.join("instances").join("versions").join(version);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join(format!("{version}.json")),
            format!(
                r#"{{"id":"{version}","type":"release","mainClass":"net.minecraft.client.main.Main","assetIndex":{{"id":"","url":"","sha1":"","size":0,"totalSize":0}},"downloads":{{}},"libraries":[],"javaVersion":null}}"#
            ),
        )
        .unwrap();
    }

    /// Run one command line (e.g. `["profile", "create", "-n", "x"]`).
    pub async fn run(&self, args: &[&str]) -> i32 {
        let owned: Vec<String> = args.iter().map(|s| s.to_string()).collect();
        rmccc_cli::execute_line(&self.ctx, &self.out, &owned).await
    }

    /// Parse the JSON envelope from stdout (one line).
    pub fn last_json(&self) -> serde_json::Value {
        let text = self.stdout.text();
        let line = text.lines().last().unwrap_or_default();
        serde_json::from_str(line).expect("stdout must contain a JSON envelope line")
    }
}

pub fn temp_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "rmccc-cli-it-{tag}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}
