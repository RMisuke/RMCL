# 01 · 基本设计风格（设计语言与组件规范）

> 本文定义全局视觉语言。所有页面必须从本文的 token 与组件库取用，
> 禁止页面私自造样式（这是现状 2253 行手写 CSS 教训的直接对策）。

## 1. 风格基调

- **干净**：大面积留白、低饱和底色、每屏只有一个视觉焦点（主按钮/关键数据）。
- **安静**：颜色只用于语义（品牌强调、成功/警告/危险），不做装饰性配色；
  边框 + 微阴影分层，拒绝重投影。
- **紧凑而清晰**：桌面工具的信息密度（列表行 44px、卡片内边距 16px），
  但排版层级用字号/字重/灰度拉开，不靠堆颜色。
- **暗色优先**：默认暗色（与现状一致），亮色为完整一等公民，跟随系统为推荐默认值。

参照系：HMCL 的 Material Design 3 token 思路（语义角色色 + 统一圆角/阴影/间距），
但视觉上更扁平、更少装饰，符合 RMCCC「< 10MB 小工具」的气质。

## 2. 设计令牌（Design Tokens）

### 2.1 基础色（主题层，8 个，来自 ThemeTokens，schema 不变）

主题插件与内置主题只能提供这 8 个（`dashboard_get_theme_tokens`）：

```css
--rmccc-background        页面底色
--rmccc-surface           卡片/面板底色
--rmccc-surface-elevated  浮层（弹窗/菜单/抽屉）底色
--rmccc-text              主文字
--rmccc-text-muted        次要文字/占位符
--rmccc-accent            品牌强调色（主按钮/选中态/链接）
--rmccc-accent-contrast   强调色上的文字色
--rmccc-border            边框/分隔线
```

### 2.2 宿主衍生色（固定层，从基础色推导，主题改不了语义）

用 CSS `color-mix()` 从基础色推导，主题自动保持对比度一致性，**不扩主题 schema**：

```css
--rmccc-hover        = mix(surface, text 6%)          悬停底
--rmccc-active       = mix(surface, text 12%)         按压底
--rmccc-surface-sunken = mix(background, surface 50%)  凹陷区（代码块/日志底）
--rmccc-accent-soft  = mix(accent, surface 85%)       选中行底/标签底
--rmccc-danger       固定安全色（亮暗模式各自固定，不从主题取）
--rmccc-success      同上
--rmccc-warning      同上
--rmccc-info         同上
--rmccc-focus-ring   = accent 60% 透明度              键盘焦点环
```

> 红线（插件指南 §9.5）：`danger/success/warning/info` 是宿主安全语义色，
> 主题插件不可覆盖；现有 `--color-red/green/yellow` 旧变量在迁移期桥接，新代码一律用 `--rmccc-*`。

### 2.3 圆角 / 间距 / 字号 / 阴影

```css
/* 圆角 */
--rmccc-radius-tag 4px;  --rmccc-radius-control 6px;
--rmccc-radius-card 8px; --rmccc-radius-modal 12px;

/* 间距（4px 栅格） */
--rmccc-space-1 4px;  --rmccc-space-2 8px;  --rmccc-space-3 12px;
--rmccc-space-4 16px; --rmccc-space-5 24px; --rmccc-space-6 32px;

/* 字号阶梯（系统字体栈，含 PingFang SC / Microsoft YaHei 回退） */
20px 页面标题 / 16px 区块标题 / 14px 正文与控件 / 13px 列表副标题 / 12px 标签与元信息
行高：正文 1.5，标题 1.3。

/* 阴影（仅两级 + 焦点环） */
--rmccc-shadow-1  卡片：0 1px 2px rgba(0,0,0,.12)（暗色下近乎不可见，靠 border 分层）
--rmccc-shadow-2  浮层：0 8px 24px rgba(0,0,0,.28)
```

## 3. 主题体系

| 来源 | 值 | 说明 |
|---|---|---|
| 内置 | `builtin:dark` / `builtin:light` | 随包内置 token |
| 模式策略 | `system`（默认）/ `dark` / `light` | `dashboard_set_theme_preference("")` = 跟随系统 |
| 插件 | `<pluginId>/<themeId>` | theme-midnight 等主题包，仅提供 8 token |

- 应用链（现有，保持）：偏好存 `UserSettings.theme` → `dashboard_get_theme_tokens` → 写 CSS 变量。
- 降级：未知/损坏主题回退内置安全主题并 toast 提示（现有行为）。
- 主题预览：设置 → 通用 → 外观的主题卡片用 `dashboard_get_theme_tokens` 画色板条
  （背景/surface/accent/text 四色条），`dashboard_get_theme_json` 用于「查看原始 JSON」调试入口。

## 4. 布局系统

| 层 | 规格 |
|---|---|
| 窗口 | 最小 1280×720；推荐默认 1440×900 |
| TitleBar | 40px，保留现有自定义实现（macOS 红绿灯偏移已在） |
| Rail | 200px（收起 64px）；导航项高 40px，图标 20px + 13px 文字 |
| PageHeader | 48px：左侧面包屑/标题（20px），右侧动作区（≤3 按钮 + ⋯ 菜单） |
| SideTabs（页内子导航） | 200px，项高 40px，图标 + 文字，选中态 accent-soft 底 + 左侧 2px accent 条 |
| 内容栅格 | 卡片网格 12 列（Dashboard 布局引擎）；表单页单列 max-width 720px 居中 |
| Dashboard 网格 | 沿用后端归一化：列数 12、gap 16、行高 120px、断点 wide/medium(≤8 列)/compact(≤4 列) |

## 5. 组件库清单

> 命名 `Rm` 前缀；全部吃 token，主题即换肤。状态：default / hover / active / focus-visible /
> disabled / loading / error。组件先做这 20 个，页面不得绕过组件直写样式。

| 组件 | 变体与规格 | 用于 |
|---|---|---|
| `RmButton` | primary（accent 底）/ secondary（surface 底 + border）/ ghost（无底）/ danger（danger 底）；高度 32（默认）/28（紧凑）/40（大，仅主页启动键） | 全局 |
| `RmIconButton` | 28/32px 方形，tooltip 必配 | 工具栏 |
| `RmInput` / `RmTextarea` | 32px 高，聚焦 focus-ring，行内错误 12px danger | 表单 |
| `RmSelect` | 原生 select 封装（下拉面板自绘，支持分组） | 筛选器 |
| `RmCheckbox` / `RmSwitch` | 16px / 32×18px | 列表启停、设置开关 |
| `RmSlider` | 滑条 + 数值框联动 | 内存设置 |
| `RmTag` | neutral（border）/ accent / danger / success / warning；4px 圆角 12px 字 | 版本类型、loader、风险 |
| `RmCard` | radius-card + border + shadow-1，可选 header（16px 标题 + 动作区） | 一切分区 |
| `RmListRow` | 高 44px，结构：`[图标24] [主文字14 + 副文字13 muted] [尾随区]`，hover 显示行操作 | 实例/账号/Mod/版本/插件列表 |
| `RmSideTabs` | 页内子导航（§4） | 实例详情/下载中心/设置 |
| `RmTabs` | 顶置 tab（下划线式），仅用于小范围切换 | 插件详情 |
| `RmDialog` | 420–560px 宽 modal，radius-modal，遮罩 40%；ESC/点遮罩关闭（表单有改动时拦截） | 确认/输入/版本选择 |
| `RmConfirmDialog` | danger 动作红色主按钮 + 说明后果 + 可选「输入名称确认」 | 删除类 |
| `RmMenu` | 下拉/右键菜单，surface-elevated，项 32px，支持分隔线与分组标题 | 浏览/管理/⋯ |
| `RmToast` | 右下角，4s 自动消失，success/error/info；带可选「查看」动作 | 轻反馈（现有 Toast 层升级） |
| `RmProgress` | 线性（任务行内）/ 环形（Rail 徽标 20px）；determinate | 任务中心 |
| `RmBadge` | 8px 圆点 / 16px 计数，danger/accent | Rail 徽标 |
| `RmEmptyState` | 图标 48px + 标题 16px + 说明 13px + CTA 按钮 | 所有列表空态 |
| `RmErrorState` | 错误说明（人话）+「重试」按钮，整块可点击重试 | 三态之一 |
| `RmSkeleton` | 列表/卡片骨架，shimmer 1.2s | 三态之一 |
| `RmContentState` | 三态容器组件：`loading → RmSkeleton`，`error → RmErrorState`，`empty → RmEmptyState` | 所有异步数据区 |
| `RmAdvancedSection` | 折叠区：标题「高级」+ 展开箭头 + 灰字摘要（收起时也可见关键值） | 渐进披露三级 |
| `RmInheritLock` | 继承/覆盖切换小按钮（20px，锁开/锁闭图标），见 04 §设置 tab | 字段级继承 |
| `RmVirtualList` | 现有组件保留，补 44px 行高预设 | 版本列表/Mod 结果 |

## 6. 图标体系

- 图标集：**Material Symbols（outlined 400 weight）内联 SVG 子集**，精选约 90 枚
  （与 HMCL 同源语义：`sports_esports` 实例、`extension` Mod/插件、`download`、`settings`、
  `smart_toy` Agent、`person`、`home`、`progress_activity` 任务、`rocket_launch` 启动…）。
- 实现约束：**不用图标字体**（体积红线）；SVG path 内联为 TS 常量（参照 HMCL `SVG.java` 做法），
  单枚 ≤ 1KB，总增量 < 90KB；选中态用同名 `_fill` 变体（Rail 导航）。
- 图标颜色继承 `currentColor`；语义色场景（danger 删除按钮）由组件注入。

## 7. 文案与 i18n 规范

- 延续现有自研 i18n（`t(key, params)` + `{name}` 插值；`en.ts` 为参考语言包，缺键 typecheck 报错）。
- 新命名空间按页面：`home.*` / `instances.*` / `instanceDetail.*` / `download.*` /
  `modStore.*` / `accounts.*` / `settings.*`（含 `settings.ai.*`/`settings.mcp.*`）/
  `plugins.*` / `tasks.*`（含归因与确认）/ `logs.ai.*` / `common.*`；
  预估总量 ~600 key（现 ~350，全部重写不迁移旧 key）。
- 文案规则：
  - 按钮用动词（「安装」「修复」），不用「确定」做破坏性动作；
  - 错误文案 = 发生了什么 + 下一步怎么办（「下载失败：镜像源不可达。可切换下载源后重试」），
    不出现错误码裸串（错误码放 tooltip/日志）；
  - 数字/日期本地化：相对时间（「3 小时前」）用前端格式化；下载量 `1.2M` 缩写。
- 中英同时写：每个 key 在 en/zh-CN 两个 bundle 同步提交。

## 8. 动效

| 场景 | 规格 |
|---|---|
| 页面切换 | 150ms 淡入（不做滑动，避免 SolidJS router 过度工程） |
| 弹窗/菜单 | 150ms 缩放 0.98→1 + 淡入；退出 100ms |
| 列表行 hover | background 120ms |
| 任务进度 | 进度条 width 200ms linear；Ring 匀速旋转 |
| 卡片拖拽（主页编辑模式） | 200ms ease，放置目标 accent-soft 高亮 |
| `prefers-reduced-motion` | 全部动效降为瞬时切换（CSS media query 统一处理） |

## 9. 可访问性

- 所有可交互元素可 Tab 聚焦，焦点环 `--rmccc-focus-ring` 2px，不用 `outline: none`。
- 文字对比度：text/background ≥ 4.5:1；muted ≥ 3:1（token 层面由内置主题保证，
  插件主题在设置页预览时给出对比度告警——预览组件内计算，不阻塞应用）。
- 图标按钮必须有 `aria-label`（i18n）；列表行操作按钮同。
- Toast 出现时 `aria-live="polite"`；危险确认对话框 `role="alertdialog"`。

## 10. CSS 架构（迁移策略）

1. **新代码**：组件级样式 + Tailwind v4 工具类混合——布局/间距用工具类
   （`flex gap-4 p-4`），皮肤一律引用 `--rmccc-*` 变量；禁止硬编码颜色字面量。
2. **旧代码**：`app.css` 2253 行手写样式**不迁移**——每个页面重写时整文件替换，
   旧类名（`settings-section`、`java-item` 等）随旧路由下线删除；
   迁移期通过 `--surface-*`/`--text-*` 桥接变量共存（现有机制），最终删桥接。
3. **验收**：每阶段结束跑一次「无未使用类」抽查（grep 旧类名出现处应只在待删文件）。

## 11. 渐进披露的视觉约定

- 一级控件：直接渲染。
- 二级分区：`RmCard` 分区标题（16px）+ 折叠由内容量决定（> 1 屏才折叠）。
- 三级高级区：统一 `RmAdvancedSection`，收起时右侧灰字显示当前关键值摘要
  （如「JVM 参数 · 3 条自定义」），让高级用户一秒判断里面有没有东西，不用点开。
