//! 进程控制（指南 §5.7.2「进程控制」）。
//!
//! Unix 用进程组 + `killpg`/`setpriority`；Windows 需 Job Object 或
//! `taskkill /T`，本机 deferred。领域层只依赖 [`PlatformProcessControl`]。

use rmccc_error::Result;

/// OS 进程优先级（HMCL `ProcessPriority`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ProcessPriority {
    Low,
    BelowNormal,
    #[default]
    Normal,
    AboveNormal,
    High,
}

impl ProcessPriority {
    /// Unix `nice -n` 值（`Normal` 返回 `None`，即不传）。
    pub fn nice_value(&self) -> Option<&'static str> {
        match self {
            ProcessPriority::Low => Some("5"),
            ProcessPriority::BelowNormal => Some("1"),
            ProcessPriority::Normal => None,
            ProcessPriority::AboveNormal => Some("-1"),
            ProcessPriority::High => Some("-5"),
        }
    }
}

/// 平台进程控制能力。
pub trait PlatformProcessControl: Send + Sync {
    /// 终止整棵进程树（Unix 对进程组发 `SIGKILL`；Windows `taskkill /T /F`）。
    fn terminate_tree(&self, pid: u32) -> Result<()>;

    /// 调整运行中进程的优先级。`Normal` 表示恢复默认。
    fn set_priority(&self, pid: u32, priority: ProcessPriority) -> Result<()>;

    /// 让子进程自成进程组的启动参数前缀（Unix `nice -n N`，其余空）。
    ///
    /// 返回的是**命令行前缀**而非系统调用，便于统一走 response file / 脚本生成。
    fn priority_command_prefix(&self, priority: ProcessPriority) -> Vec<String>;

    /// 配置子进程自成进程组（Unix `setpgid(0,0)`）。
    ///
    /// 只有这样才能在退出时一次性终止 Java 及其子进程，而不误伤启动器本身。
    /// Windows 后端为 no-op（Job Object 待原生 CI 实现）。
    fn configure_child_process_group(&self, cmd: &mut std::process::Command) -> Result<()>;
}

/// 后端标识（日志 / 诊断）。
pub fn process_backend_name() -> &'static str {
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
