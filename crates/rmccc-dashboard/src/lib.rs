//! Dashboard 布局引擎与主题令牌系统（Phase 3，ADR-005）。
//!
//! 分层：
//!
//! | 模块 | 职责 |
//! |---|---|
//! | [`layout`] | 布局 Schema + 归一化算法（确定性纯函数） + 断点规则 |
//! | [`widget`] | Widget 注册表（内置 + 插件贡献） |
//! | [`theme`] | 主题令牌 + 安全语义色边界 |
//! | [`store`] | 布局持久化（读取/迁移/备份） |
//!
//! 前端消费归一化后的布局和主题令牌，CSS Grid 渲染。

pub mod layout;
pub mod store;
pub mod theme;
pub mod widget;

pub use layout::{
    normalize, Breakpoint, BreakpointOverrides, CorrectionKind, DashboardLayout, GridLayout,
    GridPosition, GridSize, NormalizationCorrection, NormalizationResult, WidgetConstraints,
    WidgetInstance, LAYOUT_SCHEMA_VERSION,
};
pub use layout::{ConstraintMap, DEFAULT_COLUMNS, DEFAULT_GAP};

pub use store::{load_layout, save_layout, LayoutStore};
pub use theme::{
    builtin_dark, builtin_ember, builtin_forest, builtin_light, builtin_list, builtin_ocean,
    builtin_paper, builtin_sakura, builtin_sky, extract_tokens, SafetyColors, ThemeInfo, ThemeJson,
    ThemePreference, ThemeTokens,
};
pub use widget::{WidgetDescriptor, WidgetRegistry};
