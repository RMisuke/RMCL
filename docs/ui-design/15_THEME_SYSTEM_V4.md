# 12 · 主题系统 V4（配色收敛 + 日夜模式 + 动态背景）

> 状态：设计中（2026-11-04）。取代 `06_THEME_EFFECTS.md` 中"8 个内置场景"的配色组织方式；
> 渲染层约束仍以 `ui-render-refactor/02_RENDER_TIER_ENGINE.md` 为准（动效只允许
> transform/opacity，禁止全屏 blur / mix-blend）。

## 1. 目标

1. 把当前"8 个内置场景 + 插件主题 + 自适应"的散装配色收敛为 **8 个可选配色**：
   6 个预设（动态背景 + 日/夜双配色）+ 1 个自适应 + 1 个自定义。
2. 新增 **日夜模式**：关闭 / 自动（跟随系统）/ 定时（自定义起止时间）三档。
3. 每个预设的**背景动效用纯 CSS 实现**，且动效语义与主题名一致。
4. 自定义配色只暴露 **3 个基本色**（主色 / 背景 / 文字），其余令牌自动派生。
5. 配置文件重构：主题状态从"后端 preference 字符串 + 场景表"改为前端单一
   `localStorage` 配置块，后端 preference 仅保留给插件主题兼容。

## 2. 架构

```
src/theme/v4/
├── registry.ts     # 8 个主题定义（id/名称/动效 id/日夜 base3 配色）
├── palette.ts      # base3 → 完整 ThemeTokens 派生（纯函数，可测试）
├── daynight.ts     # 日夜模式解析（off/auto/schedule）+ 分钟级调度
├── store.ts        # 信号 + localStorage 持久化 + applyThemeTokens 接线
└── motion.css      # 6 套 CSS 动效（keyframes + 分层伪元素）
```

- `store.ts` 是唯一写 `--rmccc-*` 的入口（复用 `theme/index.ts#applyThemeTokens`）。
- 自适应主题仍走 `lib/adaptiveTheme.ts`；v4 只负责开关它。
- 动效层挂在 `AmbientBackdrop` 内（`data-motion-theme`），受
  `html[data-motion]` 与 render tier 闸门控制；`static` 档只渲染静态帧。

## 3. 配色模型

每个预设/自定义只存 **base3**：`{ accent, background, text }`。派生规则：

| 令牌 | 派生 |
| --- | --- |
| surface | mix(background, text, 5%) |
| surfaceElevated | mix(background, text, 9%) |
| border | mix(background, text, 16%) |
| textMuted | mix(text, background, 40%) |
| accentContrast | luminance(accent) > 0.62 ? 深色字 : 浅色字 |

预设各自提供 day / night 两组 base3；自定义只有一组（不随日夜变化）。

## 4. 预设清单（6）

| id | 名称 | 动效语义 | 夜 base3（accent/bg/text） | 日 base3 |
| --- | --- | --- | --- | --- |
| stellar | 星野 | 星点漂移 + 星云色相缓移 | #8fb7ff / #0b1020 / #e8ecf7 | #3f6fd8 / #eef3fb / #1b2333 |
| verdant | 林语 | 落叶 + 林间光柱 | #9fd478 / #0d1710 / #e6f0e2 | #3f8f4f / #eef6ea / #1c2a1c |
| abyss | 深海 | 气泡上升 + 水波焦散 | #5fd6cf / #06131a / #dceef2 | #1f8f96 / #e8f5f6 / #123033 |
| ember | 熔火 | 火星上升 + 热浪脉动 | #ff8a4d / #170b08 / #f6e7dd | #d8552b / #fbf0e8 / #33150c |
| sakura | 樱吹雪 | 花瓣飘落 + 粉色雾霭 | #f27ba0 / #170b12 / #f6e8ee | #d84f7d / #fdf1f5 / #3a1622 |
| dawn | 晨曦 | 日光扫掠 + 浮尘 | #f2b04e / #10131c / #eef0f6 | #d8862b / #fbf4e8 / #2c2415 |

## 5. 日夜模式

- `off`：使用主题默认档（预设默认 night；自定义/自适应忽略）。
- `auto`：跟随系统 `prefers-color-scheme`。
- `schedule`：`dayStart` / `dayEnd`（HH:MM），区间内为 day。
- 调度器每 30s 检查一次 + 系统变化监听；切换时重算 tokens 并发布
  `data-daynight="day|night"`。

## 6. 设置页

外观页新增：
1. 主题网格（8 卡：6 预设 + 自适应 + 自定义），卡片显示名称 + 三色预览。
2. 日夜模式分段控件 + 定时时间输入（仅 schedule 显示）。
3. 自定义编辑器（仅 custom 选中时）：3 个取色器。

## 7. 迁移

- 旧后端 preference（`builtin:*`）映射：dark→stellar、light→dawn、forest→verdant、
  ocean→abyss、ember→ember、sakura→sakura、paper→dawn、sky→dawn。
- 首次启动读取旧值写入 `rmccc.theme.v4`，之后不再读后端 preference。
