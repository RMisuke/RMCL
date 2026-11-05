# 06 · 主题特效与主题配置精简

> 目标：把主题插件的三种特效（aurora / rotating-blocks / particles）纳入
> [02 RenderTier](02_RENDER_TIER_ENGINE.md) 的统一降级，删除冗余配置，
> **同时保持主题 JSON 契约不变**（不破坏插件平台，遵守 ADR-022 与 [`theme/index.ts:198-200`](../../src/theme/index.ts) "themes never provide CSS"）。

---

## 1. 现状问题

| 问题 | 证据 | 后果 |
|---|---|---|
| 三种特效各自渲染，不受统一档位控制 | [`ThemeSurface.tsx:73-207`](../../src/components/ThemeSurface.tsx) | 与背景层叠加，全屏成本不可控 |
| `rotating-blocks` 用 3D CSS 立方体（多面 `background-image`） | ThemeSurface.tsx L91-149、[`theme-skin.css:160`](../../src/styles/theme-skin.css) `perspective:720px` | 多个 3D 面持续 transform，GPU 成本高 |
| `aurora` 三个全屏 `<i>` 动画 | ThemeSurface.tsx L73-89 | 全屏动画层，违反 [01](01_TARGET_ARCHITECTURE.md) §5.1 |
| 特效与背景媒体（video/image/poster）混在同一组件 | ThemeSurface.tsx L20-71 | 背景/装饰职责不分 |
| 门控分散：`data-motion="off"` 在每个特效各写 | theme-skin.css L276-287 | 重复、易漏 |

---

## 2. 设计思路：背景归 L0，特效归 L1，统一降级

### 2.1 职责拆分

- **主题背景媒体**（`skin.background`：video/image/poster/dim/blur，[ThemeSurface.tsx L20-71](../../src/components/ThemeSurface.tsx)）
  → 并入 [03 L0 BackdropLayer](03_BACKGROUND_SUBSYSTEM.md)，`backdropKind = "theme"`。
  主题视频走与用户视频壁纸**同一条**原生 `<video>` 主路径 + 黑屏兜底（[03](03_BACKGROUND_SUBSYSTEM.md) §3），不再各写一套。
- **主题特效**（`skin.effects`：aurora/rotating-blocks/particles）
  → 并入 [03 L1 DecorLayer](03_BACKGROUND_SUBSYSTEM.md) §6，受 RenderTier 降级。

### 2.2 三特效的降级矩阵

| 特效 | full | acrylic（默认） | static |
|---|---|---|---|
| `aurora` | 1 个 transform-only 缓动层 | **静态**柔光渐变（无动画） | 关闭 |
| `particles` | 静态纹理 + 可选 1 层漂移 | **静态**纹理（opacity 叠加，禁 blend） | 关闭 |
| `rotating-blocks` | 保留 3D 旋转（≤ count 上限） | **降级为静态贴图**（单个面，不旋转、不 3D） | 关闭 |

**关键**：acrylic/static 档下，rotating-blocks 不再渲染 5 个 3D 面 + 持续 transform，
而是把纹理作为**一张静态背景贴图**（或关闭），消除 §1 的 3D 立方体成本。

### 2.3 全屏层约束

- aurora 的三个全屏 `<i>` 合并为**单层**静态/缓动渐变（[01](01_TARGET_ARCHITECTURE.md) §5.1：全屏动画层 ≤ 1）。
- 所有特效层**禁用 `mix-blend-mode`**（§5.2），改 `opacity` 叠加。
- 特效层 `pointer-events: none` + `aria-hidden`（现有 ThemeSurface 已如此，保留）。

---

## 3. 主题配置精简（契约不变）

### 3.1 保持不变的契约

- `schemas/theme.schema.json`（358 行）**不改结构**：`colors`(8 色) / `background` / `effects`(≤3) / `components` / `typography` / `shape` / `spacing` 全部保留。
- 主题插件 manifest 与打包工具不改。
- `effects` 仍是固定预设库参数化（`kind` ∈ aurora/rotating-blocks/particles），主题不提供 CSS/JS。

### 3.2 删除的冗余（host 侧，非契约）

| 冗余 | 处置 |
|---|---|
| `ThemeSurface.tsx` 内独立的背景媒体渲染（video/image/poster） | 删除，统一走 [03 L0](03_BACKGROUND_SUBSYSTEM.md) |
| `theme-skin.css` 中每个特效各自的 `data-motion="off"` 门控（L276-287） | 删除，统一走 [05 motion.ts](05_ADAPTIVE_AND_MOTION.md) 单门控 |
| rotating-blocks 的 5 面 3D 立方体标记（ThemeSurface.tsx L120-141） | acrylic/static 档不渲染，仅 full 档保留 |
| `themeSkin()` 里与 `--soft-*` 相关的桥接（若有） | 删除（[04](04_SURFACE_TOKENS_CSS.md) §3.1） |

### 3.3 host 场景预设（`theme/presets.ts`）

- 8 个内建 scene（noctis/verdant/abyss/ember/rose/parchment/daybreak + custom）保留
  （[`presets.ts:53-150`](../../src/theme/presets.ts)），它们是**静态装饰参数**，本身无性能问题。
- 但 `particleCount` 的实际渲染数量受 [03 AmbientBackdrop](03_BACKGROUND_SUBSYSTEM.md) §4.2 与 RenderTier 降级
  （static 档不渲染粒子，acrylic 档静态）。上一轮已 ×0.55（[AmbientStage.tsx:46-53](../../src/components/AmbientStage.tsx)），保留该收敛。

---

## 4. 主题令牌应用瘦身（`theme/index.ts`）

- `applyThemeTokens` 删除 `--surface-*` / `--text-*` / `--color-blue` / `--border-*` legacy 桥接
  （[L104-115](../../src/theme/index.ts)），只写 `--rmccc-*` + `--scene-*`（[04](04_SURFACE_TOKENS_CSS.md) §3）。
- host-owned 安全色（red/green/yellow，L117-130）保留——这是**语义色**，非冗余。
- `applyThemeSkinPayload`（L278-357）保留组件级 skin 变量（`--rmccc-skin-*`），但其中
  `blur` 字段（L337-338）只在 full 档被 `.surface-glass` 消费；acrylic/static 档忽略（不产生 backdrop-filter）。

---

## 5. 验收目标

- [ ] 主题 JSON schema 与插件 manifest **零改动**；现有 4 个示例插件（theme-midnight/aurora/grass + server-dashboard）仍可加载。
- [ ] acrylic 档下 rotating-blocks 不渲染 3D 立方体（DOM 中无 `.face` × 5 × count）。
- [ ] 全屏特效层 ≤ 1 个带动画；0 全屏 `mix-blend-mode`。
- [ ] 主题背景视频与用户视频壁纸共用同一 VideoBackdrop 路径（无第二套绘制逻辑）。
- [ ] `theme/index.ts` 不再写 legacy 命名空间；`grep --surface-|--color-blue` 在 src/ 0 命中。
- [ ] 三门禁绿（typecheck/test/build），主题相关单测（`presets.test.ts`）通过。
