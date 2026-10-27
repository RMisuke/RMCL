//! `.rmcpkg` 包校验与解压（ADR-003 决策 1/2）。
//!
//! 这里是安装事务的第一道闸门：**在写入任何正式目录之前**完成包级校验。
//! 所有限制都集中在本模块，安装器只消费 [`ArchiveInspection`]。

use crate::manifest::{validate_manifest, ManifestContext, PluginManifest};
use crate::path::{is_windows_reserved, validate_relative_path};
use crate::theme::{ThemeDocument, MAX_THEME_BYTES};
use rmccc_error::{PluginErrorCode, Result, RmcccError};
use std::collections::BTreeSet;
use std::fs::File;
use std::io::{Read, Seek, Write};
use std::path::{Path, PathBuf};

use crate::capability::plugin_error;

pub const PACKAGE_EXTENSION: &str = "rmcpkg";
pub const MAX_PACKAGE_BYTES: u64 = 16 * 1024 * 1024;
pub const MAX_TOTAL_UNCOMPRESSED: u64 = 64 * 1024 * 1024;
pub const MAX_ENTRY_UNCOMPRESSED: u64 = 16 * 1024 * 1024;
pub const MAX_ENTRIES: usize = 512;
pub const MAX_MANIFEST_BYTES: u64 = 256 * 1024;
pub const MAX_CONFIG_SCHEMA_BYTES: u64 = 256 * 1024;
/// 压缩比只在条目足够大时检查，避免小文本的合法高压缩比误报。
const RATIO_CHECK_MIN_BYTES: u64 = 1024 * 1024;
const MAX_ENTRY_RATIO: u64 = 200;

const ROOT_FILES: &[&str] = &[
    "manifest.json",
    "README.md",
    "LICENSE",
    "NOTICE",
    "CHANGELOG.md",
    "config.schema.json",
];
const ROOT_DIRS: &[&str] = &["assets", "locales", "themes"];

const DENIED_EXTENSIONS: &[&str] = &[
    "js", "mjs", "cjs", "html", "htm", "wasm", "exe", "dll", "dylib", "so", "bat", "cmd", "ps1",
    "sh", "command", "app", "jar", "class", "py", "pl", "rb",
];
const ALLOWED_EXTENSIONS: &[&str] = &[
    "json", "md", "txt", "css", "png", "jpg", "jpeg", "gif", "webp", "svg", "ttf", "otf", "woff",
    "woff2",
];

/// 包内单个条目的元数据（不保存内容）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArchiveEntryInfo {
    pub path: String,
    pub uncompressed: u64,
    pub compressed: u64,
    pub is_dir: bool,
}

/// 通过包级校验后的结果。
#[derive(Debug, Clone)]
pub struct ArchiveInspection {
    pub manifest: PluginManifest,
    pub manifest_json: String,
    pub entries: Vec<ArchiveEntryInfo>,
    pub total_uncompressed: u64,
    pub sha256: String,
}

impl ArchiveInspection {
    pub fn file_paths(&self) -> impl Iterator<Item = &str> {
        self.entries
            .iter()
            .filter(|e| !e.is_dir)
            .map(|e| e.path.as_str())
    }

    pub fn contains(&self, path: &str) -> bool {
        self.entries.iter().any(|e| !e.is_dir && e.path == path)
    }
}

/// 校验包文件（扩展名、大小、ZIP 结构、Manifest 语义）。
pub fn inspect_package(path: &Path, ctx: &ManifestContext) -> Result<ArchiveInspection> {
    if path.extension().and_then(|e| e.to_str()) != Some(PACKAGE_EXTENSION) {
        return Err(plugin_error(
            PluginErrorCode::PackageInvalid,
            format!("插件包必须是 .{PACKAGE_EXTENSION} 文件：{}", path.display()),
        ));
    }
    let meta = std::fs::metadata(path)?;
    if !meta.is_file() {
        return Err(plugin_error(
            PluginErrorCode::PackageInvalid,
            format!("不是普通文件：{}", path.display()),
        ));
    }
    if meta.len() > MAX_PACKAGE_BYTES {
        return Err(plugin_error(
            PluginErrorCode::PackageInvalid,
            format!(
                "插件包过大：{} 字节（上限 {MAX_PACKAGE_BYTES}）",
                meta.len()
            ),
        ));
    }
    let sha256 = sha256_file(path)?;
    let file = File::open(path)?;
    let inspection = inspect_reader(file, ctx)?;
    Ok(ArchiveInspection {
        sha256,
        ..inspection
    })
}

/// 测试/内嵌包用：直接从字节校验。
pub fn inspect_package_bytes(bytes: &[u8], ctx: &ManifestContext) -> Result<ArchiveInspection> {
    if bytes.len() as u64 > MAX_PACKAGE_BYTES {
        return Err(plugin_error(
            PluginErrorCode::PackageInvalid,
            format!("插件包过大：{} 字节", bytes.len()),
        ));
    }
    let sha256 = sha256_bytes(bytes);
    let cursor = std::io::Cursor::new(bytes.to_vec());
    let inspection = inspect_reader(cursor, ctx)?;
    Ok(ArchiveInspection {
        sha256,
        ..inspection
    })
}

fn inspect_reader<R: Read + Seek>(reader: R, ctx: &ManifestContext) -> Result<ArchiveInspection> {
    let mut archive = zip::ZipArchive::new(reader).map_err(|e| {
        plugin_error(
            PluginErrorCode::PackageInvalid,
            format!("不是有效的 ZIP 包：{e}"),
        )
    })?;

    if archive.len() > MAX_ENTRIES {
        return Err(plugin_error(
            PluginErrorCode::PackageInvalid,
            format!("条目数 {} 超过上限 {MAX_ENTRIES}", archive.len()),
        ));
    }

    let mut entries = Vec::with_capacity(archive.len());
    let mut seen: BTreeSet<String> = BTreeSet::new();
    let mut file_paths: BTreeSet<String> = BTreeSet::new();
    let mut total_uncompressed: u64 = 0;

    for i in 0..archive.len() {
        let file = archive.by_index_raw(i).map_err(|e| {
            plugin_error(
                PluginErrorCode::PackageInvalid,
                format!("读取 ZIP 条目失败：{e}"),
            )
        })?;

        let raw_name = file.name().to_string();
        let is_dir = file.is_dir() || raw_name.ends_with('/');
        let logical = raw_name.trim_end_matches('/').to_string();
        if logical.is_empty() {
            return Err(plugin_error(
                PluginErrorCode::PackageInvalid,
                "包内存在空路径条目".to_string(),
            ));
        }
        validate_relative_path(&logical)?;

        // 类型：只允许普通文件 / 目录；拒绝符号链接与设备文件。
        if let Some(mode) = file.unix_mode() {
            validate_entry_mode(mode, &logical)?;
        }

        // 大小 / 压缩比。
        let uncompressed = file.size();
        let compressed = file.compressed_size();
        if uncompressed > MAX_ENTRY_UNCOMPRESSED {
            return Err(plugin_error(
                PluginErrorCode::PackageInvalid,
                format!(
                    "条目 {logical} 解压后过大：{uncompressed} 字节（上限 {MAX_ENTRY_UNCOMPRESSED}）"
                ),
            ));
        }
        total_uncompressed = total_uncompressed.saturating_add(uncompressed);
        if total_uncompressed > MAX_TOTAL_UNCOMPRESSED {
            return Err(plugin_error(
                PluginErrorCode::PackageInvalid,
                format!("解压后总大小超过上限 {MAX_TOTAL_UNCOMPRESSED} 字节"),
            ));
        }
        if uncompressed >= RATIO_CHECK_MIN_BYTES
            && compressed > 0
            && uncompressed / compressed > MAX_ENTRY_RATIO
        {
            return Err(plugin_error(
                PluginErrorCode::PackageInvalid,
                format!("条目 {logical} 的压缩比异常（疑似 zip bomb）"),
            ));
        }

        // 重复路径：按大小写不敏感判定（跨平台可移植性）。
        let key = logical.to_ascii_lowercase();
        if !seen.insert(key) {
            return Err(plugin_error(
                PluginErrorCode::PackageInvalid,
                format!("包内路径重复（大小写不敏感）：{logical}"),
            ));
        }

        if !is_dir {
            validate_package_file(&logical)?;
            file_paths.insert(logical.clone());
        }

        entries.push(ArchiveEntryInfo {
            path: logical,
            uncompressed,
            compressed,
            is_dir,
        });
    }

    if !file_paths.contains("manifest.json") {
        return Err(plugin_error(
            PluginErrorCode::PackageInvalid,
            "包根缺少 manifest.json".to_string(),
        ));
    }

    // 读取 Manifest（限制大小，防止用超大 JSON 卡住解析）。
    let manifest_json = {
        let mut file = archive.by_name("manifest.json").map_err(|e| {
            plugin_error(
                PluginErrorCode::PackageInvalid,
                format!("读取 manifest.json 失败：{e}"),
            )
        })?;
        if file.size() > MAX_MANIFEST_BYTES {
            return Err(plugin_error(
                PluginErrorCode::PackageInvalid,
                format!("manifest.json 过大（上限 {MAX_MANIFEST_BYTES} 字节）"),
            ));
        }
        let mut buf = String::new();
        (&mut file)
            .take(MAX_MANIFEST_BYTES + 1)
            .read_to_string(&mut buf)
            .map_err(|e| {
                plugin_error(
                    PluginErrorCode::PackageInvalid,
                    format!("manifest.json 不是 UTF-8 文本：{e}"),
                )
            })?;
        if buf.len() as u64 > MAX_MANIFEST_BYTES {
            return Err(plugin_error(
                PluginErrorCode::PackageInvalid,
                format!("manifest.json 过大（上限 {MAX_MANIFEST_BYTES} 字节）"),
            ));
        }
        buf
    };

    let manifest = PluginManifest::parse(&manifest_json)?;
    validate_manifest(&manifest, ctx)?;

    // Manifest 引用的包内文件必须存在，且**内容通过主题校验**。
    //
    // 在这里做而不是等到应用主题时：让"能打包 = 能安装 = 能应用"，
    // 避免装上一个永远选不中的主题（ADR-005 决策 7 / 主题编写规范）。
    for theme in &manifest.contributes.themes {
        if !file_paths.contains(&theme.path) {
            return Err(plugin_error(
                PluginErrorCode::PackageInvalid,
                format!("主题 {} 引用的文件不存在：{}", theme.id, theme.path),
            ));
        }
        let mut file = archive.by_name(&theme.path).map_err(|e| {
            plugin_error(
                PluginErrorCode::PackageInvalid,
                format!("读取主题 {} 失败：{e}", theme.path),
            )
        })?;
        if file.size() > MAX_THEME_BYTES {
            return Err(plugin_error(
                PluginErrorCode::PackageInvalid,
                format!(
                    "主题 {} 过大：{} 字节（上限 {MAX_THEME_BYTES}）",
                    theme.path,
                    file.size()
                ),
            ));
        }
        let mut raw = String::new();
        (&mut file)
            .take(MAX_THEME_BYTES + 1)
            .read_to_string(&mut raw)
            .map_err(|e| {
                plugin_error(
                    PluginErrorCode::PackageInvalid,
                    format!("主题 {} 不是 UTF-8 文本：{e}", theme.path),
                )
            })?;
        if raw.len() as u64 > MAX_THEME_BYTES {
            return Err(plugin_error(
                PluginErrorCode::PackageInvalid,
                format!("主题 {} 超过 {MAX_THEME_BYTES} 字节上限", theme.path),
            ));
        }
        ThemeDocument::parse(&raw).map_err(|e| {
            plugin_error(
                PluginErrorCode::PackageInvalid,
                format!("主题 {}（{}）校验失败：{e}", theme.id, theme.path),
            )
        })?;
    }

    Ok(ArchiveInspection {
        manifest,
        manifest_json,
        entries,
        total_uncompressed,
        sha256: String::new(),
    })
}

/// Unix mode 的类型位检查（独立函数便于单测：zip 写入端会丢弃类型位，
/// 只能用函数级测试覆盖 symlink/设备文件）。
pub(crate) fn validate_entry_mode(mode: u32, path: &str) -> Result<()> {
    let kind = mode & 0o170000;
    if kind == 0o120000 {
        return Err(plugin_error(
            PluginErrorCode::PackageInvalid,
            format!("包内不允许符号链接：{path}"),
        ));
    }
    if kind != 0 && kind != 0o100000 && kind != 0o040000 {
        return Err(plugin_error(
            PluginErrorCode::PackageInvalid,
            format!("包内不允许特殊文件类型：{path}"),
        ));
    }
    Ok(())
}

fn validate_package_file(path: &str) -> Result<()> {
    let segments: Vec<&str> = path.split('/').collect();
    let top = segments[0];

    // `ui/`：Phase 4 起允许（隔离容器，ADR-006）。类型白名单与路径规则集中在
    // `ui::validate_asset_path`，避免这里和方案处理器各写一套。
    if top == "ui" {
        crate::ui::validate_asset_path(path)?;
        return Ok(());
    }

    // `logic/`：Phase 5 起允许，但只接受 `.wasm`（ADR-007）。原生库与脚本仍然拒绝。
    if top == "logic" {
        crate::logic::validate_logic_file(path)?;
        return Ok(());
    }

    let top_allowed = ROOT_FILES.contains(&top) || ROOT_DIRS.contains(&top);
    if !top_allowed {
        return Err(plugin_error(
            PluginErrorCode::PackageInvalid,
            format!("不允许的顶层条目 {top:?}（见 ADR-003 决策 1）"),
        ));
    }
    // 根级只允许白名单文件，不允许其它根级文件。
    if segments.len() == 1 && !ROOT_FILES.contains(&top) {
        return Err(plugin_error(
            PluginErrorCode::PackageInvalid,
            format!("不允许的根级文件：{path}"),
        ));
    }

    let file_name = segments.last().copied().unwrap_or(path);
    if is_windows_reserved(file_name) {
        return Err(plugin_error(
            PluginErrorCode::PackageInvalid,
            format!("包内包含 Windows 保留名称：{path}"),
        ));
    }

    let ext = Path::new(file_name)
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_ascii_lowercase());
    match ext {
        Some(ext) => {
            if DENIED_EXTENSIONS.contains(&ext.as_str()) {
                return Err(plugin_error(
                    PluginErrorCode::PackageInvalid,
                    format!("包内不允许可执行/脚本文件：{path}"),
                ));
            }
            if !ALLOWED_EXTENSIONS.contains(&ext.as_str()) {
                return Err(plugin_error(
                    PluginErrorCode::PackageInvalid,
                    format!("包内文件类型不允许：{path}"),
                ));
            }
        }
        None => {
            if file_name != "LICENSE" && file_name != "NOTICE" {
                return Err(plugin_error(
                    PluginErrorCode::PackageInvalid,
                    format!("无扩展名文件只允许 LICENSE/NOTICE：{path}"),
                ));
            }
        }
    }
    Ok(())
}

/// 解压到 `dest`（必须是不存在的 staging 目录；由调用方负责清理）。
pub fn extract_package(path: &Path, dest: &Path) -> Result<()> {
    let file = File::open(path)?;
    extract_reader(file, dest)
}

pub fn extract_package_bytes(bytes: &[u8], dest: &Path) -> Result<()> {
    extract_reader(std::io::Cursor::new(bytes.to_vec()), dest)
}

fn extract_reader<R: Read + Seek>(reader: R, dest: &Path) -> Result<()> {
    if dest.exists() {
        return Err(plugin_error(
            PluginErrorCode::PackageInvalid,
            format!("解压目标已存在：{}", dest.display()),
        ));
    }
    std::fs::create_dir_all(dest)?;
    let mut archive = zip::ZipArchive::new(reader).map_err(|e| {
        plugin_error(
            PluginErrorCode::PackageInvalid,
            format!("不是有效的 ZIP 包：{e}"),
        )
    })?;
    if archive.len() > MAX_ENTRIES {
        return Err(plugin_error(
            PluginErrorCode::PackageInvalid,
            format!("条目数 {} 超过上限 {MAX_ENTRIES}", archive.len()),
        ));
    }
    let mut total_written: u64 = 0;

    for i in 0..archive.len() {
        let mut file = archive.by_index(i).map_err(|e| {
            plugin_error(
                PluginErrorCode::PackageInvalid,
                format!("读取 ZIP 条目失败：{e}"),
            )
        })?;
        let raw_name = file.name().to_string();
        let logical = raw_name.trim_end_matches('/').to_string();
        if logical.is_empty() {
            continue;
        }
        validate_relative_path(&logical)?;
        if let Some(mode) = file.unix_mode() {
            validate_entry_mode(mode, &logical)?;
        }
        let out_path = dest.join(&logical);

        if file.is_dir() || raw_name.ends_with('/') {
            std::fs::create_dir_all(&out_path)?;
            continue;
        }
        validate_package_file(&logical)?;
        if let Some(parent) = out_path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        // 即使 ZIP 头谎报大小，也用 take 硬限制单文件写入量。
        let mut out = File::create(&out_path)?;
        let written = std::io::copy(&mut (&mut file).take(MAX_ENTRY_UNCOMPRESSED + 1), &mut out)?;
        if written > MAX_ENTRY_UNCOMPRESSED {
            return Err(plugin_error(
                PluginErrorCode::PackageInvalid,
                format!("条目 {logical} 解压后超过上限"),
            ));
        }
        total_written = total_written.saturating_add(written);
        if total_written > MAX_TOTAL_UNCOMPRESSED {
            return Err(plugin_error(
                PluginErrorCode::PackageInvalid,
                format!("解压后总大小超过上限 {MAX_TOTAL_UNCOMPRESSED} 字节"),
            ));
        }
        out.flush()?;
    }
    Ok(())
}

/// 把一个目录打成 `.rmcpkg`（开发/示例用）。打包后会重新走完整校验。
pub fn pack_directory(src: &Path, out: &Path, ctx: &ManifestContext) -> Result<()> {
    if !src.is_dir() {
        return Err(plugin_error(
            PluginErrorCode::PackageInvalid,
            format!("源目录不存在：{}", src.display()),
        ));
    }
    let mut files = Vec::new();
    collect_files(src, src, &mut files)?;
    files.sort_by(|a, b| a.0.cmp(&b.0));

    if let Some(parent) = out.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let tmp = out.with_extension("rmcpkg.tmp");
    {
        let file = File::create(&tmp)?;
        let mut zip = zip::ZipWriter::new(file);
        let options = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated)
            .unix_permissions(0o644);
        for (rel, abs) in &files {
            zip.start_file(rel, options).map_err(|e| {
                plugin_error(
                    PluginErrorCode::PackageInvalid,
                    format!("写入 ZIP 失败：{e}"),
                )
            })?;
            let bytes = std::fs::read(abs)?;
            zip.write_all(&bytes)?;
        }
        zip.finish().map_err(|e| {
            plugin_error(
                PluginErrorCode::PackageInvalid,
                format!("完成 ZIP 失败：{e}"),
            )
        })?;
    }
    std::fs::rename(&tmp, out)?;
    // 打包后立即校验，确保产物可安装。
    inspect_package(out, ctx)?;
    Ok(())
}

fn collect_files(root: &Path, dir: &Path, out: &mut Vec<(String, PathBuf)>) -> Result<()> {
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        let meta = std::fs::symlink_metadata(&path)?;
        if meta.file_type().is_symlink() {
            return Err(plugin_error(
                PluginErrorCode::PackageInvalid,
                format!("打包源不允许符号链接：{}", path.display()),
            ));
        }
        if meta.is_dir() {
            collect_files(root, &path, out)?;
        } else if meta.is_file() {
            let rel = path
                .strip_prefix(root)
                .map_err(|_| {
                    plugin_error(
                        PluginErrorCode::PackageInvalid,
                        format!("路径不在源目录内：{}", path.display()),
                    )
                })?
                .to_string_lossy()
                .replace('\\', "/");
            out.push((rel, path));
        }
    }
    Ok(())
}

pub fn sha256_file(path: &Path) -> Result<String> {
    use sha2::Digest;
    let mut hasher = sha2::Sha256::new();
    let mut file = File::open(path)?;
    std::io::copy(&mut file, &mut hasher)?;
    Ok(format!("{:x}", hasher.finalize()))
}

pub fn sha256_bytes(bytes: &[u8]) -> String {
    use sha2::Digest;
    let mut hasher = sha2::Sha256::new();
    hasher.update(bytes);
    format!("{:x}", hasher.finalize())
}

/// 判断一个相对路径是否落在允许的顶层目录内（安装器二次检查用）。
pub fn is_allowed_top_level(path: &str) -> bool {
    let top = path.split('/').next().unwrap_or(path);
    ROOT_FILES.contains(&top) || ROOT_DIRS.contains(&top)
}

/// 便捷错误：非插件包。
pub fn not_a_package(path: &Path) -> RmcccError {
    plugin_error(
        PluginErrorCode::PackageInvalid,
        format!("不是 .{PACKAGE_EXTENSION} 插件包：{}", path.display()),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::manifest::ManifestContext;
    use serde_json::json;

    fn ctx() -> ManifestContext {
        ManifestContext::new("0.1.0", "darwin-aarch64").unwrap()
    }

    fn manifest() -> serde_json::Value {
        json!({
            "manifestVersion": 1,
            "id": "dev.rmccc.pkg",
            "name": "Package Test",
            "version": "1.0.0",
            "publisher": { "id": "dev.rmccc", "name": "Tests" },
            "compatibility": { "pluginApi": "1.x", "core": ">=0.1.0 <0.2.0" },
            "permissions": [{ "capability": "themes.apply" }],
            "contributes": {
                "themes": [{ "id": "midnight", "name": "Midnight", "path": "themes/midnight.json" }]
            }
        })
    }

    /// 包检查会解析主题内容，测试包必须带一份合法主题。
    const VALID_THEME: &str = r##"{
        "schemaVersion": 1,
        "name": "Midnight",
        "colors": {
            "background": "#0b1020",
            "surface": "#141a2e",
            "text": "#e6e9f2",
            "accent": "#7c6cff",
            "border": "#2a3352"
        }
    }"##;

    fn zip_with(entries: &[(&str, &[u8], u32)]) -> Vec<u8> {
        let mut buf = std::io::Cursor::new(Vec::new());
        {
            let mut zip = zip::ZipWriter::new(&mut buf);
            for (name, bytes, mode) in entries {
                let opts = zip::write::SimpleFileOptions::default()
                    .compression_method(zip::CompressionMethod::Deflated)
                    .unix_permissions(*mode);
                zip.start_file(*name, opts).unwrap();
                std::io::Write::write_all(&mut zip, bytes).unwrap();
            }
            zip.finish().unwrap();
        }
        buf.into_inner()
    }

    fn good_zip() -> Vec<u8> {
        let manifest = serde_json::to_string_pretty(&manifest()).unwrap();
        zip_with(&[
            ("manifest.json", manifest.as_bytes(), 0o644),
            ("themes/midnight.json", VALID_THEME.as_bytes(), 0o644),
            ("README.md", b"# test", 0o644),
        ])
    }

    #[test]
    fn accepts_valid_package() {
        let bytes = good_zip();
        let inspection = inspect_package_bytes(&bytes, &ctx()).unwrap();
        assert_eq!(inspection.manifest.id, "dev.rmccc.pkg");
        assert_eq!(inspection.entries.len(), 3);
        assert!(inspection.contains("themes/midnight.json"));
    }

    #[test]
    fn rejects_zip_slip_and_absolute_paths() {
        for bad in [
            "../evil.json",
            "assets/../../evil.json",
            "/etc/passwd",
            "C:/x.json",
        ] {
            let bytes = zip_with(&[("manifest.json", b"{}", 0o644), (bad, b"x", 0o644)]);
            assert!(
                inspect_package_bytes(&bytes, &ctx()).is_err(),
                "should reject {bad}"
            );
        }
    }

    #[test]
    fn rejects_duplicate_case_insensitive_paths() {
        let manifest = serde_json::to_string(&manifest()).unwrap();
        let bytes = zip_with(&[
            ("manifest.json", manifest.as_bytes(), 0o644),
            ("themes/a.json", VALID_THEME.as_bytes(), 0o644),
            ("themes/A.json", VALID_THEME.as_bytes(), 0o644),
        ]);
        let err = inspect_package_bytes(&bytes, &ctx())
            .unwrap_err()
            .to_string();
        assert!(err.contains("重复"), "{err}");
    }

    #[test]
    fn rejects_theme_content_that_would_inject_css() {
        let manifest = serde_json::to_string(&manifest()).unwrap();
        for theme in [
            // 缺必需令牌
            r##"{ "schemaVersion": 1, "colors": { "background": "#000" } }"##,
            // CSS 注入 / 外链探针
            r##"{ "schemaVersion": 1, "colors": { "background": "url(https://evil.example/p)", "surface": "#111", "text": "#fff", "accent": "#fff", "border": "#222" } }"##,
            // 版本不匹配
            r##"{ "schemaVersion": 2, "colors": { "background": "#000", "surface": "#111", "text": "#fff", "accent": "#fff", "border": "#222" } }"##,
        ] {
            let bytes = zip_with(&[
                ("manifest.json", manifest.as_bytes(), 0o644),
                ("themes/midnight.json", theme.as_bytes(), 0o644),
            ]);
            let err = inspect_package_bytes(&bytes, &ctx())
                .unwrap_err()
                .to_string();
            assert!(err.contains("主题"), "unexpected error: {err}");
        }
    }

    #[test]
    fn rejects_theme_path_with_non_json_extension() {
        let mut manifest = manifest();
        manifest["contributes"]["themes"][0]["path"] = json!("themes/midnight.txt");
        let raw = serde_json::to_string(&manifest).unwrap();
        let bytes = zip_with(&[
            ("manifest.json", raw.as_bytes(), 0o644),
            ("themes/midnight.txt", b"{}", 0o644),
        ]);
        assert!(inspect_package_bytes(&bytes, &ctx()).is_err());
    }

    #[test]
    fn allows_ui_and_wasm_assets_but_rejects_scripts() {
        // `ui/` 的 html/js（Phase 4）与 `logic/` 的 .wasm（Phase 5）合法；
        // 包内脚本、原生库仍然拒绝。
        let manifest_json = serde_json::to_string(&manifest()).unwrap();
        let bytes = zip_with(&[
            ("manifest.json", manifest_json.as_bytes(), 0o644),
            ("themes/midnight.json", VALID_THEME.as_bytes(), 0o644),
            ("ui/index.html", b"<!DOCTYPE html>", 0o644),
            ("ui/app.js", b"export {}", 0o644),
            ("ui/style.css", b"body{}", 0o644),
            ("logic/plugin.wasm", b"\0asm", 0o644),
        ]);
        inspect_package_bytes(&bytes, &ctx()).expect("ui assets should be accepted");

        for bad in [
            "assets/app.js",
            "assets/run.sh",
            "ui/run.sh",
            "ui/app.wasm",
            "logic/plugin.dylib",
            "logic/run.sh",
        ] {
            let manifest = serde_json::to_string(&manifest()).unwrap();
            let bytes = zip_with(&[
                ("manifest.json", manifest.as_bytes(), 0o644),
                (bad, b"x", 0o644),
            ]);
            assert!(
                inspect_package_bytes(&bytes, &ctx()).is_err(),
                "should reject {bad}"
            );
        }
    }

    #[test]
    fn rejects_windows_reserved_names() {
        let manifest = serde_json::to_string(&manifest()).unwrap();
        let bytes = zip_with(&[
            ("manifest.json", manifest.as_bytes(), 0o644),
            ("assets/CON.json", b"x", 0o644),
        ]);
        assert!(inspect_package_bytes(&bytes, &ctx()).is_err());
    }

    #[test]
    fn rejects_symlink_and_special_file_modes() {
        // zip crate 的写入端会 `mode & 0o777`，无法用它生成 symlink 条目；
        // 这里直接覆盖读取端的类型位判定。
        assert!(validate_entry_mode(0o120777, "assets/link").is_err());
        assert!(validate_entry_mode(0o010644, "assets/fifo").is_err());
        assert!(validate_entry_mode(0o060644, "assets/dev").is_err());
        assert!(validate_entry_mode(0o100644, "assets/file").is_ok());
        assert!(validate_entry_mode(0o040755, "assets").is_ok());
    }

    #[test]
    fn rejects_missing_or_invalid_manifest() {
        let bytes = zip_with(&[("README.md", b"x", 0o644)]);
        assert!(inspect_package_bytes(&bytes, &ctx()).is_err());

        let bytes = zip_with(&[("manifest.json", b"{not json", 0o644)]);
        assert!(inspect_package_bytes(&bytes, &ctx()).is_err());
    }

    #[test]
    fn rejects_missing_referenced_theme() {
        let manifest = serde_json::to_string(&manifest()).unwrap();
        let bytes = zip_with(&[("manifest.json", manifest.as_bytes(), 0o644)]);
        let err = inspect_package_bytes(&bytes, &ctx())
            .unwrap_err()
            .to_string();
        assert!(err.contains("引用的文件不存在"), "{err}");
    }

    #[test]
    fn rejects_too_many_entries() {
        let manifest = serde_json::to_string(&manifest()).unwrap();
        let mut entries: Vec<(String, Vec<u8>, u32)> =
            vec![("manifest.json".to_string(), manifest.into_bytes(), 0o644)];
        for i in 0..MAX_ENTRIES {
            entries.push((format!("assets/f{i}.json"), b"{}".to_vec(), 0o644));
        }
        let mut buf = std::io::Cursor::new(Vec::new());
        {
            let mut zip = zip::ZipWriter::new(&mut buf);
            for (name, bytes, mode) in &entries {
                let opts = zip::write::SimpleFileOptions::default()
                    .compression_method(zip::CompressionMethod::Deflated)
                    .unix_permissions(*mode);
                zip.start_file(name.as_str(), opts).unwrap();
                std::io::Write::write_all(&mut zip, bytes).unwrap();
            }
            zip.finish().unwrap();
        }
        let bytes = buf.into_inner();
        let err = inspect_package_bytes(&bytes, &ctx())
            .unwrap_err()
            .to_string();
        assert!(err.contains("条目数"), "{err}");
    }

    #[test]
    fn rejects_high_compression_ratio() {
        let manifest = serde_json::to_string(&manifest()).unwrap();
        let big = vec![0u8; 2 * 1024 * 1024];
        let bytes = zip_with(&[
            ("manifest.json", manifest.as_bytes(), 0o644),
            ("assets/big.png", &big, 0o644),
        ]);
        let err = inspect_package_bytes(&bytes, &ctx())
            .unwrap_err()
            .to_string();
        assert!(err.contains("压缩比"), "{err}");
    }

    #[test]
    fn pack_directory_roundtrip() {
        let dir = std::env::temp_dir().join(format!("rmccc-plugin-pack-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(dir.join("themes")).unwrap();
        std::fs::write(
            dir.join("manifest.json"),
            serde_json::to_string_pretty(&manifest()).unwrap(),
        )
        .unwrap();
        std::fs::write(dir.join("themes/midnight.json"), VALID_THEME).unwrap();
        let out = dir.join("demo.rmcpkg");
        pack_directory(&dir, &out, &ctx()).unwrap();
        let inspection = inspect_package(&out, &ctx()).unwrap();
        assert_eq!(inspection.manifest.id, "dev.rmccc.pkg");
        assert!(inspection.contains("themes/midnight.json"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn extension_must_be_rmcpkg() {
        let dir = std::env::temp_dir().join(format!("rmccc-plugin-ext-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("demo.zip");
        std::fs::write(&path, good_zip()).unwrap();
        let err = inspect_package(&path, &ctx()).unwrap_err().to_string();
        assert!(err.contains("rmcpkg"), "{err}");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
