//! 逻辑插件入口校验（Phase 5，ADR-007）。
//!
//! 逻辑插件是 **core WASM 模块**，由独立 helper 进程用 wasmi 执行；主程序从不链接
//! WASM 引擎。本模块只做"包/Manifest 层面的形状校验"，运行时限制在 `rmccc-logic`：
//!
//! - 入口必须是 `logic/` 下的 `.wasm`；
//! - 不允许原生动态库（`.dylib`/`.dll`/`.so`）或脚本；
//! - 与 `ui/` 一样，入口声明与目录内容必须互相印证（在安装器里做存在性检查）。

use rmccc_error::{PluginErrorCode, Result};

use crate::path::{plugin_error, validate_relative_path};

/// 包内逻辑资源根目录。
pub const LOGIC_ROOT_DIR: &str = "logic";

/// 逻辑模块允许的扩展名。
pub const LOGIC_EXTENSION: &str = "wasm";

/// 单个 `.wasm` 的大小上限（与 helper 的 `MAX_MODULE_BYTES` 一致）。
pub const MAX_LOGIC_BYTES: u64 = 2 * 1024 * 1024;

/// 校验 `entry.logic`。
pub fn validate_logic_entry(entry: &str) -> Result<()> {
    validate_relative_path(entry)?;
    let Some(rest) = entry.strip_prefix("logic/") else {
        return Err(plugin_error(
            PluginErrorCode::ManifestInvalid,
            format!("entry.logic 必须位于 logic/ 下：{entry}"),
        ));
    };
    if rest.is_empty() || rest.contains('/') {
        return Err(plugin_error(
            PluginErrorCode::ManifestInvalid,
            format!("entry.logic 必须是 logic/ 下的单个文件：{entry}"),
        ));
    }
    if !entry.to_ascii_lowercase().ends_with(".wasm") {
        return Err(plugin_error(
            PluginErrorCode::ManifestInvalid,
            format!("entry.logic 必须是 .wasm（不允许原生库/脚本）：{entry}"),
        ));
    }
    Ok(())
}

/// 包内 `logic/` 文件的类型白名单：只允许 `.wasm`。
pub fn validate_logic_file(path: &str) -> Result<()> {
    validate_relative_path(path)?;
    if path.to_ascii_lowercase().ends_with(".wasm") {
        Ok(())
    } else {
        Err(plugin_error(
            PluginErrorCode::PackageInvalid,
            format!("logic/ 下只允许 .wasm 文件：{path}"),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_a_single_wasm_file_under_logic() {
        validate_logic_entry("logic/plugin.wasm").expect("valid");
        validate_logic_entry("logic/Compat.WASM").expect("case-insensitive extension");
    }

    #[test]
    fn rejects_entries_outside_logic_or_with_other_extensions() {
        for bad in [
            "plugin.wasm",
            "logic/",
            "logic/plugin.dylib",
            "logic/plugin.dll",
            "logic/plugin.so",
            "logic/plugin.js",
            "logic/nested/plugin.wasm",
            "logic/../plugin.wasm",
        ] {
            assert!(validate_logic_entry(bad).is_err(), "expected reject: {bad}");
        }
    }

    #[test]
    fn package_rule_only_allows_wasm_inside_logic() {
        assert!(validate_logic_file("logic/plugin.wasm").is_ok());
        for bad in ["logic/readme.md", "logic/plugin.wasm.bak", "logic/run.sh"] {
            assert!(validate_logic_file(bad).is_err(), "expected reject: {bad}");
        }
    }
}
