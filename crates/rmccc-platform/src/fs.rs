//! 文件系统耐久与权限（指南 §5.7.2「文件权限 / 文件系统耐久」）。
//!
//! 领域 crate 不得直接 `#[cfg(unix)] set_permissions(0o600)` 或 `cfg(windows) icacls`；
//! 一律经 [`PlatformFs`] 注入调用。

use rmccc_error::Result;
use std::path::Path;

/// 平台文件系统能力：权限收紧、符号链接。
///
/// 每个方法都必须返回归一化结果或 [`rmccc_error::RmcccError::UnsupportedPlatform`]，
/// 不得静默失败或 panic（指南 §5.7.1 第 4 条）。
pub trait PlatformFs: Send + Sync {
    /// 把文件权限收紧到"仅属主可读写"（Unix `0600` / Windows 收紧 ACL）。
    ///
    /// 用于 `.key`、argfile 等含密钥或令牌的敏感文件。
    fn restrict_owner_only(&self, path: &Path) -> Result<()>;

    /// 把**目录**权限收紧到"仅属主可进入"（Unix `0700` / Windows 收紧 ACL）。
    ///
    /// 用于 `agent/`、`private/` 等敏感目录：目录需要执行位才能遍历，
    /// 不能复用文件的 `0600`。
    fn restrict_owner_only_dir(&self, path: &Path) -> Result<()>;

    /// 让文件可执行（Unix `0755`；Windows 概念上无操作）。
    fn make_executable(&self, path: &Path) -> Result<()>;

    /// 创建指向 `target` 的符号链接（Windows 需开发者模式/管理员，返回错误）。
    fn symlink(&self, target: &Path, link: &Path) -> Result<()>;

    /// 删除符号链接（不存在视为成功）。
    fn remove_symlink(&self, link: &Path) -> Result<()>;

    /// 用 `from` 原子替换 `to`（同卷）。
    ///
    /// Unix 上 `rename(2)` 覆盖目标；Windows 的 `rename` 在目标存在时会失败，
    /// 需要 `ReplaceFileW`/`MoveFileEx` 语义——因此这是平台差异，必须经适配层。
    /// 调用方负责在失败时保留旧文件（`to` 不应被提前删除）。
    fn atomic_replace(&self, from: &Path, to: &Path) -> Result<()>;
}

/// 后端标识（日志 / 诊断）。
pub fn fs_backend_name() -> &'static str {
    if cfg!(target_os = "macos") {
        "macos"
    } else if cfg!(target_os = "windows") {
        "windows"
    } else if cfg!(target_os = "linux") {
        "linux"
    } else {
        "unsupported"
    }
}
