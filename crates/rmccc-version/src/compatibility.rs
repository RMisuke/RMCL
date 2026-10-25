//! Unified compatibility rule evaluation.
//!
//! Mirrors HMCL's `CompatibilityRule` logic: a rule has an `action`
//! (`allow` / `disallow`) plus optional `os` (name/arch/version) and `features`
//! conditions. This module centralizes the OS/arch matching so that manifest
//! compatibility rules, library rules, and argument rules share one source of
//! truth (previously duplicated in `libraries.rs` and `arguments.rs`).

use rmccc_platform::PlatformDescriptor;
use std::collections::HashMap;

use super::types::{CompatibilityRule, OsCondition};

/// Whether an OS condition matches the given platform.
///
/// Supports Mojang's `^regex` OS name pattern (e.g. `^mac os x`).
/// `os.name` / `os.arch` 全部经 [`PlatformDescriptor`]，无裸 `cfg!`。
pub fn os_condition_matches(os: &OsCondition, platform: &PlatformDescriptor) -> bool {
    // OS name
    if let Some(name) = &os.name {
        let matches = if let Some(pattern) = name.strip_prefix('^') {
            platform.os.rule_name().contains(pattern)
        } else {
            // Mojang sometimes uses "macos" and sometimes "osx"; both map to us.
            platform.matches_rule_os(name)
        };
        if !matches {
            return false;
        }
    }

    // Architecture
    if !platform.matches_rule_arch(os.arch.as_deref()) {
        return false;
    }

    // OS version regex (rarely used; there is no portable OS-version API here,
    // so this stays a best-effort check against the platform name).
    if let Some(version) = &os.version {
        let os_release = platform.os.rule_name();
        if !os_release.contains(version.trim_start_matches('^')) {
            return false;
        }
    }

    true
}

/// Whether a single rule applies to the current environment.
///
/// `features` carries the current feature flags (e.g. `has_custom_resolution`);
/// a rule with feature requirements matches only if all required features
/// equal the supplied values (missing feature → `false`).
pub fn rule_applies(
    rule: &CompatibilityRule,
    features: &HashMap<String, bool>,
    platform: &PlatformDescriptor,
) -> bool {
    if let Some(os) = &rule.os {
        if !os_condition_matches(os, platform) {
            return false;
        }
    }

    if let Some(feat) = &rule.features {
        for (key, required) in feat {
            if features.get(key) != Some(required) {
                return false;
            }
        }
    }

    true
}

/// Evaluate a rule list with Mojang last-match-wins semantics（统一权威实现，
/// 审计批次 3：`libraries::library_applies` 与 `arguments::rules_match` 都是
/// 本函数的薄封装，不得再各写一套匹配逻辑）。
///
/// - No rules → allowed（无条件适用）。
/// - 有规则时：**最后一条命中的规则**决定结果（`allow` → 适用，`disallow` → 不适用）。
/// - 有规则但一条都没命中 → **不适用**（HMCL `CompatibilityRule
///   .appliesToCurrentEnvironment` 在无命中规则时返回 false；例：规则
///   `[allow osx]` 的参数在 Windows 上不出现，`[allow windows]` 的库在 macOS
///   上不下载）。
pub fn rules_allow(
    rules: &[CompatibilityRule],
    features: &HashMap<String, bool>,
    platform: &PlatformDescriptor,
) -> bool {
    if rules.is_empty() {
        return true;
    }

    let mut allowed = false;
    for rule in rules {
        if rule_applies(rule, features, platform) {
            allowed = rule.action != "disallow";
        }
    }
    allowed
}

/// Whether a **manifest**'s compatibility rules include it on the current
/// platform (HMCL `CompatibilityRule.appliesToCurrentEnvironment`).
///
/// Returns `true` when there are no rules, or when **any** rule matches the
/// current environment — a manifest with rules that never match (e.g. a
/// Windows-only snapshot on macOS) is hidden from the repository index.
pub fn applies_to_environment(rules: &[CompatibilityRule], platform: &PlatformDescriptor) -> bool {
    if rules.is_empty() {
        return true;
    }
    rules
        .iter()
        .any(|r| rule_applies(r, &HashMap::new(), platform))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{CompatibilityRule, OsCondition};

    /// 本机描述符（宿主平台真值）。
    fn host() -> PlatformDescriptor {
        PlatformDescriptor::current()
    }

    fn mac() -> PlatformDescriptor {
        PlatformDescriptor::macos_arm64()
    }

    fn win() -> PlatformDescriptor {
        PlatformDescriptor::new(
            rmccc_platform::PlatformOs::Windows,
            rmccc_platform::PlatformArch::X86_64,
        )
    }

    fn rule(action: &str, os_name: Option<&str>) -> CompatibilityRule {
        CompatibilityRule {
            action: action.to_string(),
            os: os_name.map(|name| OsCondition {
                name: Some(name.to_string()),
                arch: None,
                version: None,
            }),
            features: None,
        }
    }

    #[test]
    fn test_no_rules_allows() {
        assert!(rules_allow(&[], &HashMap::new(), &host()));
    }

    #[test]
    fn test_disallow_current_os() {
        // 用本机描述符构造"禁止本平台"的规则 → 必须被拒。
        let host_name = host().os.rule_name();
        let rules = vec![rule("disallow", Some(host_name))];
        assert!(!rules_allow(&rules, &HashMap::new(), &host()));
    }

    #[test]
    fn test_allow_other_os_no_match_excludes() {
        // 规则只允许 "solaris"，本机永不命中 → 不适用（no-match = exclude，
        // HMCL appliesToCurrentEnvironment 同款；此前误为 default-allow）。
        let rules = vec![rule("allow", Some("solaris"))];
        assert!(!rules_allow(&rules, &HashMap::new(), &host()));
    }

    #[test]
    fn test_disallow_then_unconditional_allow_last_wins() {
        // 审计跨 crate #2 的判定案例：[{disallow osx},{allow}]（第二条无 os
        // 条件、必命中）在 macOS 上最后命中者是 allow → 适用。
        // 旧 arguments.rs 的"任意命中 + disallow 一票否决"会错判为不适用。
        let rules = vec![rule("disallow", Some("osx")), rule("allow", None)];
        assert!(rules_allow(&rules, &HashMap::new(), &mac()));
        assert!(rules_allow(&rules, &HashMap::new(), &win()));
    }

    #[test]
    fn test_last_match_wins() {
        let current = host().os.rule_name();
        // allow on current, then disallow on current → last wins → disallowed
        let rules = vec![
            rule("allow", Some(current)),
            rule("disallow", Some(current)),
        ];
        assert!(!rules_allow(&rules, &HashMap::new(), &host()));

        // disallow then allow → allowed
        let rules = vec![
            rule("disallow", Some(current)),
            rule("allow", Some(current)),
        ];
        assert!(rules_allow(&rules, &HashMap::new(), &host()));
    }

    #[test]
    fn test_feature_rule() {
        let rules: Vec<CompatibilityRule> = vec![CompatibilityRule {
            action: "allow".into(),
            os: None,
            features: Some(HashMap::from([("has_custom_resolution".into(), true)])),
        }];
        assert!(rules_allow(
            &rules,
            &HashMap::from([("has_custom_resolution".into(), true)]),
            &host()
        ));
        // feature 未开启 → 规则不命中 → 不适用（no-match = exclude）。
        assert!(!rules_allow(&rules, &HashMap::new(), &host()));
    }

    #[test]
    fn test_applies_to_environment_any_rule() {
        // A manifest with a Windows-only rule must NOT apply on other platforms.
        let rules: Vec<CompatibilityRule> = vec![CompatibilityRule {
            action: "allow".into(),
            os: Some(OsCondition {
                name: Some("windows".into()),
                arch: None,
                version: None,
            }),
            features: None,
        }];
        // 同一份规则、不同描述符 → 结果确定性可断言（不再依赖宿主平台）。
        assert!(applies_to_environment(&rules, &win()));
        assert!(!applies_to_environment(&rules, &mac()));

        assert!(applies_to_environment(&[], &mac()));
    }

    #[test]
    fn test_macos_alias() {
        // Mojang sometimes writes "macos" instead of "osx".
        let rules = vec![rule("allow", Some("macos"))];
        // macOS 描述符：命中 allow。
        assert!(rules_allow(&rules, &HashMap::new(), &mac()));
        // Windows 描述符：无规则命中 → 不适用（no-match = exclude）。
        assert!(!rules_allow(&rules, &HashMap::new(), &win()));
        // disallow：macOS 上被拒；Windows 上同样不命中 → 不适用。
        let deny = vec![rule("disallow", Some("macos"))];
        assert!(!rules_allow(&deny, &HashMap::new(), &mac()));
        assert!(!rules_allow(&deny, &HashMap::new(), &win()));
    }
}
