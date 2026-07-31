//! Managed game process — wraps tokio::process::Child with stdout/stderr
//! capture and lifecycle management.
//!
//! Following HMCL's [`ManagedProcess`](https://github.com/HMCL-dev/HMCL/blob/main/HMCLCore/src/main/java/org/jackhuang/hmcl/util/platform/ManagedProcess.java)
//! pattern.

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

    /// Captured output lines (stdout + stderr interleaved).
    pub lines: Arc<Mutex<Vec<String>>>,

    /// Max number of lines to retain in memory.
    max_lines: usize,
}

impl ManagedProcess {
    /// Create a new managed process wrapper.
    pub fn new(profile_id: String, child: tokio::process::Child, max_lines: usize) -> Self {
        Self {
            profile_id,
            child: Mutex::new(Some(child)),
            running: AtomicBool::new(true),
            lines: Arc::new(Mutex::new(Vec::new())),
            max_lines,
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

        let stdout = child.stdout.take();
        let stderr = child.stderr.take();

        let lines = self.lines.clone();
        let max_lines = self.max_lines;

        // Spawn stdout reader
        let stdout_handle = if let Some(stdout) = stdout {
            let lines = lines.clone();
            Some(tokio::spawn(read_lines(stdout, lines, max_lines, false)))
        } else {
            None
        };

        // Spawn stderr reader
        let stderr_handle = if let Some(stderr) = stderr {
            Some(tokio::spawn(read_lines(stderr, lines, max_lines, true)))
        } else {
            None
        };

        // Wait for process to exit
        let exit_status = child.wait().await;

        // Wait for readers to finish
        if let Some(h) = stdout_handle {
            let _ = h.await;
        }
        if let Some(h) = stderr_handle {
            let _ = h.await;
        }

        // Signal that the process has fully exited
        self.running.store(false, Ordering::SeqCst);

        exit_status.map(|s| s.code().unwrap_or(-1)).unwrap_or(-1)
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
    _is_stderr: bool,
) {
    use tokio::io::{AsyncBufReadExt, BufReader};

    let buf = BufReader::new(&mut reader);
    let mut line_reader = buf.lines();

    while let Ok(Some(line)) = line_reader.next_line().await {
        let mut lines = lines.lock().await;
        lines.push(line);
        if lines.len() > max_lines {
            let excess = lines.len() - max_lines;
            lines.drain(0..excess);
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

    #[test]
    fn test_managed_process_creation() {
        // Can't easily test with a real process, but verify construction
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let child = tokio::process::Command::new("echo")
                .arg("test")
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::piped())
                .spawn()
                .unwrap();
            let managed = ManagedProcess::new("test-profile".into(), child, 500);
            assert_eq!(managed.profile_id, "test-profile");
            assert!(managed.is_running().await);
        });
    }
}
