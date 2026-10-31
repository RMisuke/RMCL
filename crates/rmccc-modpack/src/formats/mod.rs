//! Format detection and shared parsing helpers.
//!
//! Detection order follows HMCL `ModpackHelper.readModpackManifest` exactly:
//! MCBBS → Curse → Modrinth → HMCL → MultiMC → Server, with the
//! manually-created `.minecraft` zip as the final fallback.

pub(crate) mod curse;
pub(crate) mod hmcl;
pub(crate) mod mcbbs;
pub(crate) mod modrinth;
pub(crate) mod multimc;
pub(crate) mod server;

use crate::archive::Archive;
use crate::model::{LoaderKind, Modpack, ModpackKind, PackIcon, PackLoader};
use rmccc_error::{Result, RmcccError};
use serde_json::Value;
use std::path::Path;

/// Extensions HMCL accepts for local modpack files.
pub fn is_modpack_extension(path: &Path) -> bool {
    matches!(
        path.extension().and_then(|ext| ext.to_str()).map(|ext| ext.to_ascii_lowercase()),
        Some(ext) if ext == "zip" || ext == "mrpack"
    )
}

/// Read and parse a modpack archive.
pub fn read_modpack(path: &Path) -> Result<Modpack> {
    if !path.is_file() {
        return Err(RmcccError::Validation(format!(
            "modpack file not found: {}",
            path.display()
        )));
    }
    let mut archive = Archive::open(path)?;
    parse_archive(&mut archive, path)
}

/// Parse an already-opened archive.
pub fn parse_archive(archive: &mut Archive, path: &Path) -> Result<Modpack> {
    // HMCL `readModpackManifest` deliberately ignores provider parse errors and
    // keeps trying the next format. Preserve that tolerance: a malformed
    // Curse manifest must not hide a valid MultiMC/Server manifest.
    let mut errors: Vec<String> = Vec::new();
    let mut detected = None;
    macro_rules! try_format {
        ($detect:path, $label:literal) => {
            if detected.is_none() {
                match $detect(archive) {
                    Ok(Some(pack)) => detected = Some(pack),
                    Ok(None) => {}
                    Err(error) => errors.push(format!("{}: {error}", $label)),
                }
            }
        };
    }
    try_format!(mcbbs::detect, "MCBBS");
    try_format!(curse::detect, "CurseForge");
    try_format!(modrinth::detect, "Modrinth");
    try_format!(hmcl::detect, "HMCL");
    try_format!(multimc::detect, "MultiMC");
    try_format!(server::detect, "Server");
    let mut pack = if let Some(pack) = detected {
        pack
    } else if let Some(pack) = detect_manual(archive, path)? {
        pack
    } else if errors.is_empty() {
        return Err(RmcccError::Validation(format!(
            "unsupported modpack: {} (no recognized manifest)",
            path.display()
        )));
    } else {
        return Err(RmcccError::Validation(format!(
            "unsupported modpack: {} ({})",
            path.display(),
            errors.join("; ")
        )));
    };
    pack.archive = path.to_path_buf();
    if pack.name.trim().is_empty() {
        pack.name = path
            .file_stem()
            .and_then(|stem| stem.to_str())
            .unwrap_or("Modpack")
            .to_string();
    }
    if pack.icon.is_none() {
        pack.icon = find_icon(archive);
    }
    Ok(pack)
}

/// Non-empty trimmed string.
pub(crate) fn non_empty(value: Option<String>) -> Option<String> {
    value
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

/// Extract the first root-level icon file (HMCL `SUPPORTED_ICON_NAMES`).
pub(crate) fn find_icon(archive: &mut Archive) -> Option<PackIcon> {
    const NAMES: &[(&str, &str)] = &[
        ("icon.png", "png"),
        ("icon.jpg", "jpg"),
        ("icon.jpeg", "jpeg"),
        ("icon.bmp", "bmp"),
        ("icon.gif", "gif"),
        ("icon.webp", "webp"),
        ("icon.apng", "apng"),
    ];
    for (name, ext) in NAMES {
        if let Some(entry) = archive.entry(name) {
            if entry.is_dir {
                continue;
            }
            if let Ok(bytes) = archive.read_index(entry.index, 8 * 1024 * 1024) {
                return Some(PackIcon {
                    ext: (*ext).to_string(),
                    bytes,
                });
            }
        }
    }
    None
}

/// Convert the CurseForge `modlist.html` description into readable text.
pub(crate) fn html_to_text(html: &str) -> String {
    let mut out = String::with_capacity(html.len());
    let mut in_tag = false;
    for ch in html.chars() {
        match ch {
            '<' => in_tag = true,
            '>' => {
                in_tag = false;
                if !out.ends_with(' ') && !out.ends_with('\n') {
                    out.push(' ');
                }
            }
            _ if !in_tag => out.push(ch),
            _ => {}
        }
    }
    out.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .trim()
        .to_string()
}

/// Normalise a loader version from a manifest.
///
/// Some manifests write `1.20.1-47.2.0` while the installer version list keys
/// on `47.2.0`; Fabric may append `+1.20.1` metadata.
pub(crate) fn normalize_loader_version(kind: LoaderKind, raw: &str, game_version: &str) -> String {
    let raw = raw.trim();
    let raw = match kind {
        LoaderKind::Forge | LoaderKind::NeoForge => raw
            .strip_prefix(&format!("{game_version}-"))
            .unwrap_or(raw)
            .to_string(),
        LoaderKind::Fabric | LoaderKind::Quilt => raw
            .split_once('+')
            .map(|(version, _)| version.to_string())
            .unwrap_or_else(|| raw.to_string()),
        _ => raw.to_string(),
    };
    // Forge/NeoForge branch suffixes (`47.2.0-20240101`) are not install keys.
    match kind {
        LoaderKind::Forge | LoaderKind::NeoForge
            if raw.contains('-')
                && raw.split('-').next().is_some_and(|part| part.contains('.')) =>
        {
            raw.split('-').next().unwrap_or(&raw).to_string()
        }
        _ => raw,
    }
}

/// Best-effort filename from a download URL (HMCL `NetworkUtils.detectFileName`).
pub(crate) fn file_name_from_url(url: &str) -> Option<String> {
    let path = url.split(['?', '#']).next()?;
    let name = path.rsplit('/').next()?.trim();
    if name.is_empty() {
        None
    } else {
        Some(name.to_string())
    }
}

/// Parse a CurseForge loader id (`forge-47.2.0`).
pub(crate) fn parse_curse_loader(
    id: &str,
    primary: bool,
    game_version: &str,
) -> Option<PackLoader> {
    let (kind_name, version) = id.split_once('-')?;
    let kind = LoaderKind::from_manifest_id(kind_name)?;
    let version = normalize_loader_version(kind, version, game_version);
    if version.is_empty() {
        return None;
    }
    Some(PackLoader::new(kind, version, primary))
}

/// Detect a manually-created modpack: a directory containing `versions/`.
///
/// HMCL searches the root, then one level, then two levels, and requires the
/// directory to be named `.minecraft` (or be the archive root).
pub(crate) fn detect_manual(_archive: &mut Archive, path: &Path) -> Result<Option<Modpack>> {
    // Re-open through a scope so this helper stays independent of the caller's
    // zip cursor; Archive has no cheap clone.
    let reopened = Archive::open(path)?;
    let mut candidates: Vec<String> = vec![String::new()];
    for entry in reopened.entries() {
        let parts: Vec<&str> = entry.name.split('/').collect();
        for depth in 1..=parts.len().saturating_sub(1).min(2) {
            candidates.push(parts[..depth].join("/"));
        }
    }
    candidates.sort_by_key(|dir| dir.split('/').count());
    candidates.dedup();

    for dir in &candidates {
        let versions = if dir.is_empty() {
            "versions".to_string()
        } else {
            format!("{dir}/versions")
        };
        let needle = format!("{versions}/");
        if !reopened
            .entries()
            .iter()
            .any(|entry| entry.name == versions || entry.name.starts_with(&needle))
        {
            continue;
        }
        let mut name = if dir.is_empty() {
            path.file_stem()
                .and_then(|stem| stem.to_str())
                .unwrap_or("Modpack")
                .to_string()
        } else {
            dir.rsplit('/').next().unwrap_or(dir).to_string()
        };
        if name == ".minecraft" {
            if let Some(parent) = dir
                .trim_end_matches("/.minecraft")
                .rsplit('/')
                .next()
                .filter(|parent| !parent.is_empty())
            {
                name = parent.to_string();
            }
        }
        if name.is_empty() || name == ".minecraft" {
            name = path
                .file_stem()
                .and_then(|stem| stem.to_str())
                .unwrap_or("Modpack")
                .to_string();
        }
        return Ok(Some(Modpack {
            kind: ModpackKind::Manual,
            name,
            version: None,
            author: None,
            description: String::new(),
            game_version: None,
            loaders: Vec::new(),
            files: Vec::new(),
            override_dirs: vec![dir.clone()],
            settings: Default::default(),
            extra_libraries: Vec::new(),
            metadata_files: Vec::new(),
            icon: None,
            raw_manifest: Value::Null,
            archive: path.to_path_buf(),
            manual_root: Some(dir.clone()),
            warnings: vec![
                "This zip is not a standard modpack; it is unpacked as an external game directory."
                    .to_string(),
            ],
        }));
    }
    Ok(None)
}

/// Finish a parsed pack: attach the archive icon when the format has none.
pub(crate) fn finish(mut pack: Modpack, archive: &mut Archive) -> Result<Modpack> {
    if pack.icon.is_none() {
        pack.icon = find_icon(archive);
    }
    Ok(pack)
}
