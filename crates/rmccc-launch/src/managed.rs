//! Managed game process — wraps `tokio::process::Child` with stdout/stderr
//! stream pumps, an exit waiter and [`ProcessListener`] notifications.
//!
//! Following HMCL's [`ManagedProcess`](https://github.com/HMCL-dev/HMCL/blob/main/HMCLCore/src/main/java/org/jackhuang/hmcl/util/platform/ManagedProcess.java)
//! + `StreamPump` + `ExitWaiter` pattern:
//! - two independent pumps read stdout / stderr and forward every line
//! - an exit waiter joins the pumps and emits a single `on_exit` with the
//!   exit code and an [`ExitType`]

use crate::process_listener::{ExitType, ProcessListener};
use rmccc_error::Result;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::sync::Mutex;

/// A running Minecraft process with captured output and lifecycle control.
pub struct ManagedProcess {
    /// Profile / instance ID this process belongs to.
    pub profile_id: String,

    /// The underlying child process.
    child: Mutex<Option<tokio::process::Child>>,

    /// Whether the process is still running (set to false by the monitor
    /// when the process exits). Uses AtomicBool to avoid the race condition
    /// where is_running() returns false because the monitor already took
    /// the child handle.
    running: AtomicBool,

    /// OS process ID (captured at construction, before the monitor takes
    /// the child handle).
    pid: u32,

    /// Exit code captured by the monitor once the process has fully exited.
    exit_code: std::sync::Arc<std::sync::Mutex<Option<i32>>>,

    /// Captured output lines (stdout + stderr interleaved).
    pub lines: Arc<Mutex<Vec<String>>>,

    /// Max number of lines to retain in memory.
    max_lines: usize,

    /// Registered lifecycle listeners.
    listeners: Arc<Mutex<Vec<Arc<dyn ProcessListener>>>>,
}

impl ManagedProcess {
    /// Create a new managed process wrapper.
    pub fn new(profile_id: String, child: tokio::process::Child, max_lines: usize) -> Self {
        let pid = child.id().unwrap_or(0);
        Self {
            profile_id,
            child: Mutex::new(Some(child)),
            running: AtomicBool::new(true),
            pid,
            exit_code: std::sync::Arc::new(std::sync::Mutex::new(None)),
            lines: Arc::new(Mutex::new(Vec::new())),
            max_lines,
            listeners: Arc::new(Mutex::new(Vec::new())),
        }
    }

    /// The OS process ID of the game process.
    pub fn pid(&self) -> u32 {
        self.pid
    }

    /// Attach a process listener (HMCL `ProcessListener.setProcess` + callbacks).
    pub fn add_listener(&self, listener: Arc<dyn ProcessListener>) {
        if let Ok(mut list) = self.listeners.try_lock() {
            list.push(listener);
        }
    }

    /// Start stdout/stderr readers and the exit waiter.
    ///
    /// Returns a join handle that completes when the process exits.
    pub fn start_monitors(self: Arc<Self>) -> tokio::task::JoinHandle<i32> {
        let this = self.clone();
        tokio::spawn(async move { this.run_monitors().await })
    }

    async fn run_monitors(self: Arc<Self>) -> i32 {
        let child_opt = self.child.lock().await.take();
        let Some(mut child) = child_opt else {
            self.running.store(false, Ordering::SeqCst);
            return -1;
        };

        // Notify listeners of the managed process handle (HMCL setProcess).
        {
            let listeners = self.listeners.lock().await;
            for l in listeners.iter() {
                l.set_process(&self);
            }
        }

        let stdout = child.stdout.take();
        let stderr = child.stderr.take();

        let listeners = self.listeners.clone();

        // Spawn stdout pump (HMCL StreamPump).
        let stdout_handle = if let Some(stdout) = stdout {
            let lines = self.lines.clone();
            let max_lines = self.max_lines;
            let listeners = listeners.clone();
            Some(tokio::spawn(read_lines(stdout, lines, max_lines, false, listeners)))
        } else {
            None
        };

        // Spawn stderr pump (HMCL StreamPump).
        let stderr_handle = stderr.map(|stderr| {
            let lines = self.lines.clone();
            let max_lines = self.max_lines;
            let listeners = listeners.clone();
            tokio::spawn(read_lines(stderr, lines, max_lines, true, listeners))
        });

        // Exit waiter: wait for the process, then join the pumps.
        let exit_status = child.wait().await;

        if let Some(h) = stdout_handle {
            let _ = h.await;
        }
        if let Some(h) = stderr_handle {
            let _ = h.await;
        }

        // Signal that the process has fully exited.
        self.running.store(false, Ordering::SeqCst);

        let exit_code = exit_status
            .map(|s| s.code().unwrap_or(-1))
            .unwrap_or(-1);
        let exit_type = if exit_code == 0 {
            ExitType::Normal
        } else {
            ExitType::Abnormal
        };

        // Record the exit code for blocking callers (CLI wait_exit).
        if let Ok(mut code) = self.exit_code.lock() {
            *code = Some(exit_code);
        }

        // HMCL ExitWaiter → ProcessListener.onExit(exitCode, exitType).
        {
            let listeners = self.listeners.lock().await;
            for l in listeners.iter() {
                l.on_exit(exit_code, exit_type);
            }
        }

        exit_code
    }

    /// Block until the process exits, returning its exit code.
    ///
    /// Used by blocking orchestrators (CLI) that need the final exit status
    /// without implementing a listener.
    pub async fn wait_exit(&self) -> i32 {
        loop {
            if let Ok(code) = self.exit_code.lock() {
                if let Some(c) = *code {
                    return c;
                }
            }
            if !self.is_running().await {
                // Give the monitor a moment to record the exit code.
                tokio::time::sleep(std::time::Duration::from_millis(50)).await;
                if let Ok(code) = self.exit_code.lock() {
                    if let Some(c) = *code {
                        return c;
                    }
                }
                return -1;
            }
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        }
    }

    /// Get the most recent N output lines (snapshot, does not drain).
    pub async fn recent_lines(&self, n: usize) -> Vec<String> {
        let lines = self.lines.lock().await;
        let start = if lines.len() > n { lines.len() - n } else { 0 };
        lines[start..].to_vec()
    }

    /// Drain all buffered output lines (returns them and clears the buffer).
    pub async fn drain_lines(&self) -> Vec<String> {
        let mut lines = self.lines.lock().await;
        std::mem::take(&mut *lines)
    }

    /// Kill the process.
    pub async fn kill(&self) -> Result<()> {
        let mut child_opt = self.child.lock().await;
        if let Some(ref mut child) = *child_opt {
            child.kill().await?;
        }
        *child_opt = None;
        self.running.store(false, Ordering::SeqCst);
        Ok(())
    }

    /// Check if the process is still running.
    ///
    /// Uses an atomic flag set by the monitor task, avoiding the race
    /// condition where `child.take()` makes `child.is_some()` return
    /// false prematurely.
    pub async fn is_running(&self) -> bool {
        self.running.load(Ordering::SeqCst)
    }
}

async fn read_lines<R: tokio::io::AsyncRead + Unpin>(
    mut reader: R,
    lines: Arc<Mutex<Vec<String>>>,
    max_lines: usize,
    is_stderr: bool,
    listeners: Arc<Mutex<Vec<Arc<dyn ProcessListener>>>>,
) {
    use tokio::io::{AsyncBufReadExt, BufReader};

    let buf = BufReader::new(&mut reader);
    let mut line_reader = buf.lines();

    while let Ok(Some(line)) = line_reader.next_line().await {
        // Buffer for UI consumers (recent_lines / drain_lines).
        {
            let mut lines = lines.lock().await;
            lines.push(line.clone());
            if lines.len() > max_lines {
                let excess = lines.len() - max_lines;
                lines.drain(0..excess);
            }
        }
        // HMCL ProcessListener.onLog(line, isStderr).
        {
            let listeners = listeners.lock().await;
            for l in listeners.iter() {
                l.on_log(line.clone(), is_stderr);
            }
        }
    }
}

/// Convenience function: spawn a game and return a ManagedProcess with
/// monitors already started.
pub async fn spawn_managed(
    profile_id: String,
    java_path: &std::path::Path,
    response_file: &std::path::Path,
    game_dir: &std::path::Path,
    max_lines: usize,
) -> Result<Arc<ManagedProcess>> {
    let child = crate::process::spawn_game(java_path, response_file, game_dir, None).await?;
    let managed = Arc::new(ManagedProcess::new(profile_id, child, max_lines));
    let _monitor_handle = managed.clone().start_monitors();
    Ok(managed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex as StdMutex;

    struct Recorder {
        logs: Arc<StdMutex<Vec<String>>>,
        exits: Arc<StdMutex<Vec<i32>>>,
    }

    impl ProcessListener for Recorder {
        fn on_log(&self, line: String, _is_stderr: bool) {
            // `StdMutex::lock()` is safe here (short critical section);
            // tokio's `blocking_lock()` would panic inside the runtime.
            if let Ok(mut logs) = self.logs.lock() {
                logs.push(line);
            }
        }
        fn on_exit(&self, exit_code: i32, _exit_type: ExitType) {
            if let Ok(mut exits) = self.exits.lock() {
                exits.push(exit_code);
            }
        }
    }

    fn spawn_child(script: &str) -> tokio::process::Child {
        tokio::process::Command::new("sh")
            .arg("-c")
            .arg(script)
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .unwrap()
    }

    #[test]
    fn test_managed_process_streams_and_exit() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let managed =
                Arc::new(ManagedProcess::new("test".into(), spawn_child("echo hello; echo err >&2; exit 0"), 500));
            let recorder = Arc::new(Recorder {
                logs: Arc::new(StdMutex::new(Vec::new())),
                exits: Arc::new(StdMutex::new(Vec::new())),
            });
            managed.add_listener(recorder.clone());

            let handle = managed.clone().start_monitors();
            let code = handle.await.unwrap();
            assert_eq!(code, 0);
            assert!(!managed.is_running().await);

            let logs = recorder.logs.lock().unwrap();
            assert!(logs.iter().any(|l| l.contains("hello")));
            assert!(logs.iter().any(|l| l.contains("err")));
            let exits = recorder.exits.lock().unwrap();
            assert_eq!(exits.len(), 1);
            assert_eq!(exits[0], 0);
        });
    }

    #[test]
    fn test_managed_process_abnormal_exit() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let managed = Arc::new(ManagedProcess::new(
                "test".into(),
                spawn_child("exit 3"),
                500,
            ));
            let recorder = Arc::new(Recorder {
                logs: Arc::new(StdMutex::new(Vec::new())),
                exits: Arc::new(StdMutex::new(Vec::new())),
            });
            managed.add_listener(recorder.clone());
            let handle = managed.clone().start_monitors();
            let code = handle.await.unwrap();
            assert_eq!(code, 3);
            let exits = recorder.exits.lock().unwrap();
            assert_eq!(exits.len(), 1);
            assert_eq!(exits[0], 3);
        });
    }
}
