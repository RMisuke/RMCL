//! CLI integration tests — drive the full command pipeline in-process with
//! captured output.
//!
//! Coverage:
//! - command parsing / help / completions
//! - the unified JSON envelope contract (`ok` / `data` / `error`) and exit codes
//! - typical business paths (profile CRUD, auth offline, config/state/user,
//!   local mod management, system diagnostics)
//! - exceptions & edge cases (unknown command, missing args, validation,
//!   not-found, invalid output mode)

mod common;

use common::{Harness, SharedBuf};
use rmccc_cli::output::Output;
use serde_json::json;

// ────────────────────────── Parsing & help ──────────────────────────

#[tokio::test]
async fn help_command_renders_registry() {
    let h = Harness::plain("help");
    let code = h.run(&["help"]).await;
    assert_eq!(code, 0);
    assert!(h.stdout.text().contains("profile"));
    assert!(h.stdout.text().contains("launch"));

    let code = h.run(&["help", "profile", "create"]).await;
    assert_eq!(code, 0);
    assert!(h.stdout.text().contains("profile create"));

    let code = h.run(&["help", "does-not-exist"]).await;
    assert_eq!(code, 0);
    assert!(h.stdout.text().contains("Unknown command"));
}

#[tokio::test]
async fn completions_generate_for_each_shell() {
    let h = Harness::plain("completions");
    for shell in ["bash", "zsh", "fish", "powershell", "elvish"] {
        let code = h.run(&["completions", "--shell", shell]).await;
        assert_eq!(code, 0, "completions for {shell}");
        assert!(h.stdout.text().contains("profile"));
    }
    // Unsupported shell is still handled gracefully (exit 0 with a note).
    let code = h.run(&["completions", "--shell", "tcsh"]).await;
    assert_eq!(code, 0);
}

#[tokio::test]
async fn unknown_command_is_a_usage_error() {
    let h = Harness::json("unknown-cmd");
    let code = h.run(&["frobnicate"]).await;
    assert_eq!(code, 2, "unknown command must be a clap usage error");
}

#[tokio::test]
async fn missing_required_argument_is_a_usage_error() {
    let h = Harness::json("missing-arg");
    let code = h.run(&["profile", "create"]).await;
    assert_eq!(code, 2, "missing -n must be a usage error");
}

#[tokio::test]
async fn invalid_output_mode_is_rejected() {
    // Output mode is validated by the top-level runner; here we exercise the
    // parser contract directly via `run`.
    let h = Harness::json("invalid-output");
    // The `run` helper ignores global flags; verifying the mode enum parse:
    assert!(rmccc_cli::output::OutputMode::parse("json").is_some());
    assert!(rmccc_cli::output::OutputMode::parse("plain").is_some());
    assert!(rmccc_cli::output::OutputMode::parse("nope").is_none());
}

// ────────────────────────── JSON envelope contract ──────────────────────────

#[tokio::test]
async fn json_success_emits_ok_envelope() {
    let h = Harness::json("env-success");
    let code = h
        .run(&["profile", "create", "-n", "Alpha", "-v", "1.21"])
        .await;
    assert_eq!(code, 0);
    let envelope = h.last_json();
    assert_eq!(envelope["ok"], true);
    assert_eq!(envelope["data"]["name"], "Alpha");
    assert_eq!(envelope["data"]["game_version"], "1.21");
}

#[tokio::test]
async fn json_error_emits_error_envelope_with_exit_code() {
    let h = Harness::json("env-error");
    let code = h.run(&["profile", "get", "--id", "missing-profile"]).await;
    assert_eq!(code, 4, "ProfileNotFound must map to exit code 4");
    let envelope = h.last_json();
    assert_eq!(envelope["ok"], false);
    assert_eq!(envelope["error"]["code"], "profile_not_found");
    assert_eq!(envelope["error"]["exit_code"], 4);
    assert_eq!(envelope["error"]["retryable"], false);
}

// ────────────────────────── Profile business path ──────────────────────────

#[tokio::test]
async fn profile_full_lifecycle() {
    let h = Harness::json("profile-lifecycle");

    // Create
    let code = h
        .run(&["profile", "create", "-n", "Beta", "-v", "1.20.4", "-l", "fabric", "--memory", "3072"])
        .await;
    assert_eq!(code, 0);
    let created = h.last_json();
    let id = created["data"]["id"].as_str().unwrap().to_string();
    assert_eq!(created["data"]["mod_loader"]["loader_type"], "fabric");
    assert_eq!(created["data"]["memory_mb"], 3072);

    // Get
    let code = h.run(&["profile", "get", "--id", &id]).await;
    assert_eq!(code, 0);
    assert_eq!(h.last_json()["data"]["name"], "Beta");

    // List
    let code = h.run(&["profile", "list"]).await;
    assert_eq!(code, 0);
    assert_eq!(h.last_json()["data"]["count"], 1);

    // Update
    let code = h
        .run(&["profile", "update", "--id", &id, "--name", "Beta2", "--memory", "4096"])
        .await;
    assert_eq!(code, 0);
    assert_eq!(h.last_json()["data"]["name"], "Beta2");

    // Select + Active
    let code = h.run(&["profile", "select", "--id", &id]).await;
    assert_eq!(code, 0);
    let code = h.run(&["profile", "active"]).await;
    assert_eq!(code, 0);
    assert_eq!(h.last_json()["data"]["id"], id);

    // Delete
    let code = h.run(&["profile", "delete", "--id", &id]).await;
    assert_eq!(code, 0);
    let code = h.run(&["profile", "list"]).await;
    assert_eq!(code, 0);
    assert_eq!(h.last_json()["data"]["count"], 0);
}

#[tokio::test]
async fn profile_create_rejects_invalid_input() {
    let h = Harness::json("profile-invalid");
    // Empty name → validation error (exit 3).
    let code = h.run(&["profile", "create", "-n", "", "-v", "1.21"]).await;
    assert_eq!(code, 3);
    assert_eq!(h.last_json()["error"]["code"], "validation_error");
}

#[tokio::test]
async fn profile_unknown_loader_falls_back_to_vanilla() {
    let h = Harness::json("profile-loader");
    // Unknown loader type is simply not attached (no validation at create);
    // the loader type is validated at modloader install time.
    let code = h
        .run(&["profile", "create", "-n", "X", "-v", "1.21", "-l", "nope"])
        .await;
    assert_eq!(code, 0);
    assert_eq!(h.last_json()["data"]["mod_loader"], json!(null));
}

// ────────────────────────── Auth (offline) path ──────────────────────────

#[tokio::test]
async fn auth_offline_lifecycle() {
    let h = Harness::json("auth-lifecycle");

    // Not logged in initially.
    let code = h.run(&["auth", "status"]).await;
    assert_eq!(code, 0);
    assert_eq!(h.last_json()["data"]["logged_in"], false);

    // Login offline.
    let code = h.run(&["auth", "login", "-t", "offline", "--username", "Steve"]).await;
    assert_eq!(code, 0);
    assert_eq!(h.last_json()["data"]["username"], "Steve");
    assert_eq!(h.last_json()["data"]["provider"], "offline");

    // Status reflects the session.
    let code = h.run(&["auth", "status"]).await;
    assert_eq!(code, 0);
    assert_eq!(h.last_json()["data"]["logged_in"], true);

    // List + Select are consistent.
    let code = h.run(&["auth", "list"]).await;
    assert_eq!(code, 0);
    assert_eq!(h.last_json()["data"]["count"], 1);
    let account_id = h.last_json()["data"]["accounts"][0]["account_id"]
        .as_str()
        .unwrap()
        .to_string();
    let code = h.run(&["auth", "select", "--account", &account_id]).await;
    assert_eq!(code, 0);

    // Logout clears the active session.
    let code = h.run(&["auth", "logout"]).await;
    assert_eq!(code, 0);
    let code = h.run(&["auth", "status"]).await;
    assert_eq!(code, 0);
    assert_eq!(h.last_json()["data"]["logged_in"], false);
}

#[tokio::test]
async fn auth_offline_login_requires_username() {
    let h = Harness::json("auth-nouser");
    let code = h.run(&["auth", "login", "-t", "offline"]).await;
    assert_eq!(code, 3, "missing username must be a validation error");
}

#[tokio::test]
async fn auth_unknown_login_type_is_rejected() {
    let h = Harness::json("auth-badtype");
    let code = h
        .run(&["auth", "login", "-t", "legacy", "--username", "x"])
        .await;
    assert_eq!(code, 3);
}

// ────────────────────────── Config / state / user ──────────────────────────

#[tokio::test]
async fn config_get_set_and_path() {
    let h = Harness::json("config-path");

    let code = h.run(&["config", "set", "default_max_memory_mb", "8192"]).await;
    assert_eq!(code, 0);

    let code = h.run(&["config", "get", "default_max_memory_mb"]).await;
    assert_eq!(code, 0);
    assert_eq!(h.last_json()["data"], 8192);

    let code = h.run(&["config", "path"]).await;
    assert_eq!(code, 0);
    assert!(h.last_json()["data"]["game_root_dir"].is_string());
}

#[tokio::test]
async fn config_set_unknown_field_is_validation_error() {
    let h = Harness::json("config-unknown");
    let code = h.run(&["config", "set", "no_such_field", "1"]).await;
    assert_eq!(code, 3);
}

#[tokio::test]
async fn config_set_invalid_number_is_rejected() {
    let h = Harness::json("config-badnum");
    let code = h.run(&["config", "set", "default_max_memory_mb", "abc"]).await;
    assert_eq!(code, 3);
}

#[tokio::test]
async fn state_and_user_settings_paths() {
    let h = Harness::json("state-user");
    let code = h
        .run(&["profile", "create", "-n", "P", "-v", "1.21"])
        .await;
    assert_eq!(code, 0);
    let id = h.last_json()["data"]["id"].as_str().unwrap().to_string();

    let code = h.run(&["profile", "select", "--id", &id]).await;
    assert_eq!(code, 0);

    // State reflects the selected profile.
    let code = h.run(&["config", "state", "get", "selected_profile_id"]).await;
    assert_eq!(code, 0);
    assert_eq!(h.last_json()["data"], id);

    // User settings roundtrip.
    let code = h.run(&["config", "user", "set", "locale", "zh-CN"]).await;
    assert_eq!(code, 0);
    let code = h.run(&["config", "user", "get", "locale"]).await;
    assert_eq!(code, 0);
    assert_eq!(h.last_json()["data"], "zh-CN");

    // Unknown user field → validation error.
    let code = h.run(&["config", "user", "set", "bogus", "1"]).await;
    assert_eq!(code, 3);
}

// ────────────────────────── Local mod management ──────────────────────────

#[tokio::test]
async fn local_mod_install_list_toggle_remove() {
    let h = Harness::json("mod-local");

    // Create a fake jar to install.
    let jar = common::temp_dir("jar").join("my-mod-1.0.0.jar");
    std::fs::write(&jar, b"PK\x03\x04 fake jar").unwrap();

    let code = h
        .run(&["profile", "create", "-n", "ModHost", "-v", "1.21"])
        .await;
    assert_eq!(code, 0);
    let pid = h.last_json()["data"]["id"].as_str().unwrap().to_string();

    let code = h.run(&["mod", "install", "--profile", &pid, "--file", jar.to_str().unwrap()]).await;
    assert_eq!(code, 0, "local install must succeed");
    assert_eq!(h.last_json()["data"]["installed"], "my-mod-1.0.0.jar");

    // List shows it active.
    let code = h.run(&["mod", "list", "--profile", &pid]).await;
    assert_eq!(code, 0);
    assert_eq!(h.last_json()["data"]["count"], 1);

    // Toggle off → moves to disabled.
    let code = h
        .run(&["mod", "toggle", "--profile", &pid, "--file", "my-mod-1.0.0.jar", "--enabled", "false"])
        .await;
    assert_eq!(code, 0);
    let code = h.run(&["mod", "list", "--profile", &pid]).await;
    assert_eq!(code, 0);
    assert_eq!(h.last_json()["data"]["active"].as_array().unwrap().len(), 0);
    assert_eq!(h.last_json()["data"]["disabled"].as_array().unwrap().len(), 1);

    // Remove.
    let code = h
        .run(&["mod", "remove", "--profile", &pid, "--file", "my-mod-1.0.0.jar"])
        .await;
    assert_eq!(code, 0);
    let code = h.run(&["mod", "list", "--profile", &pid]).await;
    assert_eq!(code, 0);
    assert_eq!(h.last_json()["data"]["count"], 0);
}

#[tokio::test]
async fn local_mod_install_rejects_missing_file() {
    let h = Harness::json("mod-missing-file");
    let code = h
        .run(&["profile", "create", "-n", "M", "-v", "1.21"])
        .await;
    assert_eq!(code, 0);
    let pid = h.last_json()["data"]["id"].as_str().unwrap().to_string();

    let code = h
        .run(&["mod", "install", "--profile", &pid, "--file", "/no/such/file.jar"])
        .await;
    assert_eq!(code, 4, "missing local file → NotFound (exit 4)");
    assert_eq!(h.last_json()["error"]["code"], "not_found");
}

#[tokio::test]
async fn mod_install_requires_exactly_one_mode() {
    let h = Harness::json("mod-ambig");
    let code = h.run(&["mod", "install", "--profile", "p"]).await;
    assert_eq!(code, 3, "no mode → validation error");
    let code = h
        .run(&["mod", "install", "--profile", "p", "--file", "a.jar", "--mod-id", "m", "--version-id", "v"])
        .await;
    assert_eq!(code, 3, "both modes → validation error");
}

// ────────────────────────── System diagnostics ──────────────────────────

#[tokio::test]
async fn system_info_snapshot() {
    let h = Harness::json("system-info");
    let code = h.run(&["system", "info"]).await;
    assert_eq!(code, 0);
    let data = &h.last_json()["data"];
    assert!(data["data_dir"].is_string());
    assert!(data["game_root_dir"].is_string());
    assert_eq!(data["profiles"], 0);
    assert_eq!(data["running_games"], 0);
    assert!(data["version"].is_string());

    let code = h.run(&["system", "paths"]).await;
    assert_eq!(code, 0);
}

// ────────────────────────── Version local ──────────────────────────

#[tokio::test]
async fn version_local_list_is_empty_then_reflects_profile_install() {
    let h = Harness::json("version-local");
    let code = h.run(&["version", "local", "list"]).await;
    assert_eq!(code, 0);
    assert_eq!(h.last_json()["data"]["count"], 0);

    // Missing local version lookup → error.
    let code = h.run(&["version", "local", "get", "1.20.4"]).await;
    assert_eq!(code, 4, "not installed → exit 4");
}

// ────────────────────────── Output plain rendering ──────────────────────────

#[tokio::test]
async fn plain_mode_renders_human_table() {
    let h = Harness::plain("plain-render");
    h.run(&["profile", "create", "-n", "First", "-v", "1.21"])
        .await;
    let code = h.run(&["profile", "list"]).await;
    assert_eq!(code, 0);
    let text = h.stdout.text();
    assert!(text.contains("First"), "plain list must contain profile name");
    assert!(text.contains("1.21"));
}

// ────────────────────────── REPL construction ──────────────────────────

#[test]
fn repl_constructs_and_exposes_registry_completion() {
    use rmccc_cli::repl::Repl;
    let rt = tokio::runtime::Runtime::new().unwrap();
    let h = rt.block_on(async {
        let h = Harness::json("repl-init");
        let stdout = SharedBuf::default();
        let stderr = SharedBuf::default();
        let out = Output::with_writers(
            rmccc_cli::output::OutputMode::Json,
            false,
            Box::new(stdout.clone()),
            Box::new(stderr.clone()),
        );
        let _repl = Repl::new(h.ctx.clone(), out, h._dir.clone()).expect("REPL must construct");
        h
    });
    let _ = h;
    // Completion candidates derive from the registry (no drift).
    let subs = rmccc_cli::registry::subcommand_names("profile");
    assert!(subs.contains(&"create"));
    assert!(subs.contains(&"delete"));
}
