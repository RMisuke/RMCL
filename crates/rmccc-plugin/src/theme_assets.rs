//! 主题资产校验（ADR-014）：包内 `assets/` 的图片/字体/视频。
//!
//! 主题只提供相对路径；真正读取字节的是包检查（zip）或运行期（文件系统）。
//! 两条路径共用本模块的类型/大小/魔数/SVG 文本规则，避免"安装时放行、运行时才炸"。
//!
//! 这是纯防御：即使有人绕过安装检查手改主题文件，运行期也会再校验一次。

use crate::theme::{validate_asset_path, AssetKind, AssetSpec, ThemeDocument};
use rmccc_error::{PluginErrorCode, Result, RmcccError};
use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};

/// 位图上限（png/jpg/webp）。
pub const MAX_RASTER_BYTES: u64 = 8 * 1024 * 1024;
/// SVG 上限（文本，额外做脚本/XXE 检查）。
pub const MAX_SVG_BYTES: u64 = 256 * 1024;
/// 单个字体文件上限。
pub const MAX_FONT_BYTES: u64 = 2 * 1024 * 1024;
/// 背景视频上限（同时受包内单文件 16 MiB 限制）。
pub const MAX_VIDEO_BYTES: u64 = 16 * 1024 * 1024;

/// SVG 最多读取的字节数（用于脚本/DOCTYPE 检查）。
const SVG_HEAD_BYTES: usize = 64 * 1024;

/// 资产读取抽象：包检查与运行期各实现一次。
pub trait AssetReader {
    fn size(&mut self, path: &str) -> Result<u64>;
    fn head(&mut self, path: &str, max: usize) -> Result<Vec<u8>>;
}

/// 从插件安装目录读取资产（运行期）。
pub struct FsAssetReader {
    root: PathBuf,
}

impl FsAssetReader {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }
}

impl AssetReader for FsAssetReader {
    fn size(&mut self, path: &str) -> Result<u64> {
        let metadata = std::fs::metadata(self.root.join(path))?;
        if !metadata.is_file() {
            return Err(asset_error(format!("资产不是普通文件：{path}")));
        }
        Ok(metadata.len())
    }

    fn head(&mut self, path: &str, max: usize) -> Result<Vec<u8>> {
        let mut file = File::open(self.root.join(path))?;
        let mut buffer = vec![0u8; max];
        let read = file.read(&mut buffer)?;
        buffer.truncate(read);
        Ok(buffer)
    }
}

/// 校验主题引用的全部资产：存在、大小、扩展名、魔数、SVG 文本。
pub fn validate_theme_assets<R: AssetReader + ?Sized>(
    document: &ThemeDocument,
    reader: &mut R,
) -> Result<()> {
    for spec in document.asset_specs() {
        let size = reader
            .size(&spec.path)
            .map_err(|e| asset_error(format!("资产不存在或无法读取：{}（{e}）", spec.path)))?;
        validate_asset_size(&spec, size)?;
        let head = reader.head(&spec.path, head_bytes(&spec.path))?;
        validate_asset_head(&spec, &head)?;
    }
    Ok(())
}

/// 按种类/扩展名校验大小上限。
pub fn max_bytes(path: &str) -> u64 {
    match extension(path).as_str() {
        "svg" => MAX_SVG_BYTES,
        "woff" | "woff2" | "ttf" | "otf" => MAX_FONT_BYTES,
        "mp4" | "webm" => MAX_VIDEO_BYTES,
        _ => MAX_RASTER_BYTES,
    }
}

/// 校验单个资产的大小（独立函数便于单测）。
pub fn validate_asset_size(spec: &AssetSpec, size: u64) -> Result<()> {
    let ext = extension(&spec.path);
    if !extension_allowed(spec.kind, &ext) {
        return Err(asset_error(format!(
            "资产类型不允许：{}（期望 {:?}）",
            spec.path, spec.kind
        )));
    }
    if size == 0 {
        return Err(asset_error(format!("资产为空：{}", spec.path)));
    }
    let max = max_bytes(&spec.path);
    if size > max {
        return Err(asset_error(format!(
            "资产过大：{}（{size} 字节，上限 {max}）",
            spec.path
        )));
    }
    Ok(())
}

/// 校验资产头部（魔数/SVG 文本）。`head` 可以只包含文件前缀。
pub fn validate_asset_head(spec: &AssetSpec, head: &[u8]) -> Result<()> {
    let ext = extension(&spec.path);
    let ok = match ext.as_str() {
        "png" => head.starts_with(&[0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a]),
        "jpg" | "jpeg" => head.starts_with(&[0xff, 0xd8, 0xff]),
        "webp" => head.len() >= 12 && &head[0..4] == b"RIFF" && &head[8..12] == b"WEBP",
        "svg" => validate_svg_text(head).is_ok(),
        "woff2" => head.starts_with(b"wOF2"),
        "woff" => head.starts_with(b"wOFF"),
        "ttf" => head.starts_with(&[0x00, 0x01, 0x00, 0x00]) || head.starts_with(b"true"),
        "otf" => head.starts_with(b"OTTO"),
        "mp4" => head.len() >= 12 && &head[4..8] == b"ftyp",
        "webm" => head.starts_with(&[0x1a, 0x45, 0xdf, 0xa3]),
        _ => false,
    };
    if ok {
        Ok(())
    } else {
        Err(asset_error(format!(
            "资产内容与扩展名不符（魔数校验失败）：{}",
            spec.path
        )))
    }
}

fn validate_svg_text(head: &[u8]) -> Result<()> {
    let lossy = String::from_utf8_lossy(head);
    let text = lossy.trim_start_matches('\u{feff}');
    let lower = text.to_ascii_lowercase();
    if !lower.contains("<svg") {
        return Err(asset_error("SVG 缺少 <svg> 根元素".to_string()));
    }
    for forbidden in [
        "<script",
        "<foreignobject",
        "<!doctype",
        "<!entity",
        "javascript:",
        " onload=",
        " onclick=",
        " onerror=",
        " onbegin=",
        "xlink:href=\"http",
        "xlink:href='http",
    ] {
        if lower.contains(forbidden) {
            return Err(asset_error(format!("SVG 含不允许的内容（{forbidden}）")));
        }
    }
    Ok(())
}

fn extension(path: &str) -> String {
    path.rsplit_once('.')
        .map(|(_, ext)| ext.to_ascii_lowercase())
        .unwrap_or_default()
}

fn extension_allowed(kind: AssetKind, ext: &str) -> bool {
    match kind {
        AssetKind::Image => matches!(ext, "png" | "jpg" | "jpeg" | "webp" | "svg"),
        AssetKind::Video => matches!(ext, "mp4" | "webm"),
        AssetKind::Font => matches!(ext, "woff" | "woff2" | "ttf" | "otf"),
    }
}

fn head_bytes(path: &str) -> usize {
    if extension(path) == "svg" {
        SVG_HEAD_BYTES
    } else {
        32
    }
}

/// 校验一个路径是否可作为主题资产路径（供命令/包检查复用）。
pub fn validate_path(path: &str) -> Result<()> {
    validate_asset_path(path, "主题资产")
}

/// 便捷：从目录读取并校验单个资产（测试/调试用）。
pub fn validate_asset_in_dir(dir: &Path, spec: &AssetSpec) -> Result<()> {
    let mut reader = FsAssetReader::new(dir);
    let size = reader.size(&spec.path)?;
    validate_asset_size(spec, size)?;
    let head = reader.head(&spec.path, head_bytes(&spec.path))?;
    validate_asset_head(spec, &head)
}

fn asset_error(message: String) -> RmcccError {
    RmcccError::Plugin {
        code: PluginErrorCode::ConfigInvalid,
        message,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec(path: &str, kind: AssetKind) -> AssetSpec {
        AssetSpec {
            path: path.to_string(),
            kind,
        }
    }

    #[test]
    fn accepts_known_magic_headers() {
        let png = [0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a, 0, 0];
        validate_asset_head(&spec("assets/a.png", AssetKind::Image), &png).unwrap();
        validate_asset_head(
            &spec("assets/a.jpg", AssetKind::Image),
            &[0xff, 0xd8, 0xff, 0xe0],
        )
        .unwrap();
        validate_asset_head(
            &spec("assets/a.mp4", AssetKind::Video),
            b"\0\0\0\x18ftypisom",
        )
        .unwrap();
        validate_asset_head(&spec("assets/a.woff2", AssetKind::Font), b"wOF2\0\0").unwrap();
        validate_asset_head(
            &spec("assets/a.svg", AssetKind::Image),
            br#"<svg xmlns="http://www.w3.org/2000/svg"></svg>"#,
        )
        .unwrap();
    }

    #[test]
    fn rejects_wrong_magic_and_oversize() {
        assert!(
            validate_asset_head(&spec("assets/a.png", AssetKind::Image), b"not a png").is_err()
        );
        assert!(validate_asset_size(
            &spec("assets/a.png", AssetKind::Image),
            MAX_RASTER_BYTES + 1
        )
        .is_err());
        assert!(
            validate_asset_size(&spec("assets/a.mp4", AssetKind::Video), MAX_VIDEO_BYTES + 1)
                .is_err()
        );
        assert!(validate_asset_size(&spec("assets/a.png", AssetKind::Video), 10).is_err());
    }

    #[test]
    fn rejects_dangerous_svg_content() {
        for bad in [
            b"<svg><script>alert(1)</script></svg>".as_slice(),
            b"<svg onload=\"alert(1)\"></svg>".as_slice(),
            b"<!DOCTYPE svg [<!ENTITY xxe SYSTEM 'file:///etc/passwd'>]><svg></svg>".as_slice(),
            b"<svg><a xlink:href=\"http://evil\"></a></svg>".as_slice(),
        ] {
            assert!(validate_asset_head(&spec("assets/a.svg", AssetKind::Image), bad).is_err());
        }
    }
}
