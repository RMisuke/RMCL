//! Dashboard 布局 Schema 与归一化算法（ADR-005）。
//!
//! 归一化是**确定性纯函数**：相同输入永远产出相同输出，不依赖 DOM 渲染顺序。
//! 算法步骤：去重 → 尺寸钳制 → 位置钳制/换行 → 排序 → push-down 重叠解决 → 压缩。

use rmccc_error::{PluginErrorCode, Result, RmcccError};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use ts_rs::TS;

pub const LAYOUT_SCHEMA_VERSION: u32 = 1;
pub const DEFAULT_COLUMNS: u8 = 12;
pub const DEFAULT_GAP: u8 = 16;
pub const MAX_WIDGETS: usize = 64;

// ── DTO ──

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export_to = "types.ts")]
pub struct GridPosition {
    pub x: u32,
    pub y: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export_to = "types.ts")]
pub struct GridSize {
    pub w: u32,
    pub h: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export_to = "types.ts")]
pub struct GridLayout {
    pub columns: u8,
    pub gap: u8,
}

impl Default for GridLayout {
    fn default() -> Self {
        Self {
            columns: DEFAULT_COLUMNS,
            gap: DEFAULT_GAP,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export_to = "types.ts")]
pub struct BreakpointOverrides {
    #[serde(default)]
    pub medium: Option<GridSize>,
    #[serde(default)]
    pub compact: Option<GridSize>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export_to = "types.ts")]
pub struct WidgetInstance {
    pub instance_id: String,
    pub plugin_id: String,
    pub widget_id: String,
    #[serde(default = "default_true")]
    pub enabled: bool,
    pub position: GridPosition,
    pub size: GridSize,
    #[serde(default)]
    pub breakpoint_overrides: BreakpointOverrides,
    #[serde(default)]
    #[ts(type = "unknown")]
    pub configuration: serde_json::Value,
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export_to = "types.ts")]
pub struct DashboardLayout {
    pub schema_version: u32,
    pub layout_id: String,
    pub grid: GridLayout,
    pub widgets: Vec<WidgetInstance>,
}

impl DashboardLayout {
    pub fn parse(json: &str) -> Result<Self> {
        let layout: Self = serde_json::from_str(json).map_err(|e| RmcccError::Plugin {
            code: PluginErrorCode::ConfigInvalid,
            message: format!("布局 JSON 解析失败：{e}"),
        })?;
        if layout.schema_version != LAYOUT_SCHEMA_VERSION {
            return Err(RmcccError::Plugin {
                code: PluginErrorCode::ConfigInvalid,
                message: format!(
                    "布局 schemaVersion {} 不支持（当前 {}）",
                    layout.schema_version, LAYOUT_SCHEMA_VERSION
                ),
            });
        }
        Ok(layout)
    }

    /// 内置默认布局：启动面板 + 实例列表 + 账户卡。
    pub fn default_layout() -> Self {
        Self {
            schema_version: LAYOUT_SCHEMA_VERSION,
            layout_id: "default".into(),
            grid: GridLayout::default(),
            widgets: vec![
                WidgetInstance {
                    instance_id: "core-launcher".into(),
                    plugin_id: "org.rmccc.core-ui".into(),
                    widget_id: "instance-launcher".into(),
                    enabled: true,
                    position: GridPosition { x: 0, y: 0 },
                    size: GridSize { w: 8, h: 4 },
                    breakpoint_overrides: BreakpointOverrides::default(),
                    configuration: serde_json::json!({}),
                },
                WidgetInstance {
                    instance_id: "core-accounts".into(),
                    plugin_id: "org.rmccc.core-ui".into(),
                    widget_id: "account-card".into(),
                    enabled: true,
                    position: GridPosition { x: 8, y: 0 },
                    size: GridSize { w: 4, h: 4 },
                    breakpoint_overrides: BreakpointOverrides::default(),
                    configuration: serde_json::json!({}),
                },
                WidgetInstance {
                    instance_id: "core-versions".into(),
                    plugin_id: "org.rmccc.core-ui".into(),
                    widget_id: "version-browser".into(),
                    enabled: true,
                    position: GridPosition { x: 0, y: 4 },
                    size: GridSize { w: 12, h: 3 },
                    breakpoint_overrides: BreakpointOverrides::default(),
                    configuration: serde_json::json!({}),
                },
            ],
        }
    }
}

// ── 约束 ──

/// 单个 Widget 的尺寸约束（来自 WidgetDescriptor）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WidgetConstraints {
    pub min_size: GridSize,
    pub max_size: GridSize,
}

impl Default for WidgetConstraints {
    fn default() -> Self {
        Self {
            min_size: GridSize { w: 1, h: 1 },
            max_size: GridSize {
                w: DEFAULT_COLUMNS as u32,
                h: 24,
            },
        }
    }
}

/// 约束表：`(plugin_id, widget_id)` → 约束。未找到时用默认。
#[derive(Debug, Clone, Default)]
pub struct ConstraintMap {
    map: BTreeMap<String, WidgetConstraints>,
}

impl ConstraintMap {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn insert(&mut self, key: impl Into<String>, constraints: WidgetConstraints) {
        self.map.insert(key.into(), constraints);
    }

    pub fn get(&self, plugin_id: &str, widget_id: &str) -> WidgetConstraints {
        let key = format!("{plugin_id}/{widget_id}");
        self.map.get(&key).copied().unwrap_or_default()
    }
}

// ── 归一化 ──

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export_to = "types.ts")]
pub enum CorrectionKind {
    DuplicateInstance,
    SizeClamped,
    PositionClamped,
    WrappedToNextRow,
    PushedDownDueToOverlap,
    RowCompressed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export_to = "types.ts")]
pub struct NormalizationCorrection {
    pub instance_id: String,
    pub kind: CorrectionKind,
    pub detail: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export_to = "types.ts")]
pub struct NormalizationResult {
    pub layout: DashboardLayout,
    pub corrections: Vec<NormalizationCorrection>,
}

/// 执行归一化（ADR-005 决策 3）。
///
/// 原地修改 `layout` 并返回修正报告。
pub fn normalize(layout: &mut DashboardLayout, constraints: &ConstraintMap) -> NormalizationResult {
    let columns = layout.grid.columns.max(1) as u32;
    let mut corrections = Vec::new();

    // 1. 去重：同一 instanceId 保留第一条。
    let mut seen: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    let mut deduped = Vec::with_capacity(layout.widgets.len());
    for w in &layout.widgets {
        if seen.insert(w.instance_id.clone()) {
            deduped.push(w.clone());
        } else {
            corrections.push(NormalizationCorrection {
                instance_id: w.instance_id.clone(),
                kind: CorrectionKind::DuplicateInstance,
                detail: "重复的 instanceId，已丢弃".into(),
            });
        }
    }
    layout.widgets = deduped;

    // 2. 尺寸钳制。
    for w in &mut layout.widgets {
        let c = constraints.get(&w.plugin_id, &w.widget_id);
        let max_w = c.max_size.w.min(columns);
        let max_h = c.max_size.h;
        let min_w = c.min_size.w.min(max_w);
        let min_h = c.min_size.h.min(max_h);

        let orig = w.size;
        w.size.w = w.size.w.clamp(min_w, max_w).max(1);
        w.size.h = w.size.h.clamp(min_h, max_h).max(1);
        if w.size != orig {
            corrections.push(NormalizationCorrection {
                instance_id: w.instance_id.clone(),
                kind: CorrectionKind::SizeClamped,
                detail: format!(
                    "尺寸 {:?} → {:?}（约束 min={:?} max_w={max_w}）",
                    orig, w.size, c.min_size
                ),
            });
        }
    }

    // 3. 位置钳制/换行。
    for w in &mut layout.widgets {
        if w.position.x > columns {
            w.position.x = 0;
        }
        if w.position.x + w.size.w > columns {
            let orig = w.position;
            w.position.y += 1;
            w.position.x = 0;
            corrections.push(NormalizationCorrection {
                instance_id: w.instance_id.clone(),
                kind: CorrectionKind::WrappedToNextRow,
                detail: format!(
                    "x({}) + w({}) > {columns}，换行 {:?} → {:?}",
                    orig.x, w.size.w, orig, w.position
                ),
            });
        }
    }

    // 4. 排序。
    layout.widgets.sort_by_key(|w| (w.position.y, w.position.x));

    // 5. push-down 重叠解决。
    // 用值元组而非引用，避免可变借用与不可变借用冲突。
    let mut placed: Vec<(u32, u32, u32, u32)> = Vec::new(); // (x, y, w, h)
    for i in 0..layout.widgets.len() {
        let px = layout.widgets[i].position.x;
        let pw = layout.widgets[i].size.w;
        let ph = layout.widgets[i].size.h;
        let mut new_y = layout.widgets[i].position.y;
        loop {
            let overlaps = placed
                .iter()
                .any(|&(ax, ay, aw, ah)| overlaps_rect(ax, ay, aw, ah, px, new_y, pw, ph));
            if !overlaps {
                break;
            }
            new_y += 1;
        }
        if new_y != layout.widgets[i].position.y {
            let orig_y = layout.widgets[i].position.y;
            layout.widgets[i].position.y = new_y;
            corrections.push(NormalizationCorrection {
                instance_id: layout.widgets[i].instance_id.clone(),
                kind: CorrectionKind::PushedDownDueToOverlap,
                detail: format!("与已放置卡片重叠，y {orig_y} → {new_y}"),
            });
        }
        placed.push((px, new_y, pw, ph));
    }

    // 6. 压缩：删除空行。
    let min_y = layout
        .widgets
        .iter()
        .map(|w| w.position.y)
        .min()
        .unwrap_or(0);
    if min_y > 0 {
        for w in &mut layout.widgets {
            w.position.y -= min_y;
        }
        corrections.push(NormalizationCorrection {
            instance_id: "*".into(),
            kind: CorrectionKind::RowCompressed,
            detail: format!("整体上移 {min_y} 行"),
        });
    }

    NormalizationResult {
        layout: std::mem::take(layout),
        corrections,
    }
}

#[allow(clippy::too_many_arguments)]
fn overlaps_rect(ax: u32, ay: u32, aw: u32, ah: u32, bx: u32, by: u32, bw: u32, bh: u32) -> bool {
    ax < bx + bw && ax + aw > bx && ay < by + bh && ay + ah > by
}

/// 断点规则（ADR-005 决策 4）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export_to = "types.ts")]
pub enum Breakpoint {
    Wide,
    Medium,
    Compact,
}

impl Breakpoint {
    /// 根据窗口宽度选择断点。
    pub fn from_width(width_px: u32) -> Self {
        if width_px >= 1280 {
            Self::Wide
        } else if width_px >= 768 {
            Self::Medium
        } else {
            Self::Compact
        }
    }

    /// 该断点下的生效列数。
    pub fn effective_columns(self, declared: u8) -> u8 {
        let declared = declared.max(1);
        match self {
            Self::Wide => declared,
            Self::Medium => declared.min(8),
            Self::Compact => declared.min(4),
        }
    }

    /// 该断点下卡片的生效尺寸（应用 breakpointOverrides + 列数钳制）。
    pub fn effective_size(self, widget: &WidgetInstance, effective_columns: u8) -> GridSize {
        let override_size = match self {
            Self::Wide => None,
            Self::Medium => widget.breakpoint_overrides.medium,
            Self::Compact => widget.breakpoint_overrides.compact,
        };
        let base = override_size.unwrap_or(widget.size);
        GridSize {
            w: base.w.min(effective_columns as u32).max(1),
            h: base.h,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn layout(columns: u8, widgets: Vec<WidgetInstance>) -> DashboardLayout {
        DashboardLayout {
            schema_version: 1,
            layout_id: "test".into(),
            grid: GridLayout { columns, gap: 16 },
            widgets,
        }
    }

    fn widget(id: &str, x: u32, y: u32, w: u32, h: u32) -> WidgetInstance {
        WidgetInstance {
            instance_id: id.into(),
            plugin_id: "org.rmccc.core-ui".into(),
            widget_id: "test".into(),
            enabled: true,
            position: GridPosition { x, y },
            size: GridSize { w, h },
            breakpoint_overrides: BreakpointOverrides::default(),
            configuration: serde_json::json!({}),
        }
    }

    #[test]
    fn no_corrections_on_clean_layout() {
        let mut l = layout(
            12,
            vec![
                widget("a", 0, 0, 4, 2),
                widget("b", 4, 0, 4, 2),
                widget("c", 8, 0, 4, 2),
            ],
        );
        let result = normalize(&mut l, &ConstraintMap::new());
        assert!(result.corrections.is_empty());
        assert_eq!(result.layout.widgets.len(), 3);
    }

    #[test]
    fn removes_duplicate_instance_ids() {
        let mut l = layout(12, vec![widget("a", 0, 0, 4, 2), widget("a", 4, 0, 4, 2)]);
        let result = normalize(&mut l, &ConstraintMap::new());
        assert_eq!(result.layout.widgets.len(), 1);
        assert_eq!(result.corrections.len(), 1);
        assert_eq!(
            result.corrections[0].kind,
            CorrectionKind::DuplicateInstance
        );
    }

    #[test]
    fn clamps_oversized_widget() {
        let mut l = layout(12, vec![widget("a", 0, 0, 20, 1)]);
        let result = normalize(&mut l, &ConstraintMap::new());
        assert_eq!(result.layout.widgets[0].size.w, 12);
        assert_eq!(result.corrections.len(), 1);
        assert_eq!(result.corrections[0].kind, CorrectionKind::SizeClamped);
    }

    #[test]
    fn wraps_overflowing_widget_to_next_row() {
        let mut l = layout(12, vec![widget("a", 0, 0, 8, 2), widget("b", 10, 0, 4, 2)]);
        let result = normalize(&mut l, &ConstraintMap::new());
        assert_eq!(result.layout.widgets[1].position.x, 0);
        assert!(result.layout.widgets[1].position.y > 0);
        // 换行后可能与上一个卡片重叠 → 进一步被 push-down，两者都是合法修正。
        assert!(result
            .corrections
            .iter()
            .any(|c| c.kind == CorrectionKind::WrappedToNextRow));
    }

    #[test]
    fn pushes_down_overlapping_widgets() {
        let mut l = layout(
            12,
            vec![
                widget("a", 0, 0, 6, 2),
                widget("b", 0, 0, 6, 2),
                widget("c", 0, 1, 6, 2),
            ],
        );
        let result = normalize(&mut l, &ConstraintMap::new());
        // a stays at (0,0), b pushed to y=2, c pushed to y=4.
        assert_eq!(result.layout.widgets[0].instance_id, "a");
        assert_eq!(result.layout.widgets[0].position.y, 0);
        assert_eq!(result.layout.widgets[1].instance_id, "b");
        assert!(result.layout.widgets[1].position.y >= 2);
        assert_eq!(result.layout.widgets[2].instance_id, "c");
        assert!(result.layout.widgets[2].position.y >= result.layout.widgets[1].position.y + 2);
    }

    #[test]
    fn compresses_empty_rows() {
        let mut l = layout(12, vec![widget("a", 0, 5, 4, 2)]);
        let result = normalize(&mut l, &ConstraintMap::new());
        assert_eq!(result.layout.widgets[0].position.y, 0);
        assert!(result
            .corrections
            .iter()
            .any(|c| c.kind == CorrectionKind::RowCompressed));
    }

    #[test]
    fn applies_custom_constraints() {
        let mut l = layout(12, vec![widget("a", 0, 0, 2, 1)]);
        let mut constraints = ConstraintMap::new();
        constraints.insert(
            "org.rmccc.core-ui/test",
            WidgetConstraints {
                min_size: GridSize { w: 4, h: 2 },
                max_size: GridSize { w: 8, h: 6 },
            },
        );
        let result = normalize(&mut l, &constraints);
        assert_eq!(result.layout.widgets[0].size, GridSize { w: 4, h: 2 });
    }

    #[test]
    fn breakpoints_reduce_columns_and_clamp_widgets() {
        let w = widget("a", 0, 0, 10, 2);
        assert_eq!(Breakpoint::from_width(1400), Breakpoint::Wide);
        assert_eq!(Breakpoint::from_width(900), Breakpoint::Medium);
        assert_eq!(Breakpoint::from_width(500), Breakpoint::Compact);

        assert_eq!(Breakpoint::Wide.effective_columns(12), 12);
        assert_eq!(Breakpoint::Medium.effective_columns(12), 8);
        assert_eq!(Breakpoint::Compact.effective_columns(12), 4);

        assert_eq!(
            Breakpoint::Medium.effective_size(&w, 8),
            GridSize { w: 8, h: 2 }
        );
        assert_eq!(
            Breakpoint::Compact.effective_size(&w, 4),
            GridSize { w: 4, h: 2 }
        );
    }

    #[test]
    fn breakpoint_overrides_apply_per_breakpoint() {
        let w = WidgetInstance {
            instance_id: "a".into(),
            plugin_id: "org.rmccc.core-ui".into(),
            widget_id: "test".into(),
            enabled: true,
            position: GridPosition { x: 0, y: 0 },
            size: GridSize { w: 10, h: 2 },
            breakpoint_overrides: BreakpointOverrides {
                medium: Some(GridSize { w: 6, h: 2 }),
                compact: Some(GridSize { w: 4, h: 1 }),
            },
            configuration: serde_json::json!({}),
        };
        assert_eq!(
            Breakpoint::Wide.effective_size(&w, 12),
            GridSize { w: 10, h: 2 }
        );
        assert_eq!(
            Breakpoint::Medium.effective_size(&w, 8),
            GridSize { w: 6, h: 2 }
        );
        assert_eq!(
            Breakpoint::Compact.effective_size(&w, 4),
            GridSize { w: 4, h: 1 }
        );
    }

    #[test]
    fn default_layout_is_valid_and_clean() {
        let mut l = DashboardLayout::default_layout();
        let result = normalize(&mut l, &ConstraintMap::new());
        assert!(result.corrections.is_empty(), "{:?}", result.corrections);
        assert_eq!(result.layout.widgets.len(), 3);
    }
}
