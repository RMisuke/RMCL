# 04 · 表面原语 + 统一令牌 + CSS 拆分

> 目标：消除审计根因 §3.2（玻璃实时采样活背景）与 §3.5（五套冗余变量），
> 把 4024 行的 `app.css` 巨石与 61 处 `backdrop-filter` 收敛成**少量表面原语 + 单一令牌源**。
> 档位由 [02 RenderTier](02_RENDER_TIER_ENGINE.md) 决定；背景由 [03](03_BACKGROUND_SUBSYSTEM.md) 提供。

---

## 1. 现状问题回顾

| 问题 | 证据 | 后果 |
|---|---|---|
| 卡片普遍 `backdrop-filter: blur(16-30px)` 盖在活背景上 | [`home-stage.css:166,327,794,892,1000`](../../src/styles/home-stage.css) | 每帧 × 每卡重模糊 |
| 已在硬扛：默认 blur 24→16px、壁纸激活减半、dock 降 14px | [`glass.ts:26-28`](../../src/lib/glass.ts)、home-stage.css L1003-1012 | 治标，成本仍随卡片数放大 |
| 五套变量命名空间并行 | glass.ts L70-89、theme/index.ts L104-115、adaptiveTheme.ts L143-183 | 体积/维护/级联脆弱 |
| `app.css` 4024 行 / 139 KB 巨石 | [`src/styles/app.css`](../../src/styles/app.css) | 全量解析、难维护 |
| `backdrop-filter` 61 处散落 | 6 文件 | 无统一预算 |

---

## 2. 表面原语（Surface Primitives）

用**三个类**取代散落的自定义玻璃样式。组件只挂类 + 令牌，不再各写 blur 值。

| 原语 | 视觉 | 是否采样背景 | 启用条件 |
|---|---|---|---|
| `.surface-glass` | 真毛玻璃：`backdrop-filter: blur(var(--glass-blur)) saturate(...)` | **是** | 仅 `data-render-tier="full"` **且**背景静态 |
| `.surface-acrylic`（默认） | 染色亚克力：半透明色 + 高光边 + 静态噪点，**无 backdrop-filter** | **否** | `acrylic` 档，或任何视频壁纸场景 |
| `.surface-solid` | 实色面：不透明色 + 边 + 阴影 | 否 | `static` 档 / 减动效 / 低配 |

### 2.1 亚克力如何"看起来像玻璃"（关键设计）

不采样背景也能读出玻璃质感，靠三件套（全部静态、可缓存）：

1. **半透明染色**：`background: color-mix(in srgb, var(--rmccc-surface) X%, transparent)`，
   X 由用户 `cardOpacity` 偏好驱动（沿用 [`glass.ts`](../../src/lib/glass.ts) 的三偏好）。
2. **高光边**：`box-shadow: inset 0 1px 0 rgba(255,255,255,.06)` + `border: 1px solid var(--rmccc-border)`，
   模拟玻璃折射亮边（当前卡片已有此写法，home-stage.css L94、L165）。
3. **静态噪点/微渐变**：一层极低透明的噪点贴图或 `linear-gradient(145deg, rgba(255,255,255,.05), transparent)`
   模拟磨砂颗粒（当前 L163 已有类似渐变）。

> 取舍：亚克力**不透出背景像素**，视频壁纸上卡片是"染色"而非"透出模糊"。
> 这是消除 §3.2 每帧重模糊的根本代价；Windows 静态壁纸档（full，且需独显）仍给真玻璃。详见 [07](07_IMPLEMENTATION_PLAN.md) 风险节。

### 2.2 原语与 Tier 的绑定（CSS 只认一个属性）

```css
/* 伪代码示意，非最终 */
.surface          { /* 公共：圆角/边/阴影/内边距，来自令牌 */ }
.surface-glass    { backdrop-filter: blur(var(--glass-blur)) saturate(var(--glass-sat)); }
.surface-acrylic  { background: color-mix(...); }   /* 无 backdrop-filter */
.surface-solid    { background: var(--rmccc-surface); }

/* 由 data-render-tier 决定哪个原语生效，组件不自行判断 */
html[data-render-tier="full"]    .surface-auto { /* → glass */ }
html[data-render-tier="acrylic"] .surface-auto { /* → acrylic */ }
html[data-render-tier="static"]  .surface-auto { /* → solid */ }
```

组件挂 `.surface.surface-auto`，实际档位由 `<html data-render-tier>` 统一裁决——
**取代**当前 `data-cards-transparent` / `data-panel-transparent` / `data-video-wallpaper` 的多重特判。

### 2.3 真 backdrop-filter 的预算

`.surface-glass` 只允许用于：对话框、菜单、popover、任务中心等**有界小浮层**，
且仅在 full 档。**禁止**全屏 `.app-body` 级 backdrop-filter（删除 home-stage.css L1000-1001 那类规则）。
目标：`backdrop-filter` 从 61 处降到**个位数**。

---

## 3. 统一令牌系统（根因 §3.5）

### 3.1 保留两套，删除三套

| 命名空间 | 处置 | 说明 |
|---|---|---|
| `--rmccc-*` | **保留（唯一语义源）** | 颜色/圆角/阴影/间距/表面 |
| `--scene-*` | **保留（唯一装饰源）** | primary/secondary/tertiary/wash/line，驱动氛围与装饰 |
| `--soft-*` | **删除** | 旧 dashboard 遗留（glass.ts、adaptiveTheme 双写） |
| `--surface-*` / `--text-*` / `--color-blue` / `--border-*` | **删除** | 更旧遗留桥接（theme/index.ts L104-115） |
| `--adaptive-*` | **降级为内部中间态** | 取色计算用，不直接暴露给组件 CSS（[05](05_ADAPTIVE_AND_MOTION.md)） |

### 3.2 令牌分层（`styles/tokens.css`）

```text
tokens.css
├── 基础令牌   --rmccc-radius-{tag,control,card,modal}、--rmccc-space-{1..8}、
│              --rmccc-shadow-{card,popover,...}、--rmccc-ease-*、--rmccc-duration-*
├── 语义色     --rmccc-background/surface/surface-elevated/text/text-muted/
│              accent/accent-contrast/accent-secondary/accent-tertiary/border
├── 表面令牌   --glass-blur、--glass-sat、--acrylic-alpha、--surface-alpha（由 glass.ts 偏好写入）
└── 装饰色     --scene-primary/secondary/tertiary/wash/wash-2/line/particle-opacity
```

**写入方**：主题令牌由 [`theme/index.ts`](../../src/theme/index.ts) 写 `--rmccc-*` + `--scene-*`；
表面偏好由 [`glass.ts`](../../src/lib/glass.ts)（迁到 `render/surfaces.ts`）写 `--glass-*`/`--acrylic-*`；
自适应取色由 [05](05_ADAPTIVE_AND_MOTION.md) 写 `--rmccc-*`（覆盖主题值）。**三方都只写这两套命名空间**。

### 3.3 迁移策略

- 一次性全量替换（符合"删除所有冗余配置"），**不保留 legacy 垫片**。
- 第三方主题若引用 `--soft-*`：主题 JSON 契约本就不含 CSS（[`theme/index.ts:198-200`](../../src/theme/index.ts) 注明
  "themes never provide CSS"），故删除 `--soft-*` **不破坏主题插件契约**，只影响历史手写 skin CSS（若有）。
- `disableAdaptive` 不再需要手动重贴 base 变量（当前 adaptiveTheme.ts L583-600 的脆弱逻辑），
  因为只有一套命名空间，清除 `--adaptive-*` 覆盖即回到主题值。

---

## 4. CSS 拆分（`app.css` 4024 行巨石）

### 4.1 目标结构

```text
src/styles/
├── tokens.css          唯一令牌源（§3.2）
├── primitives.css      .surface-* / .glass-* / .rm-btn / .rm-card 等原语
├── layout.css          app-root / app-body / dock / titlebar / 拖拽区
├── components/         按组件拆分（home/、instance/、settings/、accounts/、overlays/ …）
│   └── *.css           每个 < 400 行，只消费令牌与原语，不重复定义玻璃
└── themes/
    └── ambient.css     氛围层静态样式（删动画/blend，见 03）
```

### 4.2 拆分原则

1. **去重**：61 处 backdrop-filter → 原语类；重复的卡片玻璃样式合并。
2. **就近**：组件样式随组件（`components/home/*.css`），不再全塞 app.css。
3. **令牌优先**：任何颜色/圆角/阴影/间距必须引用令牌，禁止硬编码（迁移期允许少量例外，标注 TODO）。
4. **删除死代码**：`--soft-*` 相关规则、被 Tier 取代的 `data-*` 特判规则一并删。

### 4.3 体积目标

去重 + 删 legacy 后，CSS 总体积目标 **< 当前 40%**（当前 7654 行）。
`app.css` 不再存在（拆散到上述结构）。

---

## 5. `glass.ts` → `render/surfaces.ts`

保留三个用户偏好（panelOpacity / cardOpacity / cardBlur，[`glass.ts:21-29`](../../src/lib/glass.ts)），
但：

- **删 `--soft-*` 双写**（L71、L82）。
- cardBlur 偏好改为驱动 `--glass-blur`（仅 full 档消费）；acrylic 档消费 `--acrylic-alpha`（由 cardOpacity 派生）。
- `data-panel-transparent` / `data-cards-transparent` 特判并入 Tier + 表面令牌，删除独立属性。

---

## 6. 验收目标

- [ ] `backdrop-filter` 处数从 61 降到个位数，且全部在 full 档的小浮层。
- [ ] `<html>` 上无 `--soft-*` / `--surface-*` / `--text-*` / `--color-blue` 写入；`grep` 全仓 0 命中（除迁移说明文档）。
- [ ] 组件不再自定义玻璃样式，只挂 `.surface.surface-auto`；档位随 `data-render-tier` 联动。
- [ ] `app.css` 已拆散删除；单文件 < 400 行；CSS 总体积 < 当前 40%。
- [ ] 三个用户偏好（panel/card opacity、blur）在 Settings 仍可调，且在对应 Tier 下正确生效。
- [ ] `pnpm typecheck` / `pnpm test` / `pnpm build` 三门禁绿。
