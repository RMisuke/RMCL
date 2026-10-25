//! Icon/avatar disk cache（图标缓存子系统）.
//!
//! 设计：stale-while-revalidate —— 界面永远先读磁盘（`rmcccicon://` 协议直出，
//! 无网络、无闪烁），后台按条件请求（`If-None-Match` / `If-Modified-Since`）
//! 刷新；URL 即缓存键（玩家换皮肤会得到新 texture URL，天然版本化）。
//!
//! 布局（`<data_dir>/cache/icons/`）：
//! - `<sha1(url)>.bin`      图标本体（≤2 MiB）
//! - `index.json`           条目元数据（etag/size/last_used/kind），LRU 淘汰依据
//! - `config.json`          开关 + 容量上限（默认 1 GiB）
//!
//! 并发：所有入口都在 async runtime 上执行，core 锁必须 `.read().await`
//! （`blocking_read()` 在 async 上下文会 panic，曾导致启动即闪退）。

use crate::state::AppState;
use rmccc_error::{ErrorCode, IpcError};
use serde::{Deserialize, Serialize};
use sha1::{Digest, Sha1};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{LazyLock, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use ts_rs::TS;

/// 单个图标的大小上限（头像/Mod 图标都远小于此）。
const MAX_ICON_BYTES: usize = 2 * 1024 * 1024;
/// 条目新鲜度窗口（秒）：此间隔内跳过网络校验。
const FETCH_FRESH_WINDOW: u64 = 600;
/// 默认容量上限：1 GiB。
const DEFAULT_MAX_BYTES: u64 = 1024 * 1024 * 1024;
/// 缓存协议 scheme（`rmcccicon://k/<urlencoded>`）。
pub const ICON_SCHEME: &str = "rmcccicon";

// ── 持久化结构 ──

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CacheConfig {
    #[serde(default = "default_enabled")]
    enabled: bool,
    #[serde(default = "default_max_bytes")]
    max_bytes: u64,
}

fn default_enabled() -> bool {
    true
}

fn default_max_bytes() -> u64 {
    DEFAULT_MAX_BYTES
}

impl Default for CacheConfig {
    fn default() -> Self {
        Self {
            enabled: default_enabled(),
            max_bytes: default_max_bytes(),
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct CacheEntry {
    #[serde(default)]
    etag: Option<String>,
    #[serde(default)]
    last_modified: Option<String>,
    #[serde(default)]
    content_type: String,
    #[serde(default)]
    size: u64,
    /// Unix 秒：网络校验/下载时间。
    #[serde(default)]
    fetched_at: u64,
    /// Unix 秒：最近一次被使用（协议命中/条目刷新）。
    #[serde(default)]
    last_used_at: u64,
    /// `avatar` | `modicon`（仅用于展示与排查，不参与淘汰策略）。
    #[serde(default)]
    kind: String,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct CacheIndex {
    #[serde(default)]
    entries: HashMap<String, CacheEntry>,
}

// ── 进程级缓存状态 ──

struct IconCache {
    dir: PathBuf,
    config: CacheConfig,
    index: CacheIndex,
}

static HTTP: LazyLock<reqwest::Client> = LazyLock::new(|| {
    reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        .build()
        .unwrap_or_default()
});

static CACHE: LazyLock<Mutex<Option<IconCache>>> = LazyLock::new(|| Mutex::new(None));

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn key_for(url: &str) -> String {
    let mut hasher = Sha1::new();
    hasher.update(url.as_bytes());
    format!("{:x}", hasher.finalize())
}

fn read_json<T: for<'de> Deserialize<'de>>(path: &std::path::Path, fallback: T) -> T {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or(fallback)
}

impl IconCache {
    fn open(data_dir: &std::path::Path) -> Self {
        let dir = data_dir.join("cache").join("icons");
        let _ = std::fs::create_dir_all(&dir);
        let config = read_json(&dir.join("config.json"), CacheConfig::default());
        let index = read_json(&dir.join("index.json"), CacheIndex::default());
        Self { dir, config, index }
    }

    fn persist_config(&self) {
        if let Ok(text) = serde_json::to_string_pretty(&self.config) {
            let _ = std::fs::write(self.dir.join("config.json"), text);
        }
    }

    fn persist_index(&self) {
        if let Ok(text) = serde_json::to_string(&self.index) {
            let _ = std::fs::write(self.dir.join("index.json"), text);
        }
    }

    fn file_for(&self, key: &str) -> PathBuf {
        // key 是 40 位 hex sha1，天然无路径穿越风险；仍兜底过滤。
        if key.len() != 40 || !key.chars().all(|c| c.is_ascii_hexdigit()) {
            return self.dir.join("invalid");
        }
        self.dir.join(format!("{key}.bin"))
    }

    fn total_bytes(&self) -> u64 {
        self.index.entries.values().map(|e| e.size).sum()
    }

    /// 超容量按 last_used 最旧淘汰（写路径调用；索引同步落盘）。
    fn evict_over_budget(&mut self) {
        while self.total_bytes() > self.config.max_bytes && !self.index.entries.is_empty() {
            let Some(oldest) = self
                .index
                .entries
                .iter()
                .min_by_key(|(_, e)| e.last_used_at)
                .map(|(k, _)| k.clone())
            else {
                break;
            };
            let _ = std::fs::remove_file(self.file_for(&oldest));
            self.index.entries.remove(&oldest);
            tracing::debug!(key = %oldest, "icon cache evicted (over budget)");
        }
    }
}

/// 访问缓存（首次惰性打开；data_dir 来自配置解析）。
async fn with_cache<T>(
    state: &AppState,
    f: impl FnOnce(&mut IconCache) -> rmccc_error::Result<T>,
) -> rmccc_error::Result<T> {
    let data_dir = {
        let core = state.core.read().await;
        core.config.resolved_data_dir()
    };
    let mut guard = CACHE.lock().unwrap_or_else(|p| p.into_inner());
    if guard.is_none() {
        *guard = Some(IconCache::open(&data_dir));
    }
    f(guard.as_mut().expect("cache initialized"))
}

// ── 供协议处理器调用 ──

pub struct ServedIcon {
    pub bytes: Vec<u8>,
    pub content_type: String,
}

/// `rmcccicon://k/<urlencoded>` → 磁盘内容。命中即视为"使用"（更新 last_used）。
pub async fn lookup(state: &AppState, url: &str) -> Option<ServedIcon> {
    let key = key_for(url);
    let path = with_cache(state, |cache| {
        let Some(entry) = cache.index.entries.get(&key).cloned() else {
            return Ok(None);
        };
        cache.index.entries.insert(
            key.clone(),
            CacheEntry {
                last_used_at: now_secs(),
                ..entry
            },
        );
        Ok(Some(cache.file_for(&key)))
    })
    .await
    .ok()
    .flatten()?;
    let bytes = std::fs::read(&path).ok()?;
    let content_type = with_cache(state, |cache| {
        Ok(cache
            .index
            .entries
            .get(&key)
            .map(|e| e.content_type.clone())
            .unwrap_or_else(|| "application/octet-stream".into()))
    })
    .await
    .ok()
    .unwrap_or_else(|| "application/octet-stream".into());
    // last_used 只在内存里推进；索引随下一次 fetch/clear 落盘（避免每次渲染写盘）。
    Some(ServedIcon {
        bytes,
        content_type,
    })
}

// ── IPC 视图 ──

#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export_to = "types.ts")]
pub struct IconCacheStats {
    pub enabled: bool,
    #[ts(type = "number")]
    pub max_bytes: u64,
    #[ts(type = "number")]
    pub total_bytes: u64,
    #[ts(type = "number")]
    pub entries: u64,
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export_to = "types.ts")]
pub struct IconFetchStatus {
    /// `hit`（新鲜缓存）/ `updated`（重新下载）/ `unchanged`（304）
    /// / `stale`（网络失败保留旧档）/ `miss` / `disabled` / `too_large`
    pub status: String,
    pub cached: bool,
}

// ── IPC 命令 ──

#[tauri::command]
#[tracing::instrument(skip(state), fields(url = url))]
pub async fn cache_icon_fetch(
    state: tauri::State<'_, AppState>,
    url: String,
    kind: Option<String>,
) -> Result<IconFetchStatus, IpcError> {
    // 校验：仅 http(s)，且仅允许已知的图标/皮肤域名（数据外发边界）。
    let parsed = reqwest::Url::parse(&url).map_err(|e| IpcError {
        code: ErrorCode::ValidationError,
        message: format!("invalid icon url: {e}"),
        retryable: false,
    })?;
    let host = parsed.host_str().unwrap_or_default().to_string();
    let allowed = [
        "textures.minecraft.net",
        "sessionserver.mojang.com",
        "cdn.modrinth.com",
    ]
    .iter()
    .any(|h| host == *h || host.ends_with(&format!(".{h}")));
    if !matches!(parsed.scheme(), "http" | "https") || !allowed {
        return Err(IpcError {
            code: ErrorCode::ValidationError,
            message: "icon cache only accepts known image hosts".into(),
            retryable: false,
        });
    }
    let kind = kind.unwrap_or_else(|| "modicon".into());

    // 1) 快路径：开关关闭 / 新鲜窗口内 → 直接返回。
    let key = key_for(&url);
    let snapshot = with_cache(state.inner(), |cache| {
        if !cache.config.enabled {
            return Ok(None);
        }
        let Some(entry) = cache.index.entries.get(&key).cloned() else {
            // 无条目：空壳触发首次下载。
            return Ok(Some((CacheEntry::default(), false)));
        };
        let fresh = now_secs().saturating_sub(entry.fetched_at) < FETCH_FRESH_WINDOW;
        Ok(Some((entry, fresh)))
    })
    .await?;
    let Some((entry, fresh)) = snapshot else {
        return Ok(IconFetchStatus {
            status: "disabled".into(),
            cached: false,
        });
    };
    if fresh {
        return Ok(IconFetchStatus {
            status: "hit".into(),
            cached: true,
        });
    }

    // 2) 条件请求（锁外执行网络）。
    let mut req = HTTP.get(&url);
    if let Some(etag) = &entry.etag {
        req = req.header(reqwest::header::IF_NONE_MATCH, etag);
    }
    if let Some(lm) = &entry.last_modified {
        req = req.header(reqwest::header::IF_MODIFIED_SINCE, lm);
    }
    let response = req.send().await;

    async fn commit(
        state: &AppState,
        key: &str,
        kind: &str,
        bytes: Option<Vec<u8>>,
        etag: Option<String>,
        last_modified: Option<String>,
        content_type: Option<String>,
    ) -> rmccc_error::Result<()> {
        with_cache(state, |cache| {
            if let Some(bytes) = &bytes {
                let _ = std::fs::write(cache.file_for(key), bytes);
            }
            let size = bytes
                .map(|b| b.len() as u64)
                .unwrap_or_else(|| cache.index.entries.get(key).map(|e| e.size).unwrap_or(0));
            let previous = cache.index.entries.get(key);
            let merged = CacheEntry {
                etag: etag.or_else(|| previous.and_then(|p| p.etag.clone())),
                last_modified: last_modified
                    .or_else(|| previous.and_then(|p| p.last_modified.clone())),
                content_type: content_type.unwrap_or_else(|| {
                    previous
                        .map(|p| p.content_type.clone())
                        .unwrap_or_else(|| "image/png".into())
                }),
                size,
                fetched_at: now_secs(),
                last_used_at: now_secs(),
                kind: kind.to_string(),
            };
            cache.index.entries.insert(key.to_string(), merged);
            cache.evict_over_budget();
            cache.persist_index();
            Ok(())
        })
        .await
    }

    let state_ref = state.inner();
    match response {
        Ok(resp) if resp.status() == reqwest::StatusCode::NOT_MODIFIED => {
            commit(state_ref, &key, &kind, None, None, None, None)
                .await
                .map_err(IpcError::from)?;
            Ok(IconFetchStatus {
                status: "unchanged".into(),
                cached: true,
            })
        }
        Ok(resp) if resp.status().is_success() => {
            let etag = resp
                .headers()
                .get(reqwest::header::ETAG)
                .and_then(|v| v.to_str().ok())
                .map(str::to_string);
            let last_modified = resp
                .headers()
                .get(reqwest::header::LAST_MODIFIED)
                .and_then(|v| v.to_str().ok())
                .map(str::to_string);
            let content_type = resp
                .headers()
                .get(reqwest::header::CONTENT_TYPE)
                .and_then(|v| v.to_str().ok())
                .map(str::to_string);
            let bytes = resp.bytes().await.map_err(|e| IpcError {
                code: ErrorCode::NetworkError,
                message: format!("icon download failed: {e}"),
                retryable: true,
            })?;
            if bytes.len() > MAX_ICON_BYTES {
                return Ok(IconFetchStatus {
                    status: "too_large".into(),
                    cached: false,
                });
            }
            commit(
                state_ref,
                &key,
                &kind,
                Some(bytes.to_vec()),
                etag,
                last_modified,
                content_type,
            )
            .await
            .map_err(IpcError::from)?;
            Ok(IconFetchStatus {
                status: "updated".into(),
                cached: true,
            })
        }
        // 网络失败但有旧档：保留旧档继续用（stale-while-error）。
        Ok(_) if !entry.content_type.is_empty() => {
            commit(state_ref, &key, &kind, None, None, None, None)
                .await
                .map_err(IpcError::from)?;
            Ok(IconFetchStatus {
                status: "stale".into(),
                cached: true,
            })
        }
        Ok(_) => Ok(IconFetchStatus {
            status: "miss".into(),
            cached: false,
        }),
        Err(_) if !entry.content_type.is_empty() => {
            commit(state_ref, &key, &kind, None, None, None, None)
                .await
                .map_err(IpcError::from)?;
            Ok(IconFetchStatus {
                status: "stale".into(),
                cached: true,
            })
        }
        Err(_) => Ok(IconFetchStatus {
            status: "miss".into(),
            cached: false,
        }),
    }
}

#[tauri::command]
#[tracing::instrument(skip(state))]
pub async fn cache_icon_stats(
    state: tauri::State<'_, AppState>,
) -> Result<IconCacheStats, IpcError> {
    with_cache(state.inner(), |cache| {
        Ok(IconCacheStats {
            enabled: cache.config.enabled,
            max_bytes: cache.config.max_bytes,
            total_bytes: cache.total_bytes(),
            entries: cache.index.entries.len() as u64,
        })
    })
    .await
    .map_err(IpcError::from)
}

#[tauri::command]
#[tracing::instrument(skip(state))]
pub async fn cache_icon_set_config(
    state: tauri::State<'_, AppState>,
    enabled: Option<bool>,
    max_bytes: Option<u64>,
) -> Result<IconCacheStats, IpcError> {
    with_cache(state.inner(), |cache| {
        if let Some(enabled) = enabled {
            cache.config.enabled = enabled;
        }
        if let Some(max_bytes) = max_bytes {
            // 下限 64 MiB，防止误填 0 造成抖动。
            cache.config.max_bytes = max_bytes.max(64 * 1024 * 1024);
        }
        cache.persist_config();
        cache.evict_over_budget();
        cache.persist_index();
        Ok(IconCacheStats {
            enabled: cache.config.enabled,
            max_bytes: cache.config.max_bytes,
            total_bytes: cache.total_bytes(),
            entries: cache.index.entries.len() as u64,
        })
    })
    .await
    .map_err(IpcError::from)
}

#[tauri::command]
#[tracing::instrument(skip(state))]
pub async fn cache_icon_clear(
    state: tauri::State<'_, AppState>,
) -> Result<IconCacheStats, IpcError> {
    with_cache(state.inner(), |cache| {
        let paths: Vec<PathBuf> = std::fs::read_dir(&cache.dir)
            .map(|rd| rd.flatten().map(|e| e.path()).collect())
            .unwrap_or_default();
        for path in paths {
            if path.extension().and_then(|e| e.to_str()) == Some("bin") {
                let _ = std::fs::remove_file(path);
            }
        }
        cache.index.entries.clear();
        cache.persist_index();
        Ok(IconCacheStats {
            enabled: cache.config.enabled,
            max_bytes: cache.config.max_bytes,
            total_bytes: 0,
            entries: 0,
        })
    })
    .await
    .map_err(IpcError::from)
}

/// `rmcccicon://k/<urlencoded>` 的 URL 解码（encodeURIComponent 产物）。
pub fn decode_icon_uri_path(path: &str) -> Option<String> {
    let encoded = path.strip_prefix('/')?;
    let bytes = encoded.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'%' => {
                let hex = bytes.get(i + 1..i + 3)?;
                let hi = (hex[0] as char).to_digit(16)?;
                let lo = (hex[1] as char).to_digit(16)?;
                out.push((hi * 16 + lo) as u8);
                i += 3;
            }
            b => {
                out.push(b);
                i += 1;
            }
        }
    }
    String::from_utf8(out).ok()
}
