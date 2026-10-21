//! 插件注册表与生命周期状态机（ADR-003 决策 3，指南 §11.1）。

use crate::capability::plugin_error;
use crate::manifest::PluginManifest;
use rmccc_error::{PluginErrorCode, Result, RmcccError};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use ts_rs::TS;

pub const REGISTRY_SCHEMA_VERSION: u32 = 1;

/// 生命周期状态。transient 状态（downloading/verifying/…）允许出现在内存，
/// 但只有稳定状态会被持久化（安装器在提交时写 installed_disabled/enabled）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export_to = "types.ts")]
pub enum PluginState {
    NotInstalled,
    Downloading,
    Verifying,
    Staged,
    InstalledDisabled,
    Enabled,
    Disabling,
    Disabled,
    Uninstalling,
    Removed,
    Failed,
}

impl PluginState {
    /// 状态机转移表（指南 §11.1）。任何未列出的转移都拒绝。
    pub fn can_transition_to(self, next: PluginState) -> bool {
        use PluginState::*;
        match (self, next) {
            (a, b) if a == b => true,
            (NotInstalled, Downloading)
            | (NotInstalled, Failed)
            | (Downloading, Verifying)
            | (Downloading, Failed)
            | (Verifying, Staged)
            | (Verifying, Failed)
            | (Staged, InstalledDisabled)
            | (Staged, Failed)
            | (InstalledDisabled, Enabled)
            | (InstalledDisabled, Uninstalling)
            // 升级/回滚后必须重新审查权限，回到"已安装但禁用"。
            | (Enabled, InstalledDisabled)
            | (Disabled, InstalledDisabled)
            | (InstalledDisabled, Failed)
            | (Enabled, Disabling)
            | (Enabled, Failed)
            | (Disabling, Disabled)
            | (Disabling, Failed)
            | (Disabled, Enabled)
            | (Disabled, Uninstalling)
            | (Disabled, Failed)
            | (Uninstalling, Removed)
            | (Uninstalling, Failed)
            // 失败恢复：重新校验、回到已安装但禁用、或卸载。
            | (Failed, Verifying)
            | (Failed, InstalledDisabled)
            | (Failed, Uninstalling)
            | (Failed, Removed) => true,
            _ => false,
        }
    }

    pub fn is_installed(self) -> bool {
        matches!(
            self,
            PluginState::InstalledDisabled
                | PluginState::Enabled
                | PluginState::Disabling
                | PluginState::Disabled
                | PluginState::Failed
        )
    }

    pub fn is_enabled(self) -> bool {
        matches!(self, PluginState::Enabled)
    }

    /// 是否出现在默认列表中（removed 是墓碑）。
    pub fn is_visible(self) -> bool {
        self != PluginState::Removed && self != PluginState::NotInstalled
    }

    pub fn as_str(self) -> &'static str {
        match self {
            PluginState::NotInstalled => "not_installed",
            PluginState::Downloading => "downloading",
            PluginState::Verifying => "verifying",
            PluginState::Staged => "staged",
            PluginState::InstalledDisabled => "installed_disabled",
            PluginState::Enabled => "enabled",
            PluginState::Disabling => "disabling",
            PluginState::Disabled => "disabled",
            PluginState::Uninstalling => "uninstalling",
            PluginState::Removed => "removed",
            PluginState::Failed => "failed",
        }
    }
}

/// 信任级别。Phase 2 没有签名通道，所有安装记为 `development`（ADR-003 决策 5）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export_to = "types.ts")]
pub enum TrustLevel {
    /// 官方内置贡献（Phase 2 不允许第三方安装到该级别）。
    Official,
    /// 未签名开发包：安装/启用时显示风险提示。
    Development,
}

impl TrustLevel {
    pub fn label(self) -> &'static str {
        match self {
            TrustLevel::Official => "official",
            TrustLevel::Development => "development",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export_to = "types.ts")]
pub enum PluginSourceKind {
    Local,
    Registry,
    Builtin,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export_to = "types.ts")]
pub struct PluginSource {
    pub kind: PluginSourceKind,
    #[serde(default)]
    pub reference: Option<String>,
}

impl PluginSource {
    pub fn local(path: &str) -> Self {
        Self {
            kind: PluginSourceKind::Local,
            reference: Some(path.to_string()),
        }
    }

    pub fn builtin() -> Self {
        Self {
            kind: PluginSourceKind::Builtin,
            reference: None,
        }
    }
}

/// 一个已安装插件的注册记录（`plugin-state/registry.json`）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export_to = "types.ts")]
pub struct PluginRecord {
    pub id: String,
    pub installed_version: String,
    #[serde(default)]
    pub previous_version: Option<String>,
    pub state: PluginState,
    pub trust: TrustLevel,
    pub source: PluginSource,
    pub installed_at: String,
    pub updated_at: String,
    #[serde(default)]
    pub manifest_sha256: Option<String>,
    #[serde(default)]
    pub last_error: Option<String>,
}

impl PluginRecord {
    pub fn new(
        manifest: &PluginManifest,
        trust: TrustLevel,
        source: PluginSource,
        sha256: Option<String>,
        now: &str,
    ) -> Self {
        Self {
            id: manifest.id.clone(),
            installed_version: manifest.version.clone(),
            previous_version: None,
            state: PluginState::InstalledDisabled,
            trust,
            source,
            installed_at: now.to_string(),
            updated_at: now.to_string(),
            manifest_sha256: sha256,
            last_error: None,
        }
    }

    /// 校验状态转移并更新；非法转移返回 `PluginStateConflict`。
    pub fn transition(&mut self, next: PluginState, now: &str) -> Result<()> {
        if !self.state.can_transition_to(next) {
            return Err(plugin_error(
                PluginErrorCode::StateConflict,
                format!(
                    "插件 {} 不能从 {} 转移到 {}",
                    self.id,
                    self.state.as_str(),
                    next.as_str()
                ),
            ));
        }
        self.state = next;
        self.updated_at = now.to_string();
        Ok(())
    }

    pub fn mark_failed(&mut self, reason: &str, now: &str) {
        self.state = PluginState::Failed;
        self.last_error = Some(reason.to_string());
        self.updated_at = now.to_string();
    }
}

/// `registry.json` 的完整内容。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export_to = "types.ts")]
pub struct PluginRegistry {
    pub schema_version: u32,
    pub plugins: BTreeMap<String, PluginRecord>,
}

impl Default for PluginRegistry {
    fn default() -> Self {
        Self {
            schema_version: REGISTRY_SCHEMA_VERSION,
            plugins: BTreeMap::new(),
        }
    }
}

impl PluginRegistry {
    pub fn get(&self, id: &str) -> Option<&PluginRecord> {
        self.plugins.get(id)
    }

    pub fn get_mut(&mut self, id: &str) -> Option<&mut PluginRecord> {
        self.plugins.get_mut(id)
    }

    pub fn insert(&mut self, record: PluginRecord) {
        self.plugins.insert(record.id.clone(), record);
    }

    pub fn visible(&self) -> impl Iterator<Item = &PluginRecord> {
        self.plugins.values().filter(|r| r.state.is_visible())
    }

    pub fn enabled(&self) -> impl Iterator<Item = &PluginRecord> {
        self.plugins.values().filter(|r| r.state.is_enabled())
    }

    /// 安装后的完整性检查：注册表 schema 版本必须可读。
    pub fn validate_schema(&self) -> Result<()> {
        if self.schema_version == REGISTRY_SCHEMA_VERSION {
            Ok(())
        } else if self.schema_version > REGISTRY_SCHEMA_VERSION {
            Err(RmcccError::Plugin {
                code: PluginErrorCode::StateConflict,
                message: format!(
                    "插件注册表 schemaVersion {} 高于本版本支持的 {}（只读，避免覆盖新字段）",
                    self.schema_version, REGISTRY_SCHEMA_VERSION
                ),
            })
        } else {
            // 低版本：Phase 2 尚无历史版本，按当前版本读取（未来在此加迁移）。
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lifecycle_allows_expected_path() {
        use PluginState::*;
        let path = [
            NotInstalled,
            Downloading,
            Verifying,
            Staged,
            InstalledDisabled,
            Enabled,
            Disabling,
            Disabled,
            Uninstalling,
            Removed,
        ];
        for pair in path.windows(2) {
            assert!(
                pair[0].can_transition_to(pair[1]),
                "{} -> {}",
                pair[0].as_str(),
                pair[1].as_str()
            );
        }
    }

    #[test]
    fn lifecycle_rejects_shortcuts() {
        use PluginState::*;
        assert!(!NotInstalled.can_transition_to(Enabled));
        assert!(!Staged.can_transition_to(Enabled));
        assert!(!Removed.can_transition_to(Enabled));
        // 升级/回滚是显式例外：回到待审查状态，而不是绕过状态机。
        assert!(Enabled.can_transition_to(InstalledDisabled));
        assert!(Disabled.can_transition_to(InstalledDisabled));
        assert!(Failed.can_transition_to(InstalledDisabled));
    }

    #[test]
    fn record_transition_updates_timestamp() {
        let mut record = PluginRecord {
            id: "dev.rmccc.x".into(),
            installed_version: "1.0.0".into(),
            previous_version: None,
            state: PluginState::InstalledDisabled,
            trust: TrustLevel::Development,
            source: PluginSource::builtin(),
            installed_at: "2026-01-01T00:00:00Z".into(),
            updated_at: "2026-01-01T00:00:00Z".into(),
            manifest_sha256: None,
            last_error: None,
        };
        record
            .transition(PluginState::Enabled, "2026-01-02T00:00:00Z")
            .unwrap();
        assert_eq!(record.state, PluginState::Enabled);
        assert_eq!(record.updated_at, "2026-01-02T00:00:00Z");
        assert!(record
            .transition(PluginState::Staged, "2026-01-03T00:00:00Z")
            .is_err());
    }
}
