# ADR-005: Dashboard 布局 Schema、归一化算法与断点规则

- **状态**：accepted（2026-10-21 定稿）
- **日期**：2026-10-21
- **阶段**：Phase 3（切片 3.1–3.3）
- **关联**：指南 §9.1–§9.5；ADR-003/004（插件贡献项）；审计 §10.1/§10.4

## 背景

当前主界面是 815 行固定 JSX（`Dashboard.tsx`），CSS 变量直接散落在 `:root`（1945 行 `app.css`），
没有布局配置、没有卡片注册表、只有暗色主题。Phase 2 已交付声明式插件贡献项
（`dashboardCards` / `themes`），但它们没有消费方。

Phase 3 要把固定 JSX 迁移为「注册表 + 布局配置 + 受控渲染」，并落地主题令牌系统。
在写代码前必须固定：

1. 布局文件长什么样、存在哪里；
2. 冲突/越界/非法尺寸如何归一化（确定性算法，不依赖 DOM 顺序）；
3. 响应式断点规则；
4. 主题令牌与安全语义色的边界。

## 决策

### 1. 网格模型

- 使用 **CSS Grid**，逻辑列数（默认 12），间距以像素表示。
- 卡片位置 `position = { x, y }` 是网格单元坐标（0-based），尺寸 `size = { w, h }` 是占用的列×行数。
- 不使用像素绝对坐标作为唯一布局机制。

### 2. 布局文件

`schemas/dashboard-layout.schema.json`（Draft 2020-12，严格拒绝未知字段）：

```json
{
  "schemaVersion": 1,
  "layoutId": "default",
  "grid": { "columns": 12, "gap": 16 },
  "widgets": [
    {
      "instanceId": "core-launcher-main",
      "pluginId": "org.rmccc.core-ui",
      "widgetId": "instance-launcher",
      "enabled": true,
      "position": { "x": 0, "y": 0 },
      "size": { "w": 8, "h": 3 },
      "configuration": {}
    }
  ]
}
```

- 存储路径：`<data_dir>/dashboard/layouts/<layoutId>.json`。
- `schemaVersion = 1`；未知版本只读拒绝；v1 严格拒绝未知字段。
- 损坏布局：备份 `.bak-<ts>` + 恢复默认，不阻断启动。
- 多布局模板：`dashboard/layouts/` 下可有多个文件；`layoutId` 在目录内唯一。

### 3. 归一化算法（确定性，可单测）

写入前对布局执行归一化，步骤：

1. **去重**：同一 `instanceId` 保留第一条，其余丢弃 + 记录修正。
2. **尺寸钳制**：`size` 钳到 `[minSize, maxSize] ∩ [1, columns]`。
   `minSize` 缺省 `{1,1}`，`maxSize` 缺省 `{columns, ∞}`。若 `minSize > maxSize`，取 `maxSize`。
3. **位置钳制**：`x ∈ [0, columns - w]`；`x < 0 → 0`；`x + w > columns →` 换行（`y += 1, x = 0`）。
4. **排序**：按 `(y, x)` 升序。
5. **重叠解决**：对排序后的每个卡片，检查与已放置卡片是否重叠；
   若重叠，将当前卡片下推（`y += 1`）直到无重叠。这是 **push-down** 策略：
   先出现的卡片赢，后出现的被推到下方。
6. **压缩**：删除空行（所有卡片整体上移）。
7. **返回**：归一化后的布局 + 修正报告（每条修正的原因）。

该算法**不依赖 DOM 渲染顺序**，纯函数可单测。

### 4. 断点规则

| 断点 | 窗口宽度 | 生效列数 |
|---|---|---|
| `wide` | ≥ 1280px | 布局声明的 `grid.columns`（默认 12） |
| `medium` | 768–1279px | `min(columns, 8)` |
| `compact` | < 768px | `min(columns, 4)` |

- 断点切换在渲染时钳制卡片宽度到生效列数（不修改存储的布局）。
- `WidgetInstance.breakpointOverrides` 可选：不同断点下用不同尺寸。
- 断点切换不应导致卡片消失；宽度溢出时钳制到当前列数。

### 5. Widget 注册

- **WidgetDescriptor**：`pluginId` / `widgetId` / `displayName` / `version` /
  `defaultSize` / `minSize` / `maxSize` / `settingsSchemaRef` / `requiredCapabilities` / `lifecyclePolicy`。
- **WidgetInstance**：`instanceId` / `pluginId` / `widgetId` / `enabled` /
  `position` / `size` / `breakpointOverrides` / `configuration`。
- 内置功能迁为 `org.rmccc.core-ui` Widget（启动面板 / 实例列表 / 账户卡 / 日志）。
  `org.rmccc.core-ui` 是保留命名空间，不由第三方插件注册。
- 插件贡献的 `dashboardCards`（ADR-004）自动成为 WidgetDescriptor（Phase 2 已登记）。

### 6. 卡片生命周期

每个卡片有状态：`loading` / `ready` / `empty` / `error` / `permission_denied` / `unsupported`。
单卡片失败不崩 Dashboard（错误边界）。卸载的插件卡片显示不可用占位，保留原位置与配置。

### 7. 主题令牌

- **设计令牌层**（第一层）：`background` / `surface` / `surfaceElevated` /
  `text` / `textMuted` / `accent` / `accentContrast` / `border` /
  `radius` / `spacing` / `font` / `shadow`。
- 映射到 CSS variables（`--rmccc-*` 前缀），取代当前散落的 `:root` 变量。
- **安全语义色**（`danger` / `success` / `warning`）由宿主拥有，**主题不得覆盖**。
  主题 JSON 中出现这些键时忽略并记录。
- 主题包格式 = Phase 2 的 `contributes.themes`（JSON + 静态资源，无 JS）。
- 策略：浅色 / 深色 / 跟随系统（`prefers-color-scheme`）。
- 主题无效 → 回退内置安全主题 + 日志记录。
- 主题不继承业务权限。

## 后果

- 正面：布局可测试、可迁移、可回滚；主题可扩展且不越安全边界。
- 负面：Dashboard 前端需大改（815 行 → 拆分）；迁移期间需保留旧路由直到新 Registry 烟测通过。
- 体积：无新第三方依赖（纯 Rust + CSS）。

## 迁移影响

- 首次落地，无历史布局数据。
- 内置 Widget 从 Dashboard 固定 JSX 抽取；旧 Dashboard 路由保留直到切换完成。

## 跨平台验证方式

- macOS WebKit 本机测；Windows WebView2 差异（高 DPI / 字体回退 / 输入法）deferred。
- 布局算法在 Rust 单测覆盖，不依赖 WebView。

## 补充（2026-10-27）：主题插件契约与运行期校验

决策 7 落地后，主题插件需要一条"能打包 = 能安装 = 能应用"的硬契约。补充如下
（编号接续决策 7，不改变原决策文本）：

1. **文档格式唯一来源**：`crates/rmccc-plugin/src/theme.rs` 的 `ThemeDocument`
   是主题 JSON 的权威解析器（`schemaVersion = 1`、严格拒绝未知顶层字段、`mode ∈ {dark, light}`）。
   `rmccc-dashboard` 只做"令牌映射"，复用同一份类型与颜色校验，不复制 schema。
2. **颜色语法收窄**：主题颜色只允许 `#rgb` / `#rrggbb` / `#rrggbbaa`。
   主题值最终写进 CSS 自定义属性，若允许任意字符串，`url(...)` 可能被宿主样式消费成
   外链/追踪请求，`;`/`}` 则是注入面。校验在三个位置执行：
   包检查（`package::inspect_reader`）、主题选择（`dashboard_set_theme_preference`）、
   运行期令牌（`theme_tokens_for`）。
3. **安装期校验**：主题文件必须存在、`.json` 后缀、≤ 64 KiB，并通过
   `ThemeDocument::parse`；不合法直接拒绝安装，而不是装完后"永远选不中"。
4. **明暗**：`mode` 声明优先；缺省按 `background` 相对亮度推断（与前端 `isDark` 同规则），
   `ThemeInfo.mode` 不再恒为 `custom`。
5. **内置主题可对照**：`dashboard_get_theme_json` 对 `builtin:*` 合成同 schema 的 JSON，
   方便作者以内置主题为起点；插件主题返回原文件（校验后）。
6. **选择即校验**：不存在/损坏的 key 不再"保存成功但界面回退"，后端拒绝写入偏好。
7. **安全语义色边界不变**：`danger`/`success`/`warning` 即使出现在主题文件中也不进入令牌，
   宿主按明暗固定注入。

主题作者-facing 规范见 [`docs/guides/THEME_PLUGIN_AUTHORING.md`](../guides/THEME_PLUGIN_AUTHORING.md)；
JSON Schema 见 [`schemas/theme.schema.json`](../../schemas/theme.schema.json)。

## 状态变更记录

- 2026-10-21：proposed（Phase 3 启动）
- 2026-10-21：accepted（切片 3.1 实现前定稿）
- 2026-10-27：补充决策 8（主题文档校验/安装期校验/颜色语法，见 `THEME_PLUGIN_AUTHORING.md`）
