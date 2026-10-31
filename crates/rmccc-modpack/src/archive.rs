//! ZIP access helpers: encoding-tolerant entry names, path sanitisation, and
//! bounded JSON reads.
//!
//! HMCL opens archives with a chosen charset (UTF-8 first, then GBK on
//! Chinese systems). We reproduce that fallback without pulling a charset
//! detector: raw bytes that are valid UTF-8 are used as-is, otherwise GBK.

use rmccc_error::{Result, RmcccError};
use std::collections::HashMap;
use std::fs::File;
use std::io::Read;
use std::path::Path;

/// Upper bound for manifest/config entries (HMCL has no explicit cap).
pub const MAX_MANIFEST_BYTES: u64 = 16 * 1024 * 1024;

/// One zip entry, decoded and sanitised at open time.
#[derive(Debug, Clone)]
pub struct EntryRecord {
    pub index: usize,
    /// Normalised path (`/` separators, no leading slash).
    pub name: String,
    pub is_dir: bool,
    pub is_symlink: bool,
    pub size: u64,
}

/// Opened modpack archive with a normalised entry index.
pub struct Archive {
    zip: zip::ZipArchive<File>,
    entries: Vec<EntryRecord>,
    by_name: HashMap<String, usize>,
}

impl Archive {
    /// Open a zip file and index its entries.
    pub fn open(path: &Path) -> Result<Self> {
        let file = File::open(path)?;
        let zip = zip::ZipArchive::new(file)
            .map_err(|e| RmcccError::Validation(format!("not a valid zip archive: {e}")))?;
        let mut archive = Self {
            zip,
            entries: Vec::new(),
            by_name: HashMap::new(),
        };
        archive.index_entries()?;
        Ok(archive)
    }

    fn index_entries(&mut self) -> Result<()> {
        let len = self.zip.len();
        let mut records = Vec::with_capacity(len);
        for index in 0..len {
            let file = self.zip.by_index(index).map_err(|e| {
                RmcccError::Validation(format!("zip entry #{index} is unreadable: {e}"))
            })?;
            let raw = file.name_raw().to_vec();
            let name = match safe_relative_path(&decode_name(&raw)) {
                Some(name) if !name.is_empty() => name,
                _ => continue,
            };
            let is_dir = file.is_dir() || name.ends_with('/');
            let mode = file.unix_mode().unwrap_or(0);
            let is_symlink = (mode & 0o170000) == 0o120000;
            records.push(EntryRecord {
                index,
                name,
                is_dir,
                is_symlink,
                size: file.size(),
            });
        }
        self.by_name = records
            .iter()
            .map(|entry| (entry.name.clone(), entry.index))
            .collect();
        self.entries = records;
        Ok(())
    }

    /// All normalised entries.
    pub fn entries(&self) -> &[EntryRecord] {
        &self.entries
    }

    /// Look up an exact entry name (leading `/` tolerated by the caller).
    pub fn entry(&self, name: &str) -> Option<&EntryRecord> {
        let normalized = safe_relative_path(name)?;
        self.entries.iter().find(|entry| entry.name == normalized)
    }

    /// Whether an entry exists.
    pub fn contains(&self, name: &str) -> bool {
        self.by_name
            .contains_key(&safe_relative_path(name).unwrap_or_default())
    }

    /// Read one entry as bounded bytes.
    pub fn read_bytes(&mut self, name: &str, max: u64) -> Result<Vec<u8>> {
        let normalized = safe_relative_path(name)
            .ok_or_else(|| RmcccError::Validation(format!("unsafe archive path: {name}")))?;
        let index = *self
            .by_name
            .get(&normalized)
            .ok_or_else(|| RmcccError::Validation(format!("archive entry not found: {name}")))?;
        self.read_index(index, max)
    }

    /// Read an entry by its pre-computed index.
    pub fn read_index(&mut self, index: usize, max: u64) -> Result<Vec<u8>> {
        let mut file = self.zip.by_index(index).map_err(|e| {
            RmcccError::Validation(format!("zip entry #{index} is unreadable: {e}"))
        })?;
        if file.size() > max {
            return Err(RmcccError::Validation(format!(
                "archive entry is too large ({} > {max} bytes)",
                file.size()
            )));
        }
        let mut out = Vec::with_capacity(file.size() as usize);
        file.read_to_end(&mut out)?;
        Ok(out)
    }

    /// Stream an entry into `writer` (bounded by `max` uncompressed bytes).
    ///
    /// Used for overrides: large packs may contain multi-hundred-MB files and
    /// must never be buffered fully in memory.
    pub fn copy_index(
        &mut self,
        index: usize,
        writer: &mut dyn std::io::Write,
        max: u64,
    ) -> Result<u64> {
        let mut file = self.zip.by_index(index).map_err(|e| {
            RmcccError::Validation(format!("zip entry #{index} is unreadable: {e}"))
        })?;
        if file.size() > max {
            return Err(RmcccError::Validation(format!(
                "archive entry is too large ({} > {max} bytes)",
                file.size()
            )));
        }
        Ok(std::io::copy(&mut file, writer)?)
    }

    /// Read a text entry (UTF-8/GBK decode, HMCL `readTextZipEntry`).
    pub fn read_text(&mut self, name: &str) -> Result<String> {
        let bytes = self.read_bytes(name, MAX_MANIFEST_BYTES)?;
        Ok(decode_text(&bytes))
    }

    /// Read an optional text entry.
    pub fn read_text_opt(&mut self, name: &str) -> Option<String> {
        let normalized = safe_relative_path(name)?;
        if !self.by_name.contains_key(&normalized) {
            return None;
        }
        self.read_text(&normalized).ok()
    }

    /// Entries whose path starts with `prefix` (prefix must be normalised).
    pub fn entries_under(&self, prefix: &str) -> Vec<&EntryRecord> {
        let prefix = prefix.trim_matches('/');
        let needle = if prefix.is_empty() {
            String::new()
        } else {
            format!("{prefix}/")
        };
        self.entries
            .iter()
            .filter(|entry| entry.name.starts_with(&needle))
            .collect()
    }

    /// Number of non-directory entries.
    pub fn file_count(&self) -> usize {
        self.entries.iter().filter(|entry| !entry.is_dir).count()
    }

    /// Close the archive, returning the underlying file.
    pub fn into_inner(self) -> File {
        self.zip.into_inner()
    }
}

/// Decode a raw zip filename: UTF-8 when valid, GBK otherwise (HMCL
/// `findSuitableEncoding` covers UTF-8 and GBK).
pub fn decode_name(raw: &[u8]) -> String {
    match std::str::from_utf8(raw) {
        Ok(text) => text.to_string(),
        Err(_) => {
            let (decoded, _, had_errors) = encoding_rs::GBK.decode(raw);
            if had_errors {
                String::from_utf8_lossy(raw).to_string()
            } else {
                decoded.into_owned()
            }
        }
    }
}

/// Decode manifest text: UTF-8 first, GBK fallback.
pub fn decode_text(bytes: &[u8]) -> String {
    let text = match std::str::from_utf8(bytes) {
        Ok(text) => text.to_string(),
        Err(_) => {
            let (decoded, _, had_errors) = encoding_rs::GBK.decode(bytes);
            if had_errors {
                String::from_utf8_lossy(bytes).to_string()
            } else {
                decoded.into_owned()
            }
        }
    };
    // Windows tools often prepend a UTF-8 BOM; serde_json rejects it.
    text.strip_prefix('\u{feff}').unwrap_or(&text).to_string()
}

/// Normalise an archive path and reject anything that could escape the target.
///
/// Returns `None` for absolute paths, `..` components, drive letters, NUL and
/// control characters. Backslashes are normalised to `/` (old Windows zips).
pub fn safe_relative_path(raw: &str) -> Option<String> {
    let raw = raw.replace('\\', "/");
    let mut parts = Vec::new();
    for part in raw.split('/') {
        if part.is_empty() || part == "." {
            continue;
        }
        if part == ".." {
            return None;
        }
        if part.contains('\0') || part.chars().any(char::is_control) {
            return None;
        }
        // Reject Windows drive prefixes and NTFS alternate data streams.
        if part.contains(':') {
            return None;
        }
        parts.push(part);
    }
    Some(parts.join("/"))
}

/// Join a relative path to a root and verify containment.
///
/// `relative` must already have passed [`safe_relative_path`].
pub fn safe_join(root: &Path, relative: &str) -> Result<std::path::PathBuf> {
    let relative = safe_relative_path(relative)
        .ok_or_else(|| RmcccError::Validation(format!("unsafe archive path: {relative}")))?;
    if relative.is_empty() {
        return Ok(root.to_path_buf());
    }
    let joined = root.join(&relative);
    if !joined.starts_with(root) {
        return Err(RmcccError::Validation(format!(
            "archive path escapes destination: {relative}"
        )));
    }
    Ok(joined)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn safe_paths_reject_escapes() {
        assert_eq!(
            safe_relative_path("mods/a.jar").as_deref(),
            Some("mods/a.jar")
        );
        assert_eq!(
            safe_relative_path("/mods/a.jar").as_deref(),
            Some("mods/a.jar")
        );
        assert_eq!(
            safe_relative_path("mods\\a.jar").as_deref(),
            Some("mods/a.jar")
        );
        assert!(safe_relative_path("../outside").is_none());
        assert!(safe_relative_path("mods/../../outside").is_none());
        assert!(safe_relative_path("C:/Windows").is_none());
        assert!(safe_relative_path("mods/a\0.jar").is_none());
    }

    #[test]
    fn gbk_names_decode() {
        // "中文" in GBK.
        let raw = [0xD6u8, 0xD0, 0xCE, 0xC4];
        assert_eq!(decode_name(&raw), "中文");
    }
}
