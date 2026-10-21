//! 当前平台描述符（指南 §5.7.1 第 3 条例外：**数据筛选**）。
//!
//! 领域 crate 判断"某个 library / asset / 启动参数是否属于当前平台"时，
//! 必须读这里注入的描述对象，而不是在领域函数里裸写 `cfg!(target_os = ...)`。
//! 唯一允许出现 `#[cfg]` 的地方是 [`PlatformDescriptor::current`] 与各后端文件。
//!
//! 命名对齐 Minecraft 自身的规则值：
//! - `rules[].os.name`：`windows` / `osx`（macOS，注意不是 `macos`）/ `linux`
//! - `rules[].os.arch`：`x86` / `x86_64` / `arm64` / `aarch64`

use serde::{Deserialize, Serialize};

/// Operating system family.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlatformOs {
    Macos,
    Windows,
    Linux,
    /// 既不是 macOS 也不是 Windows/Linux（FreeBSD 等）。
    Unknown,
}

impl PlatformOs {
    /// Minecraft `rules[].os.name` 值（macOS 用 `osx`）。
    pub fn rule_name(self) -> &'static str {
        match self {
            PlatformOs::Macos => "osx",
            PlatformOs::Windows => "windows",
            PlatformOs::Linux => "linux",
            PlatformOs::Unknown => "unknown",
        }
    }

    /// 人类可读名字（日志 / UI 用）。
    pub fn display_name(self) -> &'static str {
        match self {
            PlatformOs::Macos => "macOS",
            PlatformOs::Windows => "Windows",
            PlatformOs::Linux => "Linux",
            PlatformOs::Unknown => "Unknown",
        }
    }

    pub fn is_unix(self) -> bool {
        matches!(self, PlatformOs::Macos | PlatformOs::Linux)
    }
}

/// CPU architecture family.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlatformArch {
    X86,
    X86_64,
    Aarch64,
    Unknown,
}

impl PlatformArch {
    /// Minecraft `rules[].os.arch` 值。
    ///
    /// 注意 Mojang 的取值并不统一：`x86`、`x86_64`、`arm64`、`aarch64` 都出现过，
    /// 因此匹配时见 [`PlatformDescriptor::matches_rule_arch`]。
    pub fn rule_name(self) -> &'static str {
        match self {
            PlatformArch::X86 => "x86",
            PlatformArch::X86_64 => "x86_64",
            PlatformArch::Aarch64 => "aarch64",
            PlatformArch::Unknown => "unknown",
        }
    }
}

/// 平台描述符：注入到领域层的只读数据对象。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlatformDescriptor {
    pub os: PlatformOs,
    pub arch: PlatformArch,
}

impl PlatformDescriptor {
    /// 构造任意描述符（测试 / 交叉校验用）。
    pub fn new(os: PlatformOs, arch: PlatformArch) -> Self {
        Self { os, arch }
    }

    /// 编译目标平台。**这是数据层唯一允许的 `#[cfg]` 选择器。**
    pub fn current() -> Self {
        Self {
            os: current_os(),
            arch: current_arch(),
        }
    }

    /// macOS Apple Silicon 描述符（本机开发目标）。
    pub fn macos_arm64() -> Self {
        Self::new(PlatformOs::Macos, PlatformArch::Aarch64)
    }

    pub fn is_windows(&self) -> bool {
        self.os == PlatformOs::Windows
    }

    pub fn is_windows_arm64(&self) -> bool {
        self.is_windows() && self.arch == PlatformArch::Aarch64
    }

    /// classpath / 路径列表分隔符（`;` on Windows, `:` elsewhere）。
    pub fn path_list_separator(&self) -> char {
        if self.is_windows() {
            ';'
        } else {
            ':'
        }
    }

    /// classpath 分隔符的字符串形式。
    pub fn classpath_separator(&self) -> &'static str {
        if self.is_windows() {
            ";"
        } else {
            ":"
        }
    }

    /// 可执行文件后缀（Windows `.exe`，其余空串）。
    pub fn exe_suffix(&self) -> &'static str {
        if self.is_windows() {
            ".exe"
        } else {
            ""
        }
    }

    /// Minecraft 使用的架构名（`arm64` / `x86_64` / `x86`）。
    ///
    /// 注意与 [`PlatformArch::rule_name`] 的区别：后者是 Mojang 规则的**匹配值**
    /// （`aarch64`），而这里是 native classifier 拼接用的**实际命名**
    /// （macOS 上 Minecraft 用 `arm64`，见 `natives-macos-arm64`）。
    pub fn minecraft_arch(&self) -> &'static str {
        match self.arch {
            PlatformArch::X86 => "x86",
            PlatformArch::X86_64 => "x86_64",
            PlatformArch::Aarch64 => "arm64",
            PlatformArch::Unknown => "unknown",
        }
    }

    /// 该平台需要额外追加的 JVM 参数（数据，不是执行）。
    ///
    /// macOS 需要 `-Xdock:name` 才能让 Dock 显示"Minecraft <实例名>"而不是 `java`；
    /// 其他平台为空。放在适配层是为了让领域代码不写 `cfg(target_os = "macos")`。
    ///
    /// 值里的 `{instance}` 由调用方替换为实例 / 版本 id。
    pub fn extra_jvm_args(&self) -> &'static [(&'static str, &'static str)] {
        match self.os {
            PlatformOs::Macos => &[("-Xdock:name=", "Minecraft {instance}")],
            _ => &[],
        }
    }

    /// 是否需要"HOME 下非 ASCII 路径"的 natives 软链兜底（HMCL #1141）。
    ///
    /// MC < 1.19 在 Linux/macOS 上把非 ASCII 的 natives 路径直接交给 LWJGL 会
    /// 找不到库，HMCL 的做法是指向 `/tmp/hmcl-natives-<uuid>` 的软链。
    /// Windows 无此问题（由 JVM 自己做短路径映射）。
    pub fn needs_non_ascii_natives_workaround(&self) -> bool {
        self.os.is_unix()
    }

    /// 插件 Manifest 使用的平台 ID（ADR-004）。
    ///
    /// 与 Mojang 的 `rules[].os.name`（`osx`/`windows`/`linux`）是**两套命名**：
    /// 插件生态惯用 `darwin-*`，不能混用。
    pub fn plugin_platform_id(&self) -> &'static str {
        match (self.os, self.arch) {
            (PlatformOs::Macos, PlatformArch::Aarch64) => "darwin-aarch64",
            (PlatformOs::Macos, PlatformArch::X86_64) => "darwin-x86_64",
            (PlatformOs::Windows, PlatformArch::X86_64) => "windows-x86_64",
            (PlatformOs::Windows, PlatformArch::Aarch64) => "windows-aarch64",
            (PlatformOs::Linux, PlatformArch::X86_64) => "linux-x86_64",
            (PlatformOs::Linux, PlatformArch::Aarch64) => "linux-aarch64",
            _ => "unknown",
        }
    }

    /// 默认进程输出编码（Windows 用系统代码页兜底，Unix 统一 UTF-8）。
    pub fn default_encoding(&self) -> &'static str {
        if self.is_windows() {
            // Windows 控制台代码页在中文系统上通常是 GBK；检测失败时的兜底。
            "GBK"
        } else {
            "UTF-8"
        }
    }

    /// 该平台可接受的 Mojang native classifier 列表，**按优先级降序**。
    ///
    /// 这是"哪些 classifier 属于我"的**唯一事实来源**：领域层判断 native 库
    /// 是否适用时只看这里，不再自己拼 `natives-{os}-{arch}` 字符串。
    ///
    /// 含历史别名（Mojang 各时期写法不同）：
    /// - macOS：`natives-macos-<arch>` / `natives-macos` / `natives-osx`
    /// - Windows：`natives-windows-<arch>` / `natives-windows-64|32` / `natives-windows`
    /// - Linux：`natives-linux-<arch>` / `natives-linux-64|32` / `natives-linux` / `natives-unix`
    ///
    /// 未知平台返回**空列表**（= 任何 native 都不命中），避免误把别的平台的
    /// `.dll`/`.so` 解压到 natives 目录。
    pub fn native_classifier_candidates(&self) -> &'static [&'static str] {
        match (self.os, self.arch) {
            (PlatformOs::Macos, PlatformArch::Aarch64) => {
                &["natives-macos-arm64", "natives-macos", "natives-osx"]
            }
            (PlatformOs::Macos, PlatformArch::X86_64) => {
                &["natives-macos-x86_64", "natives-macos", "natives-osx"]
            }
            (PlatformOs::Macos, _) => &["natives-macos", "natives-osx"],
            (PlatformOs::Windows, PlatformArch::X86_64) => &[
                "natives-windows-x86_64",
                "natives-windows-64",
                "natives-windows",
            ],
            (PlatformOs::Windows, PlatformArch::X86) => &[
                "natives-windows-x86",
                "natives-windows-32",
                "natives-windows",
            ],
            (PlatformOs::Windows, _) => &["natives-windows"],
            (PlatformOs::Linux, PlatformArch::X86_64) => &[
                "natives-linux-x86_64",
                "natives-linux-64",
                "natives-linux",
                "natives-unix",
            ],
            (PlatformOs::Linux, PlatformArch::X86) => &[
                "natives-linux-x86",
                "natives-linux-32",
                "natives-linux",
                "natives-unix",
            ],
            (PlatformOs::Linux, _) => &["natives-linux", "natives-unix"],
            (PlatformOs::Unknown, _) => &[],
        }
    }

    /// 该 classifier 是否属于当前平台的 native 库。
    pub fn accepts_native_classifier(&self, classifier: &str) -> bool {
        self.native_classifier_candidates().contains(&classifier)
    }

    /// 判断 Mojang `rules[].os.name` 是否匹配当前平台。
    ///
    /// 接受 `osx` 与 `macos` 两种写法（社区 manifest 两种都有）。
    pub fn matches_rule_os(&self, name: &str) -> bool {
        match name {
            "osx" | "macos" => self.os == PlatformOs::Macos,
            "windows" => self.os == PlatformOs::Windows,
            "linux" => self.os == PlatformOs::Linux,
            _ => false,
        }
    }

    /// 判断 `rules[].os.arch` 是否匹配当前平台（缺省 = 任意架构）。
    pub fn matches_rule_arch(&self, arch: Option<&str>) -> bool {
        let Some(arch) = arch else {
            return true;
        };
        match arch {
            "x86" | "i386" | "i686" => self.arch == PlatformArch::X86,
            "x86_64" | "amd64" => self.arch == PlatformArch::X86_64,
            // Mojang 混用 `arm64` 与 `aarch64`（macOS 用 arm64，Linux 用 aarch64）。
            "arm64" | "aarch64" => self.arch == PlatformArch::Aarch64,
            _ => false,
        }
    }
}

impl Default for PlatformDescriptor {
    fn default() -> Self {
        Self::current()
    }
}

#[cfg(target_os = "macos")]
fn current_os() -> PlatformOs {
    PlatformOs::Macos
}

#[cfg(target_os = "windows")]
fn current_os() -> PlatformOs {
    PlatformOs::Windows
}

#[cfg(target_os = "linux")]
fn current_os() -> PlatformOs {
    PlatformOs::Linux
}

#[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
fn current_os() -> PlatformOs {
    PlatformOs::Unknown
}

#[cfg(target_arch = "x86_64")]
fn current_arch() -> PlatformArch {
    PlatformArch::X86_64
}

#[cfg(target_arch = "aarch64")]
fn current_arch() -> PlatformArch {
    PlatformArch::Aarch64
}

#[cfg(target_arch = "x86")]
fn current_arch() -> PlatformArch {
    PlatformArch::X86
}

#[cfg(not(any(target_arch = "x86_64", target_arch = "aarch64", target_arch = "x86")))]
fn current_arch() -> PlatformArch {
    PlatformArch::Unknown
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn current_descriptor_reports_a_known_os_and_arch() {
        let d = PlatformDescriptor::current();
        assert_ne!(d.os, PlatformOs::Unknown, "本机 OS 应被识别");
        assert_ne!(d.arch, PlatformArch::Unknown, "本机架构应被识别");
    }

    #[test]
    fn macos_arm64_native_candidates_prefer_arch_specific() {
        let d = PlatformDescriptor::macos_arm64();
        assert_eq!(
            d.native_classifier_candidates(),
            &["natives-macos-arm64", "natives-macos", "natives-osx"]
        );
        assert!(d.accepts_native_classifier("natives-macos-arm64"));
        assert!(d.accepts_native_classifier("natives-osx"), "历史别名");
        assert!(!d.accepts_native_classifier("natives-windows"));
    }

    #[test]
    fn windows_uses_semicolon_and_exe_suffix() {
        let d = PlatformDescriptor::new(PlatformOs::Windows, PlatformArch::X86_64);
        assert_eq!(d.classpath_separator(), ";");
        assert_eq!(d.path_list_separator(), ';');
        assert_eq!(d.exe_suffix(), ".exe");
        assert!(d.is_windows());
        assert!(!d.os.is_unix());
    }

    #[test]
    fn unix_uses_colon_and_no_exe_suffix() {
        for os in [PlatformOs::Macos, PlatformOs::Linux] {
            let d = PlatformDescriptor::new(os, PlatformArch::X86_64);
            assert_eq!(d.classpath_separator(), ":");
            assert_eq!(d.exe_suffix(), "");
            assert!(d.os.is_unix());
        }
    }

    #[test]
    fn rule_os_matching_accepts_osx_and_macos() {
        let mac = PlatformDescriptor::macos_arm64();
        assert!(mac.matches_rule_os("osx"));
        assert!(mac.matches_rule_os("macos"));
        assert!(!mac.matches_rule_os("windows"));
        assert!(!mac.matches_rule_os("linux"));

        let win = PlatformDescriptor::new(PlatformOs::Windows, PlatformArch::X86_64);
        assert!(win.matches_rule_os("windows"));
        assert!(!win.matches_rule_os("osx"));

        let linux = PlatformDescriptor::new(PlatformOs::Linux, PlatformArch::X86_64);
        assert!(linux.matches_rule_os("linux"));
        assert!(!linux.matches_rule_os("windows"));
    }

    #[test]
    fn rule_arch_matching_treats_arm64_and_aarch64_alike() {
        let arm = PlatformDescriptor::macos_arm64();
        assert!(arm.matches_rule_arch(None));
        assert!(arm.matches_rule_arch(Some("arm64")));
        assert!(arm.matches_rule_arch(Some("aarch64")));
        assert!(!arm.matches_rule_arch(Some("x86_64")));

        let intel = PlatformDescriptor::new(PlatformOs::Macos, PlatformArch::X86_64);
        assert!(intel.matches_rule_arch(Some("x86_64")));
        assert!(intel.matches_rule_arch(Some("amd64")));
        assert!(!intel.matches_rule_arch(Some("arm64")));

        let x86 = PlatformDescriptor::new(PlatformOs::Windows, PlatformArch::X86);
        assert!(x86.matches_rule_arch(Some("x86")));
        assert!(x86.matches_rule_arch(Some("i686")));
        assert!(!x86.matches_rule_arch(Some("x86_64")));
    }

    #[test]
    fn macos_adds_dock_name_jvm_arg() {
        let mac = PlatformDescriptor::macos_arm64();
        assert_eq!(
            mac.extra_jvm_args(),
            &[("-Xdock:name=", "Minecraft {instance}")],
            "`{{instance}}` 由调用方替换"
        );
        assert!(mac.needs_non_ascii_natives_workaround());

        let win = PlatformDescriptor::new(PlatformOs::Windows, PlatformArch::X86_64);
        assert!(win.extra_jvm_args().is_empty());
        assert!(!win.needs_non_ascii_natives_workaround());
    }

    #[test]
    fn unknown_platform_accepts_no_native_classifier() {
        let d = PlatformDescriptor::new(PlatformOs::Unknown, PlatformArch::Unknown);
        assert!(d.native_classifier_candidates().is_empty());
        assert!(!d.accepts_native_classifier("natives-linux"));
        assert!(!d.accepts_native_classifier("natives-macos"));
    }

    #[test]
    fn minecraft_arch_uses_minecraft_naming() {
        assert_eq!(
            PlatformDescriptor::macos_arm64().minecraft_arch(),
            "arm64",
            "Minecraft 用 arm64 而非 Rust 的 aarch64"
        );
        let intel = PlatformDescriptor::new(PlatformOs::Macos, PlatformArch::X86_64);
        assert_eq!(intel.minecraft_arch(), "x86_64");
        let x86 = PlatformDescriptor::new(PlatformOs::Windows, PlatformArch::X86);
        assert_eq!(x86.minecraft_arch(), "x86");
    }

    #[test]
    fn windows_arm64_is_detected() {
        let d = PlatformDescriptor::new(PlatformOs::Windows, PlatformArch::Aarch64);
        assert!(d.is_windows_arm64());
        assert_eq!(
            d.native_classifier_candidates(),
            &["natives-windows"],
            "Windows arm64 暂无专用 classifier，回退通用"
        );
        assert!(!d.accepts_native_classifier("natives-windows-64"));
    }
}
