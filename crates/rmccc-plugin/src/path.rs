//! 包内相对路径与插件 ID 的公共校验（ADR-003 决策 2、ADR-004 决策 2）。

use rmccc_error::{PluginErrorCode, Result, RmcccError};

pub const MAX_PATH_LEN: usize = 180;
pub const MAX_PATH_DEPTH: usize = 12;

/// 校验 Manifest 中的 entry/theme 路径：必须是包内相对路径。
///
/// 只做**语法**校验；"文件确实存在且未逃逸"由安装器在解压后检查。
pub fn validate_relative_path(path: &str) -> Result<()> {
    if path.is_empty() {
        return Err(path_error(path, "路径为空"));
    }
    if path.len() > MAX_PATH_LEN {
        return Err(path_error(path, "路径过长"));
    }
    if path.starts_with('/') || path.starts_with('\\') {
        return Err(path_error(path, "不允许绝对路径"));
    }
    if path.contains('\\') {
        return Err(path_error(path, "不允许反斜杠（跨平台可移植性）"));
    }
    if path.contains('\0') || path.chars().any(|c| c.is_control()) {
        return Err(path_error(path, "不允许控制字符"));
    }
    if path.ends_with('/') || path.ends_with('.') {
        return Err(path_error(path, "不允许以 / 或 . 结尾"));
    }
    // 盘符（`C:`）与 UNC（`//`）在跨平台包里一律拒绝。
    if path.len() >= 2 && path.as_bytes()[1] == b':' {
        return Err(path_error(path, "不允许盘符路径"));
    }

    let segments: Vec<&str> = path.split('/').collect();
    if segments.len() > MAX_PATH_DEPTH {
        return Err(path_error(path, "目录层级过深"));
    }
    for segment in &segments {
        if segment.is_empty() {
            return Err(path_error(path, "不允许空路径段"));
        }
        if *segment == "." {
            return Err(path_error(path, "不允许 `.` 路径段"));
        }
        if *segment == ".." {
            return Err(path_error(path, "不允许 `..` 路径段"));
        }
        if segment.ends_with('.') || segment.ends_with(' ') {
            return Err(path_error(path, "路径段不允许以点或空格结尾"));
        }
        if is_windows_reserved(segment) {
            return Err(path_error(path, "包含 Windows 保留名称"));
        }
    }
    Ok(())
}

/// Windows 保留设备名（大小写不敏感，含带扩展名形式）。
pub fn is_windows_reserved(segment: &str) -> bool {
    let stem = segment.split('.').next().unwrap_or(segment);
    let upper = stem.to_ascii_uppercase();
    matches!(upper.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        || (upper.len() == 4
            && (upper.starts_with("COM") || upper.starts_with("LPT"))
            && upper.as_bytes()[3].is_ascii_digit()
            && upper.as_bytes()[3] != b'0')
}

/// 插件 ID：反向域名（ADR-004 决策 2）。
pub fn validate_plugin_id(id: &str) -> Result<()> {
    if id.is_empty() || id.len() > 128 {
        return Err(id_error(id, "长度必须在 1..=128 之间"));
    }
    let labels: Vec<&str> = id.split('.').collect();
    if labels.len() < 2 {
        return Err(id_error(id, "必须是至少两段的反向域名"));
    }
    for label in labels {
        if label.is_empty() {
            return Err(id_error(id, "不允许空段"));
        }
        if !label.starts_with(|c: char| c.is_ascii_lowercase()) {
            return Err(id_error(id, "每段必须以小写字母开头"));
        }
        if !label
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
        {
            return Err(id_error(id, "只允许小写字母、数字和 `-`"));
        }
    }
    Ok(())
}

/// 贡献项本地 ID：`^[a-z][a-z0-9-]*$`。
pub fn validate_local_id(id: &str) -> Result<()> {
    let valid = !id.is_empty()
        && id.len() <= 64
        && id.starts_with(|c: char| c.is_ascii_lowercase())
        && id
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-');
    if valid {
        Ok(())
    } else {
        Err(plugin_error(
            PluginErrorCode::ManifestInvalid,
            format!("贡献 ID 非法：{id:?}（要求 ^[a-z][a-z0-9-]*$，≤64）"),
        ))
    }
}

pub(crate) fn plugin_error(code: PluginErrorCode, message: String) -> RmcccError {
    RmcccError::Plugin { code, message }
}

fn path_error(path: &str, reason: &str) -> RmcccError {
    plugin_error(
        PluginErrorCode::PackageInvalid,
        format!("包内路径非法 {path:?}：{reason}"),
    )
}

fn id_error(id: &str, reason: &str) -> RmcccError {
    plugin_error(
        PluginErrorCode::ManifestInvalid,
        format!("插件 ID 非法 {id:?}：{reason}"),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_safe_paths() {
        for ok in [
            "manifest.json",
            "assets/icon.png",
            "themes/midnight.json",
            "locales/zh-CN.json",
        ] {
            validate_relative_path(ok).unwrap_or_else(|e| panic!("{ok}: {e}"));
        }
    }

    #[test]
    fn rejects_unsafe_paths() {
        for bad in [
            "",
            "../manifest.json",
            "assets/../../evil",
            "/etc/passwd",
            "C:/windows/x",
            "assets\\icon.png",
            "assets//icon.png",
            "assets/./icon.png",
            "assets/icon.png/",
            "CON",
            "assets/CON.txt",
            "assets/com1.json",
            "assets/trailing. ",
            "a\0b",
        ] {
            assert!(
                validate_relative_path(bad).is_err(),
                "should reject {bad:?}"
            );
        }
    }

    #[test]
    fn rejects_deep_paths() {
        let deep = (0..14)
            .map(|i| format!("d{i}"))
            .collect::<Vec<_>>()
            .join("/");
        assert!(validate_relative_path(&deep).is_err());
    }

    #[test]
    fn validates_plugin_ids() {
        for ok in ["dev.rmccc.theme", "top.snow.server-dashboard", "a.b"] {
            validate_plugin_id(ok).unwrap_or_else(|e| panic!("{ok}: {e}"));
        }
        for bad in [
            "",
            "single",
            "Dev.rmccc.x",
            "dev..x",
            ".dev.x",
            "dev.x.",
            "dev._x",
            "dev.x/y",
            "org.rmccc.x ",
        ] {
            assert!(validate_plugin_id(bad).is_err(), "should reject {bad:?}");
        }
    }

    #[test]
    fn validates_local_ids() {
        assert!(validate_local_id("server-status").is_ok());
        assert!(validate_local_id("a1").is_ok());
        for bad in ["", "Server", "1abc", "a_b", "a/b", "a.b"] {
            assert!(validate_local_id(bad).is_err(), "should reject {bad:?}");
        }
    }
}
