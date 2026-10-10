//! Debounced async file writer.
//!
//! Mirrors HMCL's [`FileSaver`]: a single background thread batches writes
//! with a 200ms debounce window, coalescing repeated saves to the same path
//! (only the last content is written), and flushes everything on shutdown.
//!
//! [`FileSaver`]: https://github.com/HMCL-dev/HMCL/blob/master/HMCL/src/main/java/org/jackhuang/hmcl/util/FileSaver.java

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread::JoinHandle;
use std::time::Duration;

/// Debounce window in milliseconds: wait this long after the first save to
/// batch further changes.
pub const DEBOUNCE_MS: u64 = 200;

enum Action {
    Save { path: PathBuf, content: String },
    Wait { done: Sender<()> },
    Shutdown,
}

/// Debounced file writer.
///
/// [`FileSaver::save`] is non-blocking: the content is queued and written by a
/// background thread after a debounce window. Repeated saves to the same path
/// within the window are coalesced (only the last content is written). Call
/// [`FileSaver::wait_for_all_saves`] before the process exits to guarantee all
/// pending writes are flushed.
pub struct FileSaver {
    tx: Sender<Action>,
    handle: Option<JoinHandle<()>>,
}

impl FileSaver {
    /// Creates a file saver and starts its background writer thread.
    pub fn new() -> Self {
        let (tx, rx) = mpsc::channel();
        let handle = std::thread::Builder::new()
            .name("rmccc-file-saver".into())
            .spawn(move || run_loop(rx))
            .expect("failed to spawn FileSaver thread");
        Self {
            tx,
            handle: Some(handle),
        }
    }

    /// Queues a debounced write. Non-blocking; the last content per path wins
    /// within the debounce window.
    pub fn save(&self, path: impl Into<PathBuf>, content: impl Into<String>) {
        let _ = self.tx.send(Action::Save {
            path: path.into(),
            content: content.into(),
        });
    }

    /// Writes synchronously, bypassing the queue (blocks until the file is on
    /// disk). Used for critical data that must be durable immediately.
    pub fn save_sync(path: impl AsRef<Path>, content: impl AsRef<str>) -> std::io::Result<()> {
        write_atomic(path.as_ref(), content.as_ref())
    }

    /// Blocks until all already-queued saves have been written.
    pub fn wait_for_all_saves(&self) {
        let (done, rx) = mpsc::channel();
        if self.tx.send(Action::Wait { done }).is_err() {
            return;
        }
        let _ = rx.recv();
    }

    /// Flushes all pending writes and stops the background thread. Intended for
    /// process shutdown so no queued data is lost.
    pub fn flush_and_shutdown(&mut self) {
        self.wait_for_all_saves();
        let _ = self.tx.send(Action::Shutdown);
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

impl Default for FileSaver {
    fn default() -> Self {
        Self::new()
    }
}

/// Background writer loop (one thread per `FileSaver` instance).
fn run_loop(rx: Receiver<Action>) {
    loop {
        // Wait for the first action (or all senders being dropped).
        let first = match rx.recv() {
            Ok(action) => action,
            Err(_) => return,
        };

        let mut pending: HashMap<PathBuf, String> = HashMap::new();
        let mut waits: Vec<Sender<()>> = Vec::new();
        let mut shutting_down = false;

        match first {
            Action::Save { path, content } => {
                pending.insert(path, content);
            }
            Action::Wait { done } => waits.push(done),
            Action::Shutdown => {
                flush(&pending);
                notify_waits(&waits);
                return;
            }
        }

        // Debounce window: wait up to DEBOUNCE_MS and coalesce additional saves
        // to the same path.
        if !pending.is_empty() {
            let deadline = std::time::Instant::now() + Duration::from_millis(DEBOUNCE_MS);
            loop {
                let remaining = deadline.saturating_duration_since(std::time::Instant::now());
                if remaining.is_zero() {
                    break;
                }
                match rx.recv_timeout(remaining) {
                    Ok(Action::Save { path, content }) => {
                        pending.insert(path, content);
                    }
                    Ok(Action::Wait { done }) => waits.push(done),
                    Ok(Action::Shutdown) => {
                        shutting_down = true;
                        break;
                    }
                    Err(mpsc::RecvTimeoutError::Timeout) => break,
                    Err(mpsc::RecvTimeoutError::Disconnected) => {
                        shutting_down = true;
                        break;
                    }
                }
            }
        }

        flush(&pending);
        notify_waits(&waits);

        if shutting_down {
            return;
        }
    }
}

fn flush(pending: &HashMap<PathBuf, String>) {
    for (path, content) in pending {
        match write_atomic(path, content) {
            Ok(()) => tracing::trace!(path = %path.display(), "FileSaver wrote"),
            Err(e) => tracing::warn!(path = %path.display(), error = %e, "FileSaver write failed"),
        }
    }
}

fn notify_waits(waits: &[Sender<()>]) {
    for w in waits {
        let _ = w.send(());
    }
}

/// Atomically writes `content` to `path` (tmp + rename), creating parents.
fn write_atomic(path: &Path, content: &str) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, content)?;
    std::fs::rename(&tmp, path)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "rmccc-filesaver-test-{}-{}-{}",
            tag,
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

    #[test]
    fn test_save_writes_file_after_debounce() {
        let dir = temp_dir("basic");
        let saver = FileSaver::new();
        let path = dir.join("a.json");
        saver.save(&path, r#"{"a":1}"#);
        saver.wait_for_all_saves();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), r#"{"a":1}"#);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_coalesces_repeated_saves_to_same_path() {
        let dir = temp_dir("coalesce");
        let saver = FileSaver::new();
        let path = dir.join("a.json");
        // 同一路径连续保存：防抖窗口内合并，只有最后一次写盘。
        saver.save(&path, "v1");
        saver.save(&path, "v2");
        saver.save(&path, "v3");
        saver.wait_for_all_saves();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "v3");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_distinct_paths_both_written() {
        let dir = temp_dir("distinct");
        let saver = FileSaver::new();
        saver.save(dir.join("a.json"), "A");
        saver.save(dir.join("b.json"), "B");
        saver.wait_for_all_saves();
        assert_eq!(std::fs::read_to_string(dir.join("a.json")).unwrap(), "A");
        assert_eq!(std::fs::read_to_string(dir.join("b.json")).unwrap(), "B");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_flush_and_shutdown_persists_pending() {
        let dir = temp_dir("shutdown");
        let mut saver = FileSaver::new();
        let path = dir.join("a.json");
        saver.save(&path, "pending");
        // 不 wait，直接 flush_and_shutdown：pending 必须落盘。
        saver.flush_and_shutdown();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "pending");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_save_sync_writes_immediately() {
        let dir = temp_dir("sync");
        let path = dir.join("a.json");
        FileSaver::save_sync(&path, "sync-content").unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "sync-content");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_write_is_atomic_tmp_removed() {
        let dir = temp_dir("atomic");
        let mut saver = FileSaver::new();
        let path = dir.join("a.json");
        saver.save(&path, "data");
        saver.flush_and_shutdown();
        // tmp 文件不应残留。
        assert!(!dir.join("a.json.tmp").exists());
        assert!(path.exists());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
