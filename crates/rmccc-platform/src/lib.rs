//! 平台适配层（指南 §5.7）——**领域逻辑与平台执行之间的唯一接缝**。
//!
//! # 为什么有这个 crate
//!
//! 归属规则（指南 §5.7.1，硬约束）：
//!
//! 1. 启动器核心（Core Service、领域 crate、`pipeline`）**只定义 trait 与 DTO**，
//!    不承担平台特定执行。
//! 2. 领域逻辑**不得内联 `#[cfg(target_os = ...)]` 执行分支**。
//! 3. 唯一例外：基于 Mojang 清单 `rules[].os` 的**数据筛选**——但必须经注入的
//!    [`PlatformDescriptor`] 判断，而不是裸 `cfg!`。
//! 4. trait 必须返回归一化结果或 [`rmccc_error::RmcccError::UnsupportedPlatform`]，
//!    不得静默失败或 panic。
//! 5. 后端选择在组装层（`CoreContext`）完成一次注入，下游全链路依赖 trait。
//!
//! # 当前状态（macOS-only 开发）
//!
//! | 后端 | 状态 |
//! |------|------|
//! | macOS（aarch64） | ✅ 实现并本机实测（含原生分辨率 HEVC 壁纸） |
//! | Windows | ⚠️ 原生 HEVC WinRT 后端已落地，其余能力 `UnsupportedPlatform`（deferred，待原生 CI） |
//! | Linux | ⛔ `UnsupportedPlatform`（deferred，待原生 CI） |
//!
//! 各能力域的迁移清单与逐项销账见 `docs/roadmap/PLATFORM_ADAPTATION.md`。
//!
//! # 用法
//!
//! ```no_run
//! use rmccc_platform::{Platform, PlatformProcessControl as _};
//!
//! let platform = Platform::current();
//! // 领域层只看到 trait：
//! let _ = platform.process.terminate_tree(1234);
//! // 数据筛选走描述符，不裸写 cfg!：
//! if platform.descriptor.matches_rule_os("osx") {
//!     // macOS 专属规则命中
//! }
//! ```

pub mod descriptor;
pub mod fs;
pub mod java;
pub mod mock;
pub mod process;
pub mod shell;
pub mod system;
pub mod video;

#[cfg(target_os = "macos")]
mod macos;
#[cfg(not(target_os = "macos"))]
mod unsupported;

pub use descriptor::{PlatformArch, PlatformDescriptor, PlatformOs};
pub use fs::PlatformFs;
pub use java::{JavaDetector, JavaInstallation};
#[cfg(target_os = "macos")]
pub use macos::MacosFs;
pub use process::{PlatformProcessControl, ProcessPriority};
pub use shell::PlatformShell;
pub use system::{PhysicalMemory, SystemMemory};
pub use video::{
    probe_mp4, CachedVideoEngine, PreparedVideo, UnsupportedVideoTranscoder, VideoCodec,
    VideoEngine, VideoMetadata, VideoPrepareMode, VideoPrepareRequest,
};

use mock::MockJavaDetector;
use std::sync::Arc;

/// 组装好的平台适配器：描述符 + 各能力域后端。
///
/// 领域层持有 `Arc<Platform>`，只经 trait 调用，不感知具体后端。
#[derive(Clone)]
pub struct Platform {
    /// 当前平台描述符（数据筛选专用）。
    pub descriptor: PlatformDescriptor,
    /// 文件权限 / 符号链接。
    pub fs: Arc<dyn PlatformFs>,
    /// 进程终止、优先级、进程组。
    pub process: Arc<dyn PlatformProcessControl>,
    /// 打开 URL / 路径。
    pub shell: Arc<dyn PlatformShell>,
    /// Java 运行时探测（各 OS 的安装位置 / 注册表 / `java_home` / alternatives）。
    pub java: Arc<dyn JavaDetector>,
    /// 物理内存状态（设置页内存占用条；不参与启动决策）。
    pub memory: Arc<dyn SystemMemory>,
    /// Native-resolution wallpaper preparation with a platform transcoder.
    pub video: Arc<dyn VideoEngine>,
}

impl Platform {
    /// 按编译目标选择后端。**整个 crate 的 `#[cfg]` 选择器只有这一处。**
    pub fn current() -> Self {
        #[cfg(target_os = "macos")]
        {
            Self {
                descriptor: PlatformDescriptor::current(),
                fs: Arc::new(macos::MacosFs),
                process: Arc::new(macos::MacosProcess),
                shell: Arc::new(macos::MacosShell),
                java: Arc::new(macos::MacosJavaDetector),
                memory: Arc::new(macos::MacosSystemMemory),
                video: Arc::new(CachedVideoEngine::new(video::MacosVideoTranscoder)),
            }
        }
        #[cfg(target_os = "windows")]
        {
            Self {
                descriptor: PlatformDescriptor::current(),
                fs: Arc::new(unsupported::UnsupportedFs),
                process: Arc::new(unsupported::UnsupportedProcess),
                shell: Arc::new(unsupported::UnsupportedShell),
                java: Arc::new(unsupported::UnsupportedJavaDetector),
                memory: Arc::new(unsupported::UnsupportedSystemMemory),
                video: Arc::new(CachedVideoEngine::new(video::WindowsVideoTranscoder)),
            }
        }
        #[cfg(not(any(target_os = "macos", target_os = "windows")))]
        {
            Self {
                descriptor: PlatformDescriptor::current(),
                fs: Arc::new(unsupported::UnsupportedFs),
                process: Arc::new(unsupported::UnsupportedProcess),
                shell: Arc::new(unsupported::UnsupportedShell),
                java: Arc::new(unsupported::UnsupportedJavaDetector),
                memory: Arc::new(unsupported::UnsupportedSystemMemory),
                video: Arc::new(CachedVideoEngine::new(UnsupportedVideoTranscoder)),
            }
        }
    }

    /// 用 mock Java 探测替身替换（领域层单测：断言选版逻辑，不需要真 JVM）。
    pub fn with_mock_java(mut self, detector: mock::MockJavaDetector) -> Self {
        self.java = Arc::new(detector);
        self
    }

    /// 用 mock 后端构造（领域层单测 / 替换实现）。
    ///
    /// 描述符默认取本机，可用 [`Platform::with_descriptor`] 换成任意平台。
    pub fn mock() -> (Self, Arc<mock::MockPlatform>) {
        let backend = Arc::new(mock::MockPlatform::new());
        (
            Self {
                descriptor: PlatformDescriptor::current(),
                fs: backend.clone(),
                process: backend.clone(),
                shell: backend.clone(),
                java: Arc::new(MockJavaDetector::default()),
                memory: Arc::new(mock::MockSystemMemory),
                video: Arc::new(CachedVideoEngine::new(UnsupportedVideoTranscoder)),
            },
            backend,
        )
    }

    /// 换一个描述符（模拟 Windows/Linux 数据筛选行为）。
    pub fn with_descriptor(mut self, descriptor: PlatformDescriptor) -> Self {
        self.descriptor = descriptor;
        self
    }

    /// 后端标识，用于日志（例：`macos` / `windows`）。
    pub fn backend_name(&self) -> &'static str {
        std::env::consts::OS
    }

    /// 描述符的 classpath 分隔符（领域层判断 `;` vs `:` 的唯一入口）。
    pub fn classpath_separator(&self) -> &'static str {
        self.descriptor.classpath_separator()
    }
}

impl std::fmt::Debug for Platform {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Platform")
            .field("descriptor", &self.descriptor)
            .field("backend", &self.backend_name())
            .finish_non_exhaustive()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn current_platform_exposes_all_backends() {
        let p = Platform::current();
        assert!(!p.backend_name().is_empty());
        // 描述符与后端都就绪（不 panic、不为空）。
        assert_ne!(p.descriptor.os, PlatformOs::Unknown);
        assert_eq!(
            p.classpath_separator(),
            p.descriptor.classpath_separator(),
            "分隔符只有一个取值点"
        );
    }

    #[test]
    fn mock_platform_records_through_all_traits() {
        let (p, backend) = Platform::mock();
        p.fs.restrict_owner_only(std::path::Path::new("/tmp/x"))
            .expect("fs");
        p.process.terminate_tree(9).expect("process");
        p.shell.open_url("https://example.com").expect("shell");

        let calls = backend.calls();
        assert_eq!(calls.len(), 3);
        assert!(backend.did_call(&mock::MockCall::TerminateTree(9)));
    }

    #[test]
    fn with_descriptor_can_simulate_windows() {
        let (p, _backend) = Platform::mock();
        let win = p.with_descriptor(PlatformDescriptor::new(
            PlatformOs::Windows,
            PlatformArch::X86_64,
        ));
        assert_eq!(win.classpath_separator(), ";");
        assert!(win.descriptor.matches_rule_os("windows"));
        assert!(!win.descriptor.matches_rule_os("osx"));
    }
}
