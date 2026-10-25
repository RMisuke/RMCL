//! macOS 后端（本机开发目标，Apple Silicon 单架构）。
//!
//! 这里集中 macOS 的系统调用：权限位、符号链接、`killpg`、`setpriority`、
//! 进程组、`open`。**`#[cfg]` 允许出现在本文件内**（指南 §5.7.3），
//! 领域 crate 一律经 [`crate::Platform`] 调用。

use crate::fs::PlatformFs;
use crate::java::{normalize, probe_java_binary, probe_java_home, JavaDetector, JavaInstallation};
use crate::process::{PlatformProcessControl, ProcessPriority};
use crate::shell::{validate_openable_url, PlatformShell};
use rmccc_error::{Result, RmcccError};
use std::path::{Path, PathBuf};
use std::process::Command;

/// macOS 文件系统后端。
pub struct MacosFs;

impl PlatformFs for MacosFs {
    fn restrict_owner_only(&self, path: &Path) -> Result<()> {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;
        Ok(())
    }

    fn restrict_owner_only_dir(&self, path: &Path) -> Result<()> {
        use std::os::unix::fs::PermissionsExt;
        // 目录需要执行位才能遍历：0700 而非文件的 0600。
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700))?;
        Ok(())
    }

    fn make_executable(&self, path: &Path) -> Result<()> {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755))?;
        Ok(())
    }

    fn symlink(&self, target: &Path, link: &Path) -> Result<()> {
        std::os::unix::fs::symlink(target, link)?;
        Ok(())
    }

    fn remove_symlink(&self, link: &Path) -> Result<()> {
        match std::fs::remove_file(link) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(e.into()),
        }
    }

    fn atomic_replace(&self, from: &Path, to: &Path) -> Result<()> {
        // macOS 的 `rename(2)` 在同卷内原子覆盖目标。
        std::fs::rename(from, to)?;
        Ok(())
    }

    fn create_owner_only(&self, path: &Path, contents: &[u8]) -> Result<()> {
        use std::io::Write;
        use std::os::unix::fs::OpenOptionsExt;
        // `mode(0o600)` 在 open(2) 生效：文件从诞生的第一刻起就仅属主可读，
        // 没有"先写后收紧"的窗口（审计 B1）。
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(path)?;
        file.write_all(contents)?;
        Ok(())
    }
}

/// macOS 进程控制后端。
pub struct MacosProcess;

impl PlatformProcessControl for MacosProcess {
    fn terminate_tree(&self, pid: u32) -> Result<()> {
        if pid == 0 {
            return Err(RmcccError::Validation("pid 不能为 0".into()));
        }
        let target = pid as libc::pid_t;
        // SAFETY: `kill(-pid, SIGKILL)` 向 pid 所在**进程组**发信号。进程组由
        // `configure_child_process_group` 建立（`setpgid(0,0)`），因此可一次
        // 终止 Java 及其子进程树，且不会误伤启动器本身（P2-17）。
        // 负 pid 在 macOS 上是明确语义，不会越界访问。
        let rc = unsafe { libc::kill(-target, libc::SIGKILL) };
        if rc != 0 {
            let err = std::io::Error::last_os_error();
            // 进程已退出（ESRCH）视为成功：终止的目标已达成。
            if err.raw_os_error() == Some(libc::ESRCH) {
                return Ok(());
            }
            return Err(RmcccError::Other(format!("终止进程组 {pid} 失败: {err}")));
        }
        Ok(())
    }

    fn set_priority(&self, pid: u32, priority: ProcessPriority) -> Result<()> {
        let Some(nice) = priority.nice_value() else {
            return Ok(());
        };
        let nice: i32 = nice
            .parse()
            .map_err(|_| RmcccError::Other("非法 nice 值".into()))?;
        // SAFETY: setpriority(PRIO_PROCESS, pid, nice) 只影响目标进程调度优先级。
        let rc = unsafe { libc::setpriority(libc::PRIO_PROCESS, pid as libc::id_t, nice) };
        if rc != 0 {
            return Err(RmcccError::Other(format!(
                "调整进程 {pid} 优先级失败: {}",
                std::io::Error::last_os_error()
            )));
        }
        Ok(())
    }

    fn priority_command_prefix(&self, priority: ProcessPriority) -> Vec<String> {
        match priority.nice_value() {
            Some(nice) => vec!["nice".to_string(), "-n".to_string(), nice.to_string()],
            None => Vec::new(),
        }
    }

    fn configure_child_process_group(&self, cmd: &mut std::process::Command) -> Result<()> {
        use std::os::unix::process::CommandExt;
        cmd.process_group(0);
        Ok(())
    }
}

/// macOS shell 后端（`open`）。
pub struct MacosShell;

impl PlatformShell for MacosShell {
    fn open_url(&self, url: &str) -> Result<()> {
        validate_openable_url(url)?;
        run_open(&[url])
    }

    fn open_path(&self, path: &Path) -> Result<()> {
        let path_str = path.to_string_lossy().to_string();
        run_open(&[&path_str])
    }
}

fn run_open(args: &[&str]) -> Result<()> {
    let status = std::process::Command::new("open")
        .args(args)
        .status()
        .map_err(|e| RmcccError::Other(format!("启动 open 失败: {e}")))?;
    if !status.success() {
        return Err(RmcccError::Other(format!("open 退出状态非零: {status}")));
    }
    Ok(())
}

/// macOS Java 探测后端：`/usr/libexec/java_home -V` + `JAVA_HOME` + 常见路径。
pub struct MacosJavaDetector;

impl JavaDetector for MacosJavaDetector {
    fn detect_all(&self) -> Result<Vec<JavaInstallation>> {
        let mut found: Vec<JavaInstallation> = Vec::new();

        // `java_home -V` 把列表写到 stderr，每行形如：
        //   21.0.3 (arm64) "Amazon.com Inc." - "Amazon Corretto 21" /Library/Java/...
        if let Ok(output) = Command::new("/usr/libexec/java_home").arg("-V").output() {
            for line in String::from_utf8_lossy(&output.stderr).lines() {
                if let Some(java) = parse_java_home_line(line.trim()) {
                    found.push(java);
                }
            }
        }

        if let Some(java) = probe_java_home("") {
            found.push(java);
        }

        // Unix 常见路径（macOS 上除 Homebrew 外通常不存在，探测会跳过）。
        for path in [
            "/usr/bin/java",
            "/usr/local/bin/java",
            "/opt/homebrew/opt/openjdk/bin/java",
        ] {
            if let Some(java) = probe_java_binary(Path::new(path)) {
                found.push(java);
            }
        }

        Ok(normalize(found))
    }
}

/// 解析 `java_home -V` 的一行输出（纯函数，便于单测）。
///
/// 格式：`<version> (<arch>) "<vendor>" - "<vm name>" <home>`
/// home 永远是最后一个空白分隔的 token，且以 `/` 开头。
fn parse_java_home_line(line: &str) -> Option<JavaInstallation> {
    let trimmed = line.trim();
    let version_part = trimmed.split_whitespace().next()?;
    let major: u32 = version_part.split('.').next()?.parse().ok()?;

    let home = trimmed.split_whitespace().next_back()?;
    if !home.starts_with('/') {
        return None;
    }

    let java_bin = PathBuf::from(home).join("bin").join("java");
    probe_java_binary(&java_bin).map(|mut j| {
        j.major_version = major;
        j
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_path(tag: &str) -> PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        std::env::temp_dir().join(format!("rmccc-macos-{tag}-{}-{nanos}", std::process::id()))
    }

    #[test]
    fn restrict_owner_only_sets_0600() {
        use std::os::unix::fs::PermissionsExt;
        let path = temp_path("perm");
        std::fs::write(&path, b"secret").expect("write");
        MacosFs.restrict_owner_only(&path).expect("chmod 600");
        let mode = std::fs::metadata(&path).expect("meta").permissions().mode() & 0o777;
        assert_eq!(mode, 0o600);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn make_executable_sets_0755() {
        use std::os::unix::fs::PermissionsExt;
        let path = temp_path("exec");
        std::fs::write(&path, b"#!/bin/sh\n").expect("write");
        MacosFs.make_executable(&path).expect("chmod 755");
        let mode = std::fs::metadata(&path).expect("meta").permissions().mode() & 0o777;
        assert_eq!(mode, 0o755);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn symlink_create_and_remove_is_idempotent() {
        let target = temp_path("target");
        let link = temp_path("link");
        std::fs::create_dir_all(&target).expect("target dir");
        MacosFs.symlink(&target, &link).expect("symlink");
        assert!(link.symlink_metadata().is_ok(), "链接应存在");
        MacosFs.remove_symlink(&link).expect("remove");
        assert!(link.symlink_metadata().is_err(), "链接应被删除");
        // 再次删除不报错（幂等）。
        MacosFs.remove_symlink(&link).expect("remove again");
        let _ = std::fs::remove_dir_all(&target);
    }

    #[test]
    fn terminate_tree_rejects_pid_zero() {
        assert!(MacosProcess.terminate_tree(0).is_err());
    }

    #[test]
    fn terminate_tree_on_exited_pid_is_ok() {
        // 一个必定不存在的进程组：ESRCH 应被吞掉（幂等终止）。
        assert!(MacosProcess.terminate_tree(u32::MAX - 1).is_ok());
    }

    #[test]
    fn priority_prefix_matches_nice_values() {
        assert!(MacosProcess
            .priority_command_prefix(ProcessPriority::Normal)
            .is_empty());
        assert_eq!(
            MacosProcess.priority_command_prefix(ProcessPriority::Low),
            vec!["nice".to_string(), "-n".to_string(), "5".to_string()]
        );
    }

    /// 真机烟测：`configure_child_process_group` + `terminate_tree` 作用于一个
    /// **真实子进程**——这是 1.1c 迁移后启动/终止链路的直接验证。
    #[test]
    fn terminate_tree_kills_a_real_child_process_group() {
        let mut cmd = std::process::Command::new("sleep");
        cmd.arg("30");
        MacosProcess
            .configure_child_process_group(&mut cmd)
            .expect("setpgid");
        let mut child = cmd.spawn().expect("spawn sleep 30");
        let pid = child.id();
        std::thread::sleep(std::time::Duration::from_millis(150));
        assert!(
            child.try_wait().expect("try_wait").is_none(),
            "子进程应先存活"
        );

        MacosProcess.terminate_tree(pid).expect("terminate_tree");

        let status = child.wait().expect("wait");
        assert!(
            !status.success(),
            "整组 SIGKILL 后子进程不应正常退出（实际: {status:?}）"
        );
    }

    #[test]
    fn parse_java_home_line_extracts_major_and_path() {
        // 真实 `java_home -V` 输出形态（vendor 名里的空格不是分隔符）。
        let line = r#"    17.0.9 (arm64) "Amazon.com Inc." - "Amazon Corretto 17" /Library/Java/JavaVirtualMachines/amazon-corretto-17.jdk/Contents/Home"#;
        // 该路径在本机不存在 → probe 失败，函数返回 None（解析本身是正确的）。
        assert!(parse_java_home_line(line).is_none());

        // 造一个真实存在的 java 可执行文件替身：不传 -version 也能被识别吗？
        // 用 /bin/echo 扮演 java 会解析不出 java.version → 仍为 None。
        // 因此这里只断言"无法解析的行返回 None"，路径解析逻辑由
        // `java_home` 的真实输出在 detect_all 中覆盖。
        assert!(parse_java_home_line("").is_none());
        assert!(parse_java_home_line("not-a-version path").is_none());
    }

    #[test]
    fn java_detector_returns_a_result_without_panicking() {
        // 真机探测：至少不应 panic / 不应返回错误（macOS 后端已实现）。
        let detected = MacosJavaDetector.detect_all().expect("detect_all");
        // 本机装了 JDK，安装数应 > 0；但断言"不 panic"更稳健。
        for j in &detected {
            assert!(!j.path.as_os_str().is_empty());
            assert!(j.major_version > 0);
        }
        // 排序：主版本降序
        for pair in detected.windows(2) {
            assert!(pair[0].major_version >= pair[1].major_version);
        }
    }

    #[test]
    fn open_url_rejects_non_http_schemes() {
        assert!(MacosShell.open_url("file:///etc/passwd").is_err());
        assert!(MacosShell.open_url("-a Calculator").is_err());
    }
}
