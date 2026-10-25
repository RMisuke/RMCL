//! Java 运行时探测（指南 §5.7.2「Java 探测」）。
//!
//! 各 OS 的标准安装位置 / 注册表 / `java_home` / `update-alternatives` 差异全部
//! 落在平台后端（`macos.rs` / `windows.rs` / `linux.rs`），领域层只依赖
//! [`JavaDetector`] 与 [`JavaInstallation`]（指南 §5.7.1）。

use rmccc_error::Result;
use std::path::{Path, PathBuf};
use std::process::Command;

/// 一个已探测到的 Java 运行时。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JavaInstallation {
    /// `.../bin/java`（Windows 为 `java.exe`）的绝对路径。
    pub path: PathBuf,
    /// 主版本号（`1.8.0_392` → `8`，`17.0.9` → `17`）。
    pub major_version: u32,
    /// `java.vendor`（可能为空）。
    pub vendor: String,
    /// `sun.arch.data.model == 64`。
    pub is_64bit: bool,
}

impl JavaInstallation {
    /// 规范化路径 + 元数据（探测后端用）。
    pub fn new(path: PathBuf, major_version: u32, vendor: String, is_64bit: bool) -> Self {
        Self {
            path,
            major_version,
            vendor,
            is_64bit,
        }
    }
}

/// 平台 Java 探测能力。
pub trait JavaDetector: Send + Sync {
    /// 该平台上的全部 Java 安装，**按主版本降序**、按路径去重。
    ///
    /// 平台后端未实现时返回 [`rmccc_error::RmcccError::UnsupportedPlatform`]——
    /// 空列表会被调用方误读成"这台机器没装 Java"。
    fn detect_all(&self) -> Result<Vec<JavaInstallation>>;

    /// 为需要的版本挑选最合适的 Java。
    ///
    /// 选择规则见 [`select_for`]：精确匹配 > 最近的更高版本 > 版本不符错误。
    fn find(&self, required_major: u32) -> Result<JavaInstallation> {
        let installations = self.detect_all()?;
        select_for(&installations, required_major)
    }
}

/// 纯函数的选择规则（便于单测，不依赖真实 Java）。
///
/// 1. 精确匹配优先；
/// 2. 否则取**最近的更高版本**（不是最高版本：Java 25 的严格 module-path
///    会让只需要 17 的游戏起不来）；
/// 3. 都没有则返回 `JavaVersionMismatch`。
pub fn select_for(
    installations: &[JavaInstallation],
    required_major: u32,
) -> Result<JavaInstallation> {
    if installations.is_empty() {
        return Err(rmccc_error::RmcccError::JavaNotFound);
    }

    if let Some(exact) = installations
        .iter()
        .find(|j| j.major_version == required_major)
    {
        return Ok(exact.clone());
    }

    if let Some(closest) = installations
        .iter()
        .filter(|j| j.major_version >= required_major)
        .min_by_key(|j| j.major_version)
    {
        tracing::warn!(
            required = required_major,
            found = closest.major_version,
            "Using compatible Java version (not exact match, closest selected)"
        );
        return Ok(closest.clone());
    }

    Err(rmccc_error::RmcccError::JavaVersionMismatch {
        found: installations[0].major_version,
        required: required_major,
    })
}

/// `java.version` → 主版本号（2026-10-25 修复：legacy 命名方案此前没实现）。
///
/// - 现代（Java 9+）：`17.0.9` → `17`，`25.0.2` → `25`；
/// - legacy（≤ Java 8）：`1.8.0_392` / `1.8` → `8`——此前取首段得 `1`，
///   Java 8 安装被识别成 "Java 1"，`required=8` 时永远匹配不上
///   （日志症状：`required = 8, found = 17` 的回退）；
/// - 加后缀的 `25.0.2-internal` / `1.8.0_392-b10` 同样兼容。
fn legacy_aware_major(version: &str) -> Option<u32> {
    let mut segments = version.split('.');
    let first: u32 = segments.next()?.parse().ok()?;
    if first == 1 {
        // legacy `1.MAJOR...`：主版本在第二段
        return segments.next()?.parse().ok();
    }
    Some(first)
}

/// 读一个 java 可执行文件的版本 / vendor / 位数。
///
/// 所有平台同一条命令（`-XshowSettings:properties -version`），因此这段逻辑
/// 只有一份，放在适配层内部。
pub fn probe_java_binary(path: &Path) -> Option<JavaInstallation> {
    if !path.exists() {
        return None;
    }

    let output = Command::new(path)
        .arg("-XshowSettings:properties")
        .arg("-version")
        .output()
        .ok()?;

    // `-version` 写 stderr；properties 也主要写 stderr。
    let combined = format!(
        "{}\n{}",
        String::from_utf8_lossy(&output.stderr),
        String::from_utf8_lossy(&output.stdout)
    );

    let major = combined
        .lines()
        .find(|l| l.contains("java.version"))
        .and_then(|l| l.split('=').nth(1))
        .and_then(|v| legacy_aware_major(v.trim()));

    let Some(major) = major else {
        tracing::warn!(path = %path.display(), "Failed to parse Java version");
        return None;
    };

    let vendor = combined
        .lines()
        .find(|l| l.contains("java.vendor"))
        .and_then(|l| l.split('=').nth(1))
        .map(|v| v.trim().to_string())
        .unwrap_or_default();

    let is_64bit = combined
        .lines()
        .find(|l| l.contains("sun.arch.data.model"))
        .and_then(|l| l.split('=').nth(1))
        .map(|v| v.trim() == "64")
        .unwrap_or(true);

    Some(JavaInstallation::new(
        path.to_path_buf(),
        major,
        vendor,
        is_64bit,
    ))
}

/// 排序 + 去重（所有后端共用的收尾）。
pub fn normalize(mut found: Vec<JavaInstallation>) -> Vec<JavaInstallation> {
    found.sort_by_key(|a| std::cmp::Reverse(a.major_version));
    found.dedup_by(|a, b| a.path == b.path);
    found
}

/// `JAVA_HOME/bin/java[.exe]` 若存在则探测（各平台共有的一步）。
pub fn probe_java_home(exe_suffix: &str) -> Option<JavaInstallation> {
    let home = std::env::var("JAVA_HOME").ok()?;
    let bin = PathBuf::from(home)
        .join("bin")
        .join(format!("java{exe_suffix}"));
    probe_java_binary(&bin)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rmccc_error::RmcccError;

    fn java(major: u32) -> JavaInstallation {
        JavaInstallation::new(
            PathBuf::from(format!("/jvm/{major}/bin/java")),
            major,
            "Test".into(),
            true,
        )
    }

    #[test]
    fn empty_installation_list_reports_not_found() {
        assert!(matches!(select_for(&[], 17), Err(RmcccError::JavaNotFound)));
    }

    #[test]
    fn exact_match_wins() {
        let list = vec![java(21), java(17), java(8)];
        let selected = select_for(&list, 17).expect("exact");
        assert_eq!(selected.major_version, 17);
    }

    #[test]
    fn closest_higher_version_is_preferred_over_highest() {
        let list = vec![java(25), java(21), java(17)];
        // 需要 17 → 精确匹配 17
        assert_eq!(select_for(&list, 17).unwrap().major_version, 17);
        // 需要 18 → 最近的更高版本是 21（不是 25）
        assert_eq!(select_for(&list, 18).unwrap().major_version, 21);
        // 需要 8 → 最近的更高版本是 17
        assert_eq!(select_for(&list, 8).unwrap().major_version, 17);
    }

    #[test]
    fn too_old_javas_report_version_mismatch() {
        let list = vec![java(8)];
        match select_for(&list, 17) {
            Err(RmcccError::JavaVersionMismatch { found, required }) => {
                assert_eq!((found, required), (8, 17));
            }
            other => panic!("expected JavaVersionMismatch, got {other:?}"),
        }
    }

    #[test]
    fn normalize_sorts_desc_and_dedups_by_path() {
        let dup = java(21);
        let list = vec![java(8), dup.clone(), java(25), dup];
        let normalized = normalize(list);
        assert_eq!(
            normalized
                .iter()
                .map(|j| j.major_version)
                .collect::<Vec<_>>(),
            vec![25, 21, 8]
        );
    }

    #[test]
    fn probe_missing_binary_returns_none() {
        assert!(probe_java_binary(Path::new("/definitely/not/java")).is_none());
    }

    #[test]
    fn legacy_version_scheme_parses_second_segment() {
        // 2026-10-25 修复：`1.8.0_392` 此前取首段得 1，Java 8 被识别成 "Java 1"。
        assert_eq!(legacy_aware_major("1.8.0_392"), Some(8));
        assert_eq!(legacy_aware_major("1.8"), Some(8));
        assert_eq!(legacy_aware_major("1.8.0_392-b10"), Some(8));
        assert_eq!(legacy_aware_major("17.0.9"), Some(17));
        assert_eq!(legacy_aware_major("25.0.2"), Some(25));
        assert_eq!(legacy_aware_major("21"), Some(21));
        assert_eq!(legacy_aware_major("garbage"), None);
        assert_eq!(legacy_aware_major(""), None);
    }
}
