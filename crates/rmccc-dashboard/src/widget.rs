//! Widget 注册表（ADR-005 决策 5）。
//!
//! 内置 Widget 用 `org.rmccc.core-ui` 命名空间；插件贡献的 `dashboardCards`
//! 自动成为 WidgetDescriptor。未知的 widget 仍可在布局中存在（显示占位），
//! 但尺寸约束用默认值。

use crate::layout::{GridSize, WidgetConstraints};
use rmccc_plugin::DashboardCardView;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// Canonical card width tiers on the 12-column dashboard grid.
pub const CARD_WIDTH_TIERS: &[u32] = &[3, 4, 6, 8, 12];
/// Canonical card height tiers (92px row + 14px gap).
pub const CARD_HEIGHT_TIERS: &[u32] = &[1, 2, 3, 4];

fn snap_tier(value: u32, tiers: &[u32], min: u32, max: u32) -> u32 {
    let allowed: Vec<u32> = tiers
        .iter()
        .copied()
        .filter(|tier| *tier >= min && *tier <= max)
        .collect();
    if allowed.is_empty() {
        return tiers
            .iter()
            .copied()
            .find(|tier| *tier >= min)
            .unwrap_or_else(|| *tiers.last().unwrap_or(&min));
    }
    let mut best = allowed[0];
    let mut best_distance = best.abs_diff(value);
    for tier in allowed.into_iter().skip(1) {
        let distance = tier.abs_diff(value);
        if distance < best_distance || (distance == best_distance && tier > best) {
            best = tier;
            best_distance = distance;
        }
    }
    best
}

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
            let min_w = card
                .card
                .min_size
                .map(|m| m.w as u32)
                .unwrap_or(3)
                .clamp(1, 12);
            let min_h = card
                .card
                .min_size
                .map(|m| m.h as u32)
                .unwrap_or(1)
                .clamp(1, 4);
            let max_w = card
                .card
                .max_size
                .map(|m| m.w as u32)
                .unwrap_or(12)
                .clamp(1, 12);
            let max_h = card
                .card
                .max_size
                .map(|m| m.h as u32)
                .unwrap_or(4)
                .clamp(1, 4);
            let def_w = card.card.default_size.w as u32;
            let def_h = card.card.default_size.h as u32;
            let default_w = snap_tier(def_w, CARD_WIDTH_TIERS, min_w, max_w);
            let default_h = snap_tier(def_h, CARD_HEIGHT_TIERS, min_h, max_h);
            let min_snapped_w = snap_tier(min_w, CARD_WIDTH_TIERS, min_w, max_w);
            let min_snapped_h = snap_tier(min_h, CARD_HEIGHT_TIERS, min_h, max_h);
            let max_snapped_w = snap_tier(max_w, CARD_WIDTH_TIERS, min_w, max_w);
            let max_snapped_h = snap_tier(max_h, CARD_HEIGHT_TIERS, min_h, max_h);
            reg.descriptors.push(WidgetDescriptor {
                plugin_id: card.plugin_id.clone(),
                widget_id: format!("dashboard-card:{}", card.card.id),
                display_name: card.card.title.clone(),
                default_size: GridSize {
                    w: default_w,
                    h: default_h,
                },
                min_size: GridSize {
                    w: min_snapped_w.min(default_w),
                    h: min_snapped_h.min(default_h),
                },
                max_size: GridSize {
                    w: max_snapped_w.max(default_w),
                    h: max_snapped_h.max(default_h),
                },
                is_builtin: false,
                ui_url: card.ui_url.clone(),
            });
        }
        reg
    }

    /// 内置 `org.rmccc.core-ui` Widget。尺寸与 `src/lib/cardLayout.ts` 保持一致。
    pub fn builtin() -> Self {
        let card = |widget_id: &str,
                    display_name: &str,
                    w: u32,
                    h: u32,
                    min_w: u32,
                    min_h: u32,
                    max_w: u32,
                    max_h: u32| {
            WidgetDescriptor {
                plugin_id: "org.rmccc.core-ui".into(),
                widget_id: widget_id.into(),
                display_name: display_name.into(),
                default_size: GridSize { w, h },
                min_size: GridSize { w: min_w, h: min_h },
                max_size: GridSize { w: max_w, h: max_h },
                is_builtin: true,
                ui_url: None,
            }
        };
        Self {
            descriptors: vec![
                card("server", "Server info", 6, 2, 3, 2, 12, 4),
                card("versions", "Version browser", 12, 1, 4, 1, 12, 3),
                card("downloads", "Downloads", 6, 2, 3, 2, 12, 4),
                card("instances", "Instances", 6, 2, 3, 2, 8, 4),
                card("announcements", "Announcements", 6, 3, 4, 2, 12, 4),
                card("compare", "Version compare", 4, 2, 3, 2, 6, 3),
                card("games", "Running games", 6, 2, 4, 2, 12, 3),
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

#[cfg(test)]
mod tests {
    use super::*;
    use rmccc_plugin::manifest::{CardSize, DashboardCard};
    use rmccc_plugin::DashboardCardView;

    fn view(default: (u8, u8), min: (u8, u8), max: Option<(u8, u8)>) -> DashboardCardView {
        DashboardCardView {
            key: "dev.example.card/status".into(),
            plugin_id: "dev.example.card".into(),
            card: DashboardCard {
                id: "status".into(),
                title: "Status".into(),
                description: None,
                default_size: CardSize {
                    w: default.0,
                    h: default.1,
                },
                min_size: Some(CardSize { w: min.0, h: min.1 }),
                max_size: max.map(|(w, h)| CardSize { w, h }),
            },
            ui_url: None,
        }
    }

    #[test]
    fn snaps_plugin_sizes_into_canonical_tiers() {
        let registry = WidgetRegistry::from_contributions(&[view((5, 5), (2, 1), Some((9, 3)))]);
        let descriptor = &registry.list()[7];
        assert_eq!(descriptor.default_size, GridSize { w: 6, h: 3 });
        assert_eq!(descriptor.min_size, GridSize { w: 3, h: 1 });
        assert_eq!(descriptor.max_size, GridSize { w: 8, h: 3 });
    }

    #[test]
    fn builtin_sizes_match_the_frontend_tiers() {
        let registry = WidgetRegistry::builtin();
        for descriptor in registry.list() {
            assert!(CARD_WIDTH_TIERS.contains(&descriptor.default_size.w));
            assert!(CARD_HEIGHT_TIERS.contains(&descriptor.default_size.h));
            assert!(descriptor.min_size.w <= descriptor.default_size.w);
            assert!(descriptor.default_size.w <= descriptor.max_size.w);
        }
    }
}
