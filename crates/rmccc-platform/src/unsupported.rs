//! 未实现平台后端（Windows / Linux，**deferred**，指南 §5.7.3）。
//!
//! 本机仅 macOS；这些后端在原生 CI runner 触达前一律返回
//! [`rmccc_error::RmcccError::UnsupportedPlatform`]，**不得静默成功**。
//! 每个方法的 todo 注释写明将来的真实实现方向。

use crate::fs::PlatformFs;
use crate::java::{JavaDetector, JavaInstallation};
use crate::process::{PlatformProcessControl, ProcessPriority};
use crate::shell::{validate_openable_url, PlatformShell};
use rmccc_error::{Result, RmcccError};
use std::path::Path;

fn unsupported(what: &str) -> RmcccError {
    RmcccError::UnsupportedPlatform(format!(
        "{what} 的 {} 后端尚未实现（deferred 待原生 CI，见 docs/roadmap/PLATFORM_ADAPTATION.md）",
        std::env::consts::OS
    ))
}

/// 未实现平台的文件系统后端。
pub struct UnsupportedFs;

impl PlatformFs for UnsupportedFs {
    fn restrict_owner_only(&self, _path: &Path) -> Result<()> {
        // TODO(windows): `icacls <path> /inheritance:r /grant:r %USERNAME%:F`
        // TODO(linux): `set_permissions(0o600)` —— 与 macOS 同一实现，可直接复用。
        Err(unsupported("文件权限收紧"))
    }

    fn restrict_owner_only_dir(&self, _path: &Path) -> Result<()> {
        Err(unsupported("目录权限收紧"))
    }

    fn make_executable(&self, _path: &Path) -> Result<()> {
        // TODO(windows): 无对应概念，应返回 Ok(())。
        Err(unsupported("设置可执行位"))
    }

    fn symlink(&self, _target: &Path, _link: &Path) -> Result<()> {
        // TODO(windows): `std::os::windows::fs::symlink_dir/file`（需开发者模式）。
        Err(unsupported("符号链接"))
    }

    fn remove_symlink(&self, _link: &Path) -> Result<()> {
        Err(unsupported("删除符号链接"))
    }

    fn atomic_replace(&self, _from: &Path, _to: &Path) -> Result<()> {
        // TODO(windows): ReplaceFileW / MoveFileExW(MOVEFILE_REPLACE_EXISTING)。
        // TODO(linux): 与 macOS 相同的 rename(2)，可直接复用。
        Err(unsupported("原子替换文件"))
    }

    fn create_owner_only(&self, _path: &Path, _contents: &[u8]) -> Result<()> {
        // TODO(windows): OpenOptions + ACL 收紧。
        // TODO(linux): OpenOptionsExt::mode(0o600)，与 macOS 同一实现。
        Err(unsupported("创建仅属主可读文件"))
    }
}

/// 未实现平台的进程控制后端。
pub struct UnsupportedProcess;

impl PlatformProcessControl for UnsupportedProcess {
    fn terminate_tree(&self, _pid: u32) -> Result<()> {
        // TODO(windows): Job Object（CreateJobObject + AssignProcessToJobObject +
        // TerminateJobObject），或在拿到 pid 时记录并用 `taskkill /T /F`。
        Err(unsupported("终止进程树"))
    }

    fn set_priority(&self, _pid: u32, _priority: ProcessPriority) -> Result<()> {
        // TODO(windows): SetPriorityClass(PROCESS_SET_INFORMATION)。
        Err(unsupported("调整进程优先级"))
    }

    fn priority_command_prefix(&self, _priority: ProcessPriority) -> Vec<String> {
        // 非 Unix 无 `nice`；空前缀是**归一化结果**而非静默失败（不需要该能力）。
        Vec::new()
    }

    fn configure_child_process_group(&self, _cmd: &mut std::process::Command) -> Result<()> {
        // TODO(windows): 创建 Job Object 并在 spawn 后 AssignProcessToJobObject。
        // 当前 no-op：不加进程组不会破坏启动，只是无法一次性终止子进程树。
        Ok(())
    }
}

/// 未实现平台的 shell 后端。
pub struct UnsupportedShell;

impl PlatformShell for UnsupportedShell {
    fn open_url(&self, url: &str) -> Result<()> {
        // 校验先行：即使后端未实现，非法 scheme 也必须被拒绝。
        validate_openable_url(url)?;
        // TODO(windows): `cmd /c start "" <url>` 或 ShellExecuteW。
        // TODO(linux): `xdg-open <url>`。
        Err(unsupported("打开 URL"))
    }

    fn open_path(&self, _path: &Path) -> Result<()> {
        // TODO(windows): `explorer <path>`。
        // TODO(linux): `xdg-open <path>`。
        Err(unsupported("打开路径"))
    }
}

/// 未实现平台的 Java 探测后端。
pub struct UnsupportedJavaDetector;

impl JavaDetector for UnsupportedJavaDetector {
    fn detect_all(&self) -> Result<Vec<JavaInstallation>> {
        // TODO(windows): 注册表 `HKLM\SOFTWARE\JavaSoft\JDK` + `C:\Program Files\Java\*`。
        // TODO(linux): `/usr/lib/jvm/*` + `/etc/alternatives/java`（实现与 macOS 基本同构）。
        Err(unsupported("Java 探测"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backends_report_unsupported_instead_of_panicking() {
        let f = UnsupportedFs;
        assert!(matches!(
            f.restrict_owner_only(Path::new("/tmp/x")),
            Err(RmcccError::UnsupportedPlatform(_))
        ));
        assert!(matches!(
            UnsupportedProcess.terminate_tree(1),
            Err(RmcccError::UnsupportedPlatform(_))
        ));
        assert!(matches!(
            UnsupportedShell.open_url("https://example.com"),
            Err(RmcccError::UnsupportedPlatform(_))
        ));
        assert!(matches!(
            UnsupportedJavaDetector.detect_all(),
            Err(RmcccError::UnsupportedPlatform(_))
        ));
        assert!(matches!(
            UnsupportedJavaDetector.find(17),
            Err(RmcccError::UnsupportedPlatform(_))
        ));
    }

    #[test]
    fn shell_still_validates_scheme_before_reporting_unsupported() {
        assert!(matches!(
            UnsupportedShell.open_url("file:///etc/passwd"),
            Err(RmcccError::Validation(_))
        ));
    }

    #[test]
    fn priority_prefix_is_empty_not_an_error() {
        assert!(UnsupportedProcess
            .priority_command_prefix(ProcessPriority::High)
            .is_empty());
    }
}

/// 未实现平台的物理内存探测后端。
pub struct UnsupportedSystemMemory;

impl crate::system::SystemMemory for UnsupportedSystemMemory {
    fn physical_memory(&self) -> Result<crate::system::PhysicalMemory> {
        // TODO(windows): GlobalMemoryStatusEx。
        // TODO(linux): 解析 /proc/meminfo 的 MemTotal / MemAvailable。
        Err(unsupported("物理内存探测"))
    }
}
