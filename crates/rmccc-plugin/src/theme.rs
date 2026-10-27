//! 主题文档（插件包内 `themes/<id>.json`）的格式与校验（ADR-005 / ADR-014）。
//!
//! 主题是**纯数据**：宿主只读设计值并映射成 CSS 变量/宿主特效，不执行任何代码。
//! 本模块负责把不可信数据收窄成有限语法：
//!
//! - v1：8 个颜色令牌（ADR-005）；
//! - v2：颜色 + 字体/形状/间距/背景图/视频/组件槽/主界面特效/Tab 装饰（ADR-014）。
//!
//! 颜色只接受 `#rgb` / `#rrggbb` / `#rrggbbaa`；所有数值都有范围；未知字段拒绝；
//! 资产只允许包内 `assets/` 下的图片/字体/视频。校验同时被包检查、主题选择、
//! 运行期令牌三条路径复用，保证"能打包 = 能安装 = 能应用"。

use rmccc_error::{PluginErrorCode, Result, RmcccError};
use serde::Deserialize;
use std::collections::BTreeMap;

use crate::path::validate_relative_path;

/// 主题文档 v1 schema 版本。
pub const THEME_SCHEMA_VERSION: u32 = 1;
/// 主题文档 v2 schema 版本（外观皮肤，ADR-014）。
pub const THEME_SCHEMA_VERSION_V2: u32 = 2;

/// 单个主题文件上限。
pub const MAX_THEME_BYTES: u64 = 64 * 1024;

/// 主题名长度上限（字符数）。
pub const MAX_THEME_NAME_CHARS: usize = 80;

/// 必需颜色令牌。
pub const REQUIRED_COLORS: &[&str] = &["background", "surface", "text", "accent", "border"];

/// 可选颜色令牌（camelCase 与 snake_case 都接受，camelCase 优先）。
pub const OPTIONAL_COLORS: &[&str] = &[
    "surfaceElevated",
    "surface_elevated",
    "textMuted",
    "text_muted",
    "accentContrast",
    "accent_contrast",
];

/// 宿主安全语义色：允许出现在文件里，但**不会**被应用。
pub const SAFETY_COLORS: &[&str] = &["danger", "success", "warning"];

/// 允许的组件槽（白名单）。
pub const COMPONENT_SLOTS: &[&str] = &[
    "card", "button", "input", "menu", "dialog", "tag", "hero", "titlebar",
];

/// 允许自定义图标的导航项（与前端 `IconName` 对齐）。
pub const NAV_ICON_KEYS: &[&str] = &[
    "home",
    "gamepad",
    "download",
    "person",
    "extension",
    "settings",
];

/// 主题资产种类（决定扩展名/大小/魔数校验）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum AssetKind {
    /// 图片：png/jpg/jpeg/webp/svg。
    Image,
    /// 视频：mp4/webm。
    Video,
    /// 字体：woff2/woff/ttf/otf。
    Font,
}

/// 一条被主题引用的资产。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AssetSpec {
    pub path: String,
    pub kind: AssetKind,
}

/// 主题文档（`themes/<id>.json`）。
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ThemeDocument {
    pub schema_version: u32,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub mode: Option<String>,
    #[serde(default)]
    pub colors: BTreeMap<String, String>,

    // ── v2 外观皮肤（ADR-014）──
    #[serde(default)]
    pub typography: Option<Typography>,
    #[serde(default)]
    pub shape: Option<Shape>,
    #[serde(default)]
    pub spacing: Option<Spacing>,
    #[serde(default)]
    pub background: Option<Background>,
    #[serde(default)]
    pub components: Option<BTreeMap<String, Component>>,
    #[serde(default)]
    pub effects: Vec<Effect>,
    #[serde(default)]
    pub navigation: Option<Navigation>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Default)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Typography {
    #[serde(default)]
    pub ui: Option<FontSpec>,
    #[serde(default)]
    pub display: Option<FontSpec>,
    #[serde(default)]
    pub mono: Option<FontSpec>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FontSpec {
    pub family: String,
    #[serde(default)]
    pub fallback: Option<String>,
    #[serde(default)]
    pub files: Vec<FontFile>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FontFile {
    pub src: String,
    #[serde(default)]
    pub weight: Option<String>,
    #[serde(default)]
    pub style: Option<String>,
    #[serde(default)]
    pub display: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Default)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Shape {
    #[serde(default)]
    pub tag: Option<u8>,
    #[serde(default)]
    pub control: Option<u8>,
    #[serde(default)]
    pub card: Option<u8>,
    #[serde(default)]
    pub modal: Option<u8>,
    #[serde(default)]
    pub pill: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Default)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Spacing {
    #[serde(default)]
    pub unit: Option<u8>,
    #[serde(default)]
    pub density: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Default)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Background {
    #[serde(default)]
    pub image: Option<String>,
    #[serde(default)]
    pub video: Option<String>,
    #[serde(default)]
    pub poster: Option<String>,
    #[serde(default)]
    pub fit: Option<String>,
    #[serde(default)]
    pub position: Option<String>,
    #[serde(default)]
    pub dim: Option<f32>,
    #[serde(default)]
    pub blur: Option<u8>,
    #[serde(default)]
    pub scope: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Default)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Component {
    #[serde(default)]
    pub background: Option<String>,
    #[serde(default)]
    pub border: Option<String>,
    #[serde(default)]
    pub text: Option<String>,
    #[serde(default)]
    pub radius: Option<u8>,
    #[serde(default)]
    pub shadow: Option<String>,
    #[serde(default)]
    pub blur: Option<u8>,
    #[serde(default)]
    pub padding: Option<u8>,
    #[serde(default)]
    pub image: Option<String>,
    #[serde(default)]
    pub image_slice: Option<u8>,
    #[serde(default)]
    pub image_mode: Option<String>,
    /// 仅 button：primary 按钮的底色/文字色。
    #[serde(default)]
    pub primary_background: Option<String>,
    #[serde(default)]
    pub primary_text: Option<String>,
}

/// 主界面特效（宿主预设，主题只给参数）。
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(tag = "type", rename_all = "kebab-case", deny_unknown_fields)]
pub enum Effect {
    /// 漂浮/旋转的方块（可用包内贴图做草方块等风格）。
    RotatingBlocks {
        texture: String,
        #[serde(default)]
        count: Option<u8>,
        #[serde(default)]
        size: Option<u8>,
        #[serde(default)]
        speed: Option<u8>,
        #[serde(default)]
        opacity: Option<f32>,
        #[serde(default)]
        area: Option<String>,
    },
    /// 流动的极光/光晕层。
    Aurora {
        #[serde(default)]
        intensity: Option<f32>,
        #[serde(default)]
        speed: Option<u8>,
        #[serde(default)]
        area: Option<String>,
    },
    /// 粒子（圆点/星形/方块）。
    Particles {
        #[serde(default)]
        kind: Option<String>,
        color: String,
        #[serde(default)]
        count: Option<u8>,
        #[serde(default)]
        speed: Option<u8>,
        #[serde(default)]
        area: Option<String>,
    },
}

#[derive(Debug, Clone, PartialEq, Deserialize, Default)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Navigation {
    #[serde(default)]
    pub icons: BTreeMap<String, String>,
    #[serde(default)]
    pub ornaments: Vec<Ornament>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Ornament {
    pub image: String,
    #[serde(default)]
    pub side: Option<String>,
    #[serde(default)]
    pub width: Option<u8>,
    #[serde(default)]
    pub opacity: Option<f32>,
}

impl ThemeDocument {
    /// 解析并校验一个主题文件（大小上限 + schema + 语义/颜色/范围）。
    ///
    /// 这是主题进入系统的**唯一入口**：调用方不得绕开它直接读字段。
    pub fn parse(raw: &str) -> Result<Self> {
        if raw.len() as u64 > MAX_THEME_BYTES {
            return Err(theme_error(format!(
                "主题文件过大（{} 字节，上限 {MAX_THEME_BYTES}）",
                raw.len()
            )));
        }
        let document: Self =
            serde_json::from_str(raw).map_err(|e| theme_error(format!("主题 JSON 非法：{e}")))?;
        document.validate()?;
        Ok(document)
    }

    /// 是否使用了 v2 皮肤能力（字体/形状/间距/背景/组件/特效/导航）。
    pub fn requires_skin(&self) -> bool {
        self.typography.is_some()
            || self.shape.is_some()
            || self.spacing.is_some()
            || self.background.is_some()
            || self.components.as_ref().is_some_and(|c| !c.is_empty())
            || !self.effects.is_empty()
            || self.navigation.is_some()
    }

    /// 语义校验：版本、名称、mode、颜色、v2 各段范围。
    pub fn validate(&self) -> Result<()> {
        if !matches!(
            self.schema_version,
            THEME_SCHEMA_VERSION | THEME_SCHEMA_VERSION_V2
        ) {
            return Err(theme_error(format!(
                "不支持的 schemaVersion {}（宿主支持 {THEME_SCHEMA_VERSION} / {THEME_SCHEMA_VERSION_V2}）",
                self.schema_version
            )));
        }
        if self.schema_version == THEME_SCHEMA_VERSION && self.requires_skin() {
            return Err(theme_error(
                "v2 外观字段需要 schemaVersion: 2（v1 只允许 colors）".to_string(),
            ));
        }
        if let Some(name) = &self.name {
            if name.trim().is_empty() || name.chars().count() > MAX_THEME_NAME_CHARS {
                return Err(theme_error(format!(
                    "主题 name 非法（必须非空且 ≤ {MAX_THEME_NAME_CHARS} 字符）"
                )));
            }
        }
        if let Some(mode) = self.declared_mode() {
            if mode != "dark" && mode != "light" {
                return Err(theme_error(format!(
                    "主题 mode 只允许 dark / light，收到 {mode:?}"
                )));
            }
        }
        for key in REQUIRED_COLORS {
            if !self.colors.contains_key(*key) {
                return Err(theme_error(format!("主题缺少必需颜色令牌：{key}")));
            }
        }
        for (key, value) in &self.colors {
            if is_known_key(key) {
                validate_color(value)?;
            }
        }

        if let Some(typography) = &self.typography {
            typography.validate()?;
        }
        if let Some(shape) = &self.shape {
            shape.validate()?;
        }
        if let Some(spacing) = &self.spacing {
            spacing.validate()?;
        }
        if let Some(background) = &self.background {
            background.validate()?;
        }
        if let Some(components) = &self.components {
            if components.len() > COMPONENT_SLOTS.len() {
                return Err(theme_error("components 槽位数量超过上限".to_string()));
            }
            for (slot, component) in components {
                if !COMPONENT_SLOTS.contains(&slot.as_str()) {
                    return Err(theme_error(format!(
                        "未知组件槽 {slot:?}（允许：{}）",
                        COMPONENT_SLOTS.join(", ")
                    )));
                }
                component.validate(slot)?;
            }
        }
        if self.effects.len() > 3 {
            return Err(theme_error("effects 最多 3 条".to_string()));
        }
        for effect in &self.effects {
            effect.validate()?;
        }
        if let Some(navigation) = &self.navigation {
            navigation.validate()?;
        }
        Ok(())
    }

    /// 作者显式声明的明暗；`None` 时由宿主按背景色亮度推断。
    pub fn declared_mode(&self) -> Option<&str> {
        self.mode.as_deref()
    }

    /// 主题引用的全部资产（去重；路径已做语法校验）。
    pub fn asset_specs(&self) -> Vec<AssetSpec> {
        let mut out: BTreeMap<String, AssetKind> = BTreeMap::new();
        let mut add = |path: &str, kind: AssetKind| {
            out.entry(path.to_string()).or_insert(kind);
        };

        if let Some(typography) = &self.typography {
            for spec in [&typography.ui, &typography.display, &typography.mono]
                .into_iter()
                .flatten()
            {
                for file in &spec.files {
                    add(&file.src, AssetKind::Font);
                }
            }
        }
        if let Some(background) = &self.background {
            if let Some(image) = &background.image {
                add(image, AssetKind::Image);
            }
            if let Some(poster) = &background.poster {
                add(poster, AssetKind::Image);
            }
            if let Some(video) = &background.video {
                add(video, AssetKind::Video);
            }
        }
        if let Some(components) = &self.components {
            for component in components.values() {
                if let Some(image) = &component.image {
                    add(image, AssetKind::Image);
                }
            }
        }
        for effect in &self.effects {
            if let Effect::RotatingBlocks { texture, .. } = effect {
                add(texture, AssetKind::Image);
            }
        }
        if let Some(navigation) = &self.navigation {
            for icon in navigation.icons.values() {
                add(icon, AssetKind::Image);
            }
            for ornament in &navigation.ornaments {
                add(&ornament.image, AssetKind::Image);
            }
        }
        drop(add);
        out.into_iter()
            .map(|(path, kind)| AssetSpec { path, kind })
            .collect()
    }
}

impl Typography {
    fn validate(&self) -> Result<()> {
        if self.ui.is_none() && self.display.is_none() && self.mono.is_none() {
            return Err(theme_error("typography 至少要声明一个字体槽".to_string()));
        }
        for spec in [&self.ui, &self.display, &self.mono].into_iter().flatten() {
            spec.validate()?;
        }
        Ok(())
    }
}

impl FontSpec {
    fn validate(&self) -> Result<()> {
        validate_font_family(&self.family)?;
        if let Some(fallback) = &self.fallback {
            if !matches!(
                fallback.as_str(),
                "sans-serif" | "serif" | "monospace" | "system-ui"
            ) {
                return Err(theme_error(format!(
                    "字体 fallback 只允许 sans-serif/serif/monospace/system-ui，收到 {fallback:?}"
                )));
            }
        }
        if self.files.len() > 4 {
            return Err(theme_error("单个字体槽最多 4 个字体文件".to_string()));
        }
        for file in &self.files {
            validate_asset_path(&file.src, "字体")?;
            if let Some(weight) = &file.weight {
                validate_font_weight(weight)?;
            }
            if let Some(style) = &file.style {
                if style != "normal" && style != "italic" {
                    return Err(theme_error(format!(
                        "字体 style 只允许 normal/italic，收到 {style:?}"
                    )));
                }
            }
            if let Some(display) = &file.display {
                if !matches!(
                    display.as_str(),
                    "auto" | "swap" | "block" | "fallback" | "optional"
                ) {
                    return Err(theme_error(format!("字体 display 非法：{display:?}")));
                }
            }
        }
        Ok(())
    }
}

impl Shape {
    fn validate(&self) -> Result<()> {
        for (name, value) in [
            ("tag", self.tag),
            ("control", self.control),
            ("card", self.card),
            ("modal", self.modal),
        ] {
            if let Some(value) = value {
                if value > 32 {
                    return Err(theme_error(format!("shape.{name} 必须在 0..=32")));
                }
            }
        }
        if self.tag.is_none()
            && self.control.is_none()
            && self.card.is_none()
            && self.modal.is_none()
            && self.pill.is_none()
        {
            return Err(theme_error("shape 至少要声明一个字段".to_string()));
        }
        Ok(())
    }
}

impl Spacing {
    fn validate(&self) -> Result<()> {
        if let Some(unit) = self.unit {
            if !(3..=6).contains(&unit) {
                return Err(theme_error("spacing.unit 必须在 3..=6".to_string()));
            }
        }
        if let Some(density) = &self.density {
            if !matches!(density.as_str(), "compact" | "comfortable" | "spacious") {
                return Err(theme_error(format!(
                    "spacing.density 只允许 compact/comfortable/spacious，收到 {density:?}"
                )));
            }
        }
        if self.unit.is_none() && self.density.is_none() {
            return Err(theme_error(
                "spacing 至少要声明 unit 或 density".to_string(),
            ));
        }
        Ok(())
    }
}

impl Background {
    fn validate(&self) -> Result<()> {
        if self.image.is_none() && self.video.is_none() {
            return Err(theme_error(
                "background 至少需要 image 或 video".to_string(),
            ));
        }
        for (name, path) in [
            ("image", self.image.as_deref()),
            ("video", self.video.as_deref()),
            ("poster", self.poster.as_deref()),
        ] {
            if let Some(path) = path {
                validate_asset_path(path, name)?;
            }
        }
        if self.poster.is_some() && self.video.is_none() {
            return Err(theme_error(
                "background.poster 只能与 video 一起使用".to_string(),
            ));
        }
        if let Some(fit) = &self.fit {
            if fit != "cover" && fit != "contain" {
                return Err(theme_error(format!("background.fit 非法：{fit:?}")));
            }
        }
        if let Some(position) = &self.position {
            if !matches!(
                position.as_str(),
                "center" | "top" | "bottom" | "left" | "right"
            ) {
                return Err(theme_error(format!(
                    "background.position 非法：{position:?}"
                )));
            }
        }
        if let Some(dim) = self.dim {
            if !(0.0..=0.9).contains(&dim) {
                return Err(theme_error("background.dim 必须在 0..=0.9".to_string()));
            }
        }
        if let Some(blur) = self.blur {
            if blur > 40 {
                return Err(theme_error("background.blur 必须在 0..=40".to_string()));
            }
        }
        if let Some(scope) = &self.scope {
            if !matches!(scope.as_str(), "app" | "hero" | "both") {
                return Err(theme_error(format!("background.scope 非法：{scope:?}")));
            }
        }
        Ok(())
    }
}

impl Component {
    fn validate(&self, slot: &str) -> Result<()> {
        for (name, color) in [
            ("background", self.background.as_deref()),
            ("border", self.border.as_deref()),
            ("text", self.text.as_deref()),
            ("primaryBackground", self.primary_background.as_deref()),
            ("primaryText", self.primary_text.as_deref()),
        ] {
            if let Some(color) = color {
                validate_color(color)
                    .map_err(|e| theme_error(format!("components.{slot}.{name}：{e}")))?;
            }
        }
        if let Some(radius) = self.radius {
            if radius > 32 {
                return Err(theme_error(format!(
                    "components.{slot}.radius 必须在 0..=32"
                )));
            }
        }
        if let Some(shadow) = &self.shadow {
            if !matches!(shadow.as_str(), "none" | "soft" | "strong") {
                return Err(theme_error(format!(
                    "components.{slot}.shadow 只允许 none/soft/strong"
                )));
            }
        }
        if let Some(blur) = self.blur {
            if blur > 40 {
                return Err(theme_error(format!("components.{slot}.blur 必须在 0..=40")));
            }
        }
        if let Some(padding) = self.padding {
            if padding > 32 {
                return Err(theme_error(format!(
                    "components.{slot}.padding 必须在 0..=32"
                )));
            }
        }
        if let Some(image) = &self.image {
            validate_asset_path(image, "组件贴图")?;
            let slice = self.image_slice.unwrap_or(0);
            if slice > 64 {
                return Err(theme_error(format!(
                    "components.{slot}.imageSlice 必须在 0..=64"
                )));
            }
            if let Some(mode) = &self.image_mode {
                if mode != "border" && mode != "background" {
                    return Err(theme_error(format!(
                        "components.{slot}.imageMode 只允许 border/background"
                    )));
                }
                if mode == "border" && slice == 0 {
                    return Err(theme_error(format!(
                        "components.{slot} 用 border 模式时必须提供 imageSlice"
                    )));
                }
            }
        }
        if self.background.is_none()
            && self.border.is_none()
            && self.text.is_none()
            && self.radius.is_none()
            && self.shadow.is_none()
            && self.blur.is_none()
            && self.padding.is_none()
            && self.image.is_none()
            && self.primary_background.is_none()
            && self.primary_text.is_none()
        {
            return Err(theme_error(format!(
                "components.{slot} 至少要声明一个外观字段"
            )));
        }
        Ok(())
    }
}

impl Effect {
    fn validate(&self) -> Result<()> {
        match self {
            Effect::RotatingBlocks {
                texture,
                count,
                size,
                speed,
                opacity,
                area,
            } => {
                validate_asset_path(texture, "方块贴图")?;
                validate_range("effects.rotatingBlocks.count", *count, 1, 24)?;
                validate_range("effects.rotatingBlocks.size", *size, 24, 160)?;
                validate_range("effects.rotatingBlocks.speed", *speed, 4, 60)?;
                if let Some(opacity) = opacity {
                    if !(0.0..=1.0).contains(opacity) {
                        return Err(theme_error(
                            "effects.rotatingBlocks.opacity 必须在 0..=1".to_string(),
                        ));
                    }
                }
                validate_area("effects.rotatingBlocks.area", area)?;
            }
            Effect::Aurora {
                intensity,
                speed,
                area,
            } => {
                if let Some(intensity) = intensity {
                    if !(0.0..=1.0).contains(intensity) {
                        return Err(theme_error(
                            "effects.aurora.intensity 必须在 0..=1".to_string(),
                        ));
                    }
                }
                validate_range("effects.aurora.speed", *speed, 4, 60)?;
                validate_area("effects.aurora.area", area)?;
            }
            Effect::Particles {
                kind,
                color,
                count,
                speed,
                area,
            } => {
                if let Some(kind) = kind {
                    if !matches!(kind.as_str(), "dot" | "star" | "square") {
                        return Err(theme_error(format!(
                            "effects.particles.kind 只允许 dot/star/square，收到 {kind:?}"
                        )));
                    }
                }
                validate_color(color)?;
                validate_range("effects.particles.count", *count, 1, 80)?;
                validate_range("effects.particles.speed", *speed, 4, 60)?;
                validate_area("effects.particles.area", area)?;
            }
        }
        Ok(())
    }
}

impl Navigation {
    fn validate(&self) -> Result<()> {
        if self.icons.len() > NAV_ICON_KEYS.len() {
            return Err(theme_error("navigation.icons 数量超过上限".to_string()));
        }
        for (key, path) in &self.icons {
            if !NAV_ICON_KEYS.contains(&key.as_str()) {
                return Err(theme_error(format!(
                    "未知导航图标键 {key:?}（允许：{}）",
                    NAV_ICON_KEYS.join(", ")
                )));
            }
            validate_asset_path(path, "导航图标")?;
        }
        if self.ornaments.len() > 2 {
            return Err(theme_error("navigation.ornaments 最多 2 个".to_string()));
        }
        for ornament in &self.ornaments {
            validate_asset_path(&ornament.image, "导航装饰")?;
            if let Some(side) = &ornament.side {
                if side != "left" && side != "right" {
                    return Err(theme_error(format!(
                        "navigation.ornaments.side 只允许 left/right，收到 {side:?}"
                    )));
                }
            }
            if let Some(width) = ornament.width {
                if !(24..=240).contains(&width) {
                    return Err(theme_error(
                        "navigation.ornaments.width 必须在 24..=240".to_string(),
                    ));
                }
            }
            if let Some(opacity) = ornament.opacity {
                if !(0.0..=1.0).contains(&opacity) {
                    return Err(theme_error(
                        "navigation.ornaments.opacity 必须在 0..=1".to_string(),
                    ));
                }
            }
        }
        if self.icons.is_empty() && self.ornaments.is_empty() {
            return Err(theme_error(
                "navigation 至少要声明 icons 或 ornaments".to_string(),
            ));
        }
        Ok(())
    }
}

/// 颜色是否属于令牌表（必需/可选/安全语义色）。
pub fn is_known_key(key: &str) -> bool {
    REQUIRED_COLORS.contains(&key) || OPTIONAL_COLORS.contains(&key) || SAFETY_COLORS.contains(&key)
}

/// 校验主题颜色值：只接受 `#rgb` / `#rrggbb` / `#rrggbbaa`。
pub fn validate_color(value: &str) -> Result<()> {
    let Some(digits) = value.strip_prefix('#') else {
        return Err(theme_error(format!(
            "颜色必须是 #rgb / #rrggbb / #rrggbbaa，收到 {value:?}"
        )));
    };
    if !matches!(digits.len(), 3 | 6 | 8) || !digits.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(theme_error(format!(
            "颜色必须是 #rgb / #rrggbb / #rrggbbaa，收到 {value:?}"
        )));
    }
    Ok(())
}

/// [`validate_color`] 的布尔形式。
pub fn is_valid_color(value: &str) -> bool {
    validate_color(value).is_ok()
}

/// 主题资产路径：包内 `assets/` 下的 ASCII 相对路径。
pub fn validate_asset_path(path: &str, what: &str) -> Result<()> {
    validate_relative_path(path)
        .map_err(|e| theme_error(format!("{what}路径非法：{path}（{e}）")))?;
    let Some(rest) = path.strip_prefix("assets/") else {
        return Err(theme_error(format!("{what}必须位于 assets/ 下：{path}")));
    };
    if rest.is_empty() {
        return Err(theme_error(format!("{what}必须是具体文件：{path}")));
    }
    if !path
        .bytes()
        .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'/' | b'.' | b'-' | b'_'))
    {
        return Err(theme_error(format!(
            "{what}路径只允许 ASCII 字母/数字/`/`/`.`/`-`/`_`：{path}"
        )));
    }
    Ok(())
}

fn validate_font_family(family: &str) -> Result<()> {
    if family.trim().is_empty() || family.chars().count() > 64 {
        return Err(theme_error("字体 family 必须非空且 ≤64 字符".to_string()));
    }
    if family
        .chars()
        .any(|c| matches!(c, '"' | '\\' | ';' | '{' | '}' | '\n' | '\r' | '\t'))
    {
        return Err(theme_error(format!("字体 family 含有非法字符：{family:?}")));
    }
    Ok(())
}

fn validate_font_weight(weight: &str) -> Result<()> {
    let parts: Vec<&str> = weight.split_whitespace().collect();
    if parts.is_empty() || parts.len() > 2 {
        return Err(theme_error(format!("字体 weight 非法：{weight:?}")));
    }
    for part in &parts {
        let value: u32 = part
            .parse()
            .map_err(|_| theme_error(format!("字体 weight 非法：{weight:?}")))?;
        if !(100..=900).contains(&value) || !value.is_multiple_of(100) {
            return Err(theme_error(format!("字体 weight 非法：{weight:?}")));
        }
    }
    Ok(())
}

fn validate_range(name: &str, value: Option<u8>, min: u8, max: u8) -> Result<()> {
    if let Some(value) = value {
        if !(min..=max).contains(&value) {
            return Err(theme_error(format!("{name} 必须在 {min}..={max}")));
        }
    }
    Ok(())
}

fn validate_area(name: &str, area: &Option<String>) -> Result<()> {
    if let Some(area) = area {
        if !matches!(area.as_str(), "app" | "hero" | "both") {
            return Err(theme_error(format!("{name} 只允许 app/hero/both")));
        }
    }
    Ok(())
}

fn theme_error(message: String) -> RmcccError {
    RmcccError::Plugin {
        code: PluginErrorCode::ConfigInvalid,
        message,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const V1: &str = r##"{
        "schemaVersion": 1,
        "name": "Midnight",
        "mode": "dark",
        "colors": {
            "background": "#0b1020",
            "surface": "#141a2e",
            "text": "#e6e9f2",
            "accent": "#7c6cff",
            "border": "#2a3352"
        }
    }"##;

    const V2: &str = r##"{
        "schemaVersion": 2,
        "name": "Aurora",
        "mode": "dark",
        "colors": {
            "background": "#06111b",
            "surface": "#0c1b28",
            "text": "#e7f4f2",
            "accent": "#6fe7d2",
            "border": "#22394c"
        },
        "typography": { "ui": { "family": "Avenir Next", "fallback": "sans-serif" } },
        "shape": { "card": 22, "control": 14, "pill": true },
        "spacing": { "unit": 5, "density": "comfortable" },
        "background": { "image": "assets/bg/aurora.webp", "dim": 0.45, "scope": "both" },
        "components": {
            "card": { "background": "#0a18268c", "radius": 22, "blur": 20 },
            "button": { "radius": 24, "primaryBackground": "#6fe7d2" }
        },
        "effects": [
            { "type": "rotating-blocks", "texture": "assets/tex/grass.png", "count": 10 },
            { "type": "aurora", "intensity": 0.6 },
            { "type": "particles", "color": "#8fe3ff", "count": 12 }
        ],
        "navigation": {
            "icons": { "home": "assets/icons/home.svg" },
            "ornaments": [{ "image": "assets/ui/ornament.svg", "side": "left", "width": 120 }]
        }
    }"##;

    #[test]
    fn parses_v1_document() {
        let doc = ThemeDocument::parse(V1).unwrap();
        assert!(!doc.requires_skin());
        assert_eq!(doc.declared_mode(), Some("dark"));
    }

    #[test]
    fn parses_v2_document_and_lists_assets() {
        let doc = ThemeDocument::parse(V2).unwrap();
        assert!(doc.requires_skin());
        let paths: Vec<_> = doc.asset_specs().into_iter().map(|s| s.path).collect();
        assert!(paths.contains(&"assets/bg/aurora.webp".to_string()));
        assert!(paths.contains(&"assets/tex/grass.png".to_string()));
        assert!(paths.contains(&"assets/icons/home.svg".to_string()));
        assert_eq!(paths.len(), 4);
    }

    #[test]
    fn rejects_v2_sections_in_v1_document() {
        let raw = V2.replace("\"schemaVersion\": 2", "\"schemaVersion\": 1");
        assert!(ThemeDocument::parse(&raw).is_err());
    }

    #[test]
    fn rejects_css_injection_and_bad_ranges() {
        for mutation in [
            V2.replace("\"#6fe7d2\"", "\"url(https://x/p)\""),
            V2.replace("\"radius\": 24", "\"radius\": 40"),
            V2.replace("\"unit\": 5", "\"unit\": 9"),
            V2.replace("\"dim\": 0.45", "\"dim\": 1.4"),
            V2.replace("\"count\": 10", "\"count\": 99"),
        ] {
            assert!(ThemeDocument::parse(&mutation).is_err(), "{mutation}");
        }
    }

    #[test]
    fn rejects_unknown_component_slot_and_effect_type() {
        let raw = V2.replace("\"card\":", "\"sidebar\":");
        assert!(ThemeDocument::parse(&raw).is_err());
        let raw = V2.replace("\"type\": \"aurora\"", "\"type\": \"webgl-shader\"");
        assert!(ThemeDocument::parse(&raw).is_err());
    }

    #[test]
    fn rejects_remote_or_outside_assets() {
        for bad in [
            "https://evil.example/bg.png",
            "assets/../secret.png",
            "/etc/passwd",
            "assets/bg/aurora.webp?x=1",
            "themes/x.png",
        ] {
            assert!(validate_asset_path(bad, "测试").is_err(), "{bad}");
        }
        assert!(validate_asset_path("assets/bg/aurora.webp", "测试").is_ok());
    }

    #[test]
    fn rejects_bad_font_declarations() {
        let raw = V2.replace("\"family\": \"Avenir Next\"", "\"family\": \"Bad\\\"Font\"");
        assert!(ThemeDocument::parse(&raw).is_err());
        let raw = V2.replace("\"fallback\": \"sans-serif\"", "\"fallback\": \"url(x)\"");
        assert!(ThemeDocument::parse(&raw).is_err());
    }

    #[test]
    fn asset_paths_must_be_ascii() {
        assert!(validate_asset_path("assets/背景/图.png", "测试").is_err());
    }
}
