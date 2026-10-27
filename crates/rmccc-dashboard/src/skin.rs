//! 主题 v2 的已解析皮肤（ADR-014）。
//!
//! 主题文档里的资产是相对路径（`assets/...`）；宿主在这里把它解析成
//! `rmcccplugin://<pluginId>/...` 的只读 URL，前端只消费 URL，不自己拼。
//! 所有路径/插件 ID 在文档校验阶段已限制为 ASCII，因此这里的拼接是安全的。

use rmccc_plugin::theme::{Effect, ThemeDocument};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use ts_rs::TS;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export_to = "types.ts")]
pub struct ThemeSkin {
    pub typography: Option<SkinTypography>,
    pub shape: Option<SkinShape>,
    pub spacing: Option<SkinSpacing>,
    pub background: Option<SkinBackground>,
    pub components: BTreeMap<String, SkinComponent>,
    pub effects: Vec<SkinEffect>,
    pub navigation: Option<SkinNavigation>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export_to = "types.ts")]
pub struct SkinTypography {
    pub ui: Option<SkinFontFace>,
    pub display: Option<SkinFontFace>,
    pub mono: Option<SkinFontFace>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export_to = "types.ts")]
pub struct SkinFontFace {
    pub family: String,
    pub fallback: String,
    pub sources: Vec<SkinFontSource>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export_to = "types.ts")]
pub struct SkinFontSource {
    pub url: String,
    pub weight: String,
    pub style: String,
    pub display: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export_to = "types.ts")]
pub struct SkinShape {
    pub tag: Option<u8>,
    pub control: Option<u8>,
    pub card: Option<u8>,
    pub modal: Option<u8>,
    pub pill: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export_to = "types.ts")]
pub struct SkinSpacing {
    pub unit: Option<u8>,
    pub density: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export_to = "types.ts")]
pub struct SkinBackground {
    pub image: Option<String>,
    pub video: Option<String>,
    pub poster: Option<String>,
    pub fit: String,
    pub position: String,
    pub dim: f32,
    pub blur: u8,
    pub scope: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export_to = "types.ts")]
pub struct SkinComponent {
    pub background: Option<String>,
    pub border: Option<String>,
    pub text: Option<String>,
    pub radius: Option<u8>,
    pub shadow: Option<String>,
    pub blur: Option<u8>,
    pub padding: Option<u8>,
    pub image: Option<String>,
    pub image_slice: Option<u8>,
    pub image_mode: Option<String>,
    pub primary_background: Option<String>,
    pub primary_text: Option<String>,
}

/// 主界面特效（扁平化，前端按 `kind` 渲染宿主预设）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export_to = "types.ts")]
pub struct SkinEffect {
    pub kind: String,
    pub area: String,
    pub texture: Option<String>,
    pub color: Option<String>,
    pub shape: Option<String>,
    pub count: Option<u8>,
    pub size: Option<u8>,
    pub speed: Option<u8>,
    pub opacity: Option<f32>,
    pub intensity: Option<f32>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export_to = "types.ts")]
pub struct SkinNavigation {
    pub icons: BTreeMap<String, String>,
    pub ornaments: Vec<SkinOrnament>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export_to = "types.ts")]
pub struct SkinOrnament {
    pub image: String,
    pub side: String,
    pub width: u8,
    pub opacity: f32,
}

impl ThemeSkin {
    /// 把已校验的主题文档解析为前端可直接使用的皮肤。
    pub fn from_document(document: &ThemeDocument, plugin_id: &str) -> Self {
        let url = |path: &str| format!("rmcccplugin://{plugin_id}/{path}");

        let typography = document.typography.as_ref().map(|typo| SkinTypography {
            ui: typo.ui.as_ref().map(|font| SkinFontFace {
                family: font.family.clone(),
                fallback: font.fallback.clone().unwrap_or_else(|| "sans-serif".into()),
                sources: font
                    .files
                    .iter()
                    .map(|file| SkinFontSource {
                        url: url(&file.src),
                        weight: file.weight.clone().unwrap_or_else(|| "400".into()),
                        style: file.style.clone().unwrap_or_else(|| "normal".into()),
                        display: file.display.clone().unwrap_or_else(|| "swap".into()),
                    })
                    .collect(),
            }),
            display: typo.display.as_ref().map(|font| SkinFontFace {
                family: font.family.clone(),
                fallback: font.fallback.clone().unwrap_or_else(|| "serif".into()),
                sources: font
                    .files
                    .iter()
                    .map(|file| SkinFontSource {
                        url: url(&file.src),
                        weight: file.weight.clone().unwrap_or_else(|| "400".into()),
                        style: file.style.clone().unwrap_or_else(|| "normal".into()),
                        display: file.display.clone().unwrap_or_else(|| "swap".into()),
                    })
                    .collect(),
            }),
            mono: typo.mono.as_ref().map(|font| SkinFontFace {
                family: font.family.clone(),
                fallback: font.fallback.clone().unwrap_or_else(|| "monospace".into()),
                sources: font
                    .files
                    .iter()
                    .map(|file| SkinFontSource {
                        url: url(&file.src),
                        weight: file.weight.clone().unwrap_or_else(|| "400".into()),
                        style: file.style.clone().unwrap_or_else(|| "normal".into()),
                        display: file.display.clone().unwrap_or_else(|| "swap".into()),
                    })
                    .collect(),
            }),
        });

        let shape = document.shape.as_ref().map(|shape| SkinShape {
            tag: shape.tag,
            control: shape.control,
            card: shape.card,
            modal: shape.modal,
            pill: shape.pill,
        });

        let spacing = document.spacing.as_ref().map(|spacing| SkinSpacing {
            unit: spacing.unit,
            density: spacing.density.clone(),
        });

        let background = document.background.as_ref().map(|bg| SkinBackground {
            image: bg.image.as_deref().map(url),
            video: bg.video.as_deref().map(url),
            poster: bg.poster.as_deref().map(url),
            fit: bg.fit.clone().unwrap_or_else(|| "cover".into()),
            position: bg.position.clone().unwrap_or_else(|| "center".into()),
            dim: bg.dim.unwrap_or(0.4),
            blur: bg.blur.unwrap_or(0),
            scope: bg.scope.clone().unwrap_or_else(|| "app".into()),
        });

        let components = document
            .components
            .as_ref()
            .map(|components| {
                components
                    .iter()
                    .map(|(slot, component)| {
                        (
                            slot.clone(),
                            SkinComponent {
                                background: component.background.clone(),
                                border: component.border.clone(),
                                text: component.text.clone(),
                                radius: component.radius,
                                shadow: component.shadow.clone(),
                                blur: component.blur,
                                padding: component.padding,
                                image: component.image.as_deref().map(url),
                                image_slice: component.image_slice,
                                image_mode: component.image_mode.clone(),
                                primary_background: component.primary_background.clone(),
                                primary_text: component.primary_text.clone(),
                            },
                        )
                    })
                    .collect()
            })
            .unwrap_or_default();

        let effects = document
            .effects
            .iter()
            .map(|effect| match effect {
                Effect::RotatingBlocks {
                    texture,
                    count,
                    size,
                    speed,
                    opacity,
                    area,
                } => SkinEffect {
                    kind: "rotating-blocks".into(),
                    area: area.clone().unwrap_or_else(|| "hero".into()),
                    texture: Some(url(texture)),
                    color: None,
                    shape: None,
                    count: *count,
                    size: *size,
                    speed: *speed,
                    opacity: *opacity,
                    intensity: None,
                },
                Effect::Aurora {
                    intensity,
                    speed,
                    area,
                } => SkinEffect {
                    kind: "aurora".into(),
                    area: area.clone().unwrap_or_else(|| "app".into()),
                    texture: None,
                    color: None,
                    shape: None,
                    count: None,
                    size: None,
                    speed: *speed,
                    opacity: None,
                    intensity: *intensity,
                },
                Effect::Particles {
                    kind,
                    color,
                    count,
                    speed,
                    area,
                } => SkinEffect {
                    kind: "particles".into(),
                    area: area.clone().unwrap_or_else(|| "hero".into()),
                    texture: None,
                    color: Some(color.clone()),
                    shape: kind.clone(),
                    count: *count,
                    size: None,
                    speed: *speed,
                    opacity: None,
                    intensity: None,
                },
            })
            .collect::<Vec<_>>();

        let navigation = document.navigation.as_ref().map(|nav| SkinNavigation {
            icons: nav
                .icons
                .iter()
                .map(|(key, path)| (key.clone(), url(path)))
                .collect(),
            ornaments: nav
                .ornaments
                .iter()
                .map(|ornament| SkinOrnament {
                    image: url(&ornament.image),
                    side: ornament.side.clone().unwrap_or_else(|| "left".into()),
                    width: ornament.width.unwrap_or(120),
                    opacity: ornament.opacity.unwrap_or(0.8),
                })
                .collect(),
        });

        Self {
            typography,
            shape,
            spacing,
            background,
            components,
            effects,
            navigation,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DOC: &str = r##"{
        "schemaVersion": 2,
        "colors": {
            "background": "#06111b", "surface": "#0c1b28", "text": "#e7f4f2",
            "accent": "#6fe7d2", "border": "#22394c"
        },
        "typography": {
            "ui": {
                "family": "Aurora Sans",
                "files": [{ "src": "assets/fonts/ui.woff2", "weight": "400 700" }]
            }
        },
        "shape": { "card": 22 },
        "background": { "video": "assets/bg/loop.mp4", "poster": "assets/bg/p.webp", "scope": "both" },
        "components": { "card": { "image": "assets/ui/card.svg", "imageSlice": 24, "imageMode": "border" } },
        "effects": [{ "type": "rotating-blocks", "texture": "assets/tex/grass.png", "count": 6 }],
        "navigation": { "icons": { "home": "assets/icons/home.svg" } }
    }"##;

    #[test]
    fn resolves_asset_urls_and_defaults() {
        let doc = ThemeDocument::parse(DOC).unwrap();
        let skin = ThemeSkin::from_document(&doc, "dev.example.aurora");
        let typo = skin.typography.unwrap();
        let ui = typo.ui.unwrap();
        assert_eq!(ui.family, "Aurora Sans");
        assert_eq!(ui.fallback, "sans-serif");
        assert_eq!(
            ui.sources[0].url,
            "rmcccplugin://dev.example.aurora/assets/fonts/ui.woff2"
        );
        assert_eq!(ui.sources[0].weight, "400 700");
        assert_eq!(ui.sources[0].display, "swap");

        let bg = skin.background.unwrap();
        assert_eq!(bg.scope, "both");
        assert_eq!(bg.dim, 0.4);
        assert_eq!(
            bg.video.as_deref(),
            Some("rmcccplugin://dev.example.aurora/assets/bg/loop.mp4")
        );

        let card = &skin.components["card"];
        assert_eq!(card.image_slice, Some(24));
        assert_eq!(card.image_mode.as_deref(), Some("border"));

        assert_eq!(skin.effects[0].kind, "rotating-blocks");
        assert_eq!(skin.effects[0].area, "hero");
        assert_eq!(skin.effects[0].count, Some(6));
        assert_eq!(
            skin.navigation.unwrap().icons["home"],
            "rmcccplugin://dev.example.aurora/assets/icons/home.svg"
        );
    }
}
