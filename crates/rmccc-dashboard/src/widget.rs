//! Widget 注册表（ADR-005 决策 5）。
//!
//! 内置 Widget 用 `org.rmccc.core-ui` 命名空间；插件贡献的 `dashboardCards`
//! 自动成为 WidgetDescriptor。未知的 widget 仍可在布局中存在（显示占位），
//! 但尺寸约束用默认值。

use crate::layout::{GridSize, WidgetConstraints};
use rmccc_plugin::DashboardCardView;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// Widget 描述符（注册表项）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export_to = "types.ts")]
pub struct WidgetDescriptor {
    pub plugin_id: String,
    pub widget_id: String,
    pub display_name: String,
    pub default_size: GridSize,
    pub min_size: GridSize,
    pub max_size: GridSize,
    pub is_builtin: bool,
    /// 隔离 UI 入口（`rmcccplugin://...`，Phase 4/ADR-006）；`None` = 宿主绘制的声明式卡片。
    #[serde(default)]
    pub ui_url: Option<String>,
}

impl WidgetDescriptor {
    pub fn constraints(&self) -> WidgetConstraints {
        WidgetConstraints {
            min_size: self.min_size,
            max_size: self.max_size,
        }
    }
}

/// Widget 注册表：内置 + 插件贡献。
#[derive(Debug, Clone, Default)]
pub struct WidgetRegistry {
    descriptors: Vec<WidgetDescriptor>,
}

impl WidgetRegistry {
    /// 用内置 Widget + 插件贡献卡片构建。
    pub fn from_contributions(cards: &[DashboardCardView]) -> Self {
        let mut reg = Self::builtin();
        for card in cards {
            reg.descriptors.push(WidgetDescriptor {
                plugin_id: card.plugin_id.clone(),
                widget_id: format!("dashboard-card:{}", card.card.id),
                display_name: card.card.title.clone(),
                default_size: GridSize {
                    w: card.card.default_size.w as u32,
                    h: card.card.default_size.h as u32,
                },
                min_size: card
                    .card
                    .min_size
                    .map(|m| GridSize {
                        w: m.w as u32,
                        h: m.h as u32,
                    })
                    .unwrap_or(GridSize { w: 1, h: 1 }),
                max_size: GridSize { w: 24, h: 24 },
                is_builtin: false,
                ui_url: card.ui_url.clone(),
            });
        }
        reg
    }

    /// 内置 `org.rmccc.core-ui` Widget。
    pub fn builtin() -> Self {
        Self {
            descriptors: vec![
                WidgetDescriptor {
                    plugin_id: "org.rmccc.core-ui".into(),
                    widget_id: "instance-launcher".into(),
                    display_name: "Instance Launcher".into(),
                    default_size: GridSize { w: 8, h: 4 },
                    min_size: GridSize { w: 4, h: 2 },
                    max_size: GridSize { w: 12, h: 8 },
                    is_builtin: true,
                    ui_url: None,
                },
                WidgetDescriptor {
                    plugin_id: "org.rmccc.core-ui".into(),
                    widget_id: "account-card".into(),
                    display_name: "Account".into(),
                    default_size: GridSize { w: 4, h: 4 },
                    min_size: GridSize { w: 2, h: 2 },
                    max_size: GridSize { w: 6, h: 8 },
                    is_builtin: true,
                    ui_url: None,
                },
                WidgetDescriptor {
                    plugin_id: "org.rmccc.core-ui".into(),
                    widget_id: "version-browser".into(),
                    display_name: "Version Browser".into(),
                    default_size: GridSize { w: 12, h: 3 },
                    min_size: GridSize { w: 4, h: 2 },
                    max_size: GridSize { w: 12, h: 12 },
                    is_builtin: true,
                    ui_url: None,
                },
            ],
        }
    }

    pub fn list(&self) -> &[WidgetDescriptor] {
        &self.descriptors
    }

    /// 查找 `(plugin_id, widget_id)` 的约束。
    pub fn constraints(&self, plugin_id: &str, widget_id: &str) -> WidgetConstraints {
        self.descriptors
            .iter()
            .find(|d| d.plugin_id == plugin_id && d.widget_id == widget_id)
            .map(|d| d.constraints())
            .unwrap_or_default()
    }

    /// 构建归一化约束表。
    pub fn constraint_map(&self) -> crate::layout::ConstraintMap {
        let mut map = crate::layout::ConstraintMap::new();
        for d in &self.descriptors {
            map.insert(format!("{}/{}", d.plugin_id, d.widget_id), d.constraints());
        }
        map
    }
}
