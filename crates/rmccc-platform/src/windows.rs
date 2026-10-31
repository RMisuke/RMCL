//! Windows 后端（原生 Windows 10 1809+ / x86_64）。
//!
//! 平台差异集中在这里：ACL、进程树终止、Java 探测、物理内存、ShellExecute。
//! 领域 crate 只经 [`crate::Platform`] 的 trait 调用，不出现 `#[cfg(windows)]`。
//!
//! 原生 Windows 编译与实机验收由 Windows runner 完成；本文件的存在不代表
//! Windows 已验收（见 `docs/roadmap/PLATFORM_ADAPTATION.md`）。

use crate::fs::PlatformFs;
use crate::java::{normalize, probe_java_binary, probe_java_home, JavaDetector, JavaInstallation};
use crate::process::{hide_console_window, PlatformProcessControl, ProcessPriority};
use crate::shell::{validate_openable_url, PlatformShell};
use crate::system::{PhysicalMemory, SystemMemory};
use rmccc_error::{Result, RmcccError};
use std::collections::HashSet;
use std::ffi::OsStr;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use ::windows::core::HSTRING;
use ::windows::Win32::Foundation::CloseHandle;
use ::windows::Win32::System::SystemInformation::{GlobalMemoryStatusEx, MEMORYSTATUSEX};
use ::windows::Win32::System::Threading::{
    OpenProcess, SetPriorityClass, TerminateProcess, ABOVE_NORMAL_PRIORITY_CLASS,
    BELOW_NORMAL_PRIORITY_CLASS, HIGH_PRIORITY_CLASS, IDLE_PRIORITY_CLASS, NORMAL_PRIORITY_CLASS,
    PROCESS_SET_INFORMATION, PROCESS_TERMINATE,
};
use ::windows::Win32::UI::Shell::ShellExecuteW;
use ::windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

/// Windows 文件系统后端。
pub struct WindowsFs;

impl PlatformFs for WindowsFs {
    fn restrict_owner_only(&self, path: &Path) -> Result<()> {
        set_owner_only_acl(path, "F")
    }

    fn restrict_owner_only_dir(&self, path: &Path) -> Result<()> {
        // OI/CI 让目录中新创建的文件和子目录继承这条显式 ACE。
        set_owner_only_acl(path, "(OI)(CI)F")
    }

    fn make_executable(&self, _path: &Path) -> Result<()> {
        // Windows 没有 Unix 可执行位；`.bat`/`.ps1` 由扩展名关联执行。
        Ok(())
    }

    fn symlink(&self, target: &Path, link: &Path) -> Result<()> {
        // 需要开发者模式或 SeCreateSymbolicLinkPrivilege；失败时明确报错，
        // 不静默创建一个普通文件/目录。
        let result = if target.is_dir() {
            std::os::windows::fs::symlink_dir(target, link)
        } else {
            std::os::windows::fs::symlink_file(target, link)
        };
        result?;
        Ok(())
    }

    fn remove_symlink(&self, link: &Path) -> Result<()> {
        match std::fs::remove_file(link) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(file_error) => match std::fs::remove_dir(link) {
                Ok(()) => Ok(()),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
                Err(_) => Err(file_error.into()),
            },
        }
    }

    fn atomic_replace(&self, from: &Path, to: &Path) -> Result<()> {
        // Rust std 在 Windows 上使用 MoveFileExW（目标存在时替换），满足
        // “同卷文件原子替换”；目录替换/目标被占用时仍会返回错误。
        std::fs::rename(from, to)?;
        Ok(())
    }

    fn create_owner_only(&self, path: &Path, contents: &[u8]) -> Result<()> {
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path)?;
        file.write_all(contents)?;
        file.flush()?;
        drop(file);

        if let Err(error) = set_owner_only_acl(path, "F") {
            // 创建失败时不要留下一个权限不明的半成品。
            let _ = std::fs::remove_file(path);
            return Err(error);
        }
        Ok(())
    }
}

/// 用 `icacls` 移除继承 ACE，只给当前用户完全控制。
///
/// `rights` 为 `F`（文件）或 `(OI)(CI)F`（目录及其子项）。Windows 的
/// `%APPDATA%` 默认已经按用户隔离；这里再做显式收紧，保证 `-d` 指向共享
/// 目录时 `.key`/argfile 仍然只有当前用户可读。
fn set_owner_only_acl(path: &Path, rights: &str) -> Result<()> {
    if !path.exists() {
        return Err(RmcccError::Validation(format!(
            "ACL 目标不存在: {}",
            path.display()
        )));
    }

    let users = current_user_candidates();
    if users.is_empty() {
        return Err(RmcccError::Other(
            "无法确定当前 Windows 用户，拒绝写入敏感文件".to_string(),
        ));
    }

    let mut last_error = String::new();
    for user in users {
        let mut cmd = Command::new("icacls");
        cmd.arg(path)
            .arg("/inheritance:r")
            .arg("/grant:r")
            .arg(format!("{user}:{rights}"))
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        hide_console_window(&mut cmd);

        match cmd.status() {
            Ok(status) if status.success() => return Ok(()),
            Ok(status) => last_error = format!("icacls 退出状态 {status}"),
            Err(error) => last_error = format!("启动 icacls 失败: {error}"),
        }
    }

    Err(RmcccError::Other(format!(
        "无法收紧 ACL {}: {last_error}",
        path.display()
    )))
}

/// 当前用户的候选 `DOMAIN\User` 形式。
///
/// 先试 `USERDOMAIN\USERNAME`（域账户/本地账户），再试 `COMPUTERNAME\USERNAME`，
/// 最后只传 `USERNAME`。不同 Windows 账户类型（本地、域、Microsoft 账户）
/// 对 `icacls` 的解析不同，逐个回退比只拼一种形式可靠。
fn current_user_candidates() -> Vec<String> {
    let user = env_string("USERNAME");
    let domain = env_string("USERDOMAIN");
    let computer = env_string("COMPUTERNAME");

    let mut out = Vec::new();
    if let Some(user) = user.as_deref() {
        if let Some(domain) = domain.as_deref() {
            out.push(format!("{domain}\\{user}"));
        }
        if let Some(computer) = computer.as_deref() {
            let candidate = format!("{computer}\\{user}");
            if !out
                .iter()
                .any(|entry| entry.eq_ignore_ascii_case(&candidate))
            {
                out.push(candidate);
            }
        }
        if !out.iter().any(|entry| entry.eq_ignore_ascii_case(user)) {
            out.push(user.to_string());
        }
    }
    out
}

fn env_string(key: &str) -> Option<String> {
    std::env::var_os(key)
        .map(|value| value.to_string_lossy().into_owned())
        .filter(|value| !value.trim().is_empty())
}

/// Windows 进程控制后端。
pub struct WindowsProcess;

impl PlatformProcessControl for WindowsProcess {
    fn terminate_tree(&self, pid: u32) -> Result<()> {
        if pid == 0 {
            return Err(RmcccError::Validation("pid 不能为 0".into()));
        }

        let pid_arg = pid.to_string();
        let mut cmd = Command::new("taskkill");
        cmd.args(["/PID", pid_arg.as_str(), "/T", "/F"])
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        hide_console_window(&mut cmd);

        match cmd.status() {
            Ok(status) if status.success() => Ok(()),
            // taskkill 对“进程/进程树不存在”返回 128；终止目标已经消失，
            // 与 Unix 的 ESRCH 一样视为成功。
            Ok(status) if status.code() == Some(128) => Ok(()),
            Ok(_) | Err(_) => terminate_process(pid),
        }
    }

    fn set_priority(&self, pid: u32, priority: ProcessPriority) -> Result<()> {
        if pid == 0 {
            return Err(RmcccError::Validation("pid 不能为 0".into()));
        }
        let class = match priority {
            ProcessPriority::Low => IDLE_PRIORITY_CLASS,
            ProcessPriority::BelowNormal => BELOW_NORMAL_PRIORITY_CLASS,
            ProcessPriority::Normal => NORMAL_PRIORITY_CLASS,
            ProcessPriority::AboveNormal => ABOVE_NORMAL_PRIORITY_CLASS,
            ProcessPriority::High => HIGH_PRIORITY_CLASS,
        };

        let handle = unsafe { OpenProcess(PROCESS_SET_INFORMATION, false, pid) }
            .map_err(|error| RmcccError::Other(format!("OpenProcess({pid}) 失败: {error}")))?;
        let result = unsafe { SetPriorityClass(handle, class) };
        let _ = unsafe { CloseHandle(handle) };
        result.map_err(|error| RmcccError::Other(format!("调整进程 {pid} 优先级失败: {error}")))
    }

    fn priority_command_prefix(&self, _priority: ProcessPriority) -> Vec<String> {
        // Windows 通过 SetPriorityClass 生效，不存在 `nice` 前缀。
        Vec::new()
    }

    fn configure_child_process_group(&self, _cmd: &mut std::process::Command) -> Result<()> {
        // 终止整棵树由 taskkill /T 负责；不需要额外的进程组标记。
        Ok(())
    }
}

/// `taskkill` 不可用时的单进程兜底（至少避免游戏进程永久残留）。
fn terminate_process(pid: u32) -> Result<()> {
    let handle = unsafe { OpenProcess(PROCESS_TERMINATE, false, pid) }
        .map_err(|error| RmcccError::Other(format!("OpenProcess({pid}) 失败: {error}")))?;
    let result = unsafe { TerminateProcess(handle, 1) };
    let _ = unsafe { CloseHandle(handle) };
    result.map_err(|error| RmcccError::Other(format!("TerminateProcess({pid}) 失败: {error}")))
}

/// Windows shell 后端：ShellExecuteW 打开 URL / 目录。
pub struct WindowsShell;

impl PlatformShell for WindowsShell {
    fn open_url(&self, url: &str) -> Result<()> {
        validate_openable_url(url)?;
        shell_execute(url)
    }

    fn open_path(&self, path: &Path) -> Result<()> {
        if !path.exists() {
            return Err(RmcccError::Validation(format!(
                "路径不存在: {}",
                path.display()
            )));
        }
        shell_execute(&path.to_string_lossy())
    }
}

fn shell_execute(target: &str) -> Result<()> {
    let target = HSTRING::from(target);
    let result = unsafe {
        ShellExecuteW(
            None,
            ::windows::core::w!("open"),
            &target,
            None,
            None,
            SW_SHOWNORMAL,
        )
    };
    // ShellExecuteW 返回值 <= 32 表示错误码；不是 Win32 的 BOOL。
    if result.0 as isize <= 32 {
        return Err(RmcccError::Other(format!(
            "ShellExecuteW 失败（code {}）",
            result.0 as isize
        )));
    }
    Ok(())
}

/// Windows Java 探测：JAVA_HOME、PATH、常见 JDK 安装目录（Oracle/Adoptium/
/// Microsoft/Zulu/Liberica/Corretto/Semeru/GraalVM/JetBrains/Android Studio）。
pub struct WindowsJavaDetector;

impl JavaDetector for WindowsJavaDetector {
    fn detect_all(&self) -> Result<Vec<JavaInstallation>> {
        let mut found = Vec::new();

        // JAVA_HOME 是最明确的用户选择，优先探测。
        if let Some(java) = probe_java_home(".exe") {
            found.push(java);
        }

        let mut candidates = Vec::new();
        if let Some(path) = env_string("PATH") {
            for directory in std::env::split_paths(OsStr::new(&path)) {
                candidates.push(directory.join("java.exe"));
            }
        }
        for root in java_roots() {
            collect_java_bins(&root, 2, &mut candidates);
        }

        let mut seen = HashSet::new();
        for candidate in candidates {
            let key = candidate.to_string_lossy().to_ascii_lowercase();
            if !seen.insert(key) {
                continue;
            }
            if let Some(java) = probe_java_binary(&candidate) {
                found.push(java);
            }
        }

        Ok(normalize(found))
    }
}

fn java_roots() -> Vec<PathBuf> {
    const VENDORS: &[&str] = &[
        "Java",
        "Eclipse Adoptium",
        "Microsoft",
        "Zulu",
        "BellSoft",
        "Amazon Corretto",
        "Semeru",
        "GraalVM",
        "RedHat",
        "SapMachine",
        "OpenJDK",
        "JetBrains",
    ];

    let mut roots = Vec::new();
    for key in ["ProgramFiles", "ProgramFiles(x86)"] {
        let Some(base) = env_string(key).map(PathBuf::from) else {
            continue;
        };
        for vendor in VENDORS {
            roots.push(base.join(vendor));
        }
        roots.push(base.join("Android").join("Android Studio").join("jbr"));
    }

    if let Some(base) = env_string("LOCALAPPDATA").map(PathBuf::from) {
        let programs = base.join("Programs");
        for vendor in VENDORS {
            roots.push(programs.join(vendor));
        }
        roots.push(programs.join("Android Studio").join("jbr"));
    }

    if let Some(home) = env_string("USERPROFILE").map(PathBuf::from) {
        roots.push(home.join(".jdks"));
        roots.push(home.join("scoop").join("apps").join("openjdk"));
    }

    roots
}

/// 在 `root` 下查找 `bin/java.exe`，最多向下递归 `depth` 层。
fn collect_java_bins(root: &Path, depth: usize, out: &mut Vec<PathBuf>) {
    if !root.is_dir() {
        return;
    }

    let direct = root.join("bin").join("java.exe");
    if direct.is_file() {
        out.push(direct);
    }
    if depth == 0 {
        return;
    }

    let Ok(entries) = std::fs::read_dir(root) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_java_bins(&path, depth - 1, out);
        }
    }
}

/// Windows 物理内存探测。
pub struct WindowsSystemMemory;

impl SystemMemory for WindowsSystemMemory {
    fn physical_memory(&self) -> Result<PhysicalMemory> {
        let mut status = MEMORYSTATUSEX {
            dwLength: std::mem::size_of::<MEMORYSTATUSEX>() as u32,
            ..Default::default()
        };
        unsafe { GlobalMemoryStatusEx(&mut status) }
            .map_err(|error| RmcccError::Other(format!("GlobalMemoryStatusEx 失败: {error}")))?;

        if status.ullTotalPhys == 0 {
            return Err(RmcccError::Other(
                "GlobalMemoryStatusEx 返回总内存 0".to_string(),
            ));
        }
        Ok(PhysicalMemory {
            total_bytes: status.ullTotalPhys,
            available_bytes: status.ullAvailPhys.min(status.ullTotalPhys),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn collects_nested_jdk_layouts() {
        let root = std::env::temp_dir().join(format!(
            "rmccc-win-java-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        let jdk = root.join("jdk-21").join("bin");
        std::fs::create_dir_all(&jdk).expect("create fake jdk");
        std::fs::write(jdk.join("java.exe"), b"fake").expect("write fake java");

        let mut found = Vec::new();
        collect_java_bins(&root, 2, &mut found);
        assert_eq!(found, vec![jdk.join("java.exe")]);

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn make_executable_is_a_noop_on_windows() {
        WindowsFs
            .make_executable(Path::new("start.bat"))
            .expect("Windows has no executable bit");
    }

    #[test]
    fn atomic_replace_overwrites_existing_file() {
        let dir = std::env::temp_dir().join(format!(
            "rmccc-win-fs-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        std::fs::create_dir_all(&dir).expect("temp dir");
        let target = dir.join("state.json");
        let temporary = dir.join("state.json.tmp");
        std::fs::write(&target, b"old").expect("old");
        std::fs::write(&temporary, b"new").expect("new");

        WindowsFs
            .atomic_replace(&temporary, &target)
            .expect("replace");
        assert_eq!(std::fs::read(&target).expect("read"), b"new");
        assert!(!temporary.exists());

        let _ = std::fs::remove_dir_all(&dir);
    }
}
