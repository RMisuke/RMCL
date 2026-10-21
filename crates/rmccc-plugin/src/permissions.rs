//! 权限授予、撤销与审计（指南 §8.2/§8.3，ADR-004 决策 4）。
//!
//! Phase 2 没有执行通道，但授权模型与拒绝行为先落地并被测试锁定：
//! **权限决策由后端权威判定，不由前端按钮决定**。

use crate::capability::{plugin_error, Capability};
use crate::manifest::PluginManifest;
use crate::storage::{backup_file, now_rfc3339, read_json, write_json_atomic};
use rmccc_error::{PluginErrorCode, Result};
use rmccc_platform::PlatformFs;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use ts_rs::TS;

pub const PERMISSIONS_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export_to = "types.ts")]
pub enum GrantSource {
    /// 安装时按低风险策略自动授予。
    Auto,
    /// 用户在审查界面显式授予。
    User,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export_to = "types.ts")]
pub struct PermissionGrant {
    pub capability: String,
    pub granted_at: String,
    pub source: GrantSource,
    #[serde(default)]
    pub hosts: Vec<String>,
    /// 空 = 全部实例（读取类能力）；非空 = 仅这些实例。
    #[serde(default)]
    pub instances: Vec<String>,
}

/// `permissions.json`。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export_to = "types.ts")]
pub struct PermissionsFile {
    pub schema_version: u32,
    /// plugin id → capability → grant。
    pub plugins: BTreeMap<String, BTreeMap<String, PermissionGrant>>,
}

impl Default for PermissionsFile {
    fn default() -> Self {
        Self {
            schema_version: PERMISSIONS_SCHEMA_VERSION,
            plugins: BTreeMap::new(),
        }
    }
}

impl PermissionsFile {
    pub fn grants_for(&self, plugin_id: &str) -> Vec<PermissionGrant> {
        self.plugins
            .get(plugin_id)
            .map(|m| m.values().cloned().collect())
            .unwrap_or_default()
    }

    pub fn is_granted(&self, plugin_id: &str, capability: &str) -> bool {
        self.plugins
            .get(plugin_id)
            .map(|m| m.contains_key(capability))
            .unwrap_or(false)
    }
}

/// 权限存储 + 审计日志。
pub struct PermissionStore {
    path: PathBuf,
    audit_path: PathBuf,
    fs: Arc<dyn PlatformFs>,
    file: PermissionsFile,
    load_warning: Option<String>,
}

impl PermissionStore {
    /// 加载权限文件。损坏文件会被备份并重建默认值（不阻断启动）。
    pub fn load(
        path: PathBuf,
        audit_path: PathBuf,
        fs: Arc<dyn PlatformFs>,
    ) -> (Self, Option<String>) {
        let mut warning = None;
        let file = match read_json::<PermissionsFile>(&path) {
            Ok(Some(file)) if file.schema_version <= PERMISSIONS_SCHEMA_VERSION => file,
            Ok(Some(file)) => {
                warning = Some(format!(
                    "permissions.json schemaVersion {} 高于支持的 {}，按只读空集处理",
                    file.schema_version, PERMISSIONS_SCHEMA_VERSION
                ));
                PermissionsFile::default()
            }
            Ok(None) => PermissionsFile::default(),
            Err(e) => {
                warning = Some(format!("permissions.json 损坏，已备份并重建：{e}"));
                if let Err(backup_err) = backup_file(&path) {
                    let prefix = warning.clone().unwrap_or_default();
                    warning = Some(format!("{prefix}；备份失败：{backup_err}"));
                }
                PermissionsFile::default()
            }
        };
        (
            Self {
                path,
                audit_path,
                fs,
                file,
                load_warning: warning.clone(),
            },
            warning,
        )
    }

    pub fn load_warning(&self) -> Option<&str> {
        self.load_warning.as_deref()
    }

    pub fn file(&self) -> &PermissionsFile {
        &self.file
    }

    /// 事务回滚用：整体替换内存中的权限文件（调用方随后决定是否落盘）。
    pub fn replace_file(&mut self, file: PermissionsFile) {
        self.file = file;
    }

    pub fn grants_for(&self, plugin_id: &str) -> Vec<PermissionGrant> {
        self.file.grants_for(plugin_id)
    }

    pub fn save(&self) -> Result<()> {
        write_json_atomic(self.fs.as_ref(), &self.path, &self.file)
    }

    /// 首次安装：低风险能力自动授予（按 Manifest 声明的作用域）。
    pub fn auto_grant_low(&mut self, manifest: &PluginManifest) -> Result<()> {
        let now = now_rfc3339();
        for permission in &manifest.permissions {
            let cap = Capability::parse(&permission.capability)?;
            if !cap.risk.is_low() {
                continue;
            }
            let entry = self.file.plugins.entry(manifest.id.clone()).or_default();
            entry.insert(
                cap.name.clone(),
                PermissionGrant {
                    capability: cap.name,
                    granted_at: now.clone(),
                    source: GrantSource::Auto,
                    hosts: permission.hosts.clone(),
                    instances: permission.instances.clone(),
                },
            );
        }
        Ok(())
    }

    /// 升级：保留已有授权、移除新版本不再声明的授权。
    ///
    /// **不自动新增任何授权**（包括低风险）——升级请求新权限必须由用户审查。
    /// 新声明的低风险能力因此会出现在 [`Self::pending`] 里（见那里的说明），
    /// 否则它会既不在 granted 也不在 pending，永远无法启用。
    pub fn reconcile_upgrade(&mut self, manifest: &PluginManifest) -> Result<()> {
        let declared = manifest.capability_names()?;
        let declared: BTreeSet<&str> = declared.iter().map(|s| s.as_str()).collect();
        if let Some(grants) = self.file.plugins.get_mut(&manifest.id) {
            grants.retain(|cap, _| declared.contains(cap.as_str()));
        }
        Ok(())
    }

    /// 尚未授予的能力（升级/首次安装后的"待审查"列表）。
    ///
    /// 首次安装时低风险能力由 [`Self::auto_grant_low`] 直接授予，所以这里只会剩下
    /// 敏感能力；升级时**任何**新声明的能力都不会自动授予，必须列出让用户审查，
    /// 否则升级新增的低风险能力会变成"声明了但永远不可用"的死状态。
    pub fn pending(&self, manifest: &PluginManifest) -> Result<Vec<Capability>> {
        let mut pending = Vec::new();
        for permission in &manifest.permissions {
            let cap = Capability::parse(&permission.capability)?;
            if !self.file.is_granted(&manifest.id, &cap.name) {
                pending.push(cap);
            }
        }
        Ok(pending)
    }

    /// 授予一个能力；作用域取 Manifest 声明（不由前端任意扩大）。
    pub fn grant(
        &mut self,
        manifest: &PluginManifest,
        capability: &str,
    ) -> Result<PermissionGrant> {
        let permission = manifest
            .permissions
            .iter()
            .find(|p| p.capability == capability)
            .ok_or_else(|| {
                plugin_error(
                    PluginErrorCode::PermissionDenied,
                    format!("插件 {} 未声明能力 {capability}", manifest.id),
                )
            })?;
        let cap = Capability::parse(capability)?;
        let grant = PermissionGrant {
            capability: cap.name.clone(),
            granted_at: now_rfc3339(),
            source: GrantSource::User,
            hosts: permission.hosts.clone(),
            instances: permission.instances.clone(),
        };
        self.file
            .plugins
            .entry(manifest.id.clone())
            .or_default()
            .insert(cap.name, grant.clone());
        self.audit(
            &manifest.id,
            capability,
            "granted",
            &scope_label(&grant.hosts, &grant.instances),
        );
        Ok(grant)
    }

    pub fn revoke(&mut self, plugin_id: &str, capability: &str) -> Result<bool> {
        let removed = self
            .file
            .plugins
            .get_mut(plugin_id)
            .map(|m| m.remove(capability).is_some())
            .unwrap_or(false);
        if removed {
            self.audit(plugin_id, capability, "revoked", "");
        }
        Ok(removed)
    }

    pub fn revoke_all(&mut self, plugin_id: &str) {
        if self.file.plugins.remove(plugin_id).is_some() {
            self.audit(plugin_id, "*", "revoked_all", "");
        }
    }

    /// Host Broker 的权威检查点：**每次调用都重新检查**。
    pub fn authorize(
        &self,
        plugin_id: &str,
        capability: &str,
        host: Option<&str>,
        instance_id: Option<&str>,
    ) -> Result<()> {
        let cap = Capability::parse(capability)?;
        let grant = self
            .file
            .plugins
            .get(plugin_id)
            .and_then(|m| m.get(&cap.name));
        let Some(grant) = grant else {
            self.audit(plugin_id, capability, "denied", "not_granted");
            return Err(plugin_error(
                PluginErrorCode::PermissionDenied,
                format!("插件 {plugin_id} 未获得能力 {}", cap.name),
            ));
        };
        if cap.requires_hosts {
            let requested = host.ok_or_else(|| {
                plugin_error(
                    PluginErrorCode::PermissionDenied,
                    format!("能力 {} 需要 host 作用域", cap.name),
                )
            })?;
            if !host_allowed(requested, &grant.hosts) {
                self.audit(plugin_id, capability, "denied", "host_not_allowed");
                return Err(plugin_error(
                    PluginErrorCode::PermissionDenied,
                    format!("host {requested} 不在授权范围内"),
                ));
            }
        }
        if !grant.instances.is_empty() {
            let requested = instance_id.ok_or_else(|| {
                plugin_error(
                    PluginErrorCode::PermissionDenied,
                    format!("能力 {} 需要实例作用域", cap.name),
                )
            })?;
            if !grant.instances.iter().any(|i| i == requested) {
                self.audit(plugin_id, capability, "denied", "instance_not_allowed");
                return Err(plugin_error(
                    PluginErrorCode::PermissionDenied,
                    format!("实例 {requested} 不在授权范围内"),
                ));
            }
        }
        self.audit(plugin_id, capability, "allowed", "");
        Ok(())
    }

    /// 追加审计行；失败只记日志（不影响主流程，但会留下 tracing 警告）。
    fn audit(&self, plugin_id: &str, capability: &str, decision: &str, detail: &str) {
        let event = AuditEvent {
            timestamp: now_rfc3339(),
            plugin_id: plugin_id.to_string(),
            capability: capability.to_string(),
            decision: decision.to_string(),
            detail: detail.to_string(),
        };
        if let Err(e) = append_audit(&self.audit_path, &event) {
            tracing::warn!(error = %e, plugin_id, capability, "写入插件审计日志失败");
        }
    }
}

fn host_allowed(host: &str, allowed: &[String]) -> bool {
    allowed.iter().any(|pattern| {
        if let Some(suffix) = pattern.strip_prefix("*.") {
            host.ends_with(suffix)
                && host.len() > suffix.len()
                && host.as_bytes()[host.len() - suffix.len() - 1] == b'.'
        } else {
            host == pattern
        }
    })
}

fn scope_label(hosts: &[String], instances: &[String]) -> String {
    let mut parts = Vec::new();
    if !hosts.is_empty() {
        parts.push(format!("hosts={}", hosts.join(",")));
    }
    if !instances.is_empty() {
        parts.push(format!("instances={}", instances.join(",")));
    }
    parts.join(";")
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditEvent {
    pub timestamp: String,
    pub plugin_id: String,
    pub capability: String,
    pub decision: String,
    pub detail: String,
}

fn append_audit(path: &Path, event: &AuditEvent) -> Result<()> {
    use std::io::Write;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)?;
    let line = serde_json::to_string(event)?;
    file.write_all(line.as_bytes())?;
    file.write_all(b"\n")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use rmccc_platform::Platform;

    fn temp_store(tag: &str) -> (PermissionStore, PathBuf) {
        let dir =
            std::env::temp_dir().join(format!("rmccc-plugin-perm-{tag}-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(dir.join("plugin-state")).unwrap();
        let platform = Platform::current();
        let (store, _) = PermissionStore::load(
            dir.join("plugin-state/permissions.json"),
            dir.join("plugin-state/audit.jsonl"),
            platform.fs.clone(),
        );
        (store, dir)
    }

    fn manifest() -> PluginManifest {
        PluginManifest::parse(
            r#"{
                "manifestVersion": 1,
                "id": "dev.rmccc.demo",
                "name": "Demo",
                "version": "1.0.0",
                "publisher": { "id": "dev.rmccc", "name": "Examples" },
                "compatibility": { "pluginApi": "1.x", "core": ">=0.1.0 <0.2.0" },
                "permissions": [
                    { "capability": "dashboard.cards" },
                    { "capability": "network.request", "hosts": ["api.example.com", "*.example.org"] }
                ]
            }"#,
        )
        .unwrap()
    }

    #[test]
    fn auto_grants_only_low_risk() {
        let (mut store, dir) = temp_store("auto");
        store.auto_grant_low(&manifest()).unwrap();
        assert!(store.file().is_granted("dev.rmccc.demo", "dashboard.cards"));
        assert!(!store.file().is_granted("dev.rmccc.demo", "network.request"));
        assert_eq!(store.pending(&manifest()).unwrap().len(), 1);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn authorize_checks_host_and_revocation() {
        let (mut store, dir) = temp_store("authorize");
        let m = manifest();
        store.auto_grant_low(&m).unwrap();
        store.grant(&m, "network.request").unwrap();
        store
            .authorize(
                "dev.rmccc.demo",
                "network.request",
                Some("api.example.com"),
                None,
            )
            .unwrap();
        store
            .authorize(
                "dev.rmccc.demo",
                "network.request",
                Some("x.example.org"),
                None,
            )
            .unwrap();
        assert!(store
            .authorize(
                "dev.rmccc.demo",
                "network.request",
                Some("evil.example.net"),
                None
            )
            .is_err());
        assert!(store
            .authorize("dev.rmccc.demo", "network.request", None, None)
            .is_err());

        store.revoke("dev.rmccc.demo", "network.request").unwrap();
        assert!(store
            .authorize(
                "dev.rmccc.demo",
                "network.request",
                Some("api.example.com"),
                None
            )
            .is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn upgrade_does_not_expand_permissions() {
        let (mut store, dir) = temp_store("upgrade");
        let mut m = manifest();
        store.auto_grant_low(&m).unwrap();
        // 模拟旧版本已经授予 network.request。
        store.grant(&m, "network.request").unwrap();

        // 新版本新增 logs.read：不能被自动授予。
        m.version = "1.1.0".into();
        m.permissions.push(crate::manifest::PermissionRequest {
            capability: "logs.read".into(),
            hosts: Vec::new(),
            instances: Vec::new(),
            reason: None,
        });
        store.reconcile_upgrade(&m).unwrap();
        assert!(store.file().is_granted("dev.rmccc.demo", "network.request"));
        assert!(!store.file().is_granted("dev.rmccc.demo", "logs.read"));
        assert_eq!(store.pending(&m).unwrap().len(), 1);

        // 移除声明的能力时授权被清理。
        m.permissions.retain(|p| p.capability != "network.request");
        store.reconcile_upgrade(&m).unwrap();
        assert!(!store.file().is_granted("dev.rmccc.demo", "network.request"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn audit_log_records_decisions_without_secrets() {
        let (mut store, dir) = temp_store("audit");
        let m = manifest();
        store.auto_grant_low(&m).unwrap();
        let _ = store.authorize("dev.rmccc.demo", "dashboard.cards", None, None);
        let _ = store.authorize("dev.rmccc.demo", "network.request", None, None);
        let log = std::fs::read_to_string(dir.join("plugin-state/audit.jsonl")).unwrap();
        assert!(log.contains("\"decision\":\"allowed\""));
        assert!(log.contains("\"decision\":\"denied\""));
        assert!(log.contains("dashboard.cards"));
        assert!(!log.to_lowercase().contains("token"));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
