//! Concurrent download engine with retry, resume, SHA1 verification,
//! completion tracking, SHA1-based cache repository, and download
//! provider abstraction.
//!
//! # Cache Repository
//!
//! Following HMCL's [`DefaultCacheRepository`] pattern, the
//! [`CacheRepository`] stores downloaded files keyed by SHA1 hash in
//! `{data_dir}/cache/objects/{sha1[0:2]}/{sha1}` — the same convention
//! used by Minecraft's asset storage. This enables deduplication across
//! game instances and modloader installations.

#![allow(clippy::double_must_use)] // 工具链 1.99.1 新 lint：async_trait 宏生成的 #[must_use] 与 Future/Result 重叠，非业务逻辑

pub mod provider;

use rmccc_error::{Result, RmcccError};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio_util::sync::CancellationToken;

// ── Public types ──

#[derive(Debug, Clone)]
pub struct DownloadItem {
    /// Primary download URL.
    pub url: String,
    /// Destination file path.
    pub dest: PathBuf,
    /// Optional SHA1 hash for integrity verification.
    pub expected_sha1: Option<String>,
    /// Expected file size in bytes (for progress tracking).
    pub size: Option<u64>,
    /// Whether to check the cache repository before downloading.
    /// When `true` and `expected_sha1` is set, the download will first
    /// attempt to copy from the cache (by SHA1) before making a network request.
    /// After a successful download, the file is stored in the cache.
    pub use_cache: bool,
    /// Fallback URLs to try if the primary URL fails.
    /// Used for mirror fallback (e.g., inject_url_with_candidates).
    pub candidates: Vec<String>,
}

impl DownloadItem {
    /// Create a simple download item without caching.
    pub fn new(url: &str, dest: &Path) -> Self {
        Self {
            url: url.to_string(),
            dest: dest.to_path_buf(),
            expected_sha1: None,
            size: None,
            use_cache: false,
            candidates: Vec::new(),
        }
    }

    /// Create a download item with SHA1 verification and optional caching.
    pub fn with_sha1(url: &str, dest: &Path, sha1: &str, size: Option<u64>, cache: bool) -> Self {
        Self {
            url: url.to_string(),
            dest: dest.to_path_buf(),
            expected_sha1: Some(sha1.to_string()),
            size,
            use_cache: cache,
            candidates: Vec::new(),
        }
    }

    /// Set fallback candidate URLs (e.g., from inject_url_with_candidates).
    pub fn with_candidates(mut self, candidates: Vec<String>) -> Self {
        self.candidates = candidates;
        self
    }
}

#[derive(Debug, Clone)]
pub struct DownloadHandle {
    pub id: String,
}

#[derive(Debug, Clone)]
pub struct DownloadProgress {
    pub batch_id: String,
    pub completed_files: u64,
    pub total_files: u64,
    pub downloaded_bytes: u64,
    pub total_bytes: u64,
    pub failed_files: u64,
    pub current_file: Option<String>,
}

pub type ProgressCallback = Arc<dyn Fn(DownloadProgress) + Send + Sync>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DownloadStatus {
    Running,
    Paused,
    Completed,
    Failed,
    Cancelled,
}

struct BatchState {
    cancel_token: CancellationToken,
    paused: AtomicBool,
    /// Mirrors `paused`; `resume()` sends through it to wake the feeder
    /// immediately instead of a 100 ms busy-poll (perf audit D9).
    paused_tx: tokio::sync::watch::Sender<bool>,
    /// Signalled when the batch finishes (all workers complete or cancelled).
    completion: tokio::sync::Notify,
}

// ── SHA1 Cache Repository ──
// Follows HMCL's DefaultCacheRepository pattern:
// cache_dir/objects/{sha1[0:2]}/{sha1}

/// `true` when `s` is exactly 40 ASCII hex characters (a SHA-1 digest).
///
/// Twin of the private predicate in `rmccc-version::file_layout` — both
/// crates consume hashes from downloaded manifests and must reject anything
/// that is not a plain digest before it reaches a path join.
fn is_sha1_hex(s: &str) -> bool {
    s.len() == 40 && s.bytes().all(|b| b.is_ascii_hexdigit())
}

/// SHA1-based file cache repository.
///
/// Stores downloaded files keyed by SHA1 hash, enabling deduplication
/// across game instances. Cache directory layout:
/// `{data_dir}/cache/objects/{sha1[0:2]}/{sha1}`
///
/// This mirrors Minecraft's own asset storage convention and HMCL's
/// `DefaultCacheRepository` pattern.
pub struct CacheRepository {
    cache_dir: PathBuf,
}

impl CacheRepository {
    /// Create a new cache repository rooted at `cache_dir`.
    ///
    /// The directory is created if it doesn't exist.
    pub fn new(cache_dir: PathBuf) -> Self {
        std::fs::create_dir_all(cache_dir.join("objects")).ok();
        Self { cache_dir }
    }

    /// Look up a cached file by SHA1 hash.
    ///
    /// Returns `Some(path)` if the file exists in the cache,
    /// `None` otherwise. A malformed key (not 40 hex chars) is reported as a
    /// cache miss, never as a panic or an out-of-`objects/` path.
    ///
    /// Async (perf audit D2): the lookup stats the filesystem, which must not
    /// block the tokio worker when the caller runs on the async runtime.
    pub async fn check_by_sha1(&self, sha1: &str) -> Option<PathBuf> {
        let path = self.sha1_to_path(sha1)?;
        let meta = tokio::fs::metadata(&path).await.ok()?;
        if meta.len() > 0 {
            return Some(path);
        }
        None
    }

    /// Store a file in the cache by SHA1 hash.
    ///
    /// Copies `src` to `cache_dir/objects/{sha1[0:2]}/{sha1}`.
    /// Returns the cached path.
    ///
    /// Async (perf audit D2): the copy can move hundreds of MB (Forge
    /// installer) and must not block the tokio worker.
    pub async fn store(&self, src: &Path, sha1: &str) -> std::io::Result<PathBuf> {
        let dest = self.sha1_to_path(sha1).ok_or_else(|| {
            std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "Invalid SHA1 cache key (expected 40 hex chars)",
            )
        })?;
        if tokio::fs::metadata(&dest).await.is_ok() {
            return Ok(dest); // already cached
        }
        let parent = dest.parent().ok_or_else(|| {
            std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "Invalid cache destination path",
            )
        })?;
        tokio::fs::create_dir_all(parent).await?;
        tokio::fs::copy(src, &dest).await?;
        Ok(dest)
    }

    /// Convert a SHA1 hash to a cache file path.
    ///
    /// Returns `None` unless `sha1` is exactly 40 hex characters — the key
    /// comes from downloaded manifests / mirror responses and must never
    /// steer a read or write outside `objects/` (audit 2026-10-25 A4).
    fn sha1_to_path(&self, sha1: &str) -> Option<PathBuf> {
        if !is_sha1_hex(sha1) {
            return None;
        }
        Some(self.cache_dir.join("objects").join(&sha1[..2]).join(sha1))
    }

    /// The cache directory root.
    pub fn cache_dir(&self) -> &Path {
        &self.cache_dir
    }
}

// ── Retry config ──

pub struct RetryConfig {
    pub max_retries: u32,
    pub base_delay: std::time::Duration,
    pub max_delay: std::time::Duration,
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            max_retries: 3,
            base_delay: std::time::Duration::from_secs(1),
            max_delay: std::time::Duration::from_secs(30),
        }
    }
}

impl RetryConfig {
    pub fn delay_for_attempt(&self, attempt: u32) -> std::time::Duration {
        let delay = self.base_delay * 2u32.pow(attempt);
        delay.min(self.max_delay)
    }
}

// ── Download Manager ──

/// Trait for download managers — enables mocking and provider swapping.
#[async_trait::async_trait]
pub trait DownloadManager: Send + Sync {
    /// Submit a batch of downloads.
    async fn download_batch(
        &self,
        items: Vec<DownloadItem>,
        progress: ProgressCallback,
    ) -> Result<DownloadHandle>;

    /// Wait for all downloads in a batch to complete.
    async fn wait_for_batch(&self, batch_id: &str);

    /// Cancel all downloads in a batch.
    fn cancel_batch(&self, batch_id: &str);

    /// Pause a batch.
    fn pause(&self, batch_id: &str);

    /// Resume a paused batch.
    fn resume(&self, batch_id: &str);
}

pub struct ConcurrentDownloadManager {
    client: reqwest::Client,
    concurrency: u32,
    retry_config: RetryConfig,
    cache: Option<Arc<CacheRepository>>,
    batches: dashmap::DashMap<String, Arc<BatchState>>,
}

impl ConcurrentDownloadManager {
    pub fn new(client: reqwest::Client, concurrency: u32) -> Self {
        Self {
            client,
            concurrency,
            retry_config: RetryConfig::default(),
            cache: None,
            batches: dashmap::DashMap::new(),
        }
    }

    /// Attach a cache repository to this download manager.
    /// When set, downloads with `use_cache: true` will check the cache
    /// before making network requests and store successful downloads.
    pub fn with_cache(mut self, cache: Arc<CacheRepository>) -> Self {
        self.cache = Some(cache);
        self
    }

    /// Submit a batch of download items.
    /// Returns a handle. The work is spawned asynchronously.
    /// The returned handle can be used with cancel/pause/resume.
    pub async fn download_batch(
        &self,
        items: Vec<DownloadItem>,
        progress: ProgressCallback,
    ) -> Result<DownloadHandle> {
        let id = uuid::Uuid::new_v4().to_string();
        let total_files = items.len() as u64;

        tracing::info!(batch_id = %id, count = total_files, "Starting download batch");

        let (paused_tx, _) = tokio::sync::watch::channel(false);
        let batch_state = Arc::new(BatchState {
            cancel_token: CancellationToken::new(),
            paused: AtomicBool::new(false),
            paused_tx,
            completion: tokio::sync::Notify::new(),
        });
        self.batches.insert(id.clone(), batch_state.clone());

        let client = self.client.clone();
        let cache = self.cache.clone();
        let concurrency = self.concurrency as usize;
        let retry_config_max_retries = self.retry_config.max_retries;
        let batch_id = id.clone();

        let completed_files = Arc::new(AtomicU64::new(0));
        let failed_files = Arc::new(AtomicU64::new(0));
        let downloaded_bytes = Arc::new(AtomicU64::new(0));
        let total_bytes: u64 = items.iter().filter_map(|i| i.size).sum();

        let feed_batch_state = batch_state.clone();

        // Spawn the download task
        tokio::spawn({
            let batch_state = batch_state.clone();
            let batches = self.batches.clone();
            let progress = progress.clone();
            let batch_id = batch_id.clone();
            let completed_files = completed_files.clone();
            let failed_files = failed_files.clone();
            let downloaded_bytes = downloaded_bytes.clone();

            async move {
                // Perf audit D5/D9: the old mpsc + Mutex<Receiver> feed made
                // every worker contend on one lock per item, and pause used a
                // 100 ms busy-poll. A bounded JoinSet pulls items directly
                // (semaphore = concurrency limit) and pause waits on a watch
                // channel until resume() signals.
                let sem = Arc::new(tokio::sync::Semaphore::new(concurrency));
                let mut paused_rx = feed_batch_state.paused_tx.subscribe();
                let mut set = tokio::task::JoinSet::new();

                for item in items {
                    if feed_batch_state.cancel_token.is_cancelled() {
                        break;
                    }
                    while *paused_rx.borrow_and_update()
                        && !feed_batch_state.cancel_token.is_cancelled()
                    {
                        if paused_rx.changed().await.is_err() {
                            break;
                        }
                    }
                    if feed_batch_state.cancel_token.is_cancelled() {
                        break;
                    }
                    let Ok(permit) = sem.clone().acquire_owned().await else {
                        break;
                    };

                    let client = client.clone();
                    let cache = cache.clone();
                    let cancel = batch_state.cancel_token.clone();
                    let progress = progress.clone();
                    let batch_id = batch_id.clone();
                    let completed_files = completed_files.clone();
                    let failed_files = failed_files.clone();
                    let downloaded_bytes = downloaded_bytes.clone();

                    set.spawn(async move {
                        let _permit = permit;
                        if cancel.is_cancelled() {
                            return;
                        }
                        match download_with_retry(
                            &client,
                            &item,
                            retry_config_max_retries,
                            cache.as_deref(),
                        )
                        .await
                        {
                            Ok(bytes) => {
                                let cf = completed_files.fetch_add(1, Ordering::Relaxed) + 1;
                                let db =
                                    downloaded_bytes.fetch_add(bytes, Ordering::Relaxed) + bytes;
                                progress(DownloadProgress {
                                    batch_id,
                                    completed_files: cf,
                                    total_files,
                                    downloaded_bytes: db,
                                    total_bytes,
                                    failed_files: failed_files.load(Ordering::Relaxed),
                                    current_file: item
                                        .dest
                                        .file_name()
                                        .map(|n| n.to_string_lossy().to_string()),
                                });
                            }
                            Err(e) => {
                                failed_files.fetch_add(1, Ordering::Relaxed);
                                tracing::error!("Download failed: {} — {e}", item.url);
                            }
                        }
                    });
                }

                while let Some(joined) = set.join_next().await {
                    if let Err(e) = joined {
                        tracing::error!(batch_id = %batch_id, "Worker panicked: {e}");
                    }
                }

                let fc = completed_files.load(Ordering::Relaxed);
                let ff = failed_files.load(Ordering::Relaxed);

                if batch_state.cancel_token.is_cancelled() {
                    tracing::info!(batch_id = %batch_id, "Batch cancelled at {fc}/{total_files}");
                } else if ff > 0 {
                    tracing::warn!(batch_id = %batch_id, "Batch completed with {ff} failures: {fc}/{total_files}");
                } else {
                    tracing::info!(batch_id = %batch_id, "Batch completed: {total_files} files");
                }

                // Emit final progress with failure count.
                //
                // This MUST happen before signalling completion: callers that
                // read the failure count out of the final callback (pipeline
                // `SetupReport`) would otherwise race the waiter and observe a
                // stale zero.
                progress(DownloadProgress {
                    batch_id: batch_id.clone(),
                    completed_files: fc,
                    total_files,
                    downloaded_bytes: downloaded_bytes.load(Ordering::Relaxed),
                    total_bytes,
                    failed_files: ff,
                    current_file: None,
                });

                // Signal completion so callers waiting via wait_for_batch() are notified.
                batch_state.completion.notify_one();

                // Perf audit D6: drop the finished batch's state — entries
                // used to live forever in a long-running GUI process.
                batches.remove(&batch_id);
            }
        });

        Ok(DownloadHandle { id })
    }

    /// Wait for a batch to fully complete (all downloads finished or cancelled).
    ///
    /// This is essential for CLI usage where callers must wait for all files to
    /// be written before proceeding to the next step (Bug #5).
    pub async fn wait_for_batch(&self, batch_id: &str) {
        // Clone the state out first — the DashMap guard must never live
        // across an await (it would block the shard for other batches).
        let state = self.batches.get(batch_id).map(|s| s.clone());
        if let Some(state) = state {
            state.completion.notified().await;
        }
    }

    pub fn cancel_batch(&self, batch_id: &str) {
        if let Some(state) = self.batches.get(batch_id) {
            state.cancel_token.cancel();
            tracing::info!(batch_id = %batch_id, "Batch cancel requested");
        }
    }

    pub fn pause(&self, batch_id: &str) {
        if let Some(state) = self.batches.get(batch_id) {
            state.paused.store(true, Ordering::Relaxed);
            let _ = state.paused_tx.send(true);
            tracing::info!(batch_id = %batch_id, "Batch paused");
        }
    }

    pub fn resume(&self, batch_id: &str) {
        if let Some(state) = self.batches.get(batch_id) {
            state.paused.store(false, Ordering::Relaxed);
            let _ = state.paused_tx.send(false);
            tracing::info!(batch_id = %batch_id, "Batch resumed");
        }
    }
}

// P2-05：`DownloadManager` trait 落地——使 `ConcurrentDownloadManager` 可经
// `Box<dyn DownloadManager>` / `Arc<dyn DownloadManager>` 注入与替换（便于 mock）。
// 因 inherent 方法优先级高于 trait 方法，`self.*` 调用仍解析到 inherent 实现，无递归。
#[async_trait::async_trait]
impl DownloadManager for ConcurrentDownloadManager {
    async fn download_batch(
        &self,
        items: Vec<DownloadItem>,
        progress: ProgressCallback,
    ) -> Result<DownloadHandle> {
        self.download_batch(items, progress).await
    }

    async fn wait_for_batch(&self, batch_id: &str) {
        self.wait_for_batch(batch_id).await;
    }

    fn cancel_batch(&self, batch_id: &str) {
        self.cancel_batch(batch_id);
    }

    fn pause(&self, batch_id: &str) {
        self.pause(batch_id);
    }

    fn resume(&self, batch_id: &str) {
        self.resume(batch_id);
    }
}

// ── Internal download logic ──

/// Collect all URLs to try for an item: primary + candidates.
fn all_urls(item: &DownloadItem) -> Vec<&str> {
    let mut urls = vec![item.url.as_str()];
    for c in &item.candidates {
        urls.push(c.as_str());
    }
    urls
}

/// Staging path for a download: `{file_name}.part` next to the destination.
///
/// `dest` is only ever published by renaming a verified part file, so a
/// crashed, interrupted, or corrupted download can never land at the final
/// location and poison every later resume (P1-8).
pub fn part_path(dest: &Path) -> PathBuf {
    let name = dest
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "download".to_string());
    dest.with_file_name(format!("{name}.part"))
}

/// Outcome of one single-URL download attempt.
struct SingleDownload {
    size: u64,
    /// SHA1 computed while streaming the response body (perf audit D1) —
    /// `None` when the attempt was a Range resume onto an existing `.part`
    /// (the hash of the resumed prefix is unknown and the caller falls back
    /// to a full re-read of the staged file).
    sha1: Option<String>,
}

/// Download a single item with retry and multi-URL fallback.
///
/// Features:
/// - Cache-first: SHA1 hit skips network
/// - Multi-URL fallback: tries candidates on failure
/// - x-bmclapi-hash: BMCLAPI provides additional SHA1 in header
/// - SHA1 verify + cache store on success
/// - Stage-then-publish: content is written to [`part_path`] and renamed onto
///   `dest` only after the expected SHA1 (when provided) verified
async fn download_with_retry(
    client: &reqwest::Client,
    item: &DownloadItem,
    max_retries: u32,
    cache: Option<&CacheRepository>,
) -> Result<u64> {
    // ── Already-complete fast path ──
    // A complete, hash-verified `dest` skips the network entirely. Anything
    // else at `dest` (a leftover poisoned file from before the `.part`
    // staging, or manual corruption) is removed so the download below replaces
    // it instead of failing forever.
    if let Some(expected) = &item.expected_sha1 {
        if item.dest.exists() {
            // Size short-circuit (perf audit D3): a length mismatch already
            // proves the file is not what the manifest promised — skip the
            // full SHA1 pass (a whole re-read of the game directory on
            // repeat installs) and re-download straight away.
            let size_matches = match (tokio::fs::metadata(&item.dest).await, item.size) {
                (Ok(meta), Some(expected_size)) => meta.len() == expected_size,
                _ => true,
            };
            if size_matches {
                match sha1_file(&item.dest).await {
                    Ok(actual) if actual.eq_ignore_ascii_case(expected) => {
                        tracing::debug!("Already complete: {} ({expected})", item.dest.display());
                        let size = tokio::fs::metadata(&item.dest)
                            .await
                            .map(|m| m.len())
                            .unwrap_or(0);
                        return Ok(size);
                    }
                    Ok(actual) => {
                        tracing::warn!(
                            "Corrupt file at {} (expected sha1 {expected}, got {actual}); re-downloading",
                            item.dest.display()
                        );
                    }
                    Err(e) => {
                        tracing::warn!("Cannot hash {}: {e}; re-downloading", item.dest.display());
                    }
                }
            } else {
                tracing::debug!(
                    "Size mismatch at {} (expected {:?}); re-downloading without hashing",
                    item.dest.display(),
                    item.size
                );
            }
            let _ = tokio::fs::remove_file(&item.dest).await;
        }
    }

    // ── Cache check before download ──
    if item.use_cache {
        if let Some(ref sha1) = item.expected_sha1 {
            if let Some(cache) = cache {
                if let Some(cache_path) = cache.check_by_sha1(sha1).await {
                    let part = part_path(&item.dest);
                    if let Some(parent) = part.parent() {
                        tokio::fs::create_dir_all(parent).await?;
                    }
                    // Hardlink first (perf audit D7): cache entries were
                    // SHA1-verified when stored, so a same-inode publish needs no
                    // re-read. Fall back to copy + re-verify (P1-8) when the
                    // hardlink cannot be created (e.g. cross-device).
                    match tokio::fs::hard_link(&cache_path, &part).await {
                        Ok(()) => {
                            tokio::fs::rename(&part, &item.dest).await?;
                            let meta = tokio::fs::metadata(&item.dest).await?;
                            tracing::debug!(
                                "Cache HIT (hardlinked) for {} ({} bytes)",
                                item.url,
                                meta.len()
                            );
                            return Ok(meta.len());
                        }
                        Err(_) => {
                            tokio::fs::copy(&cache_path, &part).await?;
                            // Verify the copied fallback too: the cache is keyed
                            // by SHA1 but its contents are never re-validated on
                            // read (P1-8).
                            let actual = sha1_file(&part).await?;
                            if actual.eq_ignore_ascii_case(sha1) {
                                tokio::fs::rename(&part, &item.dest).await?;
                                let meta = tokio::fs::metadata(&item.dest).await?;
                                tracing::debug!(
                                    "Cache HIT for {} ({} bytes)",
                                    item.url,
                                    meta.len()
                                );
                                return Ok(meta.len());
                            }
                            tracing::warn!(
                            "Cache entry {} is corrupt (expected {sha1}, got {actual}); downloading from network",
                            cache_path.display()
                        );
                        }
                    }
                    let _ = tokio::fs::remove_file(&part).await;
                }
            }
        }
    }

    // ── Collect all URLs: primary + candidates ──
    let urls = all_urls(item);
    let mut last_error = None;

    for (url_idx, url) in urls.iter().enumerate() {
        for attempt in 0..=max_retries {
            if attempt > 0 {
                let delay = std::time::Duration::from_secs(2u64.pow(attempt - 1));
                tracing::debug!("Retry {attempt}/{max_retries} for {url} (candidate #{url_idx})");
                tokio::time::sleep(delay).await;
            }

            match download_single(client, item, url, cache).await {
                Ok(downloaded) => {
                    // Stage-verify-publish: verify the `.part` file and rename it
                    // onto `dest` only when it matches. A mismatch deletes the
                    // part and falls through to the next attempt / candidate
                    // URL — returning early here left the corrupt file at
                    // `dest` where every later resume appended to it (P1-8).
                    let part = part_path(&item.dest);
                    if let Some(expected) = &item.expected_sha1 {
                        // Reuse the hash computed while streaming (perf audit
                        // D1); only a Range-resumed download needs a re-read.
                        let actual = match downloaded.sha1 {
                            Some(ref digest) => digest.clone(),
                            None => sha1_file(&part).await?,
                        };
                        if !actual.eq_ignore_ascii_case(expected) {
                            let _ = tokio::fs::remove_file(&part).await;
                            tracing::warn!(
                                "SHA1 mismatch for {} from {url}: expected {expected}, got {actual}",
                                item.dest.display()
                            );
                            last_error = Some(RmcccError::HashMismatch {
                                path: item.dest.display().to_string(),
                                expected: expected.clone(),
                                actual,
                            });
                            continue;
                        }
                        // Store in cache after successful verification.
                        if item.use_cache {
                            if let Some(cache) = cache {
                                if let Err(e) = cache.store(&part, expected).await {
                                    tracing::warn!("Failed to cache {}: {e}", url);
                                }
                            }
                        }
                    }
                    tokio::fs::rename(&part, &item.dest).await?;
                    return Ok(downloaded.size);
                }
                Err(e) => {
                    tracing::warn!("Failed {url} (candidate #{url_idx}, attempt {attempt}): {e}");
                    if !e.is_retryable() || attempt == max_retries {
                        last_error = Some(e);
                        break; // Try next candidate
                    }
                    last_error = Some(e);
                }
            }
        }
    }

    Err(last_error.unwrap_or_else(|| RmcccError::DownloadFailed {
        url: item.url.clone(),
        reason: "All URLs exhausted".into(),
    }))
}

/// Download a single file from a given URL into the staged [`part_path`] file.
///
/// Supports:
/// - HTTP Range resume (from the `.part` file, never from `dest`)
/// - x-bmclapi-hash header (BMCLAPI additional SHA1)
/// - Caller-side SHA1 verification of the staged content before publishing
/// - Hash-while-streaming (perf audit D1): the digest is computed in the same
///   pass that writes the body, so verification needs no second full read
async fn download_single(
    client: &reqwest::Client,
    item: &DownloadItem,
    url: &str,
    _cache: Option<&CacheRepository>,
) -> Result<SingleDownload> {
    if let Some(parent) = item.dest.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    let part = part_path(&item.dest);

    // Resume from the staged `.part` file. `dest` only ever receives a
    // verified rename, so it is never appended to (P1-8): resuming onto it
    // both corrupted verified content and made hash-mismatch recovery
    // impossible.
    let existing_size = if part.exists() {
        tokio::fs::metadata(&part)
            .await
            .map(|m| m.len())
            .unwrap_or(0)
    } else {
        0
    };

    let mut request = client.get(url);
    let use_range = existing_size > 0;
    if use_range {
        request = request.header("Range", format!("bytes={}-", existing_size));
    }

    let mut response = request
        .send()
        .await
        .map_err(|e| RmcccError::Network(e.to_string()))?;
    let status = response.status();
    let is_resume = status.as_u16() == 206;

    // ── x-bmclapi-hash header ──
    // BMCLAPI provides an additional SHA1 hash in the response header.
    // We can use this for cache lookup to avoid re-downloading files
    // that the mirror has already verified.
    if let Some(bmclapi_hash) = response.headers().get("x-bmclapi-hash") {
        if let Ok(hash_str) = to_str_header(bmclapi_hash) {
            if hash_str.len() == 40 && hash_str.chars().all(|c| c.is_ascii_hexdigit()) {
                tracing::debug!("x-bmclapi-hash present for {url}: {hash_str}");
                // If we have a cache repo, try to find this file by the BMCLAPI hash
                // Only do this if we don't already have an expected SHA1 (which might differ)
                if item.expected_sha1.is_none() {
                    if let Some(cache) = _cache {
                        if let Some(cached_path) = cache.check_by_sha1(hash_str).await {
                            // Hardlink first — the cache was verified at store
                            // time (perf audit D7); copy + re-verify as fallback.
                            let hit = match tokio::fs::hard_link(&cached_path, &part).await {
                                Ok(()) => true,
                                Err(_) => {
                                    tokio::fs::copy(&cached_path, &part).await?;
                                    let actual = sha1_file(&part).await?;
                                    if actual.eq_ignore_ascii_case(hash_str) {
                                        true
                                    } else {
                                        tracing::warn!(
                                            "x-bmclapi-hash cache entry {} corrupt (expected {hash_str}, got {actual}); downloading",
                                            cached_path.display()
                                        );
                                        false
                                    }
                                }
                            };
                            if hit {
                                let meta = tokio::fs::metadata(&part).await?;
                                tracing::debug!("x-bmclapi-hash cache HIT for {url}");
                                return Ok(SingleDownload {
                                    size: meta.len(),
                                    sha1: Some(hash_str.to_string()),
                                });
                            }
                            let _ = tokio::fs::remove_file(&part).await;
                        }
                    }
                }
            }
        }
    }

    // HTTP 416: server rejected Range — delete the partial file and retry fresh.
    if status.as_u16() == 416 && use_range {
        tracing::debug!("Range not supported for {url}, retrying full download");
        let _ = tokio::fs::remove_file(&part).await;
        let mut response = client
            .get(url)
            .send()
            .await
            .map_err(|e| RmcccError::Network(e.to_string()))?;
        let status = response.status();
        if !status.is_success() {
            return Err(RmcccError::HttpStatus {
                status: status.as_u16(),
                url: url.to_string(),
            });
        }
        // P2-18：流式写入，避免整块读入内存
        let file = tokio::fs::File::create(&part).await?;
        let (downloaded, digest) = stream_to_file(&mut response, file).await?;
        return Ok(SingleDownload {
            size: downloaded,
            sha1: Some(digest),
        });
    }

    if !status.is_success() && !is_resume {
        return Err(RmcccError::HttpStatus {
            status: status.as_u16(),
            url: url.to_string(),
        });
    }

    let (size, sha1) = if is_resume {
        let file = tokio::fs::OpenOptions::new()
            .append(true)
            .open(&part)
            .await?;
        // P2-18：流式写入（chunk），避免 assets 数百 MB 整块读入内存。
        // Resumed prefix hash is unknown → `None` (caller re-reads if it
        // needs to verify).
        let (downloaded, _) = stream_to_file(&mut response, file).await?;
        (existing_size + downloaded, None)
    } else {
        let file = tokio::fs::File::create(&part).await?;
        let (downloaded, digest) = stream_to_file(&mut response, file).await?;
        (downloaded, Some(digest))
    };

    Ok(SingleDownload { size, sha1 })
}

/// Stream the response body into `file`, hashing as we go (perf audit D1/D4):
/// one pass, no verification re-read, and a 128 KiB write buffer instead of
/// one syscall per network chunk.
///
/// Returns `(bytes_written, sha1_hex)`.
async fn stream_to_file(
    response: &mut reqwest::Response,
    file: tokio::fs::File,
) -> Result<(u64, String)> {
    use sha1::{Digest, Sha1};
    let mut writer = tokio::io::BufWriter::with_capacity(128 * 1024, file);
    let mut hasher = Sha1::new();
    let mut downloaded: u64 = 0;
    loop {
        match response.chunk().await {
            Ok(Some(chunk)) => {
                hasher.update(&chunk);
                writer.write_all(&chunk).await?;
                downloaded += chunk.len() as u64;
            }
            Ok(None) => break,
            Err(e) => return Err(RmcccError::Network(e.to_string())),
        }
    }
    writer.flush().await?;
    Ok((downloaded, format!("{:x}", hasher.finalize())))
}

fn to_str_header(v: &reqwest::header::HeaderValue) -> std::result::Result<&str, ()> {
    v.to_str().map_err(|_| ())
}

/// Streaming SHA1 of a file (64 KiB chunks) — a full read into memory spiked
/// on large jars such as Forge installers (~180 MB).
async fn sha1_file(path: &Path) -> Result<String> {
    use sha1::{Digest, Sha1};
    let mut file = tokio::fs::File::open(path).await?;
    let mut hasher = Sha1::new();
    let mut buf = vec![0u8; 64 * 1024];
    loop {
        let n = file.read(&mut buf).await?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

pub fn check_disk_space(dest: &Path, required_bytes: u64) -> Result<()> {
    let available = fs2::free_space(dest).map_err(|e| RmcccError::Other(e.to_string()))?;
    if available < required_bytes {
        return Err(RmcccError::InsufficientDiskSpace {
            needed: required_bytes,
            available,
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn cache_rejects_malformed_sha1_keys() {
        // A4：缓存键来自下载的清单/镜像响应——短键、路径注入、多字节键
        // 一律按 miss/错误处理，绝不 panic，也绝不指向 objects/ 之外。
        let dir = std::env::temp_dir().join(format!("rmccc-cache-a4-{}", std::process::id()));
        let repo = CacheRepository::new(dir.clone());

        assert!(repo.check_by_sha1("ab").await.is_none());
        assert!(repo.check_by_sha1("../evil").await.is_none());
        assert!(repo.check_by_sha1(&"é".repeat(40)).await.is_none());
        assert!(repo.check_by_sha1("https://mirror/x").await.is_none());
        assert!(repo
            .store(Path::new("/nonexistent"), "../evil")
            .await
            .is_err());
        // 合法 40 位 hex 键不受影响（不存在的哈希 → miss）。
        assert!(repo
            .check_by_sha1("a1b2c3d4e5f6a7b8c9d0e1f2a3b4c5d6e7f8a9b0")
            .await
            .is_none());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn test_retry_delay() {
        let config = RetryConfig::default();
        assert_eq!(
            config.delay_for_attempt(0),
            std::time::Duration::from_secs(1)
        );
        assert_eq!(
            config.delay_for_attempt(1),
            std::time::Duration::from_secs(2)
        );
        assert_eq!(
            config.delay_for_attempt(2),
            std::time::Duration::from_secs(4)
        );
        assert_eq!(
            config.delay_for_attempt(10),
            std::time::Duration::from_secs(30)
        );
    }

    #[test]
    fn test_all_urls_primary_only() {
        let item = DownloadItem::new("https://example.com/file.jar", Path::new("/tmp/file.jar"));
        let urls = all_urls(&item);
        assert_eq!(urls.len(), 1);
        assert_eq!(urls[0], "https://example.com/file.jar");
    }

    #[test]
    fn test_all_urls_with_candidates() {
        let item = DownloadItem::new("https://example.com/file.jar", Path::new("/tmp/file.jar"))
            .with_candidates(vec![
                "https://mirror1.example.com/file.jar".to_string(),
                "https://mirror2.example.com/file.jar".to_string(),
            ]);
        let urls = all_urls(&item);
        assert_eq!(urls.len(), 3);
        assert_eq!(urls[1], "https://mirror1.example.com/file.jar");
    }

    // ── P1-8 regression: hash failures must never poison the destination ──

    type MockHandler = Arc<dyn Fn(usize, &str, Option<&str>) -> (u16, Vec<u8>) + Send + Sync>;

    /// Minimal HTTP/1.1 mock: one std thread serving until the process exits.
    /// The handler receives (request_index, path, Range header) and returns
    /// (status, body). Returns the base URL and the request counter.
    fn spawn_mock(handler: MockHandler) -> (String, Arc<AtomicU64>) {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let counter = Arc::new(AtomicU64::new(0));
        let counter_thread = counter.clone();
        std::thread::spawn(move || {
            use std::io::{Read, Write};
            for stream in listener.incoming() {
                let Ok(mut stream) = stream else { break };
                let mut buf = [0u8; 8192];
                let n = stream.read(&mut buf).unwrap_or(0);
                let req = String::from_utf8_lossy(&buf[..n]);
                let first = req.lines().next().unwrap_or("");
                let path = first.split_whitespace().nth(1).unwrap_or("/").to_string();
                let range = req.lines().find_map(|l| {
                    // hyper writes header names in lowercase.
                    l.strip_prefix("range: ").map(str::to_string)
                });
                let idx = counter_thread.fetch_add(1, Ordering::SeqCst) as usize;
                let (status, body) = handler(idx, &path, range.as_deref());
                let reason = match status {
                    200 => "OK",
                    206 => "Partial Content",
                    _ => "Error",
                };
                let resp = format!(
                    "HTTP/1.1 {status} {reason}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    body.len()
                );
                stream.write_all(resp.as_bytes()).ok();
                stream.write_all(&body).ok();
            }
        });
        (format!("http://{addr}/f.bin"), counter)
    }

    fn sha1_hex(data: &[u8]) -> String {
        use sha1::{Digest, Sha1};
        let mut h = Sha1::new();
        h.update(data);
        format!("{:x}", h.finalize())
    }

    fn temp_dir() -> PathBuf {
        std::env::temp_dir().join(format!("rmcl-dl-{}", uuid::Uuid::new_v4()))
    }

    fn item_at(url: String, dest: PathBuf, sha1: Option<String>) -> DownloadItem {
        let mut item = DownloadItem::new(&url, &dest);
        item.expected_sha1 = sha1;
        item
    }

    #[tokio::test]
    async fn test_hash_mismatch_falls_through_to_candidate() {
        let dir = temp_dir();
        let good = b"good-content".to_vec();
        let (good_url, _) = spawn_mock(Arc::new(|_, _, _| (200, b"good-content".to_vec())));
        let (bad_url, _) = spawn_mock(Arc::new(|_, _, _| (200, b"poison".to_vec())));

        let dest = dir.join("out.bin");
        let mut item = item_at(bad_url, dest.clone(), Some(sha1_hex(&good)));
        item.candidates = vec![good_url];

        let got = download_with_retry(&reqwest::Client::new(), &item, 1, None)
            .await
            .expect("candidate URL serves correct content");
        assert_eq!(std::fs::read(&dest).unwrap(), good);
        assert!(!part_path(&dest).exists(), "no staging file left behind");
        assert_eq!(got, good.len() as u64);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn test_hash_mismatch_retries_same_url_and_recovers() {
        let dir = temp_dir();
        let good = b"eventually-good".to_vec();
        let served = good.clone();
        // Attempt 0 serves poison; attempt 1 serves the real content.
        let (url, counter) = spawn_mock(Arc::new(move |idx, _, _| {
            if idx == 0 {
                (200, b"poison".to_vec())
            } else {
                (200, served.clone())
            }
        }));

        let dest = dir.join("out.bin");
        let item = item_at(url, dest.clone(), Some(sha1_hex(&good)));
        let got = download_with_retry(&reqwest::Client::new(), &item, 2, None)
            .await
            .expect("second attempt succeeds");
        assert_eq!(std::fs::read(&dest).unwrap(), good);
        assert_eq!(counter.load(Ordering::SeqCst), 2);
        assert_eq!(got, good.len() as u64);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn test_resume_uses_part_file_with_range() {
        let dir = temp_dir();
        let content = b"0123456789abcdef".to_vec();
        let served = content.clone();
        // Server honours Range: serves the remainder as 206.
        let (url, counter) = spawn_mock(Arc::new(move |_, _, range| {
            let offset: usize = range
                .and_then(|r| r.strip_prefix("bytes="))
                .and_then(|r| r.split('-').next())
                .and_then(|s| s.parse().ok())
                .unwrap_or(0);
            (206, served[offset..].to_vec())
        }));

        let dest = dir.join("out.bin");
        std::fs::create_dir_all(&dir).unwrap();
        // Simulate a previous interrupted download: first 4 bytes staged.
        std::fs::write(part_path(&dest), &content[..4]).unwrap();

        let item = item_at(url, dest.clone(), Some(sha1_hex(&content)));
        let got = download_with_retry(&reqwest::Client::new(), &item, 0, None)
            .await
            .expect("resumed download completes");
        assert_eq!(counter.load(Ordering::SeqCst), 1);
        assert_eq!(got, content.len() as u64);
        assert_eq!(std::fs::read(&dest).unwrap(), content);
        assert!(!part_path(&dest).exists(), "staging file published");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn test_complete_dest_skips_network() {
        let dir = temp_dir();
        let good = b"already-here".to_vec();
        let (url, counter) = spawn_mock(Arc::new(|_, _, _| (200, b"nope".to_vec())));

        let dest = dir.join("out.bin");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(&dest, &good).unwrap();

        let item = item_at(url, dest.clone(), Some(sha1_hex(&good)));
        download_with_retry(&reqwest::Client::new(), &item, 2, None)
            .await
            .expect("complete dest short-circuits");
        assert_eq!(
            counter.load(Ordering::SeqCst),
            0,
            "no network request for a verified-complete file"
        );
        assert_eq!(std::fs::read(&dest).unwrap(), good);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn test_corrupt_dest_is_replaced() {
        let dir = temp_dir();
        let good = b"verified-content".to_vec();
        let served = good.clone();
        let (url, _) = spawn_mock(Arc::new(move |_, _, _| (200, served.clone())));

        let dest = dir.join("out.bin");
        std::fs::create_dir_all(&dir).unwrap();
        // Poisoned legacy file (pre-.part download, appended Range data).
        std::fs::write(&dest, b"poisoned-appended-data").unwrap();

        let item = item_at(url, dest.clone(), Some(sha1_hex(&good)));
        download_with_retry(&reqwest::Client::new(), &item, 0, None)
            .await
            .expect("corrupt dest is replaced by a verified download");
        assert_eq!(std::fs::read(&dest).unwrap(), good);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn test_hash_mismatch_exhausts_urls_leaves_nothing_behind() {
        let dir = temp_dir();
        let (url1, _) = spawn_mock(Arc::new(|_, _, _| (200, b"bad-1".to_vec())));
        let (url2, _) = spawn_mock(Arc::new(|_, _, _| (200, b"bad-2".to_vec())));

        let dest = dir.join("out.bin");
        let mut item = item_at(url1, dest.clone(), Some(sha1_hex(b"expected")));
        item.candidates = vec![url2];

        let err = download_with_retry(&reqwest::Client::new(), &item, 0, None)
            .await
            .expect_err("all candidates serve wrong content");
        assert!(matches!(err, RmcccError::HashMismatch { .. }));
        assert!(!dest.exists(), "dest is never written with unverified data");
        assert!(
            !part_path(&dest).exists(),
            "staging file cleaned on failure"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
