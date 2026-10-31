//! Override extraction and HMCL-compatible update preservation.
//!
//! `ModpackInstallTask` in HMCL compares the old `modpack.cfg` hashes with the
//! current file on disk: user-modified files are kept, deleted files stay
//! deleted, and files removed from the new pack are deleted.

use crate::archive::{safe_join, Archive};
use crate::model::OverrideEntry;
use rmccc_error::{Result, RmcccError};
use sha1::{Digest, Sha1};
use std::collections::{HashMap, HashSet};
use std::io::{Read, Write};
use std::path::Path;

/// Extract all override directories into `run_dir`, preserving user edits when
/// `previous` is non-empty (pack update semantics).
///
/// Returns the new override hash list for `modpack.cfg`.
pub fn extract_overrides(
    archive_path: &Path,
    run_dir: &Path,
    directories: &[String],
    previous: &[OverrideEntry],
    exclude: &dyn Fn(&str) -> bool,
    progress: Option<&dyn Fn(usize, usize)>,
) -> Result<Vec<OverrideEntry>> {
    let mut archive = Archive::open(archive_path)?;
    let old: HashMap<&str, &OverrideEntry> = previous
        .iter()
        .map(|entry| (entry.path.as_str(), entry))
        .collect();
    let update = !previous.is_empty();

    // Build the extraction list first so progress can report a meaningful total.
    struct Candidate {
        index: usize,
        relative: String,
    }
    let mut candidates: Vec<Candidate> = Vec::new();
    let mut seen: HashMap<String, usize> = HashMap::new();
    for dir in directories {
        let prefix = dir.trim_matches('/');
        for entry in archive.entries_under(prefix) {
            if entry.is_dir || entry.is_symlink {
                continue;
            }
            let relative = if prefix.is_empty() {
                entry.name.clone()
            } else {
                entry.name[prefix.len() + 1..].to_string()
            };
            if relative.is_empty() || exclude(&relative) {
                continue;
            }
            let candidate = Candidate {
                index: entry.index,
                relative: relative.clone(),
            };
            if let Some(position) = seen.get(&relative).copied() {
                // Later override directories win (HMCL unzips them in order).
                candidates[position] = candidate;
            } else {
                seen.insert(relative, candidates.len());
                candidates.push(candidate);
            }
        }
    }

    std::fs::create_dir_all(run_dir)?;
    let total = candidates.len();
    let mut out: Vec<OverrideEntry> = Vec::with_capacity(total);
    let mut entries_set: HashSet<String> = HashSet::with_capacity(total);

    for (done, candidate) in candidates.iter().enumerate() {
        let Some(dest) = safe_join(run_dir, &candidate.relative).ok() else {
            tracing::warn!(path = %candidate.relative, "Skipping unsafe override path");
            continue;
        };
        let old_entry = old.get(candidate.relative.as_str()).copied();
        let should_write = if !update {
            true
        } else if old_entry.is_none() {
            // New file in the updated pack: always write.
            true
        } else if !dest.exists() {
            // User deleted it: leave it missing.
            false
        } else if let Some(old_entry) = old_entry {
            // Write only when the on-disk file still matches what the old pack
            // installed (i.e. the user has not modified it).
            file_sha1(&dest)
                .map(|current| current.eq_ignore_ascii_case(&old_entry.hash))
                .unwrap_or(false)
        } else {
            false
        };

        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent)?;
        }
        // Stream to a staging file while hashing, then publish or discard. This
        // keeps memory flat for large mods/resource packs and never leaves a
        // partial file at the destination.
        let file_name = dest
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("override");
        let staging = dest.with_file_name(format!(".{file_name}.rmccc-part"));
        let hash = {
            let mut file = std::fs::File::create(&staging)?;
            let mut hasher = Sha1::new();
            {
                let mut writer = HashingWriter {
                    inner: &mut file,
                    hasher: &mut hasher,
                };
                let result = archive.copy_index(candidate.index, &mut writer, u64::MAX);
                if let Err(error) = result {
                    let _ = std::fs::remove_file(&staging);
                    return Err(error);
                }
                writer.flush()?;
            }
            drop(file);
            format!("{:x}", hasher.finalize())
        };

        if should_write {
            std::fs::rename(&staging, &dest)?;
        } else {
            std::fs::remove_file(&staging)?;
        }
        entries_set.insert(candidate.relative.clone());
        out.push(OverrideEntry {
            path: candidate.relative.clone(),
            hash,
            download_url: None,
        });

        if let Some(cb) = progress {
            if done % 16 == 0 || done + 1 == total {
                cb(done + 1, total);
            }
        }
    }

    // Files that were part of the old pack but are gone from the new one.
    for entry in previous {
        if entries_set.contains(&entry.path) {
            continue;
        }
        if let Ok(path) = safe_join(run_dir, &entry.path) {
            if path.is_file() {
                let _ = std::fs::remove_file(path);
            }
        }
    }

    out.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(out)
}

/// Unpack a subdirectory of the archive (manual modpack import).
pub fn extract_subdirectory(
    archive_path: &Path,
    dest: &Path,
    prefix: &str,
    progress: Option<&dyn Fn(usize, usize)>,
) -> Result<usize> {
    let mut archive = Archive::open(archive_path)?;
    let prefix = prefix.trim_matches('/');
    let entries: Vec<_> = archive
        .entries_under(prefix)
        .into_iter()
        .filter(|entry| !entry.is_dir && !entry.is_symlink)
        .map(|entry| {
            (
                entry.index,
                if prefix.is_empty() {
                    entry.name.clone()
                } else {
                    entry.name[prefix.len() + 1..].to_string()
                },
                entry.size,
            )
        })
        .filter(|(_, relative, _)| !relative.is_empty())
        .collect();
    std::fs::create_dir_all(dest)?;
    let total = entries.len();
    for (done, (index, relative, _size)) in entries.into_iter().enumerate() {
        let Some(target) = safe_join(dest, &relative).ok() else {
            tracing::warn!(path = %relative, "Skipping unsafe archive path");
            continue;
        };
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let mut file = std::fs::File::create(&target)?;
        archive.copy_index(index, &mut file, u64::MAX)?;
        file.flush()?;
        if let Some(cb) = progress {
            if done % 16 == 0 || done + 1 == total {
                cb(done + 1, total);
            }
        }
    }
    Ok(total)
}

struct HashingWriter<'a, W> {
    inner: &'a mut W,
    hasher: &'a mut Sha1,
}

impl<W: Write> Write for HashingWriter<'_, W> {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        let written = self.inner.write(buf)?;
        self.hasher.update(&buf[..written]);
        Ok(written)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.inner.flush()
    }
}

/// SHA-1 of a file on disk.
pub fn file_sha1(path: &Path) -> Result<String> {
    let mut file = std::fs::File::open(path)?;
    let mut hasher = Sha1::new();
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

/// List `files` that are missing or have a wrong SHA-1.
pub fn verify_files(
    run_dir: &Path,
    files: &[(String, Option<String>)],
) -> Result<Vec<(String, bool)>> {
    let mut out = Vec::new();
    for (relative, expected) in files {
        let path = safe_join(run_dir, relative)
            .map_err(|e| RmcccError::Validation(format!("unsafe path {relative}: {e}")))?;
        let ok = if !path.is_file() {
            false
        } else if let Some(expected) = expected {
            file_sha1(&path)
                .map(|actual| actual.eq_ignore_ascii_case(expected))
                .unwrap_or(false)
        } else {
            true
        };
        out.push((relative.clone(), ok));
    }
    Ok(out)
}
