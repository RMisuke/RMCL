//! Unified compatibility rule evaluation.
//!
//! Mirrors HMCL's `CompatibilityRule` logic: a rule has an `action`
//! (`allow` / `disallow`) plus optional `os` (name/arch/version) and `features`
//! conditions. This module centralizes the OS/arch matching so that manifest
//! compatibility rules, library rules, and argument rules share one source of
//! truth (previously duplicated in `libraries.rs` and `arguments.rs`).

use std::collections::HashMap;

use super::types::{CompatibilityRule, OsCondition};

/// Current OS name as recognized by Minecraft version JSON rules.
pub fn current_os_name() -> &'static str {
    #[cfg(target_os = "windows")]
    {
        "windows"
    }
    #[cfg(target_os = "macos")]
    {
        "osx"
    }
    #[cfg(target_os = "linux")]
    {
        "linux"
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
    {
        "unknown"
    }
}

/// Current CPU architecture (Minecraft naming: `x86`, `x86_64`, `arm64`).
pub fn current_arch() -> &'static str {
    #[cfg(target_arch = "x86_64")]
    {
        "x86_64"
    }
    #[cfg(target_arch = "aarch64")]
    {
        "arm64"
    }
    #[cfg(target_arch = "x86")]
    {
        "x86"
    }
    #[cfg(not(any(target_arch = "x86_64", target_arch = "aarch64", target_arch = "x86")))]
    {
        "unknown"
    }
}

/// Whether an OS condition matches the current environment.
///
/// Supports Mojang's `^regex` OS name pattern (e.g. `^mac os x`).
pub fn os_condition_matches(os: &OsCondition) -> bool {
    // OS name
    if let Some(name) = &os.name {
        let current = current_os_name();
        let matches = if name.starts_with('^') {
            current.contains(name.trim_start_matches('^'))
        } else {
            // Mojang sometimes uses "macos" and sometimes "osx"; both map to us.
            let normalized = match name.as_str() {
                "macos" => "osx",
                other => other,
            };
            current == normalized
        };
        if !matches {
            return false;
        }
    }

    // Architecture
    if let Some(arch) = &os.arch {
        if arch != current_arch() {
            return false;
        }
    }

    // OS version regex (rarely used; treated as a contains check)
    if let Some(version) = &os.version {
        let os_release = std::env::consts::OS;
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
pub fn rule_applies(rule: &CompatibilityRule, features: &HashMap<String, bool>) -> bool {
    if let Some(os) = &rule.os {
        if !os_condition_matches(os) {
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

/// Evaluate a rule list with Mojang last-match-wins semantics.
///
/// - No rules → allowed (default).
/// - The **last** matching rule's action decides (`allow` → true, `disallow` → false).
/// - No rule matches → allowed (default), matching Mojang's library rules.
pub fn rules_allow(rules: &[CompatibilityRule], features: &HashMap<String, bool>) -> bool {
    if rules.is_empty() {
        return true;
    }

    let mut allowed = false;
    let mut any_matched = false;
    for rule in rules {
        if rule_applies(rule, features) {
            allowed = rule.action != "disallow";
            any_matched = true;
        }
    }

    if any_matched {
        allowed
    } else {
        true
    }
}

/// Whether a **manifest**'s compatibility rules include it on the current
/// platform (HMCL `CompatibilityRule.appliesToCurrentEnvironment`).
///
/// Returns `true` when there are no rules, or when **any** rule matches the
/// current environment — a manifest with rules that never match (e.g. a
/// Windows-only snapshot on macOS) is hidden from the repository index.
pub fn applies_to_environment(rules: &[CompatibilityRule]) -> bool {
    if rules.is_empty() {
        return true;
    }
    rules.iter().any(|r| rule_applies(r, &HashMap::new()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{CompatibilityRule, OsCondition};

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
        assert!(rules_allow(&[], &HashMap::new()));
    }

    #[test]
    fn test_disallow_current_os() {
        let current = current_os_name();
        let rules = vec![rule("disallow", Some(current))];
        assert!(!rules_allow(&rules, &HashMap::new()));
    }

    #[test]
    fn test_allow_other_os_no_match_default_allows() {
        // Rule allows only "solaris" which never matches — no matching rule
        // means default allow.
        let rules = vec![rule("allow", Some("solaris"))];
        assert!(rules_allow(&rules, &HashMap::new()));
    }

    #[test]
    fn test_last_match_wins() {
        let current = current_os_name();
        // allow on current, then disallow on current → last wins → disallowed
        let rules = vec![rule("allow", Some(current)), rule("disallow", Some(current))];
        assert!(!rules_allow(&rules, &HashMap::new()));

        // disallow then allow → allowed
        let rules = vec![rule("disallow", Some(current)), rule("allow", Some(current))];
        assert!(rules_allow(&rules, &HashMap::new()));
    }

    #[test]
    fn test_feature_rule() {
        let rules: Vec<CompatibilityRule> = vec![CompatibilityRule {
            action: "allow".into(),
            os: None,
            features: Some(HashMap::from([("has_custom_resolution".into(), true)])),
        }];
        assert!(rules_allow(&rules, &HashMap::from([("has_custom_resolution".into(), true)])));
        // last-match-wins: no matching rule → default allow
        assert!(rules_allow(&rules, &HashMap::new()));
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
        #[cfg(target_os = "windows")]
        assert!(applies_to_environment(&rules));
        #[cfg(not(target_os = "windows"))]
        assert!(!applies_to_environment(&rules));

        assert!(applies_to_environment(&[]));
    }

    #[test]
    fn test_macos_alias() {
        // Mojang sometimes writes "macos" instead of "osx"
        let rules = vec![rule("allow", Some("macos"))];
        #[cfg(target_os = "macos")]
        assert!(rules_allow(&rules, &HashMap::new()));
        #[cfg(not(target_os = "macos"))]
        assert!(rules_allow(&rules, &HashMap::new())); // no match → default allow
    }
}
