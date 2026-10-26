//! Library dependency resolution.
//!
//! Filters library rules per-platform and resolves artifact URLs.

use rmccc_download::DownloadItem;
use rmccc_platform::PlatformDescriptor;
use std::path::{Path, PathBuf};

use super::types::{CompatibilityRule, Library};

/// Determine if a library should be included on the given platform.
///
/// 委派到统一权威实现 [`crate::compatibility::rules_allow`]（审计批次 3 收敛，
/// 语义定版见 ARCHITECTURE.md §3.3）：last-match-wins、无规则=适用、
/// **有规则但未命中=不适用**——声明了 `rules` 的库，规则不命中就表示
/// "这个库不属于当前平台"。
///
/// `os`/`arch` 判断全部经注入的 [`PlatformDescriptor`]（指南 §5.7.1 第 3 条）。
pub fn library_applies(lib: &Library, platform: &PlatformDescriptor) -> bool {
    let Some(rules) = &lib.rules else {
        // 无 rules = 无条件适用。
        return true;
    };

    let converted: Vec<super::types::CompatibilityRule> =
        rules.iter().map(CompatibilityRule::from).collect();
    crate::compatibility::rules_allow(&converted, &std::collections::HashMap::new(), platform)
}

/// Resolve library download items for the current platform.
///
/// This generates items for both the main artifact AND classifier JARs (natives).
/// Native libraries with OS-specific classifiers like `natives-macos` are only
/// downloaded for the current platform.
pub fn resolve_library_items(
    libraries: &[Library],
    libs_dir: &Path,
    platform: &PlatformDescriptor,
) -> Vec<DownloadItem> {
    let mut items = Vec::new();

    for lib in libraries
        .iter()
        .filter(|lib| library_applies(lib, platform))
    {
        // 1. Main artifact (the standard JAR)
        if let Some(item) = make_library_item(lib, libs_dir) {
            items.push(item);
        }

        // 2. Classifier JARs (natives) — only for the current platform
        if let Some(downloads) = &lib.downloads {
            if let Some(classifiers) = &downloads.classifiers {
                for (classifier, entry) in classifiers {
                    if is_current_platform_classifier(classifier, platform) {
                        let full_name = format!("{}:{}", lib.name, classifier);
                        let dest = entry
                            .path
                            .as_ref()
                            .map(|p| libs_dir.join(p))
                            .unwrap_or_else(|| make_library_path(&full_name, libs_dir));

                        items.push(DownloadItem {
                            url: entry.url.clone(),
                            dest,
                            expected_sha1: Some(entry.sha1.clone()),
                            size: Some(entry.size),
                            use_cache: true,
                            candidates: Vec::new(),
                        });
                    }
                }
            }
        }
    }

    items
}

/// Check if a classifier matches the given platform.
///
/// 事实来源是 [`PlatformDescriptor::native_classifier_candidates`]，本函数
/// 只做一次包含判断——不再自己拼 `natives-{os}-{arch}`（那曾导致 macOS ARM
/// 与 `natives-osx` 写法的分叉）。
pub(crate) fn is_current_platform_classifier(
    classifier: &str,
    platform: &PlatformDescriptor,
) -> bool {
    platform.accepts_native_classifier(classifier)
}

fn make_library_item(lib: &Library, libs_dir: &Path) -> Option<DownloadItem> {
    let path = make_library_path(&lib.name, libs_dir);

    // If library has explicit download info, use it
    if let Some(artifact) = lib.downloads.as_ref().and_then(|d| d.artifact.as_ref()) {
        let dest = artifact
            .path
            .as_ref()
            .map(|p| libs_dir.join(p))
            .unwrap_or_else(|| path.clone());

        return Some(DownloadItem {
            url: artifact.url.clone(),
            dest,
            expected_sha1: Some(artifact.sha1.clone()),
            size: Some(artifact.size),
            use_cache: true,
            candidates: Vec::new(),
        });
    }

    // Fall back: construct Maven URL from library name (for Fabric/Quilt/etc.)
    let urls = maven_urls_from_name(&lib.name);
    let primary = urls.first().cloned().unwrap_or_default();
    let candidates: Vec<String> = urls.into_iter().skip(1).collect();
    Some(DownloadItem {
        url: primary,
        dest: path,
        expected_sha1: None,
        size: None,
        use_cache: true,
        candidates,
    })
}

/// Construct Maven repository download URLs from a library name.
///
/// Returns a list of candidate URLs ordered by preference.
/// The first URL is the primary; subsequent URLs are fallbacks.
///
/// Known repositories:
/// - `net.fabricmc:*` → `https://maven.fabricmc.net/`
/// - `org.quiltmc:*` → `https://maven.quiltmc.org/repository/release/`
/// - Everything else → `https://repo1.maven.org/maven2/` (Maven Central)
///   with `https://libraries.minecraft.net/` as secondary fallback
pub(crate) fn maven_urls_from_name(name: &str) -> Vec<String> {
    let parts: Vec<&str> = name.split(':').collect();
    if parts.len() < 3 {
        return vec![String::new()];
    }
    let group = parts[0];
    let artifact = parts[1];
    let version = parts[2];
    let group_path = group.replace('.', "/");
    let jar_path = format!("{group_path}/{artifact}/{version}/{artifact}-{version}.jar");

    if group.starts_with("net.fabricmc") {
        vec![format!("https://maven.fabricmc.net/{jar_path}")]
    } else if group.starts_with("org.quiltmc") {
        vec![format!(
            "https://maven.quiltmc.org/repository/release/{jar_path}"
        )]
    } else {
        // Primary: Maven Central (universal), Fallback: Mojang's library repo
        vec![
            format!("https://repo1.maven.org/maven2/{jar_path}"),
            format!("https://libraries.minecraft.net/{jar_path}"),
        ]
    }
}

/// Build the standard library path from its Maven-style name.
///
/// Example:
///   `com.mojang:logging:1.0.0` → `libraries/com/mojang/logging/1.0.0/logging-1.0.0.jar`
///   `org.lwjgl:lwjgl:3.3.1:natives-macos` → `libraries/org/lwjgl/lwjgl/3.3.1/lwjgl-3.3.1-natives-macos.jar`
pub fn make_library_path(name: &str, libs_dir: &Path) -> PathBuf {
    let parts: Vec<&str> = name.split(':').collect();
    if parts.len() < 3 {
        return libs_dir.join(name.replace(':', "/"));
    }

    let group = parts[0].replace('.', "/");
    let artifact_id = parts[1];
    let version = parts[2];
    let classifier = parts.get(3);

    let jar_name = match classifier {
        Some(cls) => format!("{}-{}-{}.jar", artifact_id, version, cls),
        None => format!("{}-{}.jar", artifact_id, version),
    };

    libs_dir
        .join(&group)
        .join(artifact_id)
        .join(version)
        .join(&jar_name)
}

/// Collect all library paths for the classpath.
pub fn collect_classpath(
    libraries: &[Library],
    libs_dir: &Path,
    platform: &PlatformDescriptor,
) -> Vec<PathBuf> {
    libraries
        .iter()
        .filter(|lib| library_applies(lib, platform))
        .map(|lib| {
            // If library has explicit download info, use artifact path if provided
            if let Some(artifact_path) = lib
                .downloads
                .as_ref()
                .and_then(|d| d.artifact.as_ref())
                .and_then(|a| a.path.as_ref())
            {
                return libs_dir.join(artifact_path);
            }
            // Fall back to Maven-style path from the library name.
            // Still include the path — Java will fail fast with a clear ClassNotFoundException
            // if the JAR is missing, which is better than a silent skip.
            make_library_path(&lib.name, libs_dir)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{LibraryRule, OsRule};

    #[test]
    fn test_make_library_path() {
        let dir = Path::new("/game/libraries");
        let path = make_library_path("com.mojang:logging:1.0.0", dir);
        assert_eq!(path, dir.join("com/mojang/logging/1.0.0/logging-1.0.0.jar"));
    }

    #[test]
    fn test_make_library_path_with_classifier() {
        let dir = Path::new("/libs");
        let path = make_library_path("org.lwjgl:lwjgl:3.3.1:natives-macos", dir);
        assert_eq!(
            path,
            dir.join("org/lwjgl/lwjgl/3.3.1/lwjgl-3.3.1-natives-macos.jar")
        );
    }

    /// 固定 macOS/arm64 描述符：规则测试与宿主平台解耦（1.1b）。
    fn mac() -> PlatformDescriptor {
        PlatformDescriptor::macos_arm64()
    }

    fn win() -> PlatformDescriptor {
        PlatformDescriptor::new(
            rmccc_platform::PlatformOs::Windows,
            rmccc_platform::PlatformArch::X86_64,
        )
    }

    #[test]
    fn test_library_applies_no_rules() {
        let lib = Library {
            name: "test:lib:1.0".into(),
            ..Default::default()
        };
        assert!(library_applies(&lib, &mac()));
        assert!(library_applies(&lib, &win()));
    }

    #[test]
    fn test_library_applies_osx_only() {
        let rules = vec![LibraryRule {
            action: "allow".into(),
            os: Some(OsRule {
                name: Some("osx".into()),
                arch: None,
                version: None,
            }),
        }];
        let lib = Library {
            name: "test:natives:1.0".into(),
            rules: Some(rules),
            ..Default::default()
        };
        // osx-only 规则：macOS 命中，Windows 不命中（同一份规则，确定性断言）。
        assert!(library_applies(&lib, &mac()));
        assert!(!library_applies(&lib, &win()));
    }

    #[test]
    fn test_library_applies_allow_with_disallow() {
        let rules = vec![
            LibraryRule {
                action: "allow".into(),
                os: Some(OsRule {
                    name: Some("osx".into()),
                    arch: None,
                    version: None,
                }),
            },
            LibraryRule {
                action: "disallow".into(),
                os: None,
            },
        ];
        let lib = Library {
            name: "test:natives-all-disallowed:1.0".into(),
            rules: Some(rules),
            ..Default::default()
        };
        // 有 disallow 规则 → 任何平台都不适用。
        assert!(!library_applies(&lib, &mac()));
        assert!(!library_applies(&lib, &win()));
    }

    /// 回归测试（1.1b）：真实 1.20.4 manifest 的三类 library 形态。
    ///
    /// 修复前"无规则命中 → 默认 allow"，于是在 macOS 上会把 30 个
    /// Windows/Linux natives 也选中（实测 88/88 全选），既白下载又把别的平台的
    /// jar 塞进 classpath。修复后只选当前平台的 58 个。
    ///
    /// 这里用真实名字 + 真实规则结构做定点断言，不依赖网络。
    #[test]
    fn test_library_selection_matches_real_1_20_4_semantics() {
        let lib = |name: &str, rules: Option<Vec<LibraryRule>>| Library {
            name: name.to_string(),
            rules,
            ..Default::default()
        };
        let allow_os = |os: &str| LibraryRule {
            action: "allow".into(),
            os: Some(OsRule {
                name: Some(os.into()),
                arch: None,
                version: None,
            }),
        };

        // 1) 无 rules → 任何平台都适用（如 org.lwjgl:lwjgl:3.3.2 主库）。
        let plain = lib("org.lwjgl:lwjgl:3.3.2", None);
        // 2) 仅 macOS 的 natives（真实 manifest 里就是这一条规则）。
        let mac_native = lib(
            "org.lwjgl:lwjgl:3.3.2:natives-macos",
            Some(vec![allow_os("osx")]),
        );
        // 3) 仅 Windows 的 natives。
        let win_native = lib(
            "org.lwjgl:lwjgl:3.3.2:natives-windows",
            Some(vec![allow_os("windows")]),
        );
        // 4) 仅 Linux 的 netty epoll。
        let linux_epoll = lib(
            "io.netty:netty-transport-native-epoll:4.1.97.Final:linux-x86_64",
            Some(vec![allow_os("linux")]),
        );

        let mac = rmccc_platform::PlatformDescriptor::macos_arm64();
        assert!(library_applies(&plain, &mac), "无 rules 的库必须保留");
        assert!(library_applies(&mac_native, &mac), "macOS natives 必须保留");
        assert!(
            !library_applies(&win_native, &mac),
            "Windows natives 不得进入 macOS classpath（修复前会误选）"
        );
        assert!(
            !library_applies(&linux_epoll, &mac),
            "Linux natives 不得进入 macOS classpath（修复前会误选）"
        );

        // 反向：Windows 描述符下选择结果互补。
        let win = PlatformDescriptor::new(
            rmccc_platform::PlatformOs::Windows,
            rmccc_platform::PlatformArch::X86_64,
        );
        assert!(library_applies(&plain, &win));
        assert!(!library_applies(&mac_native, &win));
        assert!(library_applies(&win_native, &win));
        assert!(!library_applies(&linux_epoll, &win));
    }

    /// 回归测试（1.1b）：`rules` 存在但一条都不命中 → **不适用**（Mojang 默认 disallow）。
    #[test]
    fn test_rules_present_but_no_match_is_denied() {
        let lib = Library {
            name: "example:only-solaris:1.0".into(),
            rules: Some(vec![LibraryRule {
                action: "allow".into(),
                os: Some(OsRule {
                    name: Some("solaris".into()),
                    arch: None,
                    version: None,
                }),
            }]),
            ..Default::default()
        };
        assert!(!library_applies(&lib, &mac()));
        assert!(!library_applies(&lib, &win()));
    }

    #[test]
    fn test_is_current_platform_classifier() {
        // macOS（含历史别名 natives-osx 与架构专用变体）
        assert!(super::is_current_platform_classifier(
            "natives-macos",
            &mac()
        ));
        assert!(super::is_current_platform_classifier("natives-osx", &mac()));
        assert!(super::is_current_platform_classifier(
            "natives-macos-arm64",
            &mac()
        ));
        assert!(!super::is_current_platform_classifier(
            "natives-windows",
            &mac()
        ));
        assert!(!super::is_current_platform_classifier(
            "natives-linux",
            &mac()
        ));

        // Windows（含历史 -64 写法）
        assert!(super::is_current_platform_classifier(
            "natives-windows",
            &win()
        ));
        assert!(super::is_current_platform_classifier(
            "natives-windows-64",
            &win()
        ));
        assert!(!super::is_current_platform_classifier(
            "natives-macos",
            &win()
        ));

        // Linux（含 natives-unix 别名）
        let linux = PlatformDescriptor::new(
            rmccc_platform::PlatformOs::Linux,
            rmccc_platform::PlatformArch::X86_64,
        );
        assert!(super::is_current_platform_classifier(
            "natives-linux",
            &linux
        ));
        assert!(super::is_current_platform_classifier(
            "natives-unix",
            &linux
        ));
        assert!(!super::is_current_platform_classifier(
            "natives-macos",
            &linux
        ));

        // 未知平台：任何 native 都不命中。
        let unknown = PlatformDescriptor::new(
            rmccc_platform::PlatformOs::Unknown,
            rmccc_platform::PlatformArch::Unknown,
        );
        assert!(!super::is_current_platform_classifier(
            "natives-linux",
            &unknown
        ));
    }
}
