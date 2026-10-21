//! End-to-end tests for the logic helper supervisor (Phase 5, ADR-007).
//!
//! These spawn the real `rmccc-logic-host` binary, so they cover the pieces that
//! unit tests cannot: the handshake, stdio framing, fuel/memory limits, the
//! import allowlist and the kill-on-timeout policy.
//!
//! The helper is a workspace member, so `cargo test --workspace` builds it at
//! `target/debug/rmccc-logic-host`. When it is missing the tests skip instead of
//! failing a `cargo test -p rmccc-logic --lib`-style invocation.

use std::path::PathBuf;
use std::time::{Duration, Instant};

use rmccc_logic::abi::LogicLimits;
use rmccc_logic::protocol::LogicErrorCode;
use rmccc_logic::{LogicHost, Request};

fn helper_path() -> Option<PathBuf> {
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let root = manifest.parent()?.parent()?;
    for profile in ["debug", "release"] {
        let candidate = root.join("target").join(profile).join("rmccc-logic-host");
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}

fn spawn() -> Option<LogicHost> {
    let exe = helper_path()?;
    Some(LogicHost::spawn(&exe).expect("spawn helper"))
}

/// A well-behaved module: echoes `{"method":..., "input":...}` back with `ok`.
const ECHO_WAT: &str = r#"(module
  (import "rmccc" "log" (func $log (param i32 i32 i32)))
  (memory (export "memory") 1)
  (global $bump (mut i32) (i32.const 1024))
  (data (i32.const 0) "{\"ok\":true}")
  (func (export "rmccc_alloc") (param $len i32) (result i32)
    (local $ptr i32)
    (local.set $ptr (global.get $bump))
    (global.set $bump (i32.add (local.get $ptr) (local.get $len)))
    (local.get $ptr))
  (func (export "rmccc_invoke") (param $ptr i32) (param $len i32) (result i64)
    (call $log (i32.const 3) (local.get $ptr) (local.get $len))
    ;; return (out_ptr = 0, out_len = 11) packed as ptr << 32 | len
    (i64.or (i64.shl (i64.const 0) (i64.const 32)) (i64.const 11))))"#;

/// A module that burns fuel forever.
const SPIN_WAT: &str = r#"(module
  (memory (export "memory") 1)
  (func (export "rmccc_alloc") (param i32) (result i32) (i32.const 1024))
  (func (export "rmccc_invoke") (param i32 i32) (result i64)
    (loop $spin (br $spin))
    (i64.const 0)))"#;

/// A module that imports something the host does not allow.
const FORBIDDEN_IMPORT_WAT: &str = r#"(module
  (import "wasi_snapshot_preview1" "fd_write" (func $write (param i32 i32 i32 i32) (result i32)))
  (memory (export "memory") 1)
  (func (export "rmccc_alloc") (param i32) (result i32) (i32.const 1024))
  (func (export "rmccc_invoke") (param i32 i32) (result i64) (i64.const 0)))"#;

/// A module with a 64 MiB memory request, above the 16 MiB limit.
const GREEDY_MEMORY_WAT: &str = r#"(module
  (memory (export "memory") 1024)
  (func (export "rmccc_alloc") (param i32) (result i32) (i32.const 1024))
  (func (export "rmccc_invoke") (param i32 i32) (result i64) (i64.const 0)))"#;

fn write_module(name: &str, wat: &str) -> PathBuf {
    let bytes = wat::parse_str(wat).expect("valid wat");
    let dir = std::env::temp_dir().join(format!("rmccc-logic-test-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("temp dir");
    let path = dir.join(format!("{name}.wasm"));
    std::fs::write(&path, bytes).expect("write wasm");
    path
}

#[test]
fn ping_reports_the_logic_api_version() {
    let Some(mut host) = spawn() else {
        eprintln!("skipping: build rmccc-logic-host first");
        return;
    };
    assert_eq!(host.ping().expect("ping"), rmccc_logic::LOGIC_API_VERSION);
    assert!(host.is_alive());
}

#[test]
fn a_module_that_only_imports_log_runs_and_returns_json() {
    let Some(mut host) = spawn() else { return };
    let wasm = write_module("echo", ECHO_WAT);
    host.load("dev.test.echo", &wasm, LogicLimits::default())
        .expect("load");

    let reply = host
        .invoke(
            "dev.test.echo",
            "echo",
            serde_json::json!({"hello": "world"}),
            LogicLimits::default(),
        )
        .expect("invoke");
    assert_eq!(reply, serde_json::json!({"ok": true}));
    host.unload("dev.test.echo").expect("unload");
}

#[test]
fn invoking_an_unknown_plugin_is_rejected() {
    let Some(mut host) = spawn() else { return };
    let wasm = write_module("echo2", ECHO_WAT);
    host.load("dev.test.echo", &wasm, LogicLimits::default())
        .expect("load");
    let error = host
        .invoke(
            "dev.test.other",
            "echo",
            serde_json::Value::Null,
            LogicLimits::default(),
        )
        .expect_err("must reject");
    assert_eq!(error.code, LogicErrorCode::NotLoaded, "{error}");
}

#[test]
fn imports_outside_the_allowlist_are_rejected_before_running() {
    let Some(mut host) = spawn() else { return };
    let wasm = write_module("forbidden", FORBIDDEN_IMPORT_WAT);
    let error = host
        .load("dev.test.forbidden", &wasm, LogicLimits::default())
        .expect_err("must reject the import");
    assert_eq!(error.code, LogicErrorCode::ImportNotAllowed, "{error}");
    assert!(error.message.contains("fd_write"), "{error}");
}

#[test]
fn fuel_exhaustion_is_reported_without_killing_the_helper() {
    let Some(mut host) = spawn() else { return };
    let wasm = write_module("spin", SPIN_WAT);
    host.load("dev.test.spin", &wasm, LogicLimits::default())
        .expect("load");

    let limits = LogicLimits {
        fuel: 100_000,
        ..LogicLimits::default()
    };
    let error = host
        .invoke("dev.test.spin", "spin", serde_json::Value::Null, limits)
        .expect_err("must run out of fuel");
    assert_eq!(error.code, LogicErrorCode::FuelExhausted, "{error}");

    // Fuel exhaustion is a normal trap inside the module: the helper survives.
    assert!(host.is_alive(), "helper should still be running");
    assert_eq!(
        host.ping().expect("ping after fuel"),
        rmccc_logic::LOGIC_API_VERSION
    );
}

#[test]
fn a_module_asking_for_too_much_memory_fails_to_instantiate() {
    let Some(mut host) = spawn() else { return };
    let wasm = write_module("greedy", GREEDY_MEMORY_WAT);
    let error = host
        .load("dev.test.greedy", &wasm, LogicLimits::default())
        .expect_err("must reject 64 MiB");
    // wasmi reports the failed memory grow during instantiation.
    assert!(
        matches!(
            error.code,
            LogicErrorCode::ModuleInvalid | LogicErrorCode::Trap
        ),
        "unexpected code: {error}"
    );
}

#[test]
fn a_wall_clock_timeout_kills_the_helper_and_the_next_call_recovers() {
    let Some(mut host) = spawn() else { return };
    let wasm = write_module("slow", SPIN_WAT);
    // Fuel high enough that the module spins past the wall-clock budget.
    let limits = LogicLimits {
        timeout_ms: 150,
        fuel: 2_000_000_000,
        ..LogicLimits::default()
    };
    host.load("dev.test.slow", &wasm, limits).expect("load");

    let started = Instant::now();
    let error = host
        .invoke("dev.test.slow", "spin", serde_json::Value::Null, limits)
        .expect_err("must time out");
    assert_eq!(error.code, LogicErrorCode::Timeout, "{error}");
    assert!(
        started.elapsed() < Duration::from_secs(3),
        "timeout must not wait for the module"
    );
    assert!(!host.is_alive(), "timeout must kill the helper");

    host.restart().expect("restart");
    assert_eq!(host.ping().expect("ping"), rmccc_logic::LOGIC_API_VERSION);
}

#[test]
fn the_protocol_rejects_a_foreign_api_version() {
    // Pure protocol check: the supervisor refuses a response with another `v`.
    let error = rmccc_logic::Response::parse(r#"{"v":99,"id":1,"ok":true,"result":null}"#)
        .expect_err("must reject v99");
    assert_eq!(error.code, LogicErrorCode::ProtocolVersion);
}

#[test]
fn requests_serialize_without_newlines() {
    let line = Request::new(1, rmccc_logic::RequestParams::Ping)
        .to_line()
        .unwrap();
    assert!(!line.contains('\n'));
    assert!(line.starts_with(r#"{"v":1,"id":1"#), "{line}");
}
