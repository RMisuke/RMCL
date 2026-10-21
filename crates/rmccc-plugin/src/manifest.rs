//! 插件 Manifest v1：解析 + 语义校验（ADR-004）。

use crate::capability::{validate_hosts, Capability};
use crate::path::{validate_local_id, validate_plugin_id, validate_relative_path};
use crate::version::{Version, VersionReq};
use rmccc_error::{PluginErrorCode, Result};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::capability::plugin_error;

pub const SUPPORTED_MANIFEST_VERSION: u32 = 1;
/// 宿主实现的插件 API 版本；Manifest 用 `MAJOR.x` 声明兼容。
pub const PLUGIN_API_VERSION: &str = "1.0.0";

/// 合法平台 ID（与 `PlatformDescriptor::plugin_platform_id()` 对应）。
pub const PLATFORM_IDS: &[&str] = &[
    "darwin-aarch64",
    "darwin-x86_64",
    "windows-x86_64",
    "windows-aarch64",
    "linux-x86_64",
    "linux-aarch64",
];

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export_to = "types.ts")]
pub struct Publisher {
    pub id: String,
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export_to = "types.ts")]
pub struct Compatibility {
    pub plugin_api: String,
    #[serde(default)]
    pub core: Option<String>,
    #[serde(default)]
    pub platforms: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export_to = "types.ts")]
pub struct Entry {
    #[serde(default)]
    pub ui: Option<String>,
    #[serde(default)]
    pub logic: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export_to = "types.ts")]
pub struct PermissionRequest {
    pub capability: String,
    #[serde(default)]
    pub hosts: Vec<String>,
    #[serde(default)]
    pub instances: Vec<String>,
    #[serde(default)]
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export_to = "types.ts")]
pub struct CardSize {
    pub w: u8,
    pub h: u8,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export_to = "types.ts")]
pub struct DashboardCard {
    pub id: String,
    pub title: String,
    #[serde(default)]
    pub description: Option<String>,
    pub default_size: CardSize,
    #[serde(default)]
    pub min_size: Option<CardSize>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export_to = "types.ts")]
pub struct SettingsPage {
    pub id: String,
    pub title: String,
    #[serde(default)]
    pub description: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export_to = "types.ts")]
pub struct Theme {
    pub id: String,
    pub name: String,
    pub path: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export_to = "types.ts")]
pub struct Contributes {
    #[serde(default)]
    pub dashboard_cards: Vec<DashboardCard>,
    #[serde(default)]
    pub settings_pages: Vec<SettingsPage>,
    #[serde(default)]
    pub themes: Vec<Theme>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export_to = "types.ts")]
pub struct ExtensionDependency {
    pub id: String,
    pub version: String,
    #[serde(default)]
    pub optional: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export_to = "types.ts")]
pub struct PluginManifest {
    pub manifest_version: u32,
    pub id: String,
    pub name: String,
    pub version: String,
    #[serde(default)]
    pub description: Option<String>,
    pub publisher: Publisher,
    pub compatibility: Compatibility,
    #[serde(default)]
    pub entry: Entry,
    #[serde(default)]
    pub permissions: Vec<PermissionRequest>,
    #[serde(default)]
    pub contributes: Contributes,
    #[serde(default)]
    pub extension_dependencies: Vec<ExtensionDependency>,
}

/// 校验所需的宿主事实（不读全局状态，便于测试）。
#[derive(Debug, Clone)]
pub struct ManifestContext {
    pub core_version: Version,
    pub platform_id: String,
}

impl ManifestContext {
    pub fn new(core_version: &str, platform_id: &str) -> Result<Self> {
        let core_version = Version::parse(core_version).ok_or_else(|| {
            plugin_error(
                PluginErrorCode::ManifestInvalid,
                format!("宿主 core 版本非法：{core_version}"),
            )
        })?;
        Ok(Self {
            core_version,
            platform_id: platform_id.to_string(),
        })
    }
}

impl PluginManifest {
    pub fn parse(json: &str) -> Result<Self> {
        serde_json::from_str(json).map_err(|e| {
            plugin_error(
                PluginErrorCode::ManifestInvalid,
                format!("manifest.json 解析失败：{e}"),
            )
        })
    }

    pub fn version(&self) -> Result<Version> {
        Version::parse(&self.version).ok_or_else(|| {
            plugin_error(
                PluginErrorCode::ManifestInvalid,
                format!("插件版本不是合法 SemVer：{:?}", self.version),
            )
        })
    }

    /// 解析后的能力列表（已去重、已校验）。
    pub fn capabilities(&self) -> Result<Vec<Capability>> {
        self.permissions
            .iter()
            .map(|p| Capability::parse(&p.capability))
            .collect()
    }

    pub fn capability_names(&self) -> Result<Vec<String>> {
        Ok(self.capabilities()?.into_iter().map(|c| c.name).collect())
    }
}

/// 执行 ADR-004 的 12 条校验中与"单文件语义"相关的部分。
///
/// 包级检查（大小/条目/路径穿越/重复名）在 [`crate::package`]，
/// 依赖图检查在 [`crate::manager`]。
pub fn validate_manifest(manifest: &PluginManifest, ctx: &ManifestContext) -> Result<()> {
    if manifest.manifest_version != SUPPORTED_MANIFEST_VERSION {
        return Err(plugin_error(
            PluginErrorCode::ManifestInvalid,
            format!(
                "不支持的 manifestVersion：{}（当前支持 {SUPPORTED_MANIFEST_VERSION}）",
                manifest.manifest_version
            ),
        ));
    }

    validate_plugin_id(&manifest.id)?;

    let name = manifest.name.trim();
    if name.is_empty() || name.chars().count() > 80 {
        return Err(plugin_error(
            PluginErrorCode::ManifestInvalid,
            "插件 name 必须非空且不超过 80 字符".to_string(),
        ));
    }
    if manifest
        .description
        .as_deref()
        .map(|d| d.chars().count() > 2000)
        .unwrap_or(false)
    {
        return Err(plugin_error(
            PluginErrorCode::ManifestInvalid,
            "description 不能超过 2000 字符".to_string(),
        ));
    }

    // 版本字符串。
    manifest.version()?;

    // 发布者。
    if manifest.publisher.name.trim().is_empty() || manifest.publisher.name.chars().count() > 80 {
        return Err(plugin_error(
            PluginErrorCode::ManifestInvalid,
            "publisher.name 必须非空且不超过 80 字符".to_string(),
        ));
    }
    validate_publisher_id(&manifest.publisher.id)?;

    // 平台 / API / Core 兼容性。
    validate_plugin_api(&manifest.compatibility.plugin_api)?;
    for platform in &manifest.compatibility.platforms {
        if !PLATFORM_IDS.contains(&platform.as_str()) {
            return Err(plugin_error(
                PluginErrorCode::ManifestInvalid,
                format!("未知平台标识：{platform:?}（见 ADR-004）"),
            ));
        }
    }
    if !manifest.compatibility.platforms.is_empty()
        && !manifest
            .compatibility
            .platforms
            .iter()
            .any(|p| p == &ctx.platform_id)
    {
        return Err(plugin_error(
            PluginErrorCode::ManifestInvalid,
            format!(
                "插件不支持当前平台 {}（声明：{:?}）",
                ctx.platform_id, manifest.compatibility.platforms
            ),
        ));
    }
    if let Some(core) = &manifest.compatibility.core {
        let req = VersionReq::parse(core).ok_or_else(|| {
            plugin_error(
                PluginErrorCode::ManifestInvalid,
                format!("compatibility.core 版本范围非法：{core:?}"),
            )
        })?;
        if !req.matches(&ctx.core_version) {
            return Err(plugin_error(
                PluginErrorCode::ManifestInvalid,
                format!("插件要求 core {core}，当前为 {}", ctx.core_version),
            ));
        }
    }

    // entry.ui：Phase 4 起允许（隔离容器，ADR-006）；entry.logic 仍然拒绝（Phase 5）。
    if let Some(ui) = &manifest.entry.ui {
        crate::ui::validate_ui_entry(ui)?;
        // 没有 UI 贡献项的入口是死文件：拒绝，避免包内藏无人加载的页面。
        let c = &manifest.contributes;
        if c.dashboard_cards.is_empty() && c.settings_pages.is_empty() {
            return Err(plugin_error(
                PluginErrorCode::ManifestInvalid,
                "entry.ui 需要至少一个 dashboardCards 或 settingsPages 贡献项".to_string(),
            ));
        }
    }
    if let Some(logic) = &manifest.entry.logic {
        validate_relative_path(logic)?;
        return Err(plugin_error(
            PluginErrorCode::ManifestInvalid,
            "Phase 2 不支持 entry.logic（逻辑运行时在 Phase 5 提供）".to_string(),
        ));
    }

    // 权限。
    if manifest.permissions.len() > 32 {
        return Err(plugin_error(
            PluginErrorCode::ManifestInvalid,
            "permissions 不能超过 32 条".to_string(),
        ));
    }
    let mut seen_caps = std::collections::BTreeSet::new();
    for permission in &manifest.permissions {
        let cap = Capability::parse(&permission.capability)?;
        if !seen_caps.insert(cap.name.clone()) {
            return Err(plugin_error(
                PluginErrorCode::ManifestInvalid,
                format!("权限重复声明：{}", cap.name),
            ));
        }
        if cap.requires_hosts && permission.hosts.is_empty() {
            return Err(plugin_error(
                PluginErrorCode::ManifestInvalid,
                format!("{} 必须声明 hosts", cap.name),
            ));
        }
        if !cap.requires_hosts && !permission.hosts.is_empty() {
            return Err(plugin_error(
                PluginErrorCode::ManifestInvalid,
                format!("{} 不接受 hosts", cap.name),
            ));
        }
        validate_hosts(&permission.hosts)?;
        if permission.instances.len() > 64 {
            return Err(plugin_error(
                PluginErrorCode::ManifestInvalid,
                "instances 不能超过 64 条".to_string(),
            ));
        }
        for id in &permission.instances {
            if uuid::Uuid::parse_str(id).is_err() {
                return Err(plugin_error(
                    PluginErrorCode::ManifestInvalid,
                    format!("instances 中的实例 id 不是 UUID：{id:?}"),
                ));
            }
        }
        if permission
            .reason
            .as_deref()
            .map(|r| r.chars().count() > 500)
            .unwrap_or(false)
        {
            return Err(plugin_error(
                PluginErrorCode::ManifestInvalid,
                "permission.reason 不能超过 500 字符".to_string(),
            ));
        }
    }

    // 贡献项。
    validate_contributions(manifest)?;

    // 依赖声明。
    if manifest.extension_dependencies.len() > 32 {
        return Err(plugin_error(
            PluginErrorCode::ManifestInvalid,
            "extensionDependencies 不能超过 32 条".to_string(),
        ));
    }
    let mut dep_ids = std::collections::BTreeSet::new();
    for dep in &manifest.extension_dependencies {
        validate_plugin_id(&dep.id)?;
        if dep.id == manifest.id {
            return Err(plugin_error(
                PluginErrorCode::ManifestInvalid,
                "插件不能依赖自身".to_string(),
            ));
        }
        if !dep_ids.insert(dep.id.clone()) {
            return Err(plugin_error(
                PluginErrorCode::ManifestInvalid,
                format!("依赖重复声明：{}", dep.id),
            ));
        }
        if VersionReq::parse(&dep.version).is_none() {
            return Err(plugin_error(
                PluginErrorCode::ManifestInvalid,
                format!("依赖 {} 的版本范围非法：{:?}", dep.id, dep.version),
            ));
        }
    }

    Ok(())
}

fn validate_plugin_api(api: &str) -> Result<()> {
    let (major, suffix) = api.split_once('.').ok_or_else(|| {
        plugin_error(
            PluginErrorCode::ManifestInvalid,
            format!("compatibility.pluginApi 必须是 MAJOR.x：{api:?}"),
        )
    })?;
    let supported_major = PLUGIN_API_VERSION
        .split_once('.')
        .map(|(m, _)| m)
        .unwrap_or("1");
    let ok = suffix == "x"
        && !major.is_empty()
        && major.chars().all(|c| c.is_ascii_digit())
        && major == supported_major;
    if ok {
        Ok(())
    } else {
        Err(plugin_error(
            PluginErrorCode::ManifestInvalid,
            format!("不兼容的 pluginApi：{api:?}（宿主支持 {supported_major}.x）"),
        ))
    }
}

fn validate_publisher_id(id: &str) -> Result<()> {
    let valid = !id.is_empty()
        && id.len() <= 128
        && id.split('.').all(|label| {
            !label.is_empty()
                && label.starts_with(|c: char| c.is_ascii_lowercase())
                && label
                    .chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
        });
    if valid {
        Ok(())
    } else {
        Err(plugin_error(
            PluginErrorCode::ManifestInvalid,
            format!("publisher.id 非法：{id:?}"),
        ))
    }
}

fn validate_contributions(manifest: &PluginManifest) -> Result<()> {
    let c = &manifest.contributes;
    if c.dashboard_cards.len() > 32 || c.settings_pages.len() > 32 || c.themes.len() > 8 {
        return Err(plugin_error(
            PluginErrorCode::ManifestInvalid,
            "贡献项数量超过上限（cards/settings 各 32，themes 8）".to_string(),
        ));
    }

    let mut ids = std::collections::BTreeSet::new();
    for card in &c.dashboard_cards {
        validate_local_id(&card.id)?;
        if !ids.insert(format!("dashboardCards/{}", card.id)) {
            return Err(plugin_error(
                PluginErrorCode::ManifestInvalid,
                format!("贡献 ID 重复：dashboardCards/{}", card.id),
            ));
        }
        if card.title.trim().is_empty() || card.title.chars().count() > 80 {
            return Err(plugin_error(
                PluginErrorCode::ManifestInvalid,
                format!("卡片 {} 的 title 非法", card.id),
            ));
        }
        validate_size("defaultSize", card.default_size)?;
        if let Some(min) = card.min_size {
            validate_size("minSize", min)?;
            if min.w > card.default_size.w || min.h > card.default_size.h {
                return Err(plugin_error(
                    PluginErrorCode::ManifestInvalid,
                    format!("卡片 {} 的 minSize 不能大于 defaultSize", card.id),
                ));
            }
        }
    }

    for page in &c.settings_pages {
        validate_local_id(&page.id)?;
        if !ids.insert(format!("settingsPages/{}", page.id)) {
            return Err(plugin_error(
                PluginErrorCode::ManifestInvalid,
                format!("贡献 ID 重复：settingsPages/{}", page.id),
            ));
        }
        if page.title.trim().is_empty() || page.title.chars().count() > 80 {
            return Err(plugin_error(
                PluginErrorCode::ManifestInvalid,
                format!("设置页 {} 的 title 非法", page.id),
            ));
        }
    }

    for theme in &c.themes {
        validate_local_id(&theme.id)?;
        if !ids.insert(format!("themes/{}", theme.id)) {
            return Err(plugin_error(
                PluginErrorCode::ManifestInvalid,
                format!("贡献 ID 重复：themes/{}", theme.id),
            ));
        }
        if theme.name.trim().is_empty() || theme.name.chars().count() > 80 {
            return Err(plugin_error(
                PluginErrorCode::ManifestInvalid,
                format!("主题 {} 的 name 非法", theme.id),
            ));
        }
        validate_relative_path(&theme.path)?;
        if !theme.path.starts_with("themes/") {
            return Err(plugin_error(
                PluginErrorCode::ManifestInvalid,
                format!("主题 {} 的 path 必须位于 themes/ 下", theme.id),
            ));
        }
    }

    Ok(())
}

fn validate_size(field: &str, size: CardSize) -> Result<()> {
    if !(1..=24).contains(&size.w) || !(1..=24).contains(&size.h) {
        return Err(plugin_error(
            PluginErrorCode::ManifestInvalid,
            format!("{field} 的 w/h 必须在 1..=24"),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx() -> ManifestContext {
        ManifestContext::new("0.1.0", "darwin-aarch64").unwrap()
    }

    const MINIMAL: &str = r#"{
        "manifestVersion": 1,
        "id": "dev.rmccc.theme-midnight",
        "name": "Midnight",
        "version": "1.0.0",
        "publisher": { "id": "dev.rmccc", "name": "RMCCC Examples" },
        "compatibility": { "pluginApi": "1.x", "core": ">=0.1.0 <0.2.0" }
    }"#;

    #[test]
    fn parses_minimal_manifest() {
        let m = PluginManifest::parse(MINIMAL).unwrap();
        assert_eq!(m.id, "dev.rmccc.theme-midnight");
        validate_manifest(&m, &ctx()).unwrap();
    }

    #[test]
    fn rejects_unknown_fields() {
        let json = MINIMAL.replace(
            "\"version\": \"1.0.0\",",
            "\"version\": \"1.0.0\", \"verison\": \"typo\",",
        );
        assert!(PluginManifest::parse(&json).is_err());
    }

    #[test]
    fn rejects_unsupported_manifest_version() {
        let json = MINIMAL.replace("\"manifestVersion\": 1", "\"manifestVersion\": 2");
        let m = PluginManifest::parse(&json).unwrap();
        assert!(validate_manifest(&m, &ctx()).is_err());
    }

    #[test]
    fn rejects_bad_id_and_version() {
        let m =
            PluginManifest::parse(&MINIMAL.replace("dev.rmccc.theme-midnight", "Bad_ID")).unwrap();
        assert!(validate_manifest(&m, &ctx()).is_err());

        let m = PluginManifest::parse(&MINIMAL.replace("\"1.0.0\"", "\"v1.0\"")).unwrap();
        assert!(validate_manifest(&m, &ctx()).is_err());
    }

    #[test]
    fn rejects_platform_mismatch() {
        let json = MINIMAL.replace(
            "\"pluginApi\": \"1.x\",",
            "\"pluginApi\": \"1.x\", \"platforms\": [\"windows-x86_64\"],",
        );
        let m = PluginManifest::parse(&json).unwrap();
        assert!(validate_manifest(&m, &ctx()).is_err());
    }

    #[test]
    fn rejects_core_range_mismatch() {
        let json = MINIMAL.replace(">=0.1.0 <0.2.0", ">=0.2.0 <0.3.0");
        let m = PluginManifest::parse(&json).unwrap();
        assert!(validate_manifest(&m, &ctx()).is_err());
    }

    #[test]
    fn accepts_a_ui_entry_that_backs_a_card() {
        let json = MINIMAL.replace(
            "\"compatibility\": { \"pluginApi\": \"1.x\", \"core\": \">=0.1.0 <0.2.0\" }",
            "\"compatibility\": { \"pluginApi\": \"1.x\", \"core\": \">=0.1.0 <0.2.0\" }, \
             \"entry\": { \"ui\": \"ui/index.html\" }, \
             \"contributes\": { \"dashboardCards\": [ { \"id\": \"card\", \"title\": \"Card\", \
             \"defaultSize\": { \"w\": 4, \"h\": 3 } } ] }",
        );
        let m = PluginManifest::parse(&json).unwrap();
        validate_manifest(&m, &ctx()).expect("ui entry with a card should validate");
    }

    #[test]
    fn rejects_a_ui_entry_without_ui_contributions() {
        let json = MINIMAL.replace(
            "\"compatibility\": { \"pluginApi\": \"1.x\", \"core\": \">=0.1.0 <0.2.0\" }",
            "\"compatibility\": { \"pluginApi\": \"1.x\", \"core\": \">=0.1.0 <0.2.0\" }, \
             \"entry\": { \"ui\": \"ui/index.html\" }",
        );
        let m = PluginManifest::parse(&json).unwrap();
        let err = validate_manifest(&m, &ctx()).unwrap_err().to_string();
        assert!(err.contains("dashboardCards"), "{err}");
    }

    #[test]
    fn rejects_a_ui_entry_outside_ui_or_not_html() {
        for entry in ["index.html", "ui/app.js", "ui/../index.html"] {
            let json = MINIMAL.replace(
                "\"compatibility\": { \"pluginApi\": \"1.x\", \"core\": \">=0.1.0 <0.2.0\" }",
                &format!(
                    "\"compatibility\": {{ \"pluginApi\": \"1.x\", \"core\": \">=0.1.0 <0.2.0\" }}, \
                     \"entry\": {{ \"ui\": \"{entry}\" }}, \
                     \"contributes\": {{ \"dashboardCards\": [ {{ \"id\": \"card\", \"title\": \"Card\", \
                     \"defaultSize\": {{ \"w\": 4, \"h\": 3 }} }} ] }}"
                ),
            );
            let m = PluginManifest::parse(&json).unwrap();
            assert!(
                validate_manifest(&m, &ctx()).is_err(),
                "expected reject: {entry}"
            );
        }
    }

    #[test]
    fn rejects_logic_entries_until_phase_5() {
        let json = MINIMAL.replace(
            "\"compatibility\": { \"pluginApi\": \"1.x\", \"core\": \">=0.1.0 <0.2.0\" }",
            "\"compatibility\": { \"pluginApi\": \"1.x\", \"core\": \">=0.1.0 <0.2.0\" }, \
             \"entry\": { \"logic\": \"logic/plugin.wasm\" }",
        );
        let m = PluginManifest::parse(&json).unwrap();
        let err = validate_manifest(&m, &ctx()).unwrap_err().to_string();
        assert!(err.contains("Phase 5"), "{err}");
    }
}
