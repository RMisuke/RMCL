//! 声明式插件系统（Phase 2）。
//!
//! **本阶段不执行任何第三方代码**：只支持数据型插件（主题/翻译/声明式卡片）。
//! `entry.ui` / `entry.logic` 在 manifest 校验阶段就被拒绝（ADR-004 决策 1）。
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
pub use version::{Version, VersionReq};
