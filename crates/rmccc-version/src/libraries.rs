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

/// Native classifiers this library declares for the current platform,
/// ordered by the platform's priority list (arch-specific first).
///
/// Two manifest shapes exist:
/// - 1.19+ vanilla: the classifier is part of the Maven coordinate
///   (`org.lwjgl:lwjgl:3.3.1:natives-macos-arm64`);
/// - Mojang legacy / HMCL MultiMC: `natives: {osx: "natives-macos"}` plus
///   `downloads.classifiers`.
pub fn declared_native_classifiers(lib: &Library, platform: &PlatformDescriptor) -> Vec<String> {
    let mut declared: Vec<String> = Vec::new();
    if let Some(last) = lib.name.split(':').nth(3) {
        if last.starts_with("natives-") {
            declared.push(last.to_string());
        }
    }
    if let Some(natives) = &lib.natives {
        for value in natives.values() {
            for arch in ["64", "32", ""] {
                let candidate = value.replace("${arch}", arch);
                if !declared.contains(&candidate) {
                    declared.push(candidate);
                }
            }
        }
    }
    if let Some(classifiers) = lib.downloads.as_ref().and_then(|d| d.classifiers.as_ref()) {
        for key in classifiers.keys() {
            if !declared.contains(key) {
                declared.push(key.clone());
            }
        }
    }
    platform
        .native_classifier_candidates()
        .iter()
        .filter(|want| declared.iter().any(|have| have == *want))
        .map(|want| (*want).to_string())
        .collect()
}

/// Path of the native JAR for `classifier`.
pub fn native_jar_path(lib: &Library, classifier: &str, libs_dir: &Path) -> PathBuf {
    if let Some(path) = lib
        .downloads
        .as_ref()
        .and_then(|d| d.classifiers.as_ref())
        .and_then(|c| c.get(classifier))
        .and_then(|entry| entry.path.as_ref())
    {
        return libs_dir.join(path);
    }
    let parts: Vec<&str> = lib.name.split(':').collect();
    let coordinate = if parts.len() >= 4 && parts[3].starts_with("natives-") {
        // Replace the declared classifier with the requested one.
        format!("{}:{}:{}:{}", parts[0], parts[1], parts[2], classifier)
    } else {
        format!("{}:{}", lib.name, classifier)
    };
    make_library_path(&coordinate, libs_dir)
}

/// Native classifier to use for this library on `platform`.
///
/// Prefers an arch-matching JAR that already exists in the shared libraries
/// directory (MultiMC packs often only declare the generic `natives-macos`,
/// which ships x86_64 binaries — unusable from an arm64 JVM). Falls back to
/// the first declared classifier so a genuinely missing JAR still fails loudly.
pub fn native_classifier_for(
    lib: &Library,
    platform: &PlatformDescriptor,
    libs_dir: &Path,
) -> Option<String> {
    let declared = declared_native_classifiers(lib, platform);
    if declared.is_empty() {
        return None;
    }
    for want in platform.native_classifier_candidates() {
        if native_jar_path(lib, want, libs_dir).is_file() {
            return Some((*want).to_string());
        }
    }
    declared.into_iter().next()
}

/// Collect all library paths for the classpath.
///
/// Duplicate paths are dropped (first occurrence wins). HMCL/PCL 的 MultiMC
/// 形态版本 JSON 会把同一个库写成两条（纯 artifact 一条、带 `classifiers`
/// natives 一条）；两条都会解析到同一个 jar，重复进 `-cp` 会让 Forge 的
/// BootstrapLauncher 在 `UnionFileSystem` 里因 "Duplicate key" 直接崩溃。
pub fn collect_classpath(
    libraries: &[Library],
    libs_dir: &Path,
    platform: &PlatformDescriptor,
) -> Vec<PathBuf> {
    let mut seen = std::collections::HashSet::new();
    let mut classpath = Vec::new();
    for lib in libraries
        .iter()
        .filter(|lib| library_applies(lib, platform))
    {
        // If library has explicit download info, use artifact path if provided
        let path = if let Some(artifact_path) = lib
            .downloads
            .as_ref()
            .and_then(|d| d.artifact.as_ref())
            .and_then(|a| a.path.as_ref())
        {
            libs_dir.join(artifact_path)
        } else {
            // Fall back to Maven-style path from the library name.
            // Still include the path — Java will fail fast with a clear ClassNotFoundException
            // if the JAR is missing, which is better than a silent skip.
            make_library_path(&lib.name, libs_dir)
        };
        if seen.insert(path.clone()) {
            classpath.push(path);
        }

        // LWJGL 3 只从 classpath 里的 natives JAR 解压 .dylib/.so/.dll，
        // 不读 java.library.path（除非 java.library.path 下存在
        // `macos/<arch>/org/lwjgl/...` 这种 bundled 路径）。Mojang 旧版
        // 与 MultiMC 形态把 natives 写在 `natives` + `downloads.classifiers`
        // 里，仅靠解压目录会让 LWJGL 报 `liblwjgl.dylib` 找不到。
        if let Some(classifier) = native_classifier_for(lib, platform, libs_dir) {
            let jar = native_jar_path(lib, &classifier, libs_dir);
            if seen.insert(jar.clone()) {
                classpath.push(jar);
            }
        }
    }
    classpath
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{LibraryRule, OsRule};

    #[test]
    fn multimc_natives_jar_enters_the_classpath_with_the_arch_variant() {
        // MultiMC 形态：名字不带 classifier，natives map + downloads.classifiers。
        // 共享 libraries 里已有 arm64 变体时必须优先，否则 arm64 JVM 会拿到
        // x64 的 .dylib 并报 UnsatisfiedLinkError。
        let dir = std::env::temp_dir().join(format!(
            "rmccc-natives-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let jar_dir = dir.join("org/lwjgl/lwjgl/3.3.1");
        std::fs::create_dir_all(&jar_dir).unwrap();
        std::fs::write(jar_dir.join("lwjgl-3.3.1-natives-macos-arm64.jar"), b"jar").unwrap();
        std::fs::write(jar_dir.join("lwjgl-3.3.1-natives-macos.jar"), b"jar").unwrap();

        let mut classifiers = std::collections::HashMap::new();
        for classifier in ["natives-macos", "natives-macos-arm64"] {
            classifiers.insert(
                classifier.to_string(),
                crate::types::DownloadEntry {
                    url: String::new(),
                    sha1: String::new(),
                    size: 1,
                    path: Some(format!(
                        "org/lwjgl/lwjgl/3.3.1/lwjgl-3.3.1-{classifier}.jar"
                    )),
                },
            );
        }
        let lib = Library {
            name: "org.lwjgl:lwjgl:3.3.1".to_string(),
            natives: Some(std::collections::HashMap::from([(
                "osx".to_string(),
                "natives-macos".to_string(),
            )])),
            downloads: Some(crate::types::LibraryDownloads {
                artifact: None,
                classifiers: Some(classifiers),
            }),
            ..Default::default()
        };

        let classpath = collect_classpath(&[lib], &dir, &mac());
        assert!(
            classpath
                .iter()
                .any(|p| p.ends_with("lwjgl-3.3.1-natives-macos-arm64.jar")),
            "arm64 natives jar missing from classpath: {classpath:?}"
        );
        assert!(
            !classpath
                .iter()
                .any(|p| p.ends_with("lwjgl-3.3.1-natives-macos.jar")),
            "x64 natives jar should not be used on arm64: {classpath:?}"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn duplicate_library_entries_yield_one_classpath_entry() {
        // MultiMC 形态 JSON 会把同一库写成纯 artifact + nudged classifiers 两条。
        let lib = Library {
            name: "org.lwjgl:lwjgl-glfw:3.3.1".to_string(),
            ..Default::default()
        };
        let classpath = collect_classpath(&[lib.clone(), lib], Path::new("/libs"), &mac());
        assert_eq!(
            classpath.len(),
            1,
            "duplicate jars must be dropped: {classpath:?}"
        );
    }

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
