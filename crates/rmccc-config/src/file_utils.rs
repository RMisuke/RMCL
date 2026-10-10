//! Safe file utilities — back up invalid configs before overwriting.
//!
//! Mirrors HMCL's [`SettingFileUtils.backupInvalidConfig`]: a malformed or
//! incompatible settings file is copied to a numbered `xxx.json.1` backup
//! before being replaced, so the original data is never silently lost.
//!
//! [`SettingFileUtils.backupInvalidConfig`]: https://github.com/HMCL-dev/HMCL/blob/master/HMCL/src/main/java/org/jackhuang/hmcl/setting/SettingFileUtils.java

use std::path::{Path, PathBuf};

/// Backs up `location` as `xxx.json.1`, `xxx.json.2`, ... (first free index).
///
/// Returns the backup path, or `None` when the file does not exist or the copy
/// fails (a failed backup is logged and non-fatal — the caller still proceeds
/// to overwrite the invalid file).
pub fn backup_invalid_config(location: &Path) -> Option<PathBuf> {
    if !location.exists() {
        return None;
    }
    let backup = next_backup_path(location);
    match std::fs::copy(location, &backup) {
        Ok(_) => {
            tracing::info!(
                from = %location.display(),
                to = %backup.display(),
                "Backed up config file"
            );
            Some(backup)
        }
        Err(e) => {
            tracing::warn!(
                path = %location.display(),
                error = %e,
                "Failed to back up config file"
            );
            None
        }
    }
}

/// Computes the first non-existing numbered backup path (`xxx.json.1`, ...).
fn next_backup_path(location: &Path) -> PathBuf {
    let file_name = location
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();
    let mut index = 1u32;
    loop {
        let candidate = location.with_file_name(format!("{file_name}.{index}"));
        if !candidate.exists() {
            return candidate;
        }
        index += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn test_backup_creates_numbered_copy() {
        let dir = std::env::temp_dir().join(format!(
            "rmccc-backup-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        let path = dir.join("launcher-settings.json");
        let mut f = std::fs::File::create(&path).unwrap();
        f.write_all(b"{\"broken\": true}").unwrap();
        f.flush().unwrap();

        let backup = backup_invalid_config(&path).expect("backup must be created");
        assert_eq!(backup, dir.join("launcher-settings.json.1"));
        assert!(backup.exists());
        assert_eq!(
            std::fs::read_to_string(&backup).unwrap(),
            "{\"broken\": true}"
        );

        // Second backup picks the next free index.
        let backup2 = backup_invalid_config(&path).expect("second backup must be created");
        assert_eq!(backup2, dir.join("launcher-settings.json.2"));

        // Missing file → None.
        assert!(backup_invalid_config(&dir.join("nope.json")).is_none());

        let _ = std::fs::remove_dir_all(&dir);
    }
}
