//! 插件管理器集成测试：安装事务/状态机/权限/依赖/损坏隔离/配置。

use super::*;
use crate::manifest::PluginManifest;
use crate::registry::{PluginRecord, PluginState, TrustLevel};
use rmccc_platform::Platform;
use serde_json::json;

fn temp_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "rmccc-plugin-manager-{tag}-{}",
        uuid::Uuid::new_v4()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn manager(dir: &Path) -> PluginManager {
    let platform = Platform::mock().0;
    PluginManager::load(dir, "0.1.0", "darwin-aarch64", platform.fs)
}

fn manifest(id: &str, version: &str) -> serde_json::Value {
    json!({
        "manifestVersion": 1,
        "id": id,
        "name": format!("Test {id}"),
        "version": version,
        "publisher": { "id": "dev.rmccc", "name": "Tests" },
        "compatibility": { "pluginApi": "1.x", "core": ">=0.1.0 <0.2.0" }
    })
}

fn pack(manifest: &serde_json::Value, extra: &[(&str, &str)]) -> Vec<u8> {
    let mut buf = std::io::Cursor::new(Vec::new());
    {
        let mut zip = zip::ZipWriter::new(&mut buf);
        let opts = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated);
        zip.start_file("manifest.json", opts).unwrap();
        std::io::Write::write_all(
            &mut zip,
            serde_json::to_string_pretty(manifest).unwrap().as_bytes(),
        )
        .unwrap();
        for (path, content) in extra {
            zip.start_file(*path, opts).unwrap();
            std::io::Write::write_all(&mut zip, content.as_bytes()).unwrap();
        }
        zip.finish().unwrap();
    }
    buf.into_inner()
}

fn install(mgr: &mut PluginManager, m: &serde_json::Value) -> InstallReport {
    mgr.install_from_bytes(&pack(m, &[]), "test.rmcpkg", false)
        .unwrap()
}

#[test]
fn installs_disabled_and_auto_grants_low_risk() {
    let dir = temp_dir("install");
    let mut mgr = manager(&dir);
    let mut m = manifest("dev.rmccc.alpha", "1.0.0");
    m["permissions"] = json!([
        { "capability": "dashboard.cards" },
        { "capability": "network.request", "hosts": ["api.example.com"] }
    ]);
    let report = install(&mut mgr, &m);
    assert_eq!(report.state, PluginState::InstalledDisabled);
    assert!(report.requires_review);
    assert_eq!(report.pending_permissions, vec!["network.request"]);

    let summary = mgr.list(false).into_iter().next().unwrap();
    assert_eq!(summary.id, "dev.rmccc.alpha");
    assert!(summary
        .granted_permissions
        .contains(&"dashboard.cards".to_string()));
    assert!(!summary
        .granted_permissions
        .contains(&"network.request".to_string()));

    assert!(dir
        .join("plugins/dev.rmccc.alpha/1.0.0/manifest.json")
        .is_file());
    assert!(dir.join("plugin-state/registry.json").is_file());
    assert!(dir.join("plugin-state/permissions.json").is_file());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn enable_requires_permission_review() {
    let dir = temp_dir("enable");
    let mut mgr = manager(&dir);
    let mut m = manifest("dev.rmccc.beta", "1.0.0");
    m["permissions"] = json!([{ "capability": "mods.install" }]);
    install(&mut mgr, &m);

    let err = mgr.enable("dev.rmccc.beta").unwrap_err();
    assert!(
        matches!(
            err,
            rmccc_error::RmcccError::Plugin {
                code: PluginErrorCode::PermissionPending,
                ..
            }
        ),
        "{err}"
    );

    mgr.grant("dev.rmccc.beta", "mods.install").unwrap();
    let summary = mgr.enable("dev.rmccc.beta").unwrap();
    assert_eq!(summary.state, PluginState::Enabled);

    // 撤销后授权立即失效（插件仍启用，但调用被拒绝）。
    mgr.revoke("dev.rmccc.beta", "mods.install").unwrap();
    assert!(mgr
        .authorize("dev.rmccc.beta", "mods.install", None, None)
        .is_err());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn disable_blocks_when_enabled_dependent_exists() {
    let dir = temp_dir("deps-disable");
    let mut mgr = manager(&dir);
    install(&mut mgr, &manifest("dev.rmccc.base", "1.0.0"));
    let mut dependent = manifest("dev.rmccc.dep", "1.0.0");
    dependent["extensionDependencies"] = json!([{ "id": "dev.rmccc.base", "version": "^1.0.0" }]);
    install(&mut mgr, &dependent);

    mgr.enable("dev.rmccc.base").unwrap();
    mgr.enable("dev.rmccc.dep").unwrap();
    let err = mgr.disable("dev.rmccc.base").unwrap_err();
    assert!(err.to_string().contains("dev.rmccc.dep"), "{err}");

    mgr.disable("dev.rmccc.dep").unwrap();
    mgr.disable("dev.rmccc.base").unwrap();
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn uninstall_keeps_config_by_default_and_blocks_dependents() {
    let dir = temp_dir("uninstall");
    let mut mgr = manager(&dir);
    let mut m = manifest("dev.rmccc.gamma", "1.0.0");
    m["contributes"] = json!({
        "themes": [{ "id": "midnight", "name": "Midnight", "path": "themes/midnight.json" }]
    });
    let bytes = pack(&m, &[("themes/midnight.json", "{}")]);
    mgr.install_from_bytes(&bytes, "test.rmcpkg", false)
        .unwrap();

    // 配置目录（即使没有 schema，测试手动放入数据）。
    let data_dir = dir.join("plugin-data/dev.rmccc.gamma");
    std::fs::create_dir_all(&data_dir).unwrap();
    std::fs::write(data_dir.join("config.json"), "{}").unwrap();

    mgr.uninstall("dev.rmccc.gamma", false).unwrap();
    assert!(data_dir.exists(), "默认卸载必须保留用户数据");
    assert!(!dir.join("plugins/dev.rmccc.gamma").exists());
    assert!(mgr.list(false).is_empty());
    assert!(mgr
        .list(true)
        .iter()
        .any(|p| p.state == PluginState::Removed));

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn upgrade_keeps_previous_version_and_new_permissions_pending() {
    let dir = temp_dir("upgrade");
    let mut mgr = manager(&dir);
    let m1 = manifest("dev.rmccc.delta", "1.0.0");
    install(&mut mgr, &m1);
    mgr.enable("dev.rmccc.delta").unwrap();

    let mut m2 = manifest("dev.rmccc.delta", "1.1.0");
    m2["permissions"] = json!([{ "capability": "logs.read" }]);
    let report = install(&mut mgr, &m2);
    assert_eq!(report.previous_version.as_deref(), Some("1.0.0"));
    assert_eq!(report.state, PluginState::InstalledDisabled);
    assert!(report.requires_review);
    assert!(dir.join("plugins/dev.rmccc.delta/1.0.0").is_dir());
    assert!(dir.join("plugins/dev.rmccc.delta/1.1.0").is_dir());

    // 回滚恢复旧版本，并清理新版本才有的授权。
    let summary = mgr.rollback("dev.rmccc.delta").unwrap();
    assert_eq!(summary.version, "1.0.0");
    assert_eq!(summary.state, PluginState::InstalledDisabled);
    assert!(!mgr
        .permissions()
        .file()
        .is_granted("dev.rmccc.delta", "logs.read"));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn rejects_downgrade_and_same_version_reinstall() {
    let dir = temp_dir("conflict");
    let mut mgr = manager(&dir);
    install(&mut mgr, &manifest("dev.rmccc.eps", "1.1.0"));
    let err = mgr
        .install_from_bytes(&pack(&manifest("dev.rmccc.eps", "1.1.0"), &[]), "x", false)
        .unwrap_err();
    assert!(err.to_string().contains("已安装"), "{err}");
    let err = mgr
        .install_from_bytes(&pack(&manifest("dev.rmccc.eps", "1.0.0"), &[]), "x", false)
        .unwrap_err();
    assert!(err.to_string().contains("降级"), "{err}");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn rejects_reserved_namespace() {
    let dir = temp_dir("reserved");
    let mut mgr = manager(&dir);
    let err = mgr
        .install_from_bytes(
            &pack(&manifest("org.rmccc.official", "1.0.0"), &[]),
            "x",
            false,
        )
        .unwrap_err();
    assert!(err.to_string().contains("保留命名空间"), "{err}");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn missing_dependency_is_rejected() {
    let dir = temp_dir("missing-dep");
    let mut mgr = manager(&dir);
    let mut m = manifest("dev.rmccc.needs", "1.0.0");
    m["extensionDependencies"] = json!([{ "id": "dev.rmccc.absent", "version": "^1.0.0" }]);
    let err = mgr
        .install_from_bytes(&pack(&m, &[]), "x", false)
        .unwrap_err();
    assert!(err.to_string().contains("缺少必需依赖"), "{err}");

    // 可选依赖缺失只记 warning，不阻断安装。
    let mut m = manifest("dev.rmccc.needs", "1.0.0");
    m["extensionDependencies"] =
        json!([{ "id": "dev.rmccc.absent", "version": "^1.0.0", "optional": true }]);
    mgr.install_from_bytes(&pack(&m, &[]), "x", false).unwrap();
    assert!(mgr
        .diagnostics()
        .iter()
        .any(|d| d.message.contains("可选依赖")));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn dependency_cycle_is_rejected() {
    let dir = temp_dir("cycle");
    let mut mgr = manager(&dir);
    let mut a = manifest("dev.rmccc.a", "1.0.0");
    a["extensionDependencies"] = json!([{ "id": "dev.rmccc.b", "version": "^1.0.0" }]);
    // 先装 b（无依赖）。
    install(&mut mgr, &manifest("dev.rmccc.b", "1.0.0"));
    // 手工把 b 改成依赖 a（模拟已安装集合中的环）。
    let b_dir = dir.join("plugins/dev.rmccc.b/1.0.0");
    let mut b = manifest("dev.rmccc.b", "1.0.0");
    b["extensionDependencies"] = json!([{ "id": "dev.rmccc.a", "version": "^1.0.0" }]);
    std::fs::write(
        b_dir.join("manifest.json"),
        serde_json::to_string_pretty(&b).unwrap(),
    )
    .unwrap();
    // 重新加载后 b 仍然有效（a 未安装时依赖不满足不影响加载）。
    let mut mgr = manager(&dir);
    let err = mgr
        .install_from_bytes(&pack(&a, &[]), "x", false)
        .unwrap_err();
    assert!(err.to_string().contains("环"), "{err}");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn broken_plugin_does_not_block_others() {
    let dir = temp_dir("broken");
    let mut mgr = manager(&dir);
    install(&mut mgr, &manifest("dev.rmccc.good", "1.0.0"));

    // 手工写一个损坏的已注册插件。
    let bad_dir = dir.join("plugins/dev.rmccc.bad/1.0.0");
    std::fs::create_dir_all(&bad_dir).unwrap();
    std::fs::write(bad_dir.join("manifest.json"), b"{not json").unwrap();
    let mut registry: PluginRegistry =
        serde_json::from_slice(&std::fs::read(dir.join("plugin-state/registry.json")).unwrap())
            .unwrap();
    let now = "2026-10-21T00:00:00Z".to_string();
    registry.insert(PluginRecord {
        id: "dev.rmccc.bad".into(),
        installed_version: "1.0.0".into(),
        previous_version: None,
        state: PluginState::InstalledDisabled,
        trust: TrustLevel::Development,
        source: PluginSource::local("test"),
        installed_at: now.clone(),
        updated_at: now,
        manifest_sha256: None,
        last_error: None,
    });
    std::fs::write(
        dir.join("plugin-state/registry.json"),
        serde_json::to_vec_pretty(&registry).unwrap(),
    )
    .unwrap();

    let mgr = manager(&dir);
    let list = mgr.list(true);
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].id, "dev.rmccc.good");
    assert!(mgr
        .diagnostics()
        .iter()
        .any(|d| d.plugin_id.as_deref() == Some("dev.rmccc.bad")));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn corrupt_registry_is_backed_up() {
    let dir = temp_dir("corrupt-registry");
    std::fs::create_dir_all(dir.join("plugin-state")).unwrap();
    std::fs::write(dir.join("plugin-state/registry.json"), b"not json").unwrap();
    let mgr = manager(&dir);
    assert!(mgr.list(true).is_empty());
    assert!(mgr.diagnostics().iter().any(|d| d.message.contains("损坏")));
    let backups: Vec<_> = std::fs::read_dir(dir.join("plugin-state"))
        .unwrap()
        .filter_map(|e| e.ok())
        .filter(|e| {
            e.file_name()
                .to_string_lossy()
                .contains("registry.json.bak-")
        })
        .collect();
    assert_eq!(backups.len(), 1);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn contributions_only_from_enabled_plugins_and_namespaced() {
    let dir = temp_dir("contrib");
    let mut mgr = manager(&dir);
    let mut m = manifest("dev.rmccc.cards", "1.0.0");
    m["permissions"] = json!([
        { "capability": "dashboard.cards" },
        { "capability": "ui.pages" }
    ]);
    m["contributes"] = json!({
        "dashboardCards": [{
            "id": "status", "title": "Status",
            "defaultSize": { "w": 4, "h": 2 }, "minSize": { "w": 2, "h": 1 }
        }],
        "settingsPages": [{ "id": "settings", "title": "Cards" }]
    });
    install(&mut mgr, &m);
    assert!(mgr.contributions().dashboard_cards.is_empty());

    mgr.enable("dev.rmccc.cards").unwrap();
    let c = mgr.contributions();
    assert_eq!(c.dashboard_cards.len(), 1);
    assert_eq!(c.dashboard_cards[0].key, "dev.rmccc.cards/status");
    assert_eq!(c.settings_pages[0].key, "dev.rmccc.cards/settings");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn config_is_validated_and_reset() {
    let dir = temp_dir("config");
    let mut mgr = manager(&dir);
    let schema = json!({
        "type": "object",
        "additionalProperties": false,
        "required": ["server"],
        "properties": {
            "server": { "type": "string", "minLength": 1, "default": "play.example.net" },
            "port": { "type": "integer", "default": 25565 }
        }
    });
    let bytes = pack(
        &manifest("dev.rmccc.cfg", "1.0.0"),
        &[(
            "config.schema.json",
            &serde_json::to_string(&schema).unwrap(),
        )],
    );
    mgr.install_from_bytes(&bytes, "x", false).unwrap();

    let (cfg, got_schema) = mgr.config_get("dev.rmccc.cfg").unwrap();
    assert_eq!(cfg.values["server"], "play.example.net");
    assert!(got_schema.is_some());

    mgr.config_set(
        "dev.rmccc.cfg",
        json!({ "server": "mc.example.net", "port": 25566 }),
    )
    .unwrap();
    assert!(mgr
        .config_set("dev.rmccc.cfg", json!({ "server": "", "port": 25566 }))
        .is_err());
    assert!(mgr
        .config_set("dev.rmccc.cfg", json!({ "server": "x", "extra": 1 }))
        .is_err());

    let cfg = mgr.config_reset("dev.rmccc.cfg").unwrap();
    assert_eq!(cfg.values["server"], "play.example.net");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn manifest_mismatch_with_directory_is_detected_on_load() {
    let dir = temp_dir("mismatch");
    let mut mgr = manager(&dir);
    install(&mut mgr, &manifest("dev.rmccc.eta", "1.0.0"));
    // 把目录里的 manifest 换成另一个 id。
    std::fs::write(
        dir.join("plugins/dev.rmccc.eta/1.0.0/manifest.json"),
        serde_json::to_string_pretty(&manifest("dev.rmccc.other", "1.0.0")).unwrap(),
    )
    .unwrap();
    let mgr = manager(&dir);
    assert!(mgr.list(true).is_empty());
    assert!(mgr
        .diagnostics()
        .iter()
        .any(|d| d.message.contains("不一致")));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn validate_example_manifest_shapes() {
    // 与 examples/plugins 保持同构的最小检查（示例文件由文档测试覆盖）。
    let m = manifest("dev.rmccc.theme-midnight", "1.0.0");
    let parsed = PluginManifest::parse(&serde_json::to_string(&m).unwrap()).unwrap();
    let ctx = ManifestContext::new("0.1.0", "darwin-aarch64").unwrap();
    validate_manifest(&parsed, &ctx).unwrap();
}

#[test]
fn example_plugins_pack_and_install() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/plugins");
    let work = temp_dir("examples");
    let mut mgr = manager(&work);
    let ctx = ManifestContext::new("0.1.0", "darwin-aarch64").unwrap();

    let theme_dir = root.join("theme-midnight");
    let theme_pkg = work.join("theme-midnight.rmcpkg");
    crate::package::pack_directory(&theme_dir, &theme_pkg, &ctx).unwrap();
    let report = mgr.install_from_path(&theme_pkg).unwrap();
    assert_eq!(report.id, "dev.rmccc.theme-midnight");
    assert!(!report.requires_review);
    mgr.enable("dev.rmccc.theme-midnight").unwrap();
    assert_eq!(mgr.contributions().themes.len(), 1);

    let dash_dir = root.join("server-dashboard");
    let dash_pkg = work.join("server-dashboard.rmcpkg");
    crate::package::pack_directory(&dash_dir, &dash_pkg, &ctx).unwrap();
    let report = mgr.install_from_path(&dash_pkg).unwrap();
    assert_eq!(report.id, "dev.rmccc.server-dashboard");
    assert!(report.requires_review);
    assert_eq!(report.pending_permissions, vec!["network.request"]);
    assert_eq!(mgr.contributions().dashboard_cards.len(), 0, "未启用不登记");
    let _ = std::fs::remove_dir_all(&work);
}

#[test]
fn revoking_a_capability_hides_its_contributions() {
    let dir = temp_dir("contrib-revoke");
    let mut mgr = manager(&dir);
    let mut m = manifest("dev.rmccc.theme-revoke", "1.0.0");
    m["permissions"] = json!([{ "capability": "themes.apply" }]);
    m["contributes"] = json!({
        "themes": [{ "id": "night", "name": "Night", "path": "themes/night.json" }]
    });
    let bytes = pack(&m, &[("themes/night.json", "{}")]);
    mgr.install_from_bytes(&bytes, "x", true).unwrap();
    assert_eq!(mgr.contributions().themes.len(), 1);

    mgr.revoke("dev.rmccc.theme-revoke", "themes.apply")
        .unwrap();
    assert!(mgr.contributions().themes.is_empty(), "撤销后立即停止登记");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn malicious_registry_ids_and_versions_never_reach_the_filesystem() {
    let dir = temp_dir("malicious-registry");
    std::fs::create_dir_all(dir.join("plugin-state")).unwrap();
    let now = "2026-10-21T00:00:00Z".to_string();
    let mut registry = PluginRegistry::default();
    registry.insert(PluginRecord {
        id: "../../escape".into(),
        installed_version: "1.0.0".into(),
        previous_version: None,
        state: PluginState::InstalledDisabled,
        trust: TrustLevel::Development,
        source: PluginSource::local("x"),
        installed_at: now.clone(),
        updated_at: now.clone(),
        manifest_sha256: None,
        last_error: None,
    });
    registry.insert(PluginRecord {
        id: "dev.rmccc.badver".into(),
        installed_version: "../../escape".into(),
        previous_version: None,
        state: PluginState::InstalledDisabled,
        trust: TrustLevel::Development,
        source: PluginSource::local("x"),
        installed_at: now.clone(),
        updated_at: now,
        manifest_sha256: None,
        last_error: None,
    });
    std::fs::write(
        dir.join("plugin-state/registry.json"),
        serde_json::to_vec_pretty(&registry).unwrap(),
    )
    .unwrap();

    let mut mgr = manager(&dir);
    assert!(mgr.list(true).is_empty());
    assert!(mgr.diagnostics().len() >= 2);
    assert!(mgr.uninstall("../../escape", false).is_err());
    // 关键：目录没有被当成 plugins/../../escape 处理。
    assert!(dir.join("plugins").is_dir());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn failed_registry_commit_removes_the_new_version_directory() {
    let dir = temp_dir("commit-fail");
    let mut mgr = manager(&dir);
    // 让 registry.json 变成一个目录：原子替换必然失败。
    std::fs::create_dir_all(dir.join("plugin-state/registry.json")).unwrap();

    let m = manifest("dev.rmccc.rollback-commit", "1.0.0");
    let err = mgr
        .install_from_bytes(&pack(&m, &[]), "x", false)
        .unwrap_err();
    assert!(
        err.to_string().contains("IO error") || err.to_string().contains("目录"),
        "{err}"
    );
    assert!(!dir.join("plugins/dev.rmccc.rollback-commit/1.0.0").exists());
    assert!(mgr.list(true).is_empty());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn failed_permission_commit_rolls_registry_back_and_removes_files() {
    let dir = temp_dir("perm-commit-fail");
    let mut mgr = manager(&dir);
    // 让 permissions.json 变成目录：写入必然失败（注册表已先写成功）。
    std::fs::create_dir_all(dir.join("plugin-state/permissions.json")).unwrap();

    let mut m = manifest("dev.rmccc.rollback-perm", "1.0.0");
    m["permissions"] = json!([{ "capability": "dashboard.cards" }]);
    let err = mgr
        .install_from_bytes(&pack(&m, &[]), "x", false)
        .unwrap_err();
    assert!(!err.to_string().is_empty());
    assert!(!dir.join("plugins/dev.rmccc.rollback-perm/1.0.0").exists());
    // 注册表已回滚：没有该插件的记录。
    let raw = std::fs::read(dir.join("plugin-state/registry.json")).unwrap_or_default();
    let text = String::from_utf8_lossy(&raw);
    assert!(!text.contains("dev.rmccc.rollback-perm"), "{text}");
    let _ = std::fs::remove_dir_all(&dir);
}

// ────────────────────────── 隔离 UI（Phase 4，ADR-006）──────────────────────────

fn ui_manifest(id: &str) -> serde_json::Value {
    let mut m = manifest(id, "1.0.0");
    m["entry"] = json!({ "ui": "ui/index.html" });
    m["permissions"] = json!([{ "capability": "dashboard.cards" }]);
    m["contributes"] = json!({
        "dashboardCards": [
            { "id": "status", "title": "Server status", "defaultSize": { "w": 4, "h": 3 } }
        ]
    });
    m
}

#[test]
fn ui_plugin_contributes_an_isolated_frame_url() {
    let dir = temp_dir("ui-url");
    let mut mgr = manager(&dir);
    let bytes = pack(
        &ui_manifest("dev.rmccc.card"),
        &[
            (
                "ui/index.html",
                "<!DOCTYPE html><script src=\"./app.js\"></script>",
            ),
            ("ui/app.js", "export const x = 1;"),
        ],
    );
    mgr.install_from_bytes(&bytes, "card.rmcpkg", true)
        .expect("ui plugin installs");

    let contributions = mgr.contributions();
    assert_eq!(contributions.dashboard_cards.len(), 1);
    let card = &contributions.dashboard_cards[0];
    assert_eq!(card.key, "dev.rmccc.card/status");
    assert_eq!(
        card.ui_url.as_deref(),
        Some("rmcccplugin://dev.rmccc.card/ui/index.html?card=status")
    );
}

#[test]
fn declarative_plugin_has_no_frame_url() {
    let dir = temp_dir("ui-none");
    let mut mgr = manager(&dir);
    let mut m = manifest("dev.rmccc.plain", "1.0.0");
    m["permissions"] = json!([{ "capability": "dashboard.cards" }]);
    m["contributes"] = json!({
        "dashboardCards": [
            { "id": "plain", "title": "Plain", "defaultSize": { "w": 4, "h": 3 } }
        ]
    });
    mgr.install_from_bytes(&pack(&m, &[]), "plain.rmcpkg", true)
        .unwrap();
    let contributions = mgr.contributions();
    assert_eq!(contributions.dashboard_cards.len(), 1);
    assert!(contributions.dashboard_cards[0].ui_url.is_none());
}

#[test]
fn ui_assets_without_an_entry_are_rejected_at_install() {
    let dir = temp_dir("ui-orphan");
    let mut mgr = manager(&dir);
    let mut m = manifest("dev.rmccc.orphan", "1.0.0");
    m["permissions"] = json!([{ "capability": "dashboard.cards" }]);
    let bytes = pack(&m, &[("ui/index.html", "<!DOCTYPE html>")]);
    let err = mgr
        .install_from_bytes(&bytes, "orphan.rmcpkg", false)
        .unwrap_err()
        .to_string();
    assert!(err.contains("entry.ui"), "{err}");
}

#[test]
fn ui_entry_pointing_at_a_missing_file_is_rejected() {
    let dir = temp_dir("ui-missing");
    let mut mgr = manager(&dir);
    let bytes = pack(
        &ui_manifest("dev.rmccc.missing"),
        &[("ui/other.html", "<!DOCTYPE html>")],
    );
    let err = mgr
        .install_from_bytes(&bytes, "missing.rmcpkg", false)
        .unwrap_err()
        .to_string();
    assert!(err.contains("entry.ui"), "{err}");
}

#[test]
fn upgrade_requiring_a_new_low_risk_capability_is_reviewed_not_silently_granted() {
    // Phase 4 才发现的状态：升级新增低风险能力时，它既不在 granted 也不在 pending，
    // 导致插件能启用但永远拿不到该能力（隔离 UI 的 instances.list 就这样被锁死）。
    let dir = temp_dir("upgrade-low");
    let mut mgr = manager(&dir);

    let mut v1 = manifest("dev.rmccc.low", "1.0.0");
    v1["permissions"] = json!([{ "capability": "dashboard.cards" }]);
    mgr.install_from_bytes(&pack(&v1, &[]), "v1.rmcpkg", true)
        .unwrap();

    let mut v2 = manifest("dev.rmccc.low", "1.1.0");
    v2["permissions"] = json!([
        { "capability": "dashboard.cards" },
        { "capability": "instance.read" }
    ]);
    let report = mgr
        .install_from_bytes(&pack(&v2, &[]), "v2.rmcpkg", false)
        .unwrap();

    assert_eq!(report.version, "1.1.0");
    assert_eq!(report.state, PluginState::InstalledDisabled);
    assert!(report.requires_review, "新增能力必须要求审查");
    assert_eq!(report.pending_permissions, vec!["instance.read"]);
    assert!(
        !mgr.permissions()
            .file()
            .is_granted("dev.rmccc.low", "instance.read"),
        "升级不得自动授予新能力"
    );

    // 授权后可启用，且能力真的生效。
    mgr.grant("dev.rmccc.low", "instance.read").unwrap();
    mgr.enable("dev.rmccc.low").unwrap();
    mgr.authorize("dev.rmccc.low", "instance.read", None, None)
        .expect("granted capability must authorize");
}
