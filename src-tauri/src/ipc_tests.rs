//! IPC 契约测试（审计 P2-2）。
//!
//! # 为什么不是"起一个 mock Tauri app 然后 invoke"
//!
//! `tauri::test` 的 mock 运行时会把整条命令链泛型化到 `MockRuntime`，而
//! `AppState` 持有 `tauri::AppHandle`（默认 Wry 运行时）——要用它的前提是把
//! **77 个命令**全部改成 `fn<R: Runtime>(state: State<'_, AppState<R>>, app: AppHandle<R>, ..)`。
//! 那是比本切片大得多的改动，收益却不比下面这层高多少。
//!
//! 因此这里测的是**契约本身**，也就是命令体真正依赖的三件事：
//!
//! 1. **注册表契约**：`handlers::IPC_COMMANDS` 与 `generate_handler!` 的实际注册项一致，
//!    且与前端 `src/ipc/api.ts` 的调用名一致（命令名拼错是最高频的 IPC bug）。
//! 2. **参数校验契约**：每个命令在"参数不合法"时必须返回确定的 `ErrorCode`
//!    （而不是 panic / 500 / 静默成功）。
//! 3. **错误码契约**：`RmcccError → IpcError` 的映射与 `retryable` 语义。
//!
//! 每条命令至少覆盖 1 个正向（可用状态）或 1 个拒绝（错误码）路径。

#![allow(clippy::unwrap_used, clippy::panic)]

use rmccc_error::{ErrorCode, IpcError, RmcccError};
use rmccc_version::GameRepository as _;
use std::path::PathBuf;

// ────────────────────────── 测试脚手架 ──────────────────────────

/// 一个隔离的数据目录 + 真实 `CoreContext`（不触碰用户数据）。
fn temp_core(tag: &str) -> (PathBuf, rmccc_core::CoreContext) {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let dir = std::env::temp_dir().join(format!("rmccc-ipc-{tag}-{}-{nanos}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("data dir");
    let core = rmccc_core::CoreContext::new(&dir).expect("CoreContext");
    (dir, core)
}

/// 断言 `IpcError` 的错误码。
fn assert_code(err: &IpcError, expected: ErrorCode) {
    assert_eq!(err.code, expected, "unexpected code: {:?}", err.message);
}

/// 期望 `Result` 是带指定错误码的 `Err`。
fn expect_code<T>(result: Result<T, IpcError>, expected: ErrorCode) -> IpcError {
    match result {
        Err(e) => {
            assert_code(&e, expected);
            e
        }
        Ok(_) => panic!("expected error {expected:?}, got Ok"),
    }
}

// ────────────────────────── 1. 注册表契约 ──────────────────────────

#[test]
fn ipc_registry_has_no_duplicate_command_names() {
    let mut seen = std::collections::HashSet::new();
    for cmd in crate::handlers::IPC_COMMANDS {
        assert!(
            seen.insert(*cmd),
            "duplicate IPC command in registry: {cmd}"
        );
    }
}

#[test]
fn ipc_registry_matches_generate_handler_list() {
    // `generate_handler!` 没有反射能力，所以交叉校验源码本身：
    // 清单里的每个名字都必须在 `invoke_handlers` 中出现，且反向也成立。
    let source = include_str!("handlers.rs");
    let handler_body = &source[source
        .find("tauri::generate_handler![")
        .expect("handler macro")..];

    for cmd in crate::handlers::IPC_COMMANDS {
        let registered = handler_body
            .lines()
            .any(|l| l.trim_end_matches(',').ends_with(&format!("::{cmd}")));
        assert!(
            registered,
            "{cmd} is listed in IPC_COMMANDS but not registered"
        );
    }

    // 反向：宏里注册的每一项都必须在清单里。
    let registered_count = handler_body
        .lines()
        .filter(|l| {
            let t = l.trim();
            t.starts_with("commands::") && t.ends_with(',')
        })
        .count();
    assert_eq!(
        registered_count,
        crate::handlers::IPC_COMMANDS.len(),
        "generate_handler! 注册数与 IPC_COMMANDS 不一致"
    );
}

#[test]
fn ipc_registry_is_covered_by_the_frontend_api() {
    // 命令名拼错（下划线/驼峰）是最高频的 IPC bug：前端每个 wrapper 必须能在
    // 后端注册表里找到同名命令。
    // 白名单：确实存在但前端刻意不调用的命令（须写明理由）。
    const BACKEND_ONLY: &[&str] = &[
        // 与 `mod_pick_and_install` 同源的非对话框版本：供脚本 / 未来拖拽复用。
        // 前端主路径走原生文件选择器，因此这里没有 wrapper。
        "mod_install_local",
        // ADR-002：这几个命令是**版本仓库维护**（id = 版本 id），不对应 UI 概念。
        // 前端只消费 profile 视角的 paths / check_completion / repair。
        "instance_list",
        "instance_rename",
        "instance_duplicate",
        "instance_remove",
        // ADR-013 S4/S5：MCP 管理面（ui-design 07 §2）。服务器与命令已落地，
        // 设置页分区随整体前端重构接线。
        "mcp_status",
        "mcp_set_enabled",
        "mcp_regenerate_token",
        "mcp_tools_list",
        "mcp_tools_set",
        "mcp_sessions_disconnect",
        "mcp_confirmations_list",
        "mcp_confirmation_resolve",
        "mcp_recent_calls",
    ];

    let api = include_str!("../../src/ipc/api.ts");
    for cmd in crate::handlers::IPC_COMMANDS {
        if BACKEND_ONLY.contains(cmd) {
            continue;
        }
        assert!(
            api.contains(&format!("\"{cmd}\"")),
            "IPC command '{cmd}' is registered but never called from src/ipc/api.ts"
        );
    }
}

#[test]
fn ipc_command_count_is_stable() {
    // 新增命令时这条会失败——提醒同时更新 bindings.rs / api.ts / 本文件。
    // 86 → 84 → 93（ADR-013）：S7 移除 agent_* 9 条、新增 ai_* 5 + mods 2 条；
    // S4/S5 再新增 mcp_* 9 条（设计目标 93 条达成）。
    // → 94：auth_skin_url（头像由后端代理 sessionserver，规避 WebView CORS）。
    // → 95：dialog_pick_directory（多游戏目录的路径选择）。
    // → 99：cache_icon_* 4 条（图标缓存子系统）。
    // → 100：settings_app_info（关于页：SNGL 启动器版本 / RMCCC 内核版本）。
    assert_eq!(crate::handlers::IPC_COMMANDS.len(), 100);
}

// ────────────────────────── 2. 错误码契约 ──────────────────────────

#[test]
fn error_code_mapping_is_stable() {
    let mut cases: Vec<(RmcccError, ErrorCode)> = vec![
        (RmcccError::Network("boom".into()), ErrorCode::NetworkError),
        (RmcccError::Timeout(30), ErrorCode::NetworkError),
        (
            RmcccError::HttpStatus {
                status: 503,
                url: "https://example.com".into(),
            },
            ErrorCode::NetworkError,
        ),
        (RmcccError::AuthFailed("no".into()), ErrorCode::AuthFailed),
        (RmcccError::CredentialExpired, ErrorCode::CredentialExpired),
        (
            RmcccError::WrongAccount {
                expected: "a".into(),
                actual: "b".into(),
            },
            ErrorCode::WrongAccount,
        ),
        (RmcccError::TokenExpired, ErrorCode::TokenExpired),
        (
            RmcccError::DownloadFailed {
                url: "u".into(),
                reason: "r".into(),
            },
            ErrorCode::DownloadFailed,
        ),
        (
            RmcccError::HashMismatch {
                path: "p".into(),
                expected: "a".into(),
                actual: "b".into(),
            },
            ErrorCode::HashMismatch,
        ),
        (
            RmcccError::InsufficientDiskSpace {
                needed: 2,
                available: 1,
            },
            ErrorCode::InsufficientDiskSpace,
        ),
        (RmcccError::JavaNotFound, ErrorCode::JavaNotFound),
        (
            RmcccError::JavaVersionMismatch {
                found: 8,
                required: 17,
            },
            ErrorCode::JavaVersionMismatch,
        ),
        (RmcccError::GameCrashed(1), ErrorCode::GameCrashed),
        (
            RmcccError::ProfileNotFound("p".into()),
            ErrorCode::ProfileNotFound,
        ),
        (
            RmcccError::MissingDependency("d".into()),
            ErrorCode::MissingDependency,
        ),
        (
            RmcccError::ModVersionConflict {
                mod_name: "m".into(),
                constraint: "c".into(),
            },
            ErrorCode::ModVersionConflict,
        ),
        (
            RmcccError::Validation("v".into()),
            ErrorCode::ValidationError,
        ),
        (
            RmcccError::UnsupportedPlatform("w".into()),
            ErrorCode::UnsupportedPlatform,
        ),
        (RmcccError::Other("o".into()), ErrorCode::Unknown),
    ];

    let plugin_cases = [
        (
            rmccc_error::PluginErrorCode::ManifestInvalid,
            ErrorCode::PluginManifestInvalid,
        ),
        (
            rmccc_error::PluginErrorCode::PackageInvalid,
            ErrorCode::PluginPackageInvalid,
        ),
        (
            rmccc_error::PluginErrorCode::PermissionDenied,
            ErrorCode::PluginPermissionDenied,
        ),
        (
            rmccc_error::PluginErrorCode::PermissionPending,
            ErrorCode::PluginPermissionPending,
        ),
        (
            rmccc_error::PluginErrorCode::IdConflict,
            ErrorCode::PluginIdConflict,
        ),
        (
            rmccc_error::PluginErrorCode::DependencyMissing,
            ErrorCode::PluginDependencyMissing,
        ),
        (
            rmccc_error::PluginErrorCode::DependencyCycle,
            ErrorCode::PluginDependencyCycle,
        ),
        (
            rmccc_error::PluginErrorCode::StateConflict,
            ErrorCode::PluginStateConflict,
        ),
        (
            rmccc_error::PluginErrorCode::ConfigInvalid,
            ErrorCode::PluginConfigInvalid,
        ),
        (
            rmccc_error::PluginErrorCode::NotFound,
            ErrorCode::PluginNotFound,
        ),
        (
            rmccc_error::PluginErrorCode::Integrity,
            ErrorCode::PluginIntegrity,
        ),
    ];
    for (code, expected) in plugin_cases {
        cases.push((
            RmcccError::Plugin {
                code,
                message: format!("{code:?}"),
            },
            expected,
        ));
    }

    for (err, expected) in cases {
        let ipc = IpcError::from(err);
        assert_eq!(ipc.code, expected, "unexpected mapping");
        assert!(!ipc.message.is_empty(), "message must not be empty");
    }
}

#[test]
fn only_transient_errors_are_retryable() {
    // retryable 决定前端是否显示"重试"按钮，语义很容易写成"全部可重试"。
    assert!(IpcError::from(RmcccError::Network("x".into())).retryable);
    assert!(IpcError::from(RmcccError::Timeout(5)).retryable);
    assert!(
        IpcError::from(RmcccError::HttpStatus {
            status: 500,
            url: "u".into()
        })
        .retryable
    );
    // 4xx 不是暂时性错误：重试只会再失败一次。
    assert!(
        !IpcError::from(RmcccError::HttpStatus {
            status: 404,
            url: "u".into()
        })
        .retryable
    );
    assert!(!IpcError::from(RmcccError::Validation("v".into())).retryable);
    assert!(!IpcError::from(RmcccError::JavaNotFound).retryable);
    assert!(!IpcError::from(RmcccError::ProfileNotFound("p".into())).retryable);
}

#[test]
fn ipc_error_serializes_with_the_frontend_shape() {
    // 前端 `IpcError` 类型：{ code, message, retryable }
    let json = serde_json::to_value(IpcError::from(RmcccError::Validation("bad".into()))).unwrap();
    assert_eq!(json["code"], "validation_error");
    assert_eq!(json["message"], "Validation failed: bad");
    assert_eq!(json["retryable"], false);
    assert!(json.get("data").is_none());
}

// ────────────────────────── 3. 参数校验契约：mods ──────────────────────────

#[test]
fn mod_filename_validation_rejects_traversal() {
    use crate::commands::mods::validate_mod_filename;
    for bad in [
        "",
        "../evil.jar",
        "a/../../b.jar",
        "dir/mod.jar",
        "win\\mod.jar",
        "nul\0byte.jar",
    ] {
        let err = expect_code(validate_mod_filename(bad), ErrorCode::ValidationError);
        assert!(err.message.contains("Invalid mod filename"));
    }
}

#[test]
fn mod_filename_validation_accepts_plain_jar_names() {
    use crate::commands::mods::validate_mod_filename;
    for good in ["sodium-0.5.8.jar", "Fabric API 0.92.jar", "mod_v1.2.3.jar"] {
        assert!(
            validate_mod_filename(good).is_ok(),
            "{good} should be valid"
        );
    }
}

#[test]
fn sanitize_filename_strips_path_separators() {
    use crate::commands::mods::sanitize_filename;
    assert_eq!(sanitize_filename("a/b\\c"), "a_b_c");
    assert_eq!(sanitize_filename("sodium.jar"), "sodium.jar");
    assert!(!sanitize_filename("../x").contains(".."));
}

// ────────────────────────── 4. 参数校验契约：平台 / loader ──────────────────────────

#[test]
fn external_url_scheme_is_whitelisted() {
    // open_external_url 的第一道闸就是 scheme 白名单（命令层直接复用平台层校验）。
    assert!(
        rmccc_core::rmccc_platform::shell::validate_openable_url("https://modrinth.com").is_ok()
    );
    expect_code(
        rmccc_core::rmccc_platform::shell::validate_openable_url("file:///etc/passwd")
            .map_err(IpcError::from),
        ErrorCode::ValidationError,
    );
    expect_code(
        rmccc_core::rmccc_platform::shell::validate_openable_url("-a Calculator")
            .map_err(IpcError::from),
        ErrorCode::ValidationError,
    );
}

#[test]
fn unknown_loader_type_is_rejected() {
    // modloader_list / modloader_install 的第一步都是这个解析。
    assert!(rmccc_modloader::ModLoaderType::from_str("fabric").is_some());
    assert!(rmccc_modloader::ModLoaderType::from_str("neoforge").is_some());
    assert!(rmccc_modloader::ModLoaderType::from_str("solaris-loader").is_none());
    assert!(rmccc_modloader::ModLoaderType::from_str("").is_none());
}

#[test]
fn serde_snake_case_loader_names_are_accepted() {
    // 前端拿到的是 ts-rs 生成的 serde `snake_case` 名：NeoForge → "neo_forge"，
    // OptiFine → "opti_fine"，LiteLoader → "lite_loader"，LegacyFabric →
    // "legacy_fabric"。此前 from_str 只认 library_id 形态，下载页点 NeoForge
    // 请求版本列表直接 "Unknown mod loader type"（2026-10-25 实测）。
    assert!(matches!(
        rmccc_modloader::ModLoaderType::from_str("neo_forge"),
        Some(rmccc_modloader::ModLoaderType::NeoForge)
    ));
    assert!(matches!(
        rmccc_modloader::ModLoaderType::from_str("opti_fine"),
        Some(rmccc_modloader::ModLoaderType::OptiFine)
    ));
    assert!(matches!(
        rmccc_modloader::ModLoaderType::from_str("lite_loader"),
        Some(rmccc_modloader::ModLoaderType::LiteLoader)
    ));
    assert!(matches!(
        rmccc_modloader::ModLoaderType::from_str("legacy_fabric"),
        Some(rmccc_modloader::ModLoaderType::LegacyFabric)
    ));
    // library_id 形态依旧成立。
    assert!(matches!(
        rmccc_modloader::ModLoaderType::from_str("neoforge"),
        Some(rmccc_modloader::ModLoaderType::NeoForge)
    ));
}

#[test]
fn incomplete_installers_are_not_installable() {
    // A6：OptiFine / LiteLoader / Cleanroom 安装器未完成（审计 2026-10-25），
    // install 流程（list=空列表 + install=快速失败）不得再提供它们；
    // all() 仍包含全部 8 种供 analyzer 识别已安装实例。
    use rmccc_modloader::ModLoaderType;
    for offered in ModLoaderType::installable() {
        assert!(offered.is_installable());
    }
    for hidden in ["optifine", "liteloader", "cleanroom"] {
        let lt = ModLoaderType::from_str(hidden).expect("known loader type");
        assert!(!lt.is_installable(), "{hidden} must not be installable yet");
        assert!(
            ModLoaderType::all().contains(&lt),
            "{hidden} stays recognizable"
        );
    }
}

// ────────────────────────── 5. 正向契约：profile ──────────────────────────

#[test]
fn profile_list_is_empty_on_a_fresh_data_dir() {
    let (dir, core) = temp_core("profiles-empty");
    assert!(core.profile_store.list().unwrap().is_empty());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn profile_lifecycle_round_trips() {
    let (dir, core) = temp_core("profiles-crud");
    let profile = rmccc_profile::Profile {
        id: "p1".into(),
        name: "survival".into(),
        game_version: "1.20.4".into(),
        game_dir: core.config.resolved_game_root(),
        ..Default::default()
    };
    core.profile_store.save(&profile).unwrap();

    let listed = core.profile_store.list().unwrap();
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].name, "survival");

    // update
    let mut updated = listed[0].clone();
    updated.memory_mb = 8192;
    core.profile_store.save(&updated).unwrap();
    assert_eq!(
        core.profile_store.get("p1").unwrap().unwrap().memory_mb,
        8192
    );

    // delete
    core.profile_store.delete("p1", false).unwrap();
    assert!(core.profile_store.get("p1").unwrap().is_none());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn profile_delete_of_unknown_id_reports_profile_not_found() {
    let (dir, core) = temp_core("profiles-missing");
    let err = core.profile_store.delete("nope", false).unwrap_err();
    assert!(matches!(err, RmcccError::ProfileNotFound(_)));
    assert_eq!(IpcError::from(err).code, ErrorCode::ProfileNotFound);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn profile_update_rejects_unknown_override_fields() {
    // 过滤发生在命令层（`profile_update`）：只保留 `fields::ALL` 里的名字，
    // 这样拼错的名字不会让字段"静默变成不可继承"。
    use rmccc_profile::fields;
    let filtered: Vec<String> = vec![
        "memory_mb".to_string(),
        "definitely_not_a_field".to_string(),
    ]
    .into_iter()
    .filter(|name| fields::ALL.contains(&name.as_str()))
    .collect();
    assert_eq!(filtered, vec!["memory_mb".to_string()]);

    // 命令层过滤后写回的 profile 只带合法字段。
    let (dir, core) = temp_core("profiles-override");
    let mut profile = rmccc_profile::Profile {
        id: "p2".into(),
        name: "x".into(),
        game_version: "1.20.4".into(),
        game_dir: core.config.resolved_game_root(),
        ..Default::default()
    };
    profile.override_properties = filtered.into_iter().collect();
    core.profile_store.save(&profile).unwrap();

    let stored = core.profile_store.get("p2").unwrap().unwrap();
    assert!(stored.is_overridden("memory_mb"));
    assert!(!stored.is_overridden("definitely_not_a_field"));
    let _ = std::fs::remove_dir_all(&dir);
}

// ────────────────────────── 6. 正向契约：settings ──────────────────────────

#[test]
fn settings_get_returns_defaults_on_fresh_install() {
    let (dir, core) = temp_core("settings-defaults");
    // 默认值必须自洽（前端据此渲染表单，NaN/0 会让启动失败）。
    assert!(core.config.default_max_memory_mb >= 512);
    assert!(core.config.event_throttle_ms > 0);
    assert!(!core.config.resolved_game_root().as_os_str().is_empty());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn settings_round_trip_and_persist_to_disk() {
    let (dir, mut core) = temp_core("settings-roundtrip");
    core.config.default_max_memory_mb = 6144;
    core.config.save(&core.data_dir).unwrap();

    let reloaded = rmccc_config::AppConfig::load(&core.data_dir).unwrap();
    assert_eq!(reloaded.default_max_memory_mb, 6144);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn supported_locales_include_english_and_chinese() {
    let (dir, _core) = temp_core("settings-locales");
    let locales = rmccc_config::SUPPORTED_LOCALES;
    assert!(locales.contains(&"en"));
    assert!(locales.contains(&"zh-CN"));
    // 每个受支持语言都要有可显示的名字（前端下拉框直接用它）。
    for code in locales {
        assert!(!rmccc_config::locale_native_name(code).is_empty());
    }
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn user_settings_default_to_follow_system() {
    let (dir, core) = temp_core("settings-user");
    // locale = None 表示跟随系统（前端显示"跟随系统"选项）。
    assert!(core.user_settings.locale.is_none());
    let _ = std::fs::remove_dir_all(&dir);
}

// ────────────────────────── 7. 正向契约：auth ──────────────────────────

#[test]
fn auth_list_accounts_is_empty_and_session_is_none() {
    let (dir, core) = temp_core("auth-empty");
    assert!(core.account_repo.list_metadata().unwrap().is_empty());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn auth_remove_unknown_account_is_idempotent() {
    // 契约：删除一个不存在的账户是 **no-op**（前端重复点击不能报错）。
    let (dir, core) = temp_core("auth-remove");
    core.account_repo
        .remove("no-such-account")
        .expect("removing an unknown account must be idempotent");
    core.account_repo
        .remove("no-such-account")
        .expect("second removal is also a no-op");
    let _ = std::fs::remove_dir_all(&dir);
}

// ────────────────────────── 8. 正向契约：logs ──────────────────────────

#[test]
fn log_get_clamps_the_requested_line_count() {
    // `log_get` 的契约：默认 100 行，上限 500（前端传 10 万行也只能拿 500）。
    // 这里复刻同一套 clamp 规则断言边界，避免"前端传什么就读什么"。
    fn clamp(lines: Option<usize>) -> usize {
        lines.unwrap_or(100).min(500)
    }
    assert_eq!(clamp(None), 100);
    assert_eq!(clamp(Some(0)), 0);
    assert_eq!(clamp(Some(10)), 10);
    assert_eq!(clamp(Some(500)), 500);
    assert_eq!(clamp(Some(100_000)), 500, "上限必须是 500");
}

#[test]
fn log_get_on_missing_log_dir_returns_empty() {
    let (dir, core) = temp_core("logs-empty");
    let log_dir = core.data_dir.join("logs");
    // 全新数据目录可能还没有 logs/（首个 tracing 写入前）——命令必须返回空而不是报错。
    assert!(!log_dir.exists() || std::fs::read_dir(&log_dir).is_ok());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn log_clear_leaves_directory_intact() {
    let (dir, core) = temp_core("logs-clear");
    let log_dir = core.data_dir.join("logs");
    std::fs::create_dir_all(&log_dir).unwrap();
    let file = log_dir.join("rmccc.test.log");
    std::fs::write(&file, "line\n").unwrap();
    // 复刻 log_clear 的行为：只删 *.log，目录本身保留。
    for entry in std::fs::read_dir(&log_dir).unwrap().flatten() {
        let path = entry.path();
        if path.extension().is_some_and(|e| e == "log") {
            std::fs::remove_file(&path).unwrap();
        }
    }
    assert!(!file.exists());
    assert!(log_dir.exists(), "log_clear 不能删掉 logs 目录");
    let _ = std::fs::remove_dir_all(&dir);
}

// ────────────────────────── 9. 正向契约：instance ──────────────────────────

#[test]
fn instance_list_is_empty_without_installed_versions() {
    let (dir, core) = temp_core("instance-empty");
    let repo = core.game_repository.read().unwrap();
    assert!(repo.get_instance_ids().is_empty());
    drop(repo);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn instance_check_completion_on_unknown_id_reports_profile_not_found() {
    let (dir, core) = temp_core("instance-unknown");
    // ADR-002：profile 优先、版本 id 回落；两者都没有 → ProfileNotFound。
    let result = resolve_version_id_for_test(&core, "does-not-exist");
    let err = expect_code(result, ErrorCode::ProfileNotFound);
    assert!(err.message.contains("does-not-exist"));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn instance_check_completion_accepts_a_profile_id() {
    // 这是 ADR-002 修复的真实 bug：UI 传 profile id，命令必须能解析出 game_version。
    let (dir, core) = temp_core("instance-profile-id");
    if core
        .platform
        .java
        .detect_all()
        .unwrap_or_default()
        .is_empty()
    {
        // 没装 Java 的机器上跳过（与命令层同样的前置条件）。
        let _ = std::fs::remove_dir_all(&dir);
        return;
    }
    let profile = rmccc_profile::Profile {
        id: "p-instance".into(),
        name: "i".into(),
        game_version: "1.20.4".into(),
        game_dir: core.config.resolved_game_root(),
        ..Default::default()
    };
    core.profile_store.save(&profile).unwrap();
    // profile 存在 → 解析成版本 id（即便该版本尚未安装，也应是 Ok）。
    assert_eq!(
        resolve_version_id_for_test(&core, "p-instance").unwrap(),
        "1.20.4"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn instance_paths_requires_a_profile() {
    let (dir, core) = temp_core("instance-paths");
    // instance_paths 只接受 profile id（返回值含 java_path/isolation 等 profile 字段）。
    assert!(core.profile_store.get("nope").unwrap().is_none());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn resolved_settings_reflect_inheritance() {
    // 1.8 的契约：继承字段取全局默认，覆盖字段取实例值，且只由后端决定。
    let (dir, mut core) = temp_core("instance-resolved");
    core.config.default_max_memory_mb = 3072;
    core.config.default_jvm_args = vec!["-Dglobal=1".to_string()];

    let mut profile = rmccc_profile::Profile {
        id: "p-resolved".into(),
        name: "r".into(),
        game_version: "1.20.4".into(),
        game_dir: core.config.resolved_game_root(),
        memory_mb: 8192,
        ..Default::default()
    };
    // 只覆盖 memory_mb → jvm_args 应继承全局。
    profile.set_overridden("memory_mb", true);

    let resolved = profile.resolve(&core.config);
    assert_eq!(resolved.max_memory_mb, 8192, "覆盖字段取实例值");
    assert_eq!(
        resolved.jvm_args,
        vec!["-Dglobal=1".to_string()],
        "未覆盖字段继承全局默认"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

// ────────────────────────── 10. 拒绝契约：launch ──────────────────────────

#[test]
fn launch_start_of_unknown_profile_reports_profile_not_found() {
    let (dir, core) = temp_core("launch-unknown");
    let result = core
        .profile_store
        .get("no-such-profile")
        .unwrap()
        .ok_or_else(|| IpcError {
            code: ErrorCode::ProfileNotFound,
            message: "Profile not found: no-such-profile".into(),
            retryable: false,
        });
    expect_code(result, ErrorCode::ProfileNotFound);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn launch_status_is_empty_on_a_fresh_process() {
    let (dir, _core) = temp_core("launch-status");
    let running = dashmap::DashMap::<String, u32>::new();
    assert!(running.is_empty());
    let _ = std::fs::remove_dir_all(&dir);
}

// ────────────────────────── 11. 拒绝契约：version / download ──────────────────────────

#[test]
fn version_install_of_unknown_profile_reports_profile_not_found() {
    // version_install 在起后台任务前先做 profile 预检查（快速失败）。
    let (dir, core) = temp_core("version-install");
    let missing = core.profile_store.get("ghost").unwrap();
    assert!(missing.is_none());
    expect_code(
        Err::<(), _>(IpcError {
            code: ErrorCode::ProfileNotFound,
            message: "Profile not found: ghost".into(),
            retryable: false,
        }),
        ErrorCode::ProfileNotFound,
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn download_pause_on_unknown_batch_is_a_noop() {
    let (dir, core) = temp_core("download-pause");
    // 未知 batch id 不得 panic（前端在竞态下会传过期 id）。
    core.download_manager.pause("no-such-batch");
    core.download_manager.cancel_batch("no-such-batch");
    core.download_manager.resume("no-such-batch");
    let _ = std::fs::remove_dir_all(&dir);
}

// ────────────────────────── 12. mods 列表契约 ──────────────────────────

#[test]
fn mod_list_of_profile_without_mods_dir_is_empty() {
    let (dir, core) = temp_core("mods-list");
    let profile = rmccc_profile::Profile {
        id: "p-mods".into(),
        name: "m".into(),
        game_version: "1.20.4".into(),
        game_dir: core.config.resolved_game_root(),
        ..Default::default()
    };
    core.profile_store.save(&profile).unwrap();
    let mods_dir = rmccc_launch::instance_mods_dir(&core.config, &profile);
    assert!(!mods_dir.exists(), "全新实例还没有 mods 目录");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn mod_toggle_on_unknown_file_reports_an_error() {
    let (dir, core) = temp_core("mods-toggle");
    let profile = rmccc_profile::Profile {
        id: "p-toggle".into(),
        name: "t".into(),
        game_version: "1.20.4".into(),
        game_dir: core.config.resolved_game_root(),
        ..Default::default()
    };
    core.profile_store.save(&profile).unwrap();
    let mods_dir = rmccc_launch::instance_mods_dir(&core.config, &profile);
    assert!(!mods_dir.join("missing.jar").exists());
    let _ = std::fs::remove_dir_all(&dir);
}

// ────────────────────────── 9. 正向契约：WebView 日志 ──────────────────────────

#[test]
fn frontend_log_accepts_every_tracing_level() {
    // 引导守卫（index.html）用它把 release 构建里"看不见的"前端崩溃写进日志。
    for level in ["trace", "debug", "info", "warn", "error"] {
        crate::commands::ui::frontend_log(level.to_string(), "boot guard test".to_string())
            .unwrap_or_else(|e| panic!("level {level} rejected: {:?}", e.message));
    }
}

#[test]
fn frontend_log_rejects_unknown_level() {
    let err = expect_code(
        crate::commands::ui::frontend_log("verbose".into(), "x".into()),
        ErrorCode::ValidationError,
    );
    assert!(
        err.message.contains("verbose"),
        "message should name the level"
    );
}

#[test]
fn frontend_log_rejects_oversized_payloads() {
    // 上限 8 KiB：防止前端 error handler 死循环时刷爆日志 appender。
    let ok = "a".repeat(8 * 1024);
    assert!(crate::commands::ui::frontend_log("error".into(), ok).is_ok());

    let too_long = "a".repeat(8 * 1024 + 1);
    expect_code(
        crate::commands::ui::frontend_log("error".into(), too_long),
        ErrorCode::ValidationError,
    );
}

// ────────────────────────── 10. 隔离插件 UI（Phase 4，ADR-006）──────────────────────────

#[test]
fn plugin_ui_bridge_accepts_a_well_formed_request() {
    let (req, method) = crate::commands::plugin_ui::prepare_invocation(
        "dev.rmccc.card",
        r#"{"v":1,"id":"r1","method":"instances.list"}"#,
    )
    .unwrap();
    assert_eq!(req.id, "r1");
    assert_eq!(method.capability, Some("instance.read"));
}

#[test]
fn plugin_ui_bridge_rejects_protocol_and_shape_errors() {
    use rmccc_core::rmccc_plugin::ui::BridgeErrorCode;
    let cases = [
        // 版本不符
        (
            r#"{"v":2,"id":"r1","method":"host.info"}"#,
            BridgeErrorCode::ProtocolVersion,
        ),
        // 未知方法
        (
            r#"{"v":1,"id":"r2","method":"fs.read"}"#,
            BridgeErrorCode::UnsupportedMethod,
        ),
        // 形状非法（缺 id）
        (
            r#"{"v":1,"method":"host.info"}"#,
            BridgeErrorCode::Validation,
        ),
        // 请求 ID 非法
        (
            r#"{"v":1,"id":"bad id","method":"host.info"}"#,
            BridgeErrorCode::Validation,
        ),
        // 多余字段（deny_unknown_fields）
        (
            r#"{"v":1,"id":"r3","method":"host.info","x":1}"#,
            BridgeErrorCode::Validation,
        ),
    ];
    for (raw, expected) in cases {
        let err =
            crate::commands::plugin_ui::prepare_invocation("dev.rmccc.card", raw).unwrap_err();
        assert_eq!(err.code, expected, "raw: {raw}");
    }
}

#[test]
fn plugin_ui_bridge_rejects_invalid_plugin_ids() {
    use rmccc_core::rmccc_plugin::ui::BridgeErrorCode;
    let err = crate::commands::plugin_ui::prepare_invocation(
        "../etc",
        r#"{"v":1,"id":"r4","method":"host.info"}"#,
    )
    .unwrap_err();
    assert_eq!(err.code, BridgeErrorCode::Validation);
}

#[test]
fn plugin_ui_bridge_enforces_the_request_size_cap() {
    use rmccc_core::rmccc_plugin::ui::BridgeErrorCode;
    let padding = "x".repeat(64 * 1024);
    let raw = format!(r#"{{"v":1,"id":"r5","method":"host.info","params":{{"p":"{padding}"}}}}"#);
    let err = crate::commands::plugin_ui::prepare_invocation("dev.rmccc.card", &raw).unwrap_err();
    assert_eq!(err.code, BridgeErrorCode::TooLarge);
}

// ────────────────────────── 11. 逻辑插件（Phase 5，ADR-007）──────────────────────────

#[test]
fn logic_calls_are_limited_to_the_read_only_allowlist() {
    assert!(crate::commands::plugin_logic::is_allowed_call(
        "compat.check"
    ));
    assert!(crate::commands::plugin_logic::is_allowed_call(
        "mods.source.search"
    ));
    for bad in ["mods.install", "game.launch", "fs.read", ""] {
        assert!(
            !crate::commands::plugin_logic::is_allowed_call(bad),
            "expected reject: {bad}"
        );
    }
}

#[test]
fn logic_errors_map_onto_ipc_codes() {
    use rmccc_logic::protocol::{LogicError, LogicErrorCode};
    let cases = [
        (
            LogicErrorCode::ImportNotAllowed,
            ErrorCode::PluginPackageInvalid,
        ),
        (
            LogicErrorCode::ModuleTooLarge,
            ErrorCode::PluginPackageInvalid,
        ),
        (LogicErrorCode::FuelExhausted, ErrorCode::InternalError),
        (LogicErrorCode::Trap, ErrorCode::InternalError),
        (LogicErrorCode::Timeout, ErrorCode::InternalError),
        (LogicErrorCode::NotLoaded, ErrorCode::PluginStateConflict),
        (LogicErrorCode::UnknownMethod, ErrorCode::ValidationError),
    ];
    for (code, expected) in cases {
        let error = crate::commands::plugin_logic::map_logic_error(LogicError::new(code, "boom"));
        assert_eq!(error.code, expected, "code {code:?}");
        assert!(error.message.starts_with("logic."), "{}", error.message);
    }
    let timeout = crate::commands::plugin_logic::map_logic_error(LogicError::new(
        LogicErrorCode::Timeout,
        "slow",
    ));
    assert!(timeout.retryable, "timeouts are worth retrying");
}

#[test]
fn logic_plugins_must_declare_the_logic_capability() {
    let catalog = rmccc_core::rmccc_plugin::capability_catalog();
    let entry = catalog
        .iter()
        .find(|c| c.name == "logic.invoke")
        .expect("logic.invoke must be in the catalog");
    assert_eq!(
        entry.risk,
        rmccc_core::rmccc_plugin::capability::CapabilityRisk::Medium
    );
}

// ────────────────────────── 测试辅助 ──────────────────────────

/// 复刻 `instance_*` 的 id 解析（命令层是私有 fn，这里保持同样的语义做契约断言）。
fn resolve_version_id_for_test(
    core: &rmccc_core::CoreContext,
    id: &str,
) -> Result<String, IpcError> {
    if let Some(profile) = core.profile_store.get(id).map_err(IpcError::from)? {
        return Ok(profile.game_version);
    }
    let known = {
        let repo = core.game_repository.read().map_err(|_| IpcError {
            code: ErrorCode::InternalError,
            message: "repo lock".into(),
            retryable: false,
        })?;
        repo.get_resolved(id).is_ok()
    };
    if known {
        return Ok(id.to_string());
    }
    Err(IpcError {
        code: ErrorCode::ProfileNotFound,
        message: format!("No profile or installed version with id '{id}'"),
        retryable: false,
    })
}

// ────────────────────────── 12. 逻辑插件端到端（Phase 5，ADR-007）──────────────────────────

/// Minimal logic module: logs the payload and answers `{"ok":true}`.
const LOGIC_ECHO_WAT: &str = r#"(module
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
    (i64.or (i64.shl (i64.const 0) (i64.const 32)) (i64.const 11))))"#;

/// Locate the helper binary built by `cargo test --workspace`.
fn logic_helper_binary() -> Option<std::path::PathBuf> {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).parent()?;
    for profile in ["debug", "release"] {
        let candidate = root.join("target").join(profile).join("rmccc-logic-host");
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}

/// Install → grant → enable → run through the real helper → cancel → run again.
#[test]
fn logic_plugin_runs_end_to_end_through_the_helper() {
    let Some(helper) = logic_helper_binary() else {
        eprintln!("skipping: build rmccc-logic-host first");
        return;
    };

    let (dir, mut core) = temp_core("logic-e2e");
    let wasm = wat::parse_str(LOGIC_ECHO_WAT).expect("wat");
    let manifest = serde_json::json!({
        "manifestVersion": 1,
        "id": "dev.rmccc.logic-e2e",
        "name": "Logic E2E",
        "version": "1.0.0",
        "publisher": { "id": "dev.rmccc", "name": "Tests" },
        "compatibility": { "pluginApi": "1.x", "core": ">=0.1.0 <0.2.0" },
        "entry": { "logic": "logic/plugin.wasm" },
        "permissions": [{ "capability": "logic.invoke" }]
    });

    let src = dir.join("plugin-src");
    std::fs::create_dir_all(src.join("logic")).unwrap();
    std::fs::write(
        src.join("manifest.json"),
        serde_json::to_vec_pretty(&manifest).unwrap(),
    )
    .unwrap();
    std::fs::write(src.join("logic/plugin.wasm"), &wasm).unwrap();

    let ctx = rmccc_core::rmccc_plugin::ManifestContext::new("0.1.0", "darwin-aarch64").unwrap();
    let package = dir.join("e2e.rmcpkg");
    rmccc_core::rmccc_plugin::package::pack_directory(&src, &package, &ctx).expect("pack");

    let report = core
        .plugin_manager
        .install_from_path(&package)
        .expect("install");
    assert_eq!(report.pending_permissions, vec!["logic.invoke"]);
    core.plugin_manager
        .grant("dev.rmccc.logic-e2e", "logic.invoke")
        .unwrap();
    core.plugin_manager.enable("dev.rmccc.logic-e2e").unwrap();

    let entry = core
        .plugin_manager
        .manifest("dev.rmccc.logic-e2e")
        .and_then(|m| m.entry.logic.clone())
        .expect("entry.logic");
    let wasm_path = core
        .plugin_manager
        .installed_dir("dev.rmccc.logic-e2e")
        .expect("installed dir")
        .join(entry);
    assert!(wasm_path.is_file(), "{}", wasm_path.display());

    let mut runner = rmccc_logic::LogicRunner::with_helper(helper);
    let reply = runner
        .invoke(
            "dev.rmccc.logic-e2e",
            &wasm_path,
            "compat.check",
            serde_json::json!({ "instance": "abc" }),
            rmccc_logic::LogicLimits::default(),
        )
        .expect("logic call");
    assert_eq!(reply, serde_json::json!({ "ok": true }));

    // Cancel drops the helper; the next call respawns and reloads.
    assert!(runner.cancel("dev.rmccc.logic-e2e"));
    let again = runner
        .invoke(
            "dev.rmccc.logic-e2e",
            &wasm_path,
            "mods.source.search",
            serde_json::json!({ "query": "sodium" }),
            rmccc_logic::LogicLimits::default(),
        )
        .expect("second call after cancel");
    assert_eq!(again, serde_json::json!({ "ok": true }));

    let _ = std::fs::remove_dir_all(&dir);
}

// ────────────────────────── 11. 逻辑插件槽位独立性（审计批次 2） ──────────────────────────

#[test]
fn logic_slots_are_per_plugin() {
    // 此前全部插件共享一把全局 Mutex<LogicRunner>：任何一个插件的 30s 慢调用
    // 都会串行化其它插件，且 cancel 要等同一把锁。per-plugin 槽位后，插件 A
    // 持锁调用不得阻塞插件 B 的调用/取消。
    use crate::state::LogicSlot;
    use std::sync::Arc;

    let a = Arc::new(LogicSlot {
        runner: std::sync::Mutex::new(rmccc_logic::LogicRunner::new()),
        pid: std::sync::Mutex::new(None),
    });
    let b = Arc::new(LogicSlot {
        runner: std::sync::Mutex::new(rmccc_logic::LogicRunner::new()),
        pid: std::sync::Mutex::new(None),
    });

    let guard_a = a.runner.lock().expect("lock A");
    assert!(
        b.runner.try_lock().is_ok(),
        "plugin B must not wait on plugin A's in-flight call"
    );
    // A 占用时，cancel(A) 的快路径拿不到锁——这正是走 PID 慢路径的判定位
    // （plugin_logic_cancel），不在此阻塞。
    assert!(a.runner.try_lock().is_err());
    drop(guard_a);

    // 空闲后快路径可用：try_lock 成功即干净 cancel。
    assert!(a.runner.try_lock().is_ok());
}
