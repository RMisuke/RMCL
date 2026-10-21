//! Supervisor for the logic helper process.
//!
//! Responsibilities:
//!
//! - resolve and spawn `rmccc-logic-host` (env override → next to the launcher);
//! - frame requests/responses as one JSON object per line;
//! - enforce a wall-clock timeout per call, then **kill** the helper (a WASM
//!   module cannot be trusted to cooperate with cooperative fuel limits);
//! - restart transparently for the next call, so one hung plugin does not take
//!   the feature down until the app restarts;
//! - surface helper crashes as `internal`/`trap` errors instead of hanging.

use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::thread::JoinHandle;
use std::time::Duration;

use serde_json::Value;

use crate::abi::{LogicLimits, LOGIC_API_VERSION};
use crate::protocol::{LogicError, LogicErrorCode, Request, RequestParams, Response};

/// Environment variable used by tests and by a packaged helper that lives
/// outside the app bundle.
pub const HELPER_ENV: &str = "RMCCC_LOGIC_HOST";

/// Binary name looked up next to the launcher executable.
pub const HELPER_NAME: &str = "rmccc-logic-host";

/// Default per-call budget when the caller does not pass limits.
const DEFAULT_TIMEOUT: Duration = Duration::from_secs(5);

/// A running helper process plus its stdio plumbing.
pub struct LogicHost {
    child: Child,
    stdin: ChildStdin,
    lines: Receiver<String>,
    reader: Option<JoinHandle<()>>,
    next_id: u64,
    alive: bool,
    executable: PathBuf,
}

impl LogicHost {
    /// Spawn the helper resolved from [`HELPER_ENV`] or next to the current exe.
    pub fn spawn_default() -> Result<Self, LogicError> {
        let exe = resolve_helper()?;
        Self::spawn(&exe)
    }

    /// Spawn a specific helper binary (tests use this).
    pub fn spawn(executable: &Path) -> Result<Self, LogicError> {
        let mut child = Command::new(executable)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .map_err(|e| {
                LogicError::new(
                    LogicErrorCode::Internal,
                    format!("cannot spawn {}: {e}", executable.display()),
                )
            })?;

        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| LogicError::new(LogicErrorCode::Internal, "helper has no stdin pipe"))?;
        let stdout = child.stdout.take().ok_or_else(|| {
            LogicError::new(LogicErrorCode::Internal, "helper has no stdout pipe")
        })?;

        let (tx, lines) = mpsc::channel();
        let reader = std::thread::Builder::new()
            .name("rmccc-logic-host-reader".into())
            .spawn(move || {
                let reader = BufReader::new(stdout);
                for line in reader.lines() {
                    match line {
                        Ok(line) => {
                            if tx.send(line).is_err() {
                                break;
                            }
                        }
                        Err(_) => break,
                    }
                }
            })
            .map_err(|e| {
                LogicError::new(LogicErrorCode::Internal, format!("reader thread: {e}"))
            })?;

        let mut host = Self {
            child,
            stdin,
            lines,
            reader: Some(reader),
            next_id: 1,
            alive: true,
            executable: executable.to_path_buf(),
        };

        // Handshake: proves the helper runs and agrees on the API version.
        let version = host.ping()?;
        if version != LOGIC_API_VERSION {
            let error = LogicError::new(
                LogicErrorCode::ProtocolVersion,
                format!(
                    "helper speaks logic API v{version}, launcher expects v{LOGIC_API_VERSION}"
                ),
            );
            host.kill();
            return Err(error);
        }
        Ok(host)
    }

    /// Path of the helper binary this supervisor talks to.
    pub fn executable(&self) -> &Path {
        &self.executable
    }

    /// Whether the helper is still running.
    pub fn is_alive(&mut self) -> bool {
        if !self.alive {
            return false;
        }
        match self.child.try_wait() {
            Ok(None) => true,
            _ => {
                self.alive = false;
                false
            }
        }
    }

    /// Round-trip the logic API version.
    pub fn ping(&mut self) -> Result<u32, LogicError> {
        let result = self.request(RequestParams::Ping, DEFAULT_TIMEOUT)?;
        result
            .get("logicApiVersion")
            .and_then(Value::as_u64)
            .map(|v| v as u32)
            .ok_or_else(|| {
                LogicError::new(
                    LogicErrorCode::Internal,
                    "ping reply has no logicApiVersion",
                )
            })
    }

    /// Compile and instantiate a module (one module per helper in v1).
    pub fn load(
        &mut self,
        plugin_id: &str,
        wasm_path: &Path,
        limits: LogicLimits,
    ) -> Result<(), LogicError> {
        let timeout = Duration::from_millis(limits.clamped().timeout_ms);
        self.request(
            RequestParams::Load {
                plugin_id: plugin_id.to_string(),
                path: wasm_path.display().to_string(),
                limits,
            },
            timeout.max(Duration::from_secs(1)),
        )
        .map(|_| ())
    }

    /// Call the loaded module under a wall-clock budget.
    pub fn invoke(
        &mut self,
        plugin_id: &str,
        method: &str,
        input: Value,
        limits: LogicLimits,
    ) -> Result<Value, LogicError> {
        let timeout = Duration::from_millis(limits.clamped().timeout_ms);
        self.request(
            RequestParams::Invoke {
                plugin_id: plugin_id.to_string(),
                call: method.to_string(),
                input,
                limits,
            },
            timeout,
        )
    }

    /// Drop the loaded module.
    pub fn unload(&mut self, plugin_id: &str) -> Result<(), LogicError> {
        self.request(
            RequestParams::Unload {
                plugin_id: plugin_id.to_string(),
            },
            DEFAULT_TIMEOUT,
        )
        .map(|_| ())
    }

    /// Send one request and wait for its reply.
    fn request(&mut self, params: RequestParams, timeout: Duration) -> Result<Value, LogicError> {
        if !self.is_alive() {
            return Err(LogicError::new(
                LogicErrorCode::Internal,
                "helper process is not running",
            ));
        }
        let id = self.next_id;
        self.next_id = self.next_id.wrapping_add(1);
        let request = Request::new(id, params);
        let line = request.to_line()?;

        self.stdin
            .write_all(line.as_bytes())
            .and_then(|()| self.stdin.write_all(b"\n"))
            .and_then(|()| self.stdin.flush())
            .map_err(|e| {
                self.alive = false;
                LogicError::new(LogicErrorCode::Internal, format!("write to helper: {e}"))
            })?;

        match self.lines.recv_timeout(timeout) {
            Ok(line) => {
                let response = Response::parse(&line).map_err(|mut error| {
                    error.message = format!("{} (raw: {line})", error.message);
                    error
                })?;
                response.into_result(id)
            }
            Err(RecvTimeoutError::Timeout) => {
                // No cooperation from the module: contain it by killing the
                // process. The next call respawns.
                self.kill();
                Err(LogicError::new(
                    LogicErrorCode::Timeout,
                    format!("helper did not answer within {} ms", timeout.as_millis()),
                ))
            }
            Err(RecvTimeoutError::Disconnected) => {
                self.alive = false;
                let status = self.child.try_wait().ok().flatten();
                Err(LogicError::new(
                    LogicErrorCode::Internal,
                    match status {
                        Some(status) => format!("helper exited: {status}"),
                        None => "helper stdout closed".to_string(),
                    },
                ))
            }
        }
    }

    /// Kill the helper and release its stdio. Idempotent.
    pub fn kill(&mut self) {
        self.alive = false;
        let _ = self.child.kill();
        let _ = self.child.wait();
        if let Some(handle) = self.reader.take() {
            // The reader ends on EOF once the child is gone; do not block on it.
            let _ = handle;
        }
    }

    /// Kill and spawn a fresh helper, preserving the executable path.
    pub fn restart(&mut self) -> Result<(), LogicError> {
        let exe = self.executable.clone();
        self.kill();
        let fresh = Self::spawn(&exe)?;
        *self = fresh;
        Ok(())
    }
}

impl Drop for LogicHost {
    fn drop(&mut self) {
        self.kill();
    }
}

/// Resolve the helper binary: `RMCCC_LOGIC_HOST` wins, then a sibling of the
/// current executable.
pub fn resolve_helper() -> Result<PathBuf, LogicError> {
    if let Ok(path) = std::env::var(HELPER_ENV) {
        let path = PathBuf::from(path);
        if path.is_file() {
            return Ok(path);
        }
        return Err(LogicError::new(
            LogicErrorCode::Internal,
            format!("{HELPER_ENV}={} is not a file", path.display()),
        ));
    }

    let current = std::env::current_exe()
        .map_err(|e| LogicError::new(LogicErrorCode::Internal, format!("current exe: {e}")))?;
    let sibling = current.with_file_name(HELPER_NAME);
    if sibling.is_file() {
        return Ok(sibling);
    }
    Err(LogicError::new(
        LogicErrorCode::Internal,
        format!(
            "logic helper not found: set {HELPER_ENV} or install {HELPER_NAME} next to {}",
            current.display()
        ),
    ))
}
