//! 插件状态文件的原子读写与备份（ADR-003 决策 3，指南 §12.3）。

use rmccc_error::Result;
use rmccc_platform::PlatformFs;
use serde::de::DeserializeOwned;
use serde::Serialize;
use std::path::{Path, PathBuf};

pub fn now_rfc3339() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
}

/// 写 JSON：临时文件 → fsync → `PlatformFs::atomic_replace`。
///
/// 失败时清理临时文件，**原文件保持不变**。
pub fn write_json_atomic<T: Serialize>(fs: &dyn PlatformFs, path: &Path, value: &T) -> Result<()> {
    let bytes = serde_json::to_vec_pretty(value)?;
    write_bytes_atomic(fs, path, &bytes)
}

pub fn write_bytes_atomic(fs: &dyn PlatformFs, path: &Path, bytes: &[u8]) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let tmp = tmp_path(path);
    let result = (|| -> Result<()> {
        {
            use std::io::Write;
            let mut file = std::fs::File::create(&tmp)?;
            file.write_all(bytes)?;
            file.sync_all()?;
        }
        fs.atomic_replace(&tmp, path)?;
        Ok(())
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    result
}

fn tmp_path(path: &Path) -> PathBuf {
    let name = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("state.json");
    path.with_file_name(format!(".{name}.tmp-{}", uuid::Uuid::new_v4()))
}

/// 读取 JSON；文件不存在返回 `None`。
pub fn read_json<T: DeserializeOwned>(path: &Path) -> Result<Option<T>> {
    match std::fs::read(path) {
        Ok(bytes) => Ok(Some(serde_json::from_slice(&bytes)?)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e.into()),
    }
}

/// 把损坏/不可读的文件改名备份（不删除用户数据）。
pub fn backup_file(path: &Path) -> Result<Option<PathBuf>> {
    if !path.exists() {
        return Ok(None);
    }
    let name = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("state.json");
    let backup = path.with_file_name(format!(
        "{name}.bak-{}",
        chrono::Utc::now().format("%Y%m%dT%H%M%S")
    ));
    std::fs::rename(path, &backup)?;
    Ok(Some(backup))
}

#[cfg(test)]
mod tests {
    use super::*;
    use rmccc_platform::Platform;

    fn temp_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "rmccc-plugin-storage-{tag}-{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn writes_and_reads_json_atomically() {
        let dir = temp_dir("json");
        let platform = Platform::current();
        let path = dir.join("state/registry.json");
        #[derive(serde::Serialize, serde::Deserialize, PartialEq, Debug)]
        struct Doc {
            a: u32,
        }
        write_json_atomic(platform.fs.as_ref(), &path, &Doc { a: 7 }).unwrap();
        let read: Doc = read_json(&path).unwrap().unwrap();
        assert_eq!(read, Doc { a: 7 });
        // 临时文件不残留。
        let leftovers: Vec<_> = std::fs::read_dir(path.parent().unwrap())
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_name().to_string_lossy().contains(".tmp-"))
            .collect();
        assert!(leftovers.is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn missing_file_is_none() {
        let dir = temp_dir("missing");
        let got: Option<serde_json::Value> = read_json(&dir.join("nope.json")).unwrap();
        assert!(got.is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn backup_preserves_original_bytes() {
        let dir = temp_dir("backup");
        let path = dir.join("registry.json");
        std::fs::write(&path, b"not json").unwrap();
        let backup = backup_file(&path).unwrap().unwrap();
        assert!(!path.exists());
        assert_eq!(std::fs::read(&backup).unwrap(), b"not json");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
