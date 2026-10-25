//! 测试/替换用 mock 后端（指南 §5.7.1 第 5 条：便于测试替身）。
//!
//! 记录调用而不触碰真实文件系统 / 进程，用于领域层单测。

use crate::fs::PlatformFs;
use crate::java::{JavaDetector, JavaInstallation};
use crate::process::{PlatformProcessControl, ProcessPriority};
use crate::shell::{validate_openable_url, PlatformShell};
use rmccc_error::Result;
use std::path::Path;
use std::sync::{Arc, Mutex};

/// 一次后端调用的记录。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MockCall {
    RestrictOwnerOnly(String),
    RestrictOwnerOnlyDir(String),
    MakeExecutable(String),
    Symlink(String, String),
    RemoveSymlink(String),
    AtomicReplace(String, String),
    CreateOwnerOnly(String),
    TerminateTree(u32),
    SetPriority(u32, ProcessPriority),
    ConfigureProcessGroup,
    OpenUrl(String),
    OpenPath(String),
}

/// 可配置的 Java 探测替身：直接返回预置列表，不碰真实 JVM。
#[derive(Debug, Clone, Default)]
pub struct MockJavaDetector {
    installations: Vec<JavaInstallation>,
    fail: bool,
}

impl MockJavaDetector {
    pub fn with_installations(installations: Vec<JavaInstallation>) -> Self {
        Self {
            installations,
            fail: false,
        }
    }

    /// 让探测返回 `UnsupportedPlatform`（验证调用方的错误处理）。
    pub fn failing() -> Self {
        Self {
            installations: Vec::new(),
            fail: true,
        }
    }
}

impl JavaDetector for MockJavaDetector {
    fn detect_all(&self) -> Result<Vec<JavaInstallation>> {
        if self.fail {
            return Err(rmccc_error::RmcccError::UnsupportedPlatform(
                "mock java detector".into(),
            ));
        }
        Ok(self.installations.clone())
    }
}

/// 记录调用的内存后端。
#[derive(Debug, Clone, Default)]
pub struct MockPlatform {
    calls: Arc<Mutex<Vec<MockCall>>>,
    /// `terminate_tree` 等方法的返回值（默认成功）。
    fail_next: Arc<Mutex<bool>>,
}

impl MockPlatform {
    pub fn new() -> Self {
        Self::default()
    }

    /// 记录下来的全部调用（按发生顺序）。
    pub fn calls(&self) -> Vec<MockCall> {
        self.calls.lock().map(|c| c.clone()).unwrap_or_default()
    }

    /// 断言某次调用发生过。
    pub fn did_call(&self, expected: &MockCall) -> bool {
        self.calls().iter().any(|c| c == expected)
    }

    /// 让下一次调用失败（验证调用方的错误处理）。
    pub fn fail_next_call(&self) {
        if let Ok(mut f) = self.fail_next.lock() {
            *f = true;
        }
    }

    fn record(&self, call: MockCall) -> Result<()> {
        let should_fail = match self.fail_next.lock() {
            Ok(mut f) => std::mem::replace(&mut *f, false),
            Err(_) => false,
        };
        if let Ok(mut calls) = self.calls.lock() {
            calls.push(call);
        }
        if should_fail {
            return Err(rmccc_error::RmcccError::Other("mock failure".into()));
        }
        Ok(())
    }
}

impl PlatformFs for MockPlatform {
    fn restrict_owner_only(&self, path: &Path) -> Result<()> {
        self.record(MockCall::RestrictOwnerOnly(path.display().to_string()))
    }

    fn restrict_owner_only_dir(&self, path: &Path) -> Result<()> {
        self.record(MockCall::RestrictOwnerOnlyDir(path.display().to_string()))
    }

    fn make_executable(&self, path: &Path) -> Result<()> {
        self.record(MockCall::MakeExecutable(path.display().to_string()))
    }

    fn symlink(&self, target: &Path, link: &Path) -> Result<()> {
        self.record(MockCall::Symlink(
            target.display().to_string(),
            link.display().to_string(),
        ))
    }

    fn remove_symlink(&self, link: &Path) -> Result<()> {
        self.record(MockCall::RemoveSymlink(link.display().to_string()))
    }

    fn atomic_replace(&self, from: &Path, to: &Path) -> Result<()> {
        self.record(MockCall::AtomicReplace(
            from.display().to_string(),
            to.display().to_string(),
        ))?;
        // Mock 需要真的完成替换，否则调用方的断言会失败。
        std::fs::rename(from, to)?;
        Ok(())
    }

    fn create_owner_only(&self, path: &Path, contents: &[u8]) -> Result<()> {
        self.record(MockCall::CreateOwnerOnly(path.display().to_string()))?;
        std::fs::write(path, contents)?;
        Ok(())
    }
}

impl PlatformProcessControl for MockPlatform {
    fn terminate_tree(&self, pid: u32) -> Result<()> {
        self.record(MockCall::TerminateTree(pid))
    }

    fn set_priority(&self, pid: u32, priority: ProcessPriority) -> Result<()> {
        self.record(MockCall::SetPriority(pid, priority))
    }

    fn priority_command_prefix(&self, priority: ProcessPriority) -> Vec<String> {
        match priority.nice_value() {
            Some(nice) => vec!["nice".to_string(), "-n".to_string(), nice.to_string()],
            None => Vec::new(),
        }
    }

    fn configure_child_process_group(&self, _cmd: &mut std::process::Command) -> Result<()> {
        self.record(MockCall::ConfigureProcessGroup)
    }
}

impl PlatformShell for MockPlatform {
    fn open_url(&self, url: &str) -> Result<()> {
        validate_openable_url(url)?;
        self.record(MockCall::OpenUrl(url.to_string()))
    }

    fn open_path(&self, path: &Path) -> Result<()> {
        self.record(MockCall::OpenPath(path.display().to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn records_calls_in_order() {
        let mock = MockPlatform::new();
        mock.restrict_owner_only(Path::new("/tmp/key"))
            .expect("restrict");
        mock.terminate_tree(42).expect("terminate");

        assert_eq!(
            mock.calls(),
            vec![
                MockCall::RestrictOwnerOnly("/tmp/key".into()),
                MockCall::TerminateTree(42),
            ]
        );
    }

    #[test]
    fn fail_next_call_only_fails_once() {
        let mock = MockPlatform::new();
        mock.fail_next_call();
        assert!(mock.terminate_tree(7).is_err());
        assert!(mock.terminate_tree(7).is_ok(), "只失败一次");
        assert_eq!(mock.calls().len(), 2, "失败的调用也被记录");
    }

    #[test]
    fn open_url_still_validates() {
        let mock = MockPlatform::new();
        assert!(mock.open_url("javascript:alert(1)").is_err());
        assert!(!mock.did_call(&MockCall::OpenUrl("javascript:alert(1)".into())));
    }
}
