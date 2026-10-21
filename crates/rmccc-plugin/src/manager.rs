//! 插件管理器：注册表加载、安装/启用/禁用/卸载/回滚、配置、贡献项（Phase 2 门面）。
//!
//! 设计约束：
//! - **损坏插件不阻断启动**：加载失败只产生诊断，不影响其它插件与主应用；
//! - 权限决策以后端为准：启用前检查待审权限，调用前由
//!   [`PermissionStore::authorize`] 复核；
//! - 所有状态变更先落注册表/权限文件，再更新内存视图。

use crate::capability::{plugin_error, Capability, CapabilityRisk, CapabilitySpec, CAPABILITIES};
use crate::config::{
    load_or_init, load_schema, plugin_config_path, reset_to_defaults, save_values, PluginConfig,
};
use crate::installer::{
    commit_content, remove_dir_if_exists, stage_package_bytes, stage_package_path, StagedPackage,
};
use crate::manifest::{
    validate_manifest, Contributes, DashboardCard, ManifestContext, PluginManifest, SettingsPage,
    Theme,
};
use crate::package::sha256_file;
use crate::path::validate_plugin_id;
use crate::paths::PluginPaths;
use crate::permissions::{PermissionGrant, PermissionStore};
use crate::registry::{
    PluginRecord, PluginRegistry, PluginSource, PluginState, TrustLevel, REGISTRY_SCHEMA_VERSION,
};
use crate::storage::{backup_file, now_rfc3339, read_json, write_json_atomic};
use crate::version::{Version, VersionReq};
use rmccc_error::{PluginErrorCode, Result};
use rmccc_platform::PlatformFs;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use ts_rs::TS;

// ── DTOs ──

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export_to = "types.ts")]
pub enum DiagnosticSeverity {
    Warning,
    Error,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export_to = "types.ts")]
pub struct PluginDiagnostic {
    #[serde(default)]
    pub plugin_id: Option<String>,
    pub severity: DiagnosticSeverity,
    pub message: String,
}

impl PluginDiagnostic {
    fn error(plugin_id: Option<String>, message: String) -> Self {
        Self {
            plugin_id,
            severity: DiagnosticSeverity::Error,
            message,
        }
    }

    fn warning(plugin_id: Option<String>, message: String) -> Self {
        Self {
            plugin_id,
            severity: DiagnosticSeverity::Warning,
            message,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export_to = "types.ts")]
pub struct PluginSummary {
    pub id: String,
    pub name: String,
    pub version: String,
    #[serde(default)]
    pub previous_version: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    pub publisher: crate::manifest::Publisher,
    pub state: PluginState,
    pub trust: TrustLevel,
    pub declared_permissions: Vec<String>,
    pub granted_permissions: Vec<String>,
    pub pending_permissions: Vec<String>,
    pub dashboard_cards: u32,
    pub settings_pages: u32,
    pub themes: u32,
    pub installed_at: String,
    pub updated_at: String,
    #[serde(default)]
    pub last_error: Option<String>,
    #[serde(default)]
    pub config_error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export_to = "types.ts")]
pub struct DependencyStatus {
    pub id: String,
    pub version: String,
    pub optional: bool,
    #[serde(default)]
    pub installed_version: Option<String>,
    pub satisfied: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export_to = "types.ts")]
pub struct PluginDetail {
    pub summary: PluginSummary,
    pub manifest: PluginManifest,
    pub granted: Vec<PermissionGrant>,
    pub pending: Vec<String>,
    pub dependencies: Vec<DependencyStatus>,
    pub dependents: Vec<String>,
    pub installed_path: String,
    pub has_config_schema: bool,
    #[ts(type = "unknown")]
    pub config: Option<Value>,
    #[ts(type = "unknown")]
    pub config_schema: Option<Value>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export_to = "types.ts")]
pub struct InstallReport {
    pub id: String,
    pub version: String,
    #[serde(default)]
    pub previous_version: Option<String>,
    pub state: PluginState,
    pub pending_permissions: Vec<String>,
    pub requires_review: bool,
    #[serde(default)]
    pub config_error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export_to = "types.ts")]
pub struct CapabilityInfo {
    pub name: String,
    pub description: String,
    pub risk: CapabilityRisk,
    pub requires_hosts: bool,
    pub instance_scoped: bool,
    pub executable: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export_to = "types.ts")]
pub struct DashboardCardView {
    /// 全局注册键 `<pluginId>/<localId>`。
    pub key: String,
    pub plugin_id: String,
    pub card: DashboardCard,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export_to = "types.ts")]
pub struct SettingsPageView {
    pub key: String,
    pub plugin_id: String,
    pub page: SettingsPage,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export_to = "types.ts")]
pub struct ThemeView {
    pub key: String,
    pub plugin_id: String,
    pub theme: Theme,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export_to = "types.ts")]
pub struct PluginContributions {
    pub dashboard_cards: Vec<DashboardCardView>,
    pub settings_pages: Vec<SettingsPageView>,
    pub themes: Vec<ThemeView>,
}

// ── Manager ──

struct InstalledPlugin {
    manifest: PluginManifest,
    dir: PathBuf,
    config_error: Option<String>,
}

pub struct PluginManager {
    paths: PluginPaths,
    ctx: ManifestContext,
    fs: Arc<dyn PlatformFs>,
    registry: PluginRegistry,
    permissions: PermissionStore,
    plugins: BTreeMap<String, InstalledPlugin>,
    diagnostics: Vec<PluginDiagnostic>,
    /// 注册表来自更新版本时为只读，禁止任何写操作（避免覆盖新字段）。
    registry_read_only: bool,
}

impl PluginManager {
    /// 加载插件系统。**永不返回 Err**：损坏的注册表/插件只产生诊断。
    pub fn load(
        data_dir: &Path,
        core_version: &str,
        platform_id: &str,
        fs: Arc<dyn PlatformFs>,
    ) -> Self {
        let paths = PluginPaths::new(data_dir);
        let mut diagnostics = Vec::new();
        if let Err(e) = paths.ensure_base_dirs() {
            diagnostics.push(PluginDiagnostic::error(
                None,
                format!("创建插件目录失败：{e}"),
            ));
        }

        let ctx = match ManifestContext::new(core_version, platform_id) {
            Ok(ctx) => ctx,
            Err(e) => {
                diagnostics.push(PluginDiagnostic::error(None, e.to_string()));
                // 版本号来自编译期常量；兜底 0.1.0 不会发生。
                ManifestContext::new("0.1.0", platform_id).unwrap_or(ManifestContext {
                    core_version: Version::new(0, 1, 0),
                    platform_id: platform_id.to_string(),
                })
            }
        };

        let (mut registry, registry_read_only, mut reg_diags) = load_registry(&paths.registry_file);
        diagnostics.append(&mut reg_diags);

        let (permissions, perm_warning) = PermissionStore::load(
            paths.permissions_file.clone(),
            paths.audit_file.clone(),
            fs.clone(),
        );
        if let Some(warning) = perm_warning {
            diagnostics.push(PluginDiagnostic::warning(None, warning));
        }

        // 低版本注册表按当前版本读取（Phase 2 无历史版本）。
        if registry.schema_version < REGISTRY_SCHEMA_VERSION {
            registry.schema_version = REGISTRY_SCHEMA_VERSION;
        }

        let mut manager = Self {
            paths,
            ctx,
            fs,
            registry,
            permissions,
            plugins: BTreeMap::new(),
            diagnostics,
            registry_read_only,
        };
        manager.scan();
        manager
    }

    pub fn core_version(&self) -> &Version {
        &self.ctx.core_version
    }

    pub fn platform_id(&self) -> &str {
        &self.ctx.platform_id
    }

    pub fn diagnostics(&self) -> &[PluginDiagnostic] {
        &self.diagnostics
    }

    pub fn permissions(&self) -> &PermissionStore {
        &self.permissions
    }

    /// 能力清单（IPC / UI 展示用）。
    pub fn capability_catalog(&self) -> Vec<CapabilityInfo> {
        CAPABILITIES.iter().map(capability_info).collect()
    }

    pub fn list(&self, include_removed: bool) -> Vec<PluginSummary> {
        let mut out: Vec<PluginSummary> = self
            .registry
            .plugins
            .values()
            .filter(|r| include_removed || r.state.is_visible())
            .filter_map(|r| self.summary(&r.id))
            .collect();
        out.sort_by_key(|p| p.name.to_lowercase());
        out
    }

    pub fn inspect(&mut self, id: &str) -> Result<PluginDetail> {
        let summary = self.summary(id).ok_or_else(|| not_found(id))?;
        let (manifest, dir) = {
            let plugin = self.plugins.get(id).ok_or_else(|| not_found(id))?;
            (plugin.manifest.clone(), plugin.dir.clone())
        };
        let installed_path = dir.display().to_string();
        let schema = load_schema(&dir.join("config.schema.json"))?;
        let config = if schema.is_some() {
            self.config_get(id).ok().map(|v| v.0.values)
        } else {
            None
        };

        let dependencies = self.dependency_status(&manifest);
        let dependents = self.dependents(id);
        let pending: Vec<String> = self
            .permissions
            .pending(&manifest)
            .unwrap_or_default()
            .into_iter()
            .map(|c| c.name)
            .collect();
        Ok(PluginDetail {
            summary,
            manifest,
            granted: self.permissions.grants_for(id),
            pending,
            dependencies,
            dependents,
            installed_path,
            has_config_schema: schema.is_some(),
            config,
            config_schema: schema,
        })
    }

    /// 安装（默认保持 `installed_disabled`，由用户审查权限后启用）。
    pub fn install_from_path(&mut self, path: &Path) -> Result<InstallReport> {
        self.install_from_path_with(path, false)
    }

    pub fn install_from_path_with(&mut self, path: &Path, enable: bool) -> Result<InstallReport> {
        self.ensure_writable()?;
        let source = PluginSource::local(&path.display().to_string());
        let mut staged = stage_package_path(&self.paths.staging_root, path, &self.ctx)?;
        let result = self.commit_install(&mut staged, source, enable);
        staged.cleanup();
        result
    }

    /// 测试/内嵌包入口：`reference` 用于 `source.reference`。
    pub fn install_from_bytes(
        &mut self,
        bytes: &[u8],
        reference: &str,
        enable: bool,
    ) -> Result<InstallReport> {
        self.ensure_writable()?;
        let source = PluginSource::local(reference);
        let mut staged = stage_package_bytes(&self.paths.staging_root, bytes, &self.ctx)?;
        let result = self.commit_install(&mut staged, source, enable);
        staged.cleanup();
        result
    }

    fn commit_install(
        &mut self,
        staged: &mut StagedPackage,
        source: PluginSource,
        enable: bool,
    ) -> Result<InstallReport> {
        let manifest = staged.inspection.manifest.clone();
        let id = manifest.id.clone();
        let version = manifest.version.clone();

        // 保留命名空间：Phase 2 没有签名通道，不允许第三方占用官方命名空间。
        if id == "org.rmccc" || id.starts_with("org.rmccc.") {
            return Err(plugin_error(
                PluginErrorCode::IdConflict,
                format!("{id} 属于保留命名空间 org.rmccc.*，Phase 2 只允许官方内置贡献"),
            ));
        }

        let conflicting = self
            .registry
            .get(&id)
            .filter(|r| r.state != PluginState::Removed)
            .cloned();
        if let Some(record) = &conflicting {
            if record.installed_version == version {
                return Err(plugin_error(
                    PluginErrorCode::IdConflict,
                    format!("{id} {version} 已安装（如需重装请先卸载）"),
                ));
            }
            let installed = Version::parse(&record.installed_version).ok_or_else(|| {
                plugin_error(
                    PluginErrorCode::StateConflict,
                    format!("注册表中的版本号非法：{}", record.installed_version),
                )
            })?;
            let candidate = manifest.version()?;
            if candidate < installed {
                return Err(plugin_error(
                    PluginErrorCode::StateConflict,
                    format!(
                        "不支持通过安装降级：当前 {}，待装 {version}；请使用回滚",
                        record.installed_version
                    ),
                ));
            }
        }

        self.check_dependencies(&manifest)?;
        self.check_no_cycle(&manifest)?;

        // 权限快照：失败时整体恢复。
        let registry_before = self.registry.clone();
        let permissions_before = self.permissions.file().clone();
        if conflicting.is_some() {
            self.permissions.reconcile_upgrade(&manifest)?;
        } else {
            self.permissions.auto_grant_low(&manifest)?;
        }
        let pending: Vec<String> = self
            .permissions
            .pending(&manifest)?
            .into_iter()
            .map(|c| c.name)
            .collect();

        let target = self.paths.version_dir(&id, &version);
        if let Err(e) = commit_content(staged, &target) {
            self.permissions.replace_file(permissions_before);
            return Err(e);
        }

        let now = now_rfc3339();
        let sha = staged.inspection.sha256.clone();
        let record = match &conflicting {
            Some(old) => {
                let mut record = old.clone();
                record.installed_version = version.clone();
                record.previous_version = Some(old.installed_version.clone());
                record.state = PluginState::InstalledDisabled;
                record.manifest_sha256 = Some(sha);
                record.last_error = None;
                record.updated_at = now;
                record
            }
            None => PluginRecord::new(&manifest, TrustLevel::Development, source, Some(sha), &now),
        };
        let previous_version = record.previous_version.clone();
        self.registry.insert(record);

        // 先写注册表，再写权限；任一步失败都恢复旧状态并删除新版本目录。
        if let Err(e) = self.save_registry() {
            self.registry = registry_before;
            self.permissions.replace_file(permissions_before);
            let _ = remove_dir_if_exists(&target);
            return Err(e);
        }
        if let Err(e) = self.permissions.save() {
            self.registry = registry_before;
            self.permissions.replace_file(permissions_before);
            let _ = self.save_registry();
            let _ = remove_dir_if_exists(&target);
            return Err(e);
        }

        let config_error = match self.ensure_config(&id) {
            Ok(()) => None,
            Err(e) => {
                let message = e.to_string();
                self.diagnostics
                    .push(PluginDiagnostic::error(Some(id.clone()), message.clone()));
                Some(message)
            }
        };

        self.reload_plugin(&id);

        let state = self
            .registry
            .get(&id)
            .map(|r| r.state)
            .unwrap_or(PluginState::InstalledDisabled);
        let requires_review = !pending.is_empty();

        if enable && !requires_review && config_error.is_none() {
            let _ = self.enable(&id);
        }

        let state = self.registry.get(&id).map(|r| r.state).unwrap_or(state);
        Ok(InstallReport {
            id,
            version,
            previous_version,
            state,
            pending_permissions: pending,
            requires_review,
            config_error,
        })
    }

    pub fn enable(&mut self, id: &str) -> Result<PluginSummary> {
        ensure_valid_id(id)?;
        self.ensure_writable()?;
        let manifest = self
            .plugins
            .get(id)
            .map(|p| p.manifest.clone())
            .ok_or_else(|| not_found(id))?;
        let record = self
            .registry
            .get(id)
            .cloned()
            .ok_or_else(|| not_found(id))?;
        if record.state.is_enabled() {
            return self.summary(id).ok_or_else(|| not_found(id));
        }

        let pending: Vec<String> = self
            .permissions
            .pending(&manifest)?
            .into_iter()
            .map(|c| c.name)
            .collect();
        if !pending.is_empty() {
            return Err(plugin_error(
                PluginErrorCode::PermissionPending,
                format!(
                    "插件 {id} 有未审查的权限：{}；请先授予或拒绝",
                    pending.join(", ")
                ),
            ));
        }
        if let Some(err) = self.plugins.get(id).and_then(|p| p.config_error.clone()) {
            return Err(plugin_error(
                PluginErrorCode::ConfigInvalid,
                format!("插件 {id} 配置不可用：{err}"),
            ));
        }
        for dep in &manifest.extension_dependencies {
            if dep.optional {
                continue;
            }
            let enabled = self
                .registry
                .get(&dep.id)
                .map(|r| r.state.is_enabled())
                .unwrap_or(false);
            if !enabled {
                return Err(plugin_error(
                    PluginErrorCode::DependencyMissing,
                    format!("依赖 {} 未安装或未启用，无法启用 {id}", dep.id),
                ));
            }
        }

        let mut record = record;
        record.transition(PluginState::Enabled, &now_rfc3339())?;
        self.registry.insert(record);
        self.save_registry()?;
        self.summary(id).ok_or_else(|| not_found(id))
    }

    pub fn disable(&mut self, id: &str) -> Result<PluginSummary> {
        ensure_valid_id(id)?;
        self.ensure_writable()?;
        let record = self
            .registry
            .get(id)
            .cloned()
            .ok_or_else(|| not_found(id))?;
        if !record.state.is_enabled() {
            return self.summary(id).ok_or_else(|| not_found(id));
        }
        let blockers = self.enabled_dependents(id);
        if !blockers.is_empty() {
            return Err(plugin_error(
                PluginErrorCode::StateConflict,
                format!("以下已启用插件依赖 {id}：{}", blockers.join(", ")),
            ));
        }
        let now = now_rfc3339();
        let mut record = record;
        record.transition(PluginState::Disabling, &now)?;
        record.transition(PluginState::Disabled, &now)?;
        self.registry.insert(record);
        self.save_registry()?;
        self.summary(id).ok_or_else(|| not_found(id))
    }

    pub fn uninstall(&mut self, id: &str, delete_data: bool) -> Result<()> {
        ensure_valid_id(id)?;
        self.ensure_writable()?;
        let record = self
            .registry
            .get(id)
            .cloned()
            .ok_or_else(|| not_found(id))?;
        if record.state == PluginState::Removed {
            return Ok(());
        }
        let blockers = self.enabled_dependents(id);
        if !blockers.is_empty() {
            return Err(plugin_error(
                PluginErrorCode::StateConflict,
                format!("以下已启用插件依赖 {id}：{}", blockers.join(", ")),
            ));
        }
        // 任意已安装（含禁用）依赖者也阻止卸载，避免"依赖缺失"的半损坏状态。
        let installed_blockers = self.installed_dependents(id);
        if !installed_blockers.is_empty() {
            return Err(plugin_error(
                PluginErrorCode::StateConflict,
                format!(
                    "以下已安装插件依赖 {id}：{}；请先卸载它们",
                    installed_blockers.join(", ")
                ),
            ));
        }

        let now = now_rfc3339();
        let mut record = record;
        if record.state.is_enabled() {
            record.transition(PluginState::Disabling, &now)?;
            record.transition(PluginState::Disabled, &now)?;
        }
        record.transition(PluginState::Uninstalling, &now)?;
        self.registry.insert(record);
        self.save_registry()?;

        let plugin_dir = self.paths.plugin_dir(id);
        if let Err(e) = remove_dir_if_exists(&plugin_dir) {
            if let Some(r) = self.registry.get_mut(id) {
                r.mark_failed(&format!("删除插件目录失败：{e}"), &now);
            }
            let _ = self.save_registry();
            return Err(e);
        }

        if let Some(r) = self.registry.get_mut(id) {
            r.state = PluginState::Removed;
            r.last_error = None;
            r.updated_at = now_rfc3339();
        }
        self.permissions.revoke_all(id);
        self.save_registry()?;
        self.permissions.save()?;
        self.plugins.remove(id);

        if delete_data {
            let data_dir = self.paths.config_dir(id);
            if data_dir.exists() {
                if let Err(e) = std::fs::remove_dir_all(&data_dir) {
                    self.diagnostics.push(PluginDiagnostic::warning(
                        Some(id.to_string()),
                        format!("插件已卸载，但删除配置目录失败：{e}"),
                    ));
                }
            }
        }
        Ok(())
    }

    /// 回滚到上一个版本目录（升级保留旧版本，见 ADR-003 决策 3/4）。
    pub fn rollback(&mut self, id: &str) -> Result<PluginSummary> {
        ensure_valid_id(id)?;
        self.ensure_writable()?;
        let record = self
            .registry
            .get(id)
            .cloned()
            .ok_or_else(|| not_found(id))?;
        let previous = record.previous_version.clone().ok_or_else(|| {
            plugin_error(
                PluginErrorCode::StateConflict,
                format!("插件 {id} 没有可回滚的上一个版本"),
            )
        })?;
        // previousVersion 来自注册表，同样不能直接拼路径。
        if Version::parse(&previous).is_none() {
            return Err(plugin_error(
                PluginErrorCode::StateConflict,
                format!("上一个版本号非法：{previous:?}"),
            ));
        }
        let dir = self.paths.version_dir(id, &previous);
        if !dir.is_dir() {
            return Err(plugin_error(
                PluginErrorCode::StateConflict,
                format!("上一个版本目录不存在：{}", dir.display()),
            ));
        }
        let manifest = self.read_and_validate_manifest(&dir)?;
        if manifest.id != id || manifest.version != previous {
            return Err(plugin_error(
                PluginErrorCode::Integrity,
                "回滚目录中的 manifest 与注册表不一致".to_string(),
            ));
        }

        self.permissions.reconcile_upgrade(&manifest)?;
        let sha = sha256_file(&dir.join("manifest.json")).ok();

        let now = now_rfc3339();
        let mut record = record;
        record.transition(PluginState::InstalledDisabled, &now)?;
        record.previous_version = Some(record.installed_version.clone());
        record.installed_version = previous;
        record.manifest_sha256 = sha;
        record.last_error = None;
        self.registry.insert(record);
        self.save_registry()?;
        self.permissions.save()?;
        self.ensure_config(id)?;
        self.reload_plugin(id);
        self.summary(id).ok_or_else(|| not_found(id))
    }

    // ── 权限 ──

    pub fn grant(&mut self, id: &str, capability: &str) -> Result<PluginSummary> {
        ensure_valid_id(id)?;
        self.ensure_writable()?;
        let manifest = self
            .plugins
            .get(id)
            .map(|p| p.manifest.clone())
            .ok_or_else(|| not_found(id))?;
        self.permissions.grant(&manifest, capability)?;
        self.permissions.save()?;
        self.summary(id).ok_or_else(|| not_found(id))
    }

    pub fn revoke(&mut self, id: &str, capability: &str) -> Result<PluginSummary> {
        ensure_valid_id(id)?;
        self.ensure_writable()?;
        if !self.plugins.contains_key(id) {
            return Err(not_found(id));
        }
        self.permissions.revoke(id, capability)?;
        self.permissions.save()?;
        self.summary(id).ok_or_else(|| not_found(id))
    }

    /// Host Broker 入口：每次调用都重新检查（Phase 2 无执行通道，先提供权威 API）。
    pub fn authorize(
        &self,
        plugin_id: &str,
        capability: &str,
        host: Option<&str>,
        instance_id: Option<&str>,
    ) -> Result<()> {
        ensure_valid_id(plugin_id)?;
        let record = self
            .registry
            .get(plugin_id)
            .ok_or_else(|| not_found(plugin_id))?;
        if !record.state.is_enabled() {
            return Err(plugin_error(
                PluginErrorCode::PermissionDenied,
                format!("插件 {plugin_id} 未启用"),
            ));
        }
        self.permissions
            .authorize(plugin_id, capability, host, instance_id)
    }

    // ── 配置 ──

    pub fn config_get(&mut self, id: &str) -> Result<(PluginConfig, Option<Value>)> {
        let dir = self.plugin_dir(id)?;
        let schema = load_schema(&dir.join("config.schema.json"))?;
        let Some(schema) = schema else {
            return Err(plugin_error(
                PluginErrorCode::ConfigInvalid,
                format!("插件 {id} 没有 config.schema.json，无用户配置"),
            ));
        };
        let config = load_or_init(
            self.fs.as_ref(),
            &plugin_config_path(&self.paths.data_dir, id),
            &schema,
        )?;
        if let Some(plugin) = self.plugins.get_mut(id) {
            plugin.config_error = None;
        }
        Ok((config, Some(schema)))
    }

    pub fn config_set(&mut self, id: &str, values: Value) -> Result<PluginConfig> {
        let dir = self.plugin_dir(id)?;
        let schema = load_schema(&dir.join("config.schema.json"))?.ok_or_else(|| {
            plugin_error(
                PluginErrorCode::ConfigInvalid,
                format!("插件 {id} 没有 config.schema.json，不能写入配置"),
            )
        })?;
        let config = save_values(
            self.fs.as_ref(),
            &plugin_config_path(&self.paths.data_dir, id),
            &schema,
            values,
        )?;
        if let Some(plugin) = self.plugins.get_mut(id) {
            plugin.config_error = None;
        }
        Ok(config)
    }

    pub fn config_reset(&mut self, id: &str) -> Result<PluginConfig> {
        let dir = self.plugin_dir(id)?;
        let schema = load_schema(&dir.join("config.schema.json"))?.ok_or_else(|| {
            plugin_error(
                PluginErrorCode::ConfigInvalid,
                format!("插件 {id} 没有 config.schema.json"),
            )
        })?;
        let config = reset_to_defaults(
            self.fs.as_ref(),
            &plugin_config_path(&self.paths.data_dir, id),
            &schema,
        )?;
        if let Some(plugin) = self.plugins.get_mut(id) {
            plugin.config_error = None;
        }
        Ok(config)
    }

    // ── 贡献项 ──

    /// 只汇总**已启用**插件的贡献；键为 `<pluginId>/<localId>`。
    pub fn contributions(&self) -> PluginContributions {
        let mut out = PluginContributions::default();
        let mut keys = BTreeSet::new();
        for record in self.registry.enabled() {
            let Some(plugin) = self.plugins.get(&record.id) else {
                continue;
            };
            let c: &Contributes = &plugin.manifest.contributes;
            // 权限撤销立即生效：未授予对应能力时贡献项不再登记。
            let granted =
                |capability: &str| self.permissions.file().is_granted(&record.id, capability);
            if granted("dashboard.cards") {
                for card in &c.dashboard_cards {
                    let key = format!("{}/{}", record.id, card.id);
                    if keys.insert(key.clone()) {
                        out.dashboard_cards.push(DashboardCardView {
                            key,
                            plugin_id: record.id.clone(),
                            card: card.clone(),
                        });
                    }
                }
            }
            if granted("ui.pages") {
                for page in &c.settings_pages {
                    let key = format!("{}/{}", record.id, page.id);
                    if keys.insert(key.clone()) {
                        out.settings_pages.push(SettingsPageView {
                            key,
                            plugin_id: record.id.clone(),
                            page: page.clone(),
                        });
                    }
                }
            }
            if granted("themes.apply") {
                for theme in &c.themes {
                    let key = format!("{}/{}", record.id, theme.id);
                    if keys.insert(key.clone()) {
                        out.themes.push(ThemeView {
                            key,
                            plugin_id: record.id.clone(),
                            theme: theme.clone(),
                        });
                    }
                }
            }
        }
        out
    }

    // ── internals ──

    fn ensure_writable(&self) -> Result<()> {
        if self.registry_read_only {
            return Err(plugin_error(
                PluginErrorCode::StateConflict,
                "插件注册表来自更新版本，当前启动器只读".to_string(),
            ));
        }
        Ok(())
    }

    fn save_registry(&self) -> Result<()> {
        write_json_atomic(self.fs.as_ref(), &self.paths.registry_file, &self.registry)
    }

    fn summary(&self, id: &str) -> Option<PluginSummary> {
        let record = self.registry.get(id)?;
        let Some(plugin) = self.plugins.get(id) else {
            // 已卸载的墓碑没有版本目录/Manifest，只返回最小记录。
            if record.state == PluginState::Removed {
                return Some(PluginSummary {
                    id: record.id.clone(),
                    name: record.id.clone(),
                    version: record.installed_version.clone(),
                    previous_version: record.previous_version.clone(),
                    description: None,
                    publisher: crate::manifest::Publisher {
                        id: String::new(),
                        name: String::new(),
                    },
                    state: record.state,
                    trust: record.trust,
                    declared_permissions: Vec::new(),
                    granted_permissions: Vec::new(),
                    pending_permissions: Vec::new(),
                    dashboard_cards: 0,
                    settings_pages: 0,
                    themes: 0,
                    installed_at: record.installed_at.clone(),
                    updated_at: record.updated_at.clone(),
                    last_error: record.last_error.clone(),
                    config_error: None,
                });
            }
            return None;
        };
        let declared = plugin.manifest.capability_names().unwrap_or_default();
        let granted: Vec<String> = self
            .permissions
            .grants_for(id)
            .into_iter()
            .map(|g| g.capability)
            .collect();
        let pending: Vec<String> = self
            .permissions
            .pending(&plugin.manifest)
            .unwrap_or_default()
            .into_iter()
            .map(|c| c.name)
            .collect();
        Some(PluginSummary {
            id: record.id.clone(),
            name: plugin.manifest.name.clone(),
            version: record.installed_version.clone(),
            previous_version: record.previous_version.clone(),
            description: plugin.manifest.description.clone(),
            publisher: plugin.manifest.publisher.clone(),
            state: record.state,
            trust: record.trust,
            declared_permissions: declared,
            granted_permissions: granted,
            pending_permissions: pending,
            dashboard_cards: plugin.manifest.contributes.dashboard_cards.len() as u32,
            settings_pages: plugin.manifest.contributes.settings_pages.len() as u32,
            themes: plugin.manifest.contributes.themes.len() as u32,
            installed_at: record.installed_at.clone(),
            updated_at: record.updated_at.clone(),
            last_error: record.last_error.clone(),
            config_error: plugin.config_error.clone(),
        })
    }

    fn scan(&mut self) {
        self.plugins.clear();
        let ids: Vec<String> = self.registry.plugins.keys().cloned().collect();
        for id in ids {
            let Some(record) = self.registry.get(&id).cloned() else {
                continue;
            };
            if record.state == PluginState::Removed {
                continue;
            }
            // 注册表是用户可写文件：在拼路径/删目录前必须重新校验 id 与版本。
            if let Err(e) = validate_plugin_id(&id).and_then(|_| {
                Version::parse(&record.installed_version)
                    .map(|_| ())
                    .ok_or_else(|| {
                        plugin_error(
                            PluginErrorCode::StateConflict,
                            format!("注册表中的版本号非法：{}", record.installed_version),
                        )
                    })
            }) {
                self.diagnostics
                    .push(PluginDiagnostic::error(Some(id.clone()), e.to_string()));
                continue;
            }
            let dir = self.paths.version_dir(&id, &record.installed_version);
            let manifest = match self.read_and_validate_manifest(&dir) {
                Ok(manifest) => manifest,
                Err(e) => {
                    let message = e.to_string();
                    self.diagnostics
                        .push(PluginDiagnostic::error(Some(id.clone()), message.clone()));
                    if let Some(r) = self.registry.get_mut(&id) {
                        r.mark_failed(&message, &now_rfc3339());
                    }
                    continue;
                }
            };
            if manifest.id != id {
                let message = format!(
                    "目录 {} 的 manifest.id={} 与注册表 id 不一致",
                    dir.display(),
                    manifest.id
                );
                self.diagnostics
                    .push(PluginDiagnostic::error(Some(id.clone()), message.clone()));
                if let Some(r) = self.registry.get_mut(&id) {
                    r.mark_failed(&message, &now_rfc3339());
                }
                continue;
            }
            if manifest.version != record.installed_version {
                let message = format!(
                    "manifest.version={} 与注册表 installedVersion={} 不一致",
                    manifest.version, record.installed_version
                );
                self.diagnostics
                    .push(PluginDiagnostic::error(Some(id.clone()), message.clone()));
                if let Some(r) = self.registry.get_mut(&id) {
                    r.mark_failed(&message, &now_rfc3339());
                }
                continue;
            }

            let mut config_error = None;
            if let Err(e) = self.load_config_for(&id, &dir) {
                let message = e.to_string();
                config_error = Some(message.clone());
                self.diagnostics
                    .push(PluginDiagnostic::warning(Some(id.clone()), message));
            }

            self.plugins.insert(
                id,
                InstalledPlugin {
                    manifest,
                    dir,
                    config_error,
                },
            );
        }
        self.scan_orphans();
    }

    fn scan_orphans(&mut self) {
        let entries = match std::fs::read_dir(&self.paths.plugins_root) {
            Ok(entries) => entries,
            Err(_) => return,
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }
            let Some(id) = path.file_name().and_then(|n| n.to_str()) else {
                continue;
            };
            if id.starts_with('.') {
                continue;
            }
            if self.registry.get(id).is_none() {
                self.diagnostics.push(PluginDiagnostic::warning(
                    Some(id.to_string()),
                    format!("未注册的插件目录（不会自动加载）：{}", path.display()),
                ));
                continue;
            }
            let (installed_version, previous_version) = match self.registry.get(id) {
                Some(record) => (
                    record.installed_version.clone(),
                    record.previous_version.clone(),
                ),
                None => continue,
            };
            if let Ok(versions) = std::fs::read_dir(&path) {
                for version in versions.flatten() {
                    if !version.path().is_dir() {
                        continue;
                    }
                    let name = version.file_name().to_string_lossy().to_string();
                    let tracked = name == installed_version
                        || previous_version.as_deref() == Some(name.as_str());
                    if !tracked {
                        self.diagnostics.push(PluginDiagnostic::warning(
                            Some(id.to_string()),
                            format!("未跟踪的版本目录：{}", version.path().display()),
                        ));
                    }
                }
            }
        }
    }

    fn read_and_validate_manifest(&self, dir: &Path) -> Result<PluginManifest> {
        let path = dir.join("manifest.json");
        let raw = std::fs::read_to_string(&path)?;
        let manifest = PluginManifest::parse(&raw)?;
        validate_manifest(&manifest, &self.ctx)?;
        Ok(manifest)
    }

    fn load_config_for(&self, id: &str, dir: &Path) -> Result<()> {
        let schema = load_schema(&dir.join("config.schema.json"))?;
        if let Some(schema) = schema {
            load_or_init(
                self.fs.as_ref(),
                &plugin_config_path(&self.paths.data_dir, id),
                &schema,
            )?;
        }
        Ok(())
    }

    fn ensure_config(&mut self, id: &str) -> Result<()> {
        let dir = self.plugin_dir(id)?;
        self.load_config_for(id, &dir)
    }

    fn plugin_dir(&self, id: &str) -> Result<PathBuf> {
        ensure_valid_id(id)?;
        let record = self.registry.get(id).ok_or_else(|| not_found(id))?;
        Ok(self.paths.version_dir(id, &record.installed_version))
    }

    fn reload_plugin(&mut self, id: &str) {
        let Some(record) = self.registry.get(id).cloned() else {
            self.plugins.remove(id);
            return;
        };
        let dir = self.paths.version_dir(id, &record.installed_version);
        match self.read_and_validate_manifest(&dir) {
            Ok(manifest) => {
                let config_error = self.load_config_for(id, &dir).err().map(|e| e.to_string());
                self.plugins.insert(
                    id.to_string(),
                    InstalledPlugin {
                        manifest,
                        dir,
                        config_error,
                    },
                );
            }
            Err(e) => {
                self.diagnostics
                    .push(PluginDiagnostic::error(Some(id.to_string()), e.to_string()));
            }
        }
    }

    fn check_dependencies(&mut self, manifest: &PluginManifest) -> Result<()> {
        for dep in &manifest.extension_dependencies {
            let installed = self
                .plugins
                .get(&dep.id)
                .map(|p| p.manifest.version.clone());
            let Some(installed_version) = installed else {
                if dep.optional {
                    self.diagnostics.push(PluginDiagnostic::warning(
                        Some(manifest.id.clone()),
                        format!("可选依赖 {} 未安装", dep.id),
                    ));
                    continue;
                }
                return Err(plugin_error(
                    PluginErrorCode::DependencyMissing,
                    format!("缺少必需依赖 {} {}", dep.id, dep.version),
                ));
            };
            let req = VersionReq::parse(&dep.version).ok_or_else(|| {
                plugin_error(
                    PluginErrorCode::ManifestInvalid,
                    format!("依赖 {} 的版本范围非法", dep.id),
                )
            })?;
            let installed_v = Version::parse(&installed_version).ok_or_else(|| {
                plugin_error(
                    PluginErrorCode::DependencyMissing,
                    format!("依赖 {} 的已安装版本号非法", dep.id),
                )
            })?;
            if !req.matches(&installed_v) {
                if dep.optional {
                    self.diagnostics.push(PluginDiagnostic::warning(
                        Some(manifest.id.clone()),
                        format!(
                            "可选依赖 {} 版本不满足：需要 {}，已安装 {}",
                            dep.id, dep.version, installed_version
                        ),
                    ));
                    continue;
                }
                return Err(plugin_error(
                    PluginErrorCode::DependencyMissing,
                    format!(
                        "依赖 {} 版本不满足：需要 {}，已安装 {}",
                        dep.id, dep.version, installed_version
                    ),
                ));
            }
        }
        Ok(())
    }

    fn check_no_cycle(&self, candidate: &PluginManifest) -> Result<()> {
        let mut graph: BTreeMap<String, Vec<String>> = BTreeMap::new();
        for (id, plugin) in &self.plugins {
            graph.insert(
                id.clone(),
                plugin
                    .manifest
                    .extension_dependencies
                    .iter()
                    .map(|d| d.id.clone())
                    .collect(),
            );
        }
        graph.insert(
            candidate.id.clone(),
            candidate
                .extension_dependencies
                .iter()
                .map(|d| d.id.clone())
                .collect(),
        );

        fn visit(
            id: &str,
            graph: &BTreeMap<String, Vec<String>>,
            visiting: &mut BTreeSet<String>,
            done: &mut BTreeSet<String>,
            path: &mut Vec<String>,
        ) -> Result<()> {
            if done.contains(id) {
                return Ok(());
            }
            if !visiting.insert(id.to_string()) {
                let cycle = path.join(" -> ");
                return Err(plugin_error(
                    PluginErrorCode::DependencyCycle,
                    format!("依赖图存在环：{cycle} -> {id}"),
                ));
            }
            path.push(id.to_string());
            if let Some(deps) = graph.get(id) {
                for dep in deps {
                    if graph.contains_key(dep) {
                        visit(dep, graph, visiting, done, path)?;
                    }
                }
            }
            path.pop();
            visiting.remove(id);
            done.insert(id.to_string());
            Ok(())
        }

        let mut visiting = BTreeSet::new();
        let mut done = BTreeSet::new();
        let mut path = Vec::new();
        visit(&candidate.id, &graph, &mut visiting, &mut done, &mut path)
    }

    fn dependency_status(&self, manifest: &PluginManifest) -> Vec<DependencyStatus> {
        manifest
            .extension_dependencies
            .iter()
            .map(|dep| {
                let installed = self
                    .registry
                    .get(&dep.id)
                    .map(|r| r.installed_version.clone());
                let satisfied = installed
                    .as_deref()
                    .and_then(Version::parse)
                    .zip(VersionReq::parse(&dep.version))
                    .map(|(v, r)| r.matches(&v))
                    .unwrap_or(false);
                DependencyStatus {
                    id: dep.id.clone(),
                    version: dep.version.clone(),
                    optional: dep.optional,
                    installed_version: installed,
                    satisfied,
                }
            })
            .collect()
    }

    fn dependents(&self, id: &str) -> Vec<String> {
        self.plugins
            .values()
            .filter(|p| p.manifest.extension_dependencies.iter().any(|d| d.id == id))
            .map(|p| p.manifest.id.clone())
            .collect()
    }

    fn enabled_dependents(&self, id: &str) -> Vec<String> {
        self.dependents(id)
            .into_iter()
            .filter(|dep| {
                self.registry
                    .get(dep)
                    .map(|r| r.state.is_enabled())
                    .unwrap_or(false)
            })
            .collect()
    }

    fn installed_dependents(&self, id: &str) -> Vec<String> {
        self.dependents(id)
            .into_iter()
            .filter(|dep| self.plugins.contains_key(dep))
            .collect()
    }
}

fn capability_info(spec: &CapabilitySpec) -> CapabilityInfo {
    CapabilityInfo {
        name: spec.name.to_string(),
        description: spec.description.to_string(),
        risk: spec.risk,
        requires_hosts: spec.requires_hosts,
        instance_scoped: spec.instance_scoped,
        executable: spec.executable,
    }
}

fn ensure_valid_id(id: &str) -> Result<()> {
    validate_plugin_id(id)
}

fn not_found(id: &str) -> rmccc_error::RmcccError {
    plugin_error(
        PluginErrorCode::NotFound,
        format!("插件未安装或不可用：{id}"),
    )
}

fn load_registry(path: &Path) -> (PluginRegistry, bool, Vec<PluginDiagnostic>) {
    let mut diags = Vec::new();
    match read_json::<PluginRegistry>(path) {
        Ok(Some(registry)) => {
            if let Err(e) = registry.validate_schema() {
                diags.push(PluginDiagnostic::error(None, e.to_string()));
                return (PluginRegistry::default(), true, diags);
            }
            (registry, false, diags)
        }
        Ok(None) => (PluginRegistry::default(), false, diags),
        Err(e) => {
            let mut message = format!("plugin-state/registry.json 损坏：{e}");
            match backup_file(path) {
                Ok(Some(backup)) => message.push_str(&format!("；已备份到 {}", backup.display())),
                Ok(None) => {}
                Err(backup_err) => message.push_str(&format!("；备份失败：{backup_err}")),
            }
            diags.push(PluginDiagnostic::error(None, message));
            (PluginRegistry::default(), false, diags)
        }
    }
}

/// 供 CLI/UI 展示：能力列表的静态访问（不构造 manager）。
pub fn capability_catalog() -> Vec<CapabilityInfo> {
    CAPABILITIES.iter().map(capability_info).collect()
}

/// 把权限声明转成可读的能力列表（诊断用）。
pub fn declared_capabilities(manifest: &PluginManifest) -> Result<Vec<Capability>> {
    manifest.capabilities()
}

#[cfg(test)]
mod tests;
