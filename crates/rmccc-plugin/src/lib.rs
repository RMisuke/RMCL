//! 声明式插件系统（Phase 2）+ 隔离 UI（Phase 4）。
//!
//! Phase 2 只支持数据型插件（主题/翻译/声明式卡片）；Phase 4 起允许插件携带
//! `ui/` 静态资源，在独立源 `rmcccplugin://<id>/` 的 sandbox iframe 中执行，
//! 通过版本化 Host Bridge 取数据（ADR-006）。`entry.logic` 仍然拒绝（Phase 5）。
//!
//! 分层：
//!
//! | 模块 | 职责 |
//! |---|---|
//! | [`manifest`] | Manifest v1 解析 + 12 条语义校验 |
//! | [`version`] | 严格 SemVer + 受限范围语法 |
//! | [`capability`] | 能力清单、风险等级、host/IP 策略 |
//! | [`package`] | `.rmcpkg` 包级校验与解压限额 |
//! | [`installer`] | staging → verify → commit 的暂存区 |
//! | [`registry`] | `registry.json` + 生命周期状态机 |
//! | [`permissions`] | 授予/撤销/复核/审计 |
//! | [`config`] | `plugin-data/<id>/config.json` + schema 子集 |
//! | [`manager`] | 对 CLI/Tauri 暴露的窄门面 |
//! | [`ui`] | 隔离 UI 资源白名单、CSP、版本化 Host Bridge 协议 |
//!
//! 平台归属：路径校验、zip 解压是 OS 无关逻辑；**原子替换/权限**经
//! [`rmccc_platform::PlatformFs`]，不在本 crate 内写 `#[cfg]`（ADR-003 决策 7）。

pub mod capability;
pub mod config;
pub mod installer;
pub mod manager;
pub mod manifest;
pub mod package;
pub mod path;
pub mod paths;
pub mod permissions;
pub mod registry;
pub mod storage;
pub mod ui;
pub mod version;

pub use capability::{Capability, CapabilityRisk, CAPABILITIES};
pub use config::PluginConfig;
pub use manager::{
    capability_catalog, CapabilityInfo, DashboardCardView, DependencyStatus, InstallReport,
    PluginContributions, PluginDetail, PluginDiagnostic, PluginManager, PluginSummary,
    SettingsPageView, ThemeView,
};
pub use manifest::{ManifestContext, PluginManifest, PLUGIN_API_VERSION};
pub use package::{ArchiveInspection, PACKAGE_EXTENSION};
pub use paths::PluginPaths;
pub use permissions::{PermissionGrant, PermissionStore, PermissionsFile};
pub use registry::{
    PluginRecord, PluginRegistry, PluginSource, PluginSourceKind, PluginState, TrustLevel,
};
pub use ui::{
    BridgeError, BridgeErrorCode, BridgeMethod, BridgeRequest, BRIDGE_METHODS, CSP, UI_API_VERSION,
    UI_SCHEME,
};
pub use version::{Version, VersionReq};
