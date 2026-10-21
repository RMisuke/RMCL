# Phase 3 — Dashboard 卡片与主题引擎

> **对应指南**：§9，§17 Phase 3。**状态**：✅ 3.1–3.5 完成（2026-10-21）。
> **决策**：[ADR-005](../adr/ADR-005-dashboard-layout-and-theme.md) accepted。
> **当前事实来源**：`crates/rmccc-dashboard`、`schemas/dashboard-layout.schema.json`、
> `src/theme/index.ts`、`src/routes/Dashboard.tsx`。

## 1. 目标

把固定 JSX 主界面迁移为「注册表 + 布局配置 + 受控渲染」，并落地主题令牌系统。
用户可自定义布局并重启后保留；插件卡片失败或卸载不损坏其它卡片/主程序。

## 2. 前置条件（已完成）

- Phase 2 完成（插件注册表/状态机/声明式插件可用）。
- ADR-005（Layout Schema/归一化/断点/主题令牌）accepted。

## 3. 实际产出

| 指南要求 | 产出 | 位置 |
|---|---|---|
| Widget Registry | `WidgetDescriptor` / `WidgetRegistry`（内置 + 插件卡片） | `crates/rmccc-dashboard/src/widget.rs` |
| Layout Schema | JSON Schema + 严格 Rust 解析 + 损坏备份/默认恢复 | `schemas/dashboard-layout.schema.json`、`layout.rs`、`store.rs` |
| 归一化算法 | 去重→尺寸钳制→位置换行→排序→push-down→压缩；17 项单测 | `layout.rs` |
| 拖放/调整/隐藏 | CSS Grid + HTML5 拖放 + 宽度循环 + 隐藏/恢复/重置 | `src/routes/Dashboard.tsx` |
| 主题令牌 | `--rmccc-*` 令牌层 + 桥接旧变量 + 浅/深/跟随系统 | `src/theme/index.ts`、`theme.rs` |
| 卡片生命周期 | `ErrorBoundary` + 未知/卸载占位（保留位置） | `Dashboard.tsx` |
| 内置功能迁移 | `org.rmccc.core-ui` 三个 Widget（账户/实例/版本） | `Dashboard.tsx` |

## 4. 任务切片

### 3.1 Layout Schema + 归一化算法（ADR-005）✅

- 布局文件：`dashboard/layouts/<layoutId>.json`，字段
  `schemaVersion/layoutId/grid{columns,gap}/widgets[]`。
- 归一化（确定性纯函数，不依赖 DOM）：
  1. 同一 `instanceId` 去重；
  2. 尺寸钳到 `[minSize, maxSize] ∩ [1, columns]`；
  3. `x + w > columns` 换行；`x` 越界归零；
  4. 按 `(y, x)` 排序；
  5. 与已放置卡片重叠时 push-down；
  6. 删除空行（整体上移）。
- 损坏布局：备份 `.bak-<ts>` + 默认布局；未知 schemaVersion 拒绝。
- **测试**：`layout` 12 项 + `store` 3 项 + `theme` 4 项 = **17 passed**。

### 3.2 Widget Registry + 受控渲染 ✅

- `WidgetDescriptor`：`pluginId/widgetId/displayName/defaultSize/minSize/maxSize/isBuiltin`。
- 内置 `org.rmccc.core-ui`：`instance-launcher`（8×4）、`account-card`（4×4）、
  `version-browser`（12×3）。
- 插件 `dashboardCards` 自动映射为 `dashboard-card:<cardId>` WidgetDescriptor。
- 9 条 IPC：`dashboard_get_layout/save_layout/reset_layout/list_widgets/list_themes/
  get_theme_tokens/set_theme_preference/get_theme_preference/get_theme_json`。

### 3.3 拖放与响应式 ✅

- CSS Grid（默认 12 列、120px 行高、gap=16）。
- 断点：≥1280 wide / 768–1279 medium（≤8 列）/ <768 compact（≤4 列）；
  宽度在渲染时钳制，不修改存储布局。
- **编辑模式**：默认视图态（无卡片头、无拖放、卡片可点）；「编辑布局」进入编辑态后才有
  `⠿` 拖动、`↔` 循环宽度、`×` 隐藏与「添加插件卡片 / 恢复隐藏 / 恢复默认」。
  卡片头只在编辑态渲染（`opacity: 0` 遮挡点击是 2026-10-21 修复的回归）。
- **测试**：`Breakpoint::from_width` / `effective_columns` / `effective_size` / overrides。

### 3.4 主题令牌系统 ✅

- `ThemeTokens`：background/surface/surfaceElevated/text/textMuted/accent/accentContrast/border。
- 映射到 `--rmccc-*` 并桥接旧 `--surface-*`/`--text-*`/`--color-blue`/`--border-*`。
- 安全语义色（danger/success/warning）由宿主按明暗模式设置，主题 JSON 中出现时忽略。
- 主题来源：内置 light/dark + 已启用插件的 `contributes.themes`（JSON，无 JS）。
- 偏好：`light` / `dark` / 跟随系统（`prefers-color-scheme` 监听）。
- 无效/未知主题回退内置安全主题 + 日志。

### 3.5 插件卸载占位 + 错误边界 ✅

- `ErrorBoundary` 包裹每个卡片；单卡片失败只显示 `WidgetError`。
- `widgetDescriptors` 缺失（插件卸载/禁用）时渲染 `UnknownWidget`，
  保留布局位置与配置，重新安装后可恢复。
- 隐藏卡片保留 `enabled=false`，恢复隐藏/重置布局可恢复。

## 5. 验收标准

- [x] 用户可自定义布局并重启保留；恢复默认可用。
- [x] 内置功能表现为 `org.rmccc.core-ui` Widget（账户/实例/版本）。
- [x] 插件卡片失败/卸载不损坏其它卡片或主程序。
- [x] 主题令牌 + 浅/深/跟随系统；无效主题回退。
- [x] 布局算法单测覆盖冲突/越界/迁移。
- [x] ADR-005 accepted；`pnpm typecheck/build` 通过；Windows UI 差异 = deferred。

## 6. 验证命令

```bash
cargo test -p rmccc-dashboard    # 17 passed
cargo test --workspace           # 485 passed / 0 failed
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
pnpm typecheck && pnpm build
```

## 7. 平台策略

- WebView 差异（macOS WebKit vs Windows WebView2）按指南 §5.6 分平台验证；
  本机只测 macOS，Windows 标 deferred。
- 高 DPI / 字体回退 / 输入法差异 deferred（无原生 CI）。

## 8. 风险与回滚

- 旧 Dashboard 路由已被网格替换；回滚 = `git revert` 本切片，布局文件独立于主配置。
- 主题覆盖安全语义色：令牌层在 Rust/Theme 模块中拒绝 danger/success/warning。
- 插件卡片真正渲染在 Phase 4（隔离 UI）；Phase 3 只显示占位。

## 9. 文档同步

- `ARCHITECTURE.md` §9.7；`AUDIT` §10.1/§10.4；`PROJECT_STATUS` 回填；
  `00_ROADMAP_OVERVIEW` 更新测试数与阶段进度。
