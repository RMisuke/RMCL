//! 游戏日志的「新鲜」读取（ADR-013 S2 重构）。
//!
//! 分析输入必须属于**当前/最近一次运行**。优先级：
//!
//! 1. `game_output`：启动器捕获的进程输出。`launch_start` 时清空、仅累积本轮 →
//!    天然 run-scoped，是「运行中 / 刚退出」最可信的来源；
//! 2. 磁盘 `latest.log`：仅当 `mtime ≥ run.started_at` 时才视为本轮日志。启动早期
//!    log4j 尚未重写文件，旧 `latest.log` 仍在磁盘上，**绝不能**把它当本轮；
//! 3. 无 run 记录（App 重启后分析上次崩溃）：磁盘尾读，标注 [`LogSourceKind::DiskUnbound`]。
//!
//! AI 分析与 MCP `logs.game` 共用本模块，避免两套回退逻辑行为漂移。

use crate::state::AppState;
use rmccc_error::RmcccError;
use std::path::{Path, PathBuf};

/// 日志来源。前端与 MCP 据此判断"是否本轮"。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogSourceKind {
    /// 启动器捕获的本轮进程输出。
    Captured,
    /// 磁盘 `latest.log`，且 mtime 不早于本次启动。
    DiskFresh,
    /// 磁盘 `latest.log`，但无运行记录可校验（App 重启后的尽力而为）。
    DiskUnbound,
}

impl LogSourceKind {
    pub fn as_str(self) -> &'static str {
        match self {
            LogSourceKind::Captured => "captured",
            LogSourceKind::DiskFresh => "disk-fresh",
            LogSourceKind::DiskUnbound => "disk-unbound",
        }
    }
}

#[derive(Debug, Clone)]
pub struct ResolvedLog {
    /// 尾部窗口内的原始文本（未脱敏、未消化）。
    pub text: String,
    pub kind: LogSourceKind,
    /// 磁盘来源的绝对路径；captured 为 None。
    pub path: Option<PathBuf>,
    /// 来源的完整行数（用于前端显示"共 N 行，发送 M 行"）。
    pub total_lines: usize,
}

#[derive(Debug)]
pub enum LogError {
    /// 无法证明日志属于本次运行（或是本轮尚无日志）。
    NoFreshLog(String),
    Io(std::io::Error),
}

impl LogError {
    pub fn message(&self) -> String {
        match self {
            LogError::NoFreshLog(message) => message.clone(),
            LogError::Io(error) => format!("读取游戏日志失败：{error}"),
        }
    }
}

impl From<std::io::Error> for LogError {
    fn from(error: std::io::Error) -> Self {
        LogError::Io(error)
    }
}

impl From<LogError> for RmcccError {
    fn from(error: LogError) -> Self {
        match error {
            LogError::NoFreshLog(message) => RmcccError::Validation(message),
            LogError::Io(error) => RmcccError::Io(error),
        }
    }
}

/// 解析某实例（或共享目录）的**新鲜**日志尾部。
///
/// `window_chars` 是返回文本的字符上限（调用方按提交模式给不同预算）。
pub async fn resolve_fresh_log(
    state: &AppState,
    profile_id: Option<&str>,
    window_chars: usize,
) -> Result<ResolvedLog, LogError> {
    match profile_id {
        Some(key) => resolve_instance(state, key, window_chars).await,
        None => {
            let path = shared_log_path(state).await;
            let (text, _) = read_tail(&path, window_chars).await?.ok_or_else(|| {
                LogError::NoFreshLog(format!(
                    "未找到游戏日志（{}），请先启动一次游戏再分析",
                    path.display()
                ))
            })?;
            Ok(ResolvedLog {
                total_lines: text.lines().count(),
                text,
                kind: LogSourceKind::DiskUnbound,
                path: Some(path),
            })
        }
    }
}

async fn resolve_instance(
    state: &AppState,
    key: &str,
    window_chars: usize,
) -> Result<ResolvedLog, LogError> {
    // 1) captured：天然属于本轮（launch_start 已清空），优先。
    let captured = state.game_output.get(key).map(|buffer| {
        buffer
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .join("\n")
    });
    if let Some(raw) = captured.filter(|text| !text.trim().is_empty()) {
        let total_lines = raw.lines().count();
        let text = tail_chars(&raw, window_chars).to_string();
        return Ok(ResolvedLog {
            text,
            kind: LogSourceKind::Captured,
            path: None,
            total_lines,
        });
    }

    // 2) 磁盘：需要 run 记录校验 mtime。
    let path = instance_log_path(state, key).await?;
    let run = state
        .runs
        .get(key)
        .map(|r| (r.run_id.clone(), r.started_at_ms));
    match read_tail(&path, window_chars).await {
        Ok(Some((text, mtime_ms))) => match run {
            Some((run_id, started_at_ms)) if mtime_ms.saturating_add(2_000) < started_at_ms => {
                Err(LogError::NoFreshLog(format!(
                    "本次运行（{run_id}）尚未写入日志（latest.log 早于本次启动），请稍后再分析"
                )))
            }
            Some(_) => Ok(ResolvedLog {
                total_lines: text.lines().count(),
                text,
                kind: LogSourceKind::DiskFresh,
                path: Some(path),
            }),
            None => Ok(ResolvedLog {
                total_lines: text.lines().count(),
                text,
                kind: LogSourceKind::DiskUnbound,
                path: Some(path),
            }),
        },
        Ok(None) => Err(LogError::NoFreshLog(format!(
            "未找到游戏日志（{}），请先启动一次游戏再分析",
            path.display()
        ))),
        Err(error) => Err(LogError::Io(error)),
    }
}

async fn instance_log_path(state: &AppState, key: &str) -> Result<PathBuf, LogError> {
    let core = state.core.read().await;
    let mut service = core
        .instances
        .lock()
        .map_err(|_| LogError::NoFreshLog("实例服务锁不可用".into()))?;
    let detail = service
        .get(key)
        .map_err(|error| LogError::NoFreshLog(error.to_string()))?;
    Ok(detail.paths.logs_dir.join("latest.log"))
}

async fn shared_log_path(state: &AppState) -> PathBuf {
    let core = state.core.read().await;
    core.config
        .resolved_game_root()
        .join("logs")
        .join("latest.log")
}

/// 尾读文件（off-thread，避免把整份大日志读进 async worker）。
/// 返回 `(尾部文本, mtime_ms)`；文件缺失/为空返回 `Ok(None)`。
async fn read_tail(path: &Path, window_chars: usize) -> std::io::Result<Option<(String, u64)>> {
    let path = path.to_path_buf();
    tokio::task::spawn_blocking(move || -> std::io::Result<Option<(String, u64)>> {
        use std::io::{Read, Seek, SeekFrom};
        let mut file = match std::fs::File::open(&path) {
            Ok(file) => file,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(error),
        };
        let meta = file.metadata()?;
        let mtime_ms = meta
            .modified()
            .ok()
            .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|duration| duration.as_millis() as u64)
            .unwrap_or(0);
        // ×3 字节覆盖多字节 UTF-8；精确截断由 `tail_chars` 按字符完成。
        let want = (window_chars as u64).saturating_mul(3).min(meta.len());
        if want > 0 {
            file.seek(SeekFrom::End(-(want as i64)))?;
        }
        let mut buf = Vec::with_capacity(want as usize);
        file.read_to_end(&mut buf)?;
        let text = String::from_utf8_lossy(&buf).into_owned();
        if text.trim().is_empty() {
            return Ok(None);
        }
        Ok(Some((
            tail_chars(&text, window_chars).to_string(),
            mtime_ms,
        )))
    })
    .await
    .map_err(|error| std::io::Error::other(format!("log tail read join error: {error}")))?
}

/// 取尾部 `max_chars` 个字符（按字符边界截断，不劈开多字节字符）。
pub fn tail_chars(text: &str, max_chars: usize) -> &str {
    let total = text.chars().count();
    if total <= max_chars {
        return text;
    }
    let start = text
        .char_indices()
        .nth(total - max_chars)
        .map(|(index, _)| index)
        .unwrap_or(0);
    text[start..].trim_start_matches('\n')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tail_chars_keeps_the_tail_and_cuts_on_char_boundary() {
        let text = "abcdefghij";
        assert_eq!(tail_chars(text, 20), text);
        assert_eq!(tail_chars(text, 4), "ghij");
        let cjk = "一二三四五";
        assert_eq!(tail_chars(cjk, 2), "四五");
    }

    #[test]
    fn source_kinds_have_stable_labels() {
        assert_eq!(LogSourceKind::Captured.as_str(), "captured");
        assert_eq!(LogSourceKind::DiskFresh.as_str(), "disk-fresh");
        assert_eq!(LogSourceKind::DiskUnbound.as_str(), "disk-unbound");
    }
}
