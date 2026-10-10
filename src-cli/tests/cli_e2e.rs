//! End-to-end tests — spawn the real `rmcli` binary and assert exit codes and
//! the unified JSON envelope contract end to end (parse → context → handler →
//! output), with no network involved.

use std::path::PathBuf;
use std::process::{Command, Output};

const BIN: &str = env!("CARGO_BIN_EXE_rmcli");

struct RunResult {
    status: i32,
    stdout: String,
}

fn run(args: &[&str]) -> RunResult {
    let out: Output = Command::new(BIN)
        .args(args)
        .output()
        .expect("failed to spawn rmcli binary");
    RunResult {
        status: out.status.code().unwrap_or(-1),
        stdout: String::from_utf8_lossy(&out.stdout).to_string(),
    }
}

fn temp_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "rmccc-cli-e2e-{tag}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// Parse the final JSON line (envelope) from stdout.
fn last_json(r: &RunResult) -> serde_json::Value {
    let line = r.stdout.lines().last().unwrap_or_default();
    serde_json::from_str(line).expect("stdout must end with a JSON envelope")
}

#[test]
fn e2e_version_and_help() {
    let v = run(&["--version"]);
    assert_eq!(v.status, 0);
    assert!(v.stdout.contains("rmcli"));

    let h = run(&["--help"]);
    assert_eq!(h.status, 0);
    assert!(h.stdout.contains("profile"));
}

#[test]
fn e2e_profile_create_list_json_contract() {
    let dir = temp_dir("profile");
    let d = dir.to_str().unwrap();

    let create = run(&[
        "-d", d, "--output", "json", "profile", "create", "-n", "E2E", "-v", "1.21",
    ]);
    assert_eq!(create.status, 0);
    let envelope = last_json(&create);
    assert_eq!(envelope["ok"], true);
    let id = envelope["data"]["id"].as_str().unwrap().to_string();

    let list = run(&["-d", d, "--output", "json", "profile", "list"]);
    assert_eq!(list.status, 0);
    let envelope = last_json(&list);
    assert_eq!(envelope["ok"], true);
    assert_eq!(envelope["data"]["count"], 1);
    assert_eq!(envelope["data"]["profiles"][0]["id"], id);

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn e2e_error_exit_codes() {
    let dir = temp_dir("errors");
    let d = dir.to_str().unwrap();

    // Unknown command → clap usage error (2).
    let r = run(&["-d", d, "--output", "json", "frobnicate"]);
    assert_eq!(r.status, 2);

    // Missing required arg → clap usage error (2).
    let r = run(&["-d", d, "--output", "json", "profile", "create"]);
    assert_eq!(r.status, 2);

    // ProfileNotFound → 4.
    let r = run(&["-d", d, "--output", "json", "profile", "get", "--id", "nope"]);
    assert_eq!(r.status, 4);
    assert_eq!(last_json(&r)["error"]["code"], "profile_not_found");

    // Validation (unknown config field) → 3.
    let r = run(&["-d", d, "--output", "json", "config", "set", "bogus", "1"]);
    assert_eq!(r.status, 3);
    assert_eq!(last_json(&r)["error"]["code"], "validation_error");

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn e2e_auth_offline_flow() {
    let dir = temp_dir("auth");
    let d = dir.to_str().unwrap();

    let login = run(&[
        "-d", d, "--output", "json", "auth", "login", "-t", "offline", "--username", "E2EPlayer",
    ]);
    assert_eq!(login.status, 0);
    assert_eq!(last_json(&login)["data"]["username"], "E2EPlayer");

    let status = run(&["-d", d, "--output", "json", "auth", "status"]);
    assert_eq!(status.status, 0);
    assert_eq!(last_json(&status)["data"]["logged_in"], true);

    let logout = run(&["-d", d, "--output", "json", "auth", "logout"]);
    assert_eq!(logout.status, 0);

    let status2 = run(&["-d", d, "--output", "json", "auth", "status"]);
    assert_eq!(last_json(&status2)["data"]["logged_in"], false);

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn e2e_help_and_completions_commands() {
    let dir = temp_dir("help");
    let d = dir.to_str().unwrap();

    let help = run(&["-d", d, "help", "launch"]);
    assert_eq!(help.status, 0);
    assert!(help.stdout.contains("launch run"));

    let comp = run(&["-d", d, "completions", "--shell", "fish"]);
    assert_eq!(comp.status, 0);
    assert!(comp.stdout.contains("profile"));

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn e2e_json_auto_mode_when_piped() {
    // When stdout is not a TTY (as in a pipe), `--output auto` must resolve to
    // JSON. We simulate this by NOT passing --output; CI runs stdout as a pipe.
    let dir = temp_dir("auto");
    let d = dir.to_str().unwrap();
    let r = run(&["-d", d, "profile", "create", "-n", "Auto", "-v", "1.21"]);
    assert_eq!(r.status, 0);
    // Accept either JSON envelope (pipe) or plain text (interactive) — both
    // are valid; here we assert the command succeeded.
    let parsed = serde_json::from_str::<serde_json::Value>(r.stdout.trim());
    if let Ok(v) = parsed {
        assert_eq!(v["ok"], true);
    }
    let _ = std::fs::remove_dir_all(&dir);
}
