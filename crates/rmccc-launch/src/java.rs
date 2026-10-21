//! Java 运行时探测 — 已迁入平台适配层（指南 §5.7.2）。
//!
//! 各 OS 的安装位置差异（macOS `/usr/libexec/java_home`、Windows 注册表 +
//! `Program Files`、Linux `/usr/lib/jvm` + `update-alternatives`）现在全部由
//! [`rmccc_platform::JavaDetector`] 的后端承担，本模块只保留类型与便捷函数的
//! **re-export**，保证 `rmccc_launch::JavaInstallation` 等历史路径继续可用。

pub use rmccc_platform::{JavaDetector, JavaInstallation};

/// 探测本机所有 Java 安装（按主版本降序）。
///
/// 便捷封装：内部用 [`rmccc_platform::Platform::current`] 选择后端。**新代码
/// 应经 `core.platform.java` 注入调用**，以便测试替换与跨平台注入。
pub fn detect_java_installations() -> Vec<JavaInstallation> {
    rmccc_platform::Platform::current()
        .java
        .detect_all()
        .unwrap_or_else(|e| {
            tracing::warn!(error = %e, "Java detection unavailable on this platform");
            Vec::new()
        })
}

/// 为需要的版本挑选 Java（精确匹配 > 最近的更高版本）。
///
/// 同上：新代码应经 `core.platform.java.find(required)`。
pub fn find_java(required_version: u32) -> rmccc_error::Result<JavaInstallation> {
    rmccc_platform::Platform::current()
        .java
        .find(required_version)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rmccc_platform::{Platform, PlatformArch, PlatformOs};

    #[test]
    fn selection_is_injectable_and_deterministic() {
        let (platform, _backend) = Platform::mock();
        let platform = platform.with_mock_java(
            rmccc_platform::mock::MockJavaDetector::with_installations(vec![
                JavaInstallation::new("/jvm/17/bin/java".into(), 17, "Test".into(), true),
                JavaInstallation::new("/jvm/21/bin/java".into(), 21, "Test".into(), true),
            ]),
        );

        // 精确匹配
        assert_eq!(platform.java.find(17).expect("17").major_version, 17);
        // 需要 18 → 最近的更高版本 21
        assert_eq!(platform.java.find(18).expect("21").major_version, 21);
        // 需要 25 → 全部太旧
        assert!(platform.java.find(25).is_err());
    }

    #[test]
    fn unsupported_platform_reports_error_not_empty_list() {
        let (platform, _backend) = Platform::mock();
        let platform = platform.with_mock_java(rmccc_platform::mock::MockJavaDetector::failing());
        assert!(platform.java.detect_all().is_err());
    }

    #[test]
    fn descriptor_still_drives_exe_suffix() {
        // Java 可执行文件名随平台变化（`java` / `java.exe`），由描述符给出。
        assert_eq!(
            rmccc_platform::PlatformDescriptor::macos_arm64().exe_suffix(),
            ""
        );
        assert_eq!(
            rmccc_platform::PlatformDescriptor::new(PlatformOs::Windows, PlatformArch::X86_64)
                .exe_suffix(),
            ".exe"
        );
    }
}
