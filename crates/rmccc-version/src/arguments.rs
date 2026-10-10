//! Version argument parsing and template variable resolution.
//!
//! Minecraft version JSON (1.13+) uses `${variable}` template placeholders in
//! both JVM and game arguments. This module handles:
//! - Template variable substitution
//! - Rule-based argument filtering (OS, features)
//! - Legacy minecraftArguments string parsing
//!
//! Following HMCL's [`Arguments`] and [`Argument`] patterns.

use crate::types::{ArgumentRule, ArgumentValue, VersionArgument};
use std::collections::HashMap;

/// Standard template variable names used by Minecraft.
pub mod vars {
    pub const AUTH_PLAYER_NAME: &str = "auth_player_name";
    pub const AUTH_SESSION: &str = "auth_session";
    pub const AUTH_ACCESS_TOKEN: &str = "auth_access_token";
    pub const AUTH_UUID: &str = "auth_uuid";
    pub const VERSION_NAME: &str = "version_name";
    pub const PROFILE_NAME: &str = "profile_name";
    pub const VERSION_TYPE: &str = "version_type";
    pub const GAME_DIRECTORY: &str = "game_directory";
    pub const USER_TYPE: &str = "user_type";
    pub const ASSETS_ROOT: &str = "assets_root";
    pub const ASSETS_INDEX_NAME: &str = "assets_index_name";
    pub const USER_PROPERTIES: &str = "user_properties";
    pub const RESOLUTION_WIDTH: &str = "resolution_width";
    pub const RESOLUTION_HEIGHT: &str = "resolution_height";
    pub const NATIVES_DIRECTORY: &str = "natives_directory";
    pub const CLASSPATH: &str = "classpath";
    pub const LAUNCHER_NAME: &str = "launcher_name";
    pub const LAUNCHER_VERSION: &str = "launcher_version";
    pub const LIBRARY_DIRECTORY: &str = "library_directory";
    pub const CLASSPATH_SEPARATOR: &str = "classpath_separator";
}

/// Build the standard template variable map for launching Minecraft.
///
/// This map provides all `${variable}` values expected by the version JSON
/// arguments, compatible with both Mojang's official launcher and HMCL.
///
/// # Arguments
/// * `username` - Player display name
/// * `uuid` - Player UUID (compact, no dashes)
/// * `access_token` - Authentication token
/// * `user_type` - "msa" or "legacy"
/// * `version_name` - Display version name (e.g. "1.20.4")
/// * `version_type` - Release type label (e.g. "release", "snapshot")
/// * `game_dir` - Game run directory (.minecraft)
/// * `assets_dir` - Assets root directory
/// * `asset_index_id` - Asset index ID (e.g. "17")
/// * `natives_dir` - Native libraries directory
/// * `classpath` - Full classpath as string
/// * `launcher_name` - Launcher brand name
/// * `launcher_version` - Launcher version string
/// * `width` - Optional window width
/// * `height` - Optional window height
#[allow(clippy::too_many_arguments)]
pub fn build_template_vars(
    username: &str,
    uuid: &str,
    access_token: &str,
    user_type: &str,
    version_name: &str,
    version_type: &str,
    game_dir: &str,
    assets_dir: &str,
    asset_index_id: &str,
    natives_dir: &str,
    classpath: &str,
    launcher_name: &str,
    launcher_version: &str,
    width: Option<u32>,
    height: Option<u32>,
) -> HashMap<String, String> {
    let mut vars = HashMap::new();
    vars.insert(vars::AUTH_PLAYER_NAME.to_string(), username.to_string());
    vars.insert(vars::AUTH_SESSION.to_string(), access_token.to_string());
    vars.insert(
        vars::AUTH_ACCESS_TOKEN.to_string(),
        access_token.to_string(),
    );
    vars.insert(vars::AUTH_UUID.to_string(), uuid.to_string());
    vars.insert(vars::VERSION_NAME.to_string(), version_name.to_string());
    vars.insert(vars::PROFILE_NAME.to_string(), version_name.to_string());
    vars.insert(vars::VERSION_TYPE.to_string(), version_type.to_string());
    vars.insert(vars::GAME_DIRECTORY.to_string(), game_dir.to_string());
    vars.insert(vars::USER_TYPE.to_string(), user_type.to_string());
    vars.insert(vars::ASSETS_ROOT.to_string(), assets_dir.to_string());
    vars.insert(
        vars::ASSETS_INDEX_NAME.to_string(),
        asset_index_id.to_string(),
    );
    vars.insert(vars::USER_PROPERTIES.to_string(), "{}".to_string());
    vars.insert(vars::NATIVES_DIRECTORY.to_string(), natives_dir.to_string());
    vars.insert(vars::CLASSPATH.to_string(), classpath.to_string());
    vars.insert(vars::LAUNCHER_NAME.to_string(), launcher_name.to_string());
    vars.insert(
        vars::LAUNCHER_VERSION.to_string(),
        launcher_version.to_string(),
    );
    // Java classpath separator: ':' on Unix, ';' on Windows
    #[cfg(target_os = "windows")]
    let cp_separator = ";";
    #[cfg(not(target_os = "windows"))]
    let cp_separator = ":";
    vars.insert(
        vars::CLASSPATH_SEPARATOR.to_string(),
        cp_separator.to_string(),
    );

    // Library directory (NeoForge uses ${library_directory} in boot layer module path)
    // Always {game_dir}/libraries — Maven paths are too deep (4+ levels) for
    // reliable parent() derivation from classpath entries.
    let library_dir = format!("{}/libraries", game_dir.trim_end_matches('/'));
    vars.insert(vars::LIBRARY_DIRECTORY.to_string(), library_dir);

    // Resolution settings (default 0 = not set)
    vars.insert(
        vars::RESOLUTION_WIDTH.to_string(),
        width.map(|w| w.to_string()).unwrap_or_default(),
    );
    vars.insert(
        vars::RESOLUTION_HEIGHT.to_string(),
        height.map(|h| h.to_string()).unwrap_or_default(),
    );

    vars
}

/// Replace `${variable}` templates in a string using the provided variable map.
///
/// Unrecognized variables are left as-is (Minecraft ignores unknown placeholders).
///
/// # Example
/// ```
/// use std::collections::HashMap;
/// use rmccc_version::replace_templates;
/// let mut vars = HashMap::new();
/// vars.insert("auth_player_name".to_string(), "Steve".to_string());
/// let result = replace_templates("--username ${auth_player_name}", &vars);
/// assert_eq!(result, "--username Steve");
/// ```
pub fn replace_templates(arg: &str, vars: &HashMap<String, String>) -> String {
    let mut result = String::with_capacity(arg.len());
    let bytes = arg.as_bytes();
    let mut i = 0;

    while i < arg.len() {
        if bytes[i] == b'$' && i + 2 < arg.len() && bytes[i + 1] == b'{' {
            // Find closing brace
            let start = i + 2;
            if let Some(end) = arg[start..].find('}') {
                let key = &arg[start..start + end];
                if let Some(value) = vars.get(key) {
                    result.push_str(value);
                } else {
                    // Keep unrecognized variable as-is
                    result.push_str(&arg[i..start + end + 1]);
                }
                i = start + end + 1;
                continue;
            }
        }
        result.push(arg.as_bytes()[i] as char);
        i += 1;
    }

    result
}

/// Resolve a [`VersionArguments`] into flat argument lists, applying rules and
/// template substitution.
///
/// # Arguments
/// * `args` - The `arguments.jvm` or `arguments.game` section
/// * `vars` - Template variable map
/// * `features` - Feature flags (e.g. `has_custom_resolution`)
///
/// # Returns
/// A vector of resolved argument strings.
pub fn resolve_arguments(
    args: &[VersionArgument],
    vars: &HashMap<String, String>,
    features: &HashMap<String, bool>,
) -> Vec<String> {
    let mut result = Vec::new();

    for arg in args {
        match arg {
            VersionArgument::Simple(s) => {
                result.push(replace_templates(s, vars));
            }
            VersionArgument::Ruled { rules, value } => {
                if rules_match(rules, features) {
                    match value {
                        ArgumentValue::Single(s) => {
                            result.push(replace_templates(s, vars));
                        }
                        ArgumentValue::Multiple(list) => {
                            for s in list {
                                result.push(replace_templates(s, vars));
                            }
                        }
                    }
                }
            }
        }
    }

    result
}

/// Check if a set of argument rules matches the current environment.
///
/// Rules use an "allow" (default) or "disallow" action. If any rule disallows,
/// the argument is excluded. If any rule allows, the argument is included
/// (assuming no disallow rule matched).
fn rules_match(rules: &[ArgumentRule], features: &HashMap<String, bool>) -> bool {
    if rules.is_empty() {
        return true;
    }

    let mut allowed = false;
    let mut disallowed = false;

    for rule in rules {
        let matches = rule_matches(rule, features);
        if matches {
            match rule.action.as_str() {
                "disallow" => disallowed = true,
                _ => allowed = true, // "allow" or any other action
            }
        }
    }

    // If any rule disallows, the argument is excluded
    if disallowed {
        return false;
    }

    // If no rule matched at all, exclude (default behavior for rule-gated args)
    if !allowed {
        return false;
    }

    true
}

/// Check if a single rule matches the current environment.
fn rule_matches(rule: &ArgumentRule, features: &HashMap<String, bool>) -> bool {
    // Check OS condition
    if let Some(os) = &rule.os {
        let current_os = current_os_name();
        let matches_os = match &os.name {
            Some(name) => {
                if name.starts_with('^') {
                    // Regex pattern (HMCL uses simple prefix matching too)
                    let pattern = name.trim_start_matches('^');
                    current_os.contains(pattern)
                } else {
                    current_os == name.as_str()
                }
            }
            None => true,
        };

        let matches_arch = match &os.arch {
            Some(arch) => arch == current_arch(),
            None => true,
        };

        if !matches_os || !matches_arch {
            return false;
        }
    }

    // Check features
    if let Some(feat) = &rule.features {
        for (key, required) in feat {
            if features.get(key.as_str()) != Some(required) {
                return false;
            }
        }
    }

    true
}

/// Get the current OS name as recognized by Minecraft's version JSON rules.
fn current_os_name() -> &'static str {
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

/// Get the current CPU architecture.
fn current_arch() -> &'static str {
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

/// Parse legacy `minecraftArguments` string into individual tokens.
///
/// The legacy format is a single space-separated string with ${variable} templates.
pub fn parse_legacy_arguments(minecraft_args: &str) -> Vec<String> {
    minecraft_args
        .split_whitespace()
        .map(|s| s.to_string())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::OsCondition;

    #[test]
    fn test_replace_simple_variable() {
        let mut vars = HashMap::new();
        vars.insert("auth_player_name".to_string(), "Steve".to_string());
        let result = replace_templates("--username ${auth_player_name}", &vars);
        assert_eq!(result, "--username Steve");
    }

    #[test]
    fn test_replace_multiple_variables() {
        let mut vars = HashMap::new();
        vars.insert("auth_player_name".to_string(), "Alex".to_string());
        vars.insert("version_type".to_string(), "release".to_string());
        let result = replace_templates("${auth_player_name} plays ${version_type}", &vars);
        assert_eq!(result, "Alex plays release");
    }

    #[test]
    fn test_unknown_variable_preserved() {
        let vars = HashMap::new();
        let result = replace_templates("--server ${unknown_var}", &vars);
        assert_eq!(result, "--server ${unknown_var}");
    }

    #[test]
    fn test_no_variables() {
        let vars = HashMap::new();
        let result = replace_templates("--username Steve", &vars);
        assert_eq!(result, "--username Steve");
    }

    #[test]
    fn test_empty_variable_name() {
        let vars = HashMap::new();
        let result = replace_templates("${}", &vars);
        assert_eq!(result, "${}");
    }

    #[test]
    fn test_resolve_simple_arguments() {
        let args = vec![
            VersionArgument::Simple("--username ${auth_player_name}".to_string()),
            VersionArgument::Simple("--version ${version_name}".to_string()),
        ];
        let mut vars = HashMap::new();
        vars.insert("auth_player_name".to_string(), "Steve".to_string());
        vars.insert("version_name".to_string(), "1.20.4".to_string());

        let result = resolve_arguments(&args, &vars, &HashMap::new());
        assert_eq!(result, vec!["--username Steve", "--version 1.20.4"]);
    }

    #[test]
    fn test_osx_os_condition() {
        // On non-macOS, this argument should be excluded
        let args = vec![VersionArgument::Ruled {
            rules: vec![ArgumentRule {
                action: "allow".to_string(),
                os: Some(OsCondition {
                    name: Some("osx".to_string()),
                    arch: None,
                    version: None,
                }),
                features: None,
            }],
            value: ArgumentValue::Single("-Xdock:name=Minecraft".to_string()),
        }];

        let result = resolve_arguments(&args, &HashMap::new(), &HashMap::new());

        #[cfg(target_os = "macos")]
        assert_eq!(result, vec!["-Xdock:name=Minecraft"]);
        #[cfg(not(target_os = "macos"))]
        assert!(result.is_empty());
    }

    #[test]
    fn test_feature_gated_argument() {
        let mut features = HashMap::new();
        features.insert("has_custom_resolution".to_string(), true);

        let args = vec![VersionArgument::Ruled {
            rules: vec![ArgumentRule {
                action: "allow".to_string(),
                os: None,
                features: Some({
                    let mut f = HashMap::new();
                    f.insert("has_custom_resolution".to_string(), true);
                    f
                }),
            }],
            value: ArgumentValue::Multiple(vec![
                "--width".to_string(),
                "${resolution_width}".to_string(),
                "--height".to_string(),
                "${resolution_height}".to_string(),
            ]),
        }];

        let mut vars = HashMap::new();
        vars.insert("resolution_width".to_string(), "1920".to_string());
        vars.insert("resolution_height".to_string(), "1080".to_string());

        let result = resolve_arguments(&args, &vars, &features);
        assert_eq!(result, vec!["--width", "1920", "--height", "1080"]);

        // Without the feature, the argument should be excluded
        let empty_features = HashMap::new();
        let result2 = resolve_arguments(&args, &vars, &empty_features);
        assert!(result2.is_empty());
    }

    #[test]
    fn test_legacy_arguments_parsing() {
        let args = " --username ${auth_player_name} --version ${version_name} --gameDir ${game_directory} ";
        let result = parse_legacy_arguments(args);
        assert_eq!(result.len(), 6);
        assert_eq!(result[0], "--username");
        assert_eq!(result[2], "--version");
    }

    #[test]
    fn test_build_template_vars_coverage() {
        let vars = build_template_vars(
            "Steve",
            "uuid",
            "token",
            "msa",
            "1.20.4",
            "release",
            "/game",
            "/game/assets",
            "17",
            "/game/natives",
            "/game/libraries/*:/game/versions/1.20.4/1.20.4.jar",
            "RMCL",
            "0.1.0",
            Some(1920),
            Some(1080),
        );
        assert_eq!(vars.get("auth_player_name").unwrap(), "Steve");
        assert_eq!(vars.get("version_type").unwrap(), "release");
        assert_eq!(vars.get("resolution_width").unwrap(), "1920");
        assert_eq!(vars.get("launcher_name").unwrap(), "RMCL");
        assert!(vars.contains_key("classpath"));
    }
    #[test]
    fn test_replace_special_chars_in_value() {
        let mut vars = HashMap::new();
        vars.insert(
            "game_directory".to_string(),
            "/home/user/My Games/.minecraft".to_string(),
        );
        let result = replace_templates("--gameDir ${game_directory}", &vars);
        assert_eq!(result, "--gameDir /home/user/My Games/.minecraft");
    }

    #[test]
    fn test_replace_adjacent_variables() {
        let mut vars = HashMap::new();
        vars.insert("a".to_string(), "hello".to_string());
        vars.insert("b".to_string(), "world".to_string());
        let result = replace_templates("${a}${b}", &vars);
        assert_eq!(result, "helloworld");
    }

    #[test]
    fn test_dollar_without_brace_preserved() {
        let vars = HashMap::new();
        let result = replace_templates("just $ a dollar sign ${}", &vars);
        assert_eq!(result, "just $ a dollar sign ${}");
    }

    #[test]
    fn test_incomplete_brace_no_closing() {
        let mut vars = HashMap::new();
        vars.insert("x".to_string(), "value".to_string());
        let result = replace_templates("${x and ${y", &vars);
        // Only ${x should be consumed if {y} isn't properly closed
        assert_eq!(result, "${x and ${y");
    }

    #[test]
    fn test_variable_with_dashes_and_underscores() {
        let mut vars = HashMap::new();
        vars.insert("my_var-name".to_string(), "test_value".to_string());
        let result = replace_templates("--option ${my_var-name}", &vars);
        assert_eq!(result, "--option test_value");
    }

    #[test]
    fn test_disallow_rule_excludes_argument() {
        let args = vec![VersionArgument::Ruled {
            rules: vec![
                ArgumentRule {
                    action: "allow".to_string(),
                    os: None,
                    features: None,
                },
                ArgumentRule {
                    action: "disallow".to_string(),
                    os: Some(OsCondition {
                        name: Some("linux".to_string()),
                        arch: None,
                        version: None,
                    }),
                    features: None,
                },
            ],
            value: ArgumentValue::Single("--linux-only-arg".to_string()),
        }];

        let result = resolve_arguments(&args, &HashMap::new(), &HashMap::new());

        #[cfg(target_os = "linux")]
        assert!(result.is_empty()); // disallowed on linux
        #[cfg(not(target_os = "linux"))]
        assert_eq!(result, vec!["--linux-only-arg"]); // allowed elsewhere
    }

    #[test]
    fn test_multiple_rules_all_match() {
        let args = vec![VersionArgument::Ruled {
            rules: vec![ArgumentRule {
                action: "allow".to_string(),
                os: None,
                features: None,
            }],
            value: ArgumentValue::Multiple(vec![
                "--width".to_string(),
                "1920".to_string(),
                "--height".to_string(),
                "1080".to_string(),
            ]),
        }];

        let result = resolve_arguments(&args, &HashMap::new(), &HashMap::new());
        assert_eq!(result, vec!["--width", "1920", "--height", "1080"]);
    }

    #[test]
    fn test_empty_arguments_list() {
        let args: Vec<VersionArgument> = vec![];
        let result = resolve_arguments(&args, &HashMap::new(), &HashMap::new());
        assert!(result.is_empty());
    }

    #[test]
    fn test_legacy_arguments_empty_string() {
        let result = parse_legacy_arguments("");
        assert!(result.is_empty());
    }

    #[test]
    fn test_legacy_arguments_single_token() {
        let result = parse_legacy_arguments("--username");
        assert_eq!(result, vec!["--username"]);
    }

    #[test]
    fn test_build_template_vars_no_resolution() {
        let vars = build_template_vars(
            "Alex",
            "uuid-here",
            "token123",
            "legacy",
            "1.16.5",
            "release",
            "/game",
            "/game/assets",
            "1",
            "/game/natives",
            "/game/libs/*",
            "RMCL",
            "0.1.0",
            None,
            None,
        );
        assert_eq!(vars.get("resolution_width").unwrap(), "");
        assert_eq!(vars.get("resolution_height").unwrap(), "");
        assert_eq!(vars.get("user_type").unwrap(), "legacy");
    }

    #[test]
    fn test_feature_not_present_is_false() {
        let mut features = HashMap::new();
        features.insert("has_custom_resolution".to_string(), false);

        let args = vec![VersionArgument::Ruled {
            rules: vec![ArgumentRule {
                action: "allow".to_string(),
                os: None,
                features: Some({
                    let mut f = HashMap::new();
                    f.insert("has_custom_resolution".to_string(), true);
                    f
                }),
            }],
            value: ArgumentValue::Single("--resolution-arg".to_string()),
        }];

        // Feature exists but is false, so rule shouldn't match
        let result = resolve_arguments(&args, &HashMap::new(), &features);
        assert!(result.is_empty());
    }
}
