# 03 · 背景子系统 `src/backdrop/`

> 目标：把当前**三个并行全屏背景层**（氛围 / 视频 / 图片）+ 全屏 blend 装饰，
> 收敛成**唯一一层可缓存背景**（[01](01_TARGET_ARCHITECTURE.md) §3 的 L0），
> 消除审计根因 §3.1（视频逐帧 canvas）与 §3.3（全屏 blend + 大模糊）。

---

## 1. 现状问题回顾

| 问题 | 证据 | 后果 |
|---|---|---|
| 视频每帧 `drawImage` 到全屏 canvas | [`BackgroundVideo.tsx:99-186`](../../src/components/BackgroundVideo.tsx) | GPU 常驻满负荷 |
| 1s watchdog + 三级恢复 + focus/visibility 回调链 | 同上 L193-313、L363 | 长期定时器/回调 |
| 氛围层 veil `blur(58px)` 全屏 | [`ambient-stage.css:46-51`](../../src/styles/ambient-stage.css) | 无法缓存 |
| particles `screen` / grain `overlay` 全屏 blend | 同上 L120-126、L179-187 | 强制回读整层 |
| 三背景层各自 `Show` 独立挂载 | [`App.tsx:117-120`](../../src/App.tsx) | 层间叠加、状态分散 |

---

## 2. 目标结构：唯一 L0 背景层

```text
src/backdrop/
├── BackdropLayer.tsx     唯一入口：按 backdropKind 互斥渲染下面三者之一
├── VideoBackdrop.tsx     原生 <video> 为主，canvas 仅黑屏兜底
├── ImageBackdrop.tsx     静态图片（object-fit: cover）
├── AmbientBackdrop.tsx   静态烘焙渐变（默认主题壁纸）
└── decor/
    └── DecorLayer.tsx    L1 装饰（粒子/极光）；受 RenderTier 降级，全屏禁 blend
```

`BackdropLayer` 挂载/切换时调用 [`setBackdropKind()`](02_RENDER_TIER_ENGINE.md)（[02](02_RENDER_TIER_ENGINE.md) §3），
驱动 RenderTier 重算。**互斥**保证任意时刻只有 1 个全屏背景在合成（[01](01_TARGET_ARCHITECTURE.md) P1）。

---

## 3. VideoBackdrop 重写（根因 §3.1）

### 3.1 设计思路

当前用"逐帧 canvas blit"这一条最贵路径同时兜两端。改为**能力驱动的双路径**：

- **主路径（默认）**：直接用原生 `<video>` 元素铺满 L0，`object-fit: cover`，
  dim/scrim 用一个**静态**叠加层（`background: rgba`）实现，**不逐帧绘制**。
  Windows WebView2 与多数 macOS 场景走这条，GPU 只承担解码 + 一次合成。
- **兜底路径（按需）**：仅当探测到 WKWebView"原生合成面丢失（黑屏）"时，
  才启用 canvas 逐帧绘制。**探测**而非"常驻"：播放后若 `requestVideoFrameCallback`
  报告有帧呈现、但 `<video>` 区域实测为黑（一次性像素探针），才切兜底。

### 3.2 兜底触发与退出

```text
onPlaying ->
  等待 N 帧 requestVideoFrameCallback（现有能力，见 BackgroundVideo.tsx:170）
  若原生面正常呈现（探针非黑）=> 保持主路径，零 rAF、零 canvas
  若探针持续黑屏          => 启用 canvas 兜底（保留现有 drawImage 逻辑，但仅此场景）
```

- **探针**：播放稳定后抓 1 帧、缩到极小（如 4×4）canvas 读亮度一次，判定是否黑屏；不常驻。
- **watchdog 收敛**：主路径下**不需要** 1s watchdog；兜底路径下保留一个轻量存活检查，
  且仅在 `document.hasFocus() && visible` 时运行（现有 `checkPlayback` 已有此门控，L236）。
- **恢复逻辑保留但降级**：`recoverSurface`/`recoverPlayback`（现有 L193-313）只在兜底路径存在。

### 3.3 分辨率与解码

- 继续吃后端已有的 H.264 hardware-safe 代理（[`backgroundVideo.ts`](../../src/lib/backgroundVideo.ts) 的 `prepare`，
  后端负责探测/转换，前端不重复造）。
- canvas 兜底若启用，dpr 上限维持 `≤1.5`（现有 L88），且节流 33ms（现有 L101）。

### 3.4 预期

多数场景（尤其 Windows、以及原生面正常的 macOS）**彻底不跑 canvas 循环**，
删除约 300 行常驻绘制/看门狗代码的执行路径（代码保留为兜底分支，但默认不激活）。

---

## 4. AmbientBackdrop 重写（根因 §3.3）

### 4.1 设计思路：烘焙，不实时

当前氛围层 = 多层全屏渐变 + veil(58px blur) + grid + particles(blend) + grain(blend) + vignette。
改为**单层静态烘焙**：

- **渐变**：保留 `--scene-*` 驱动的多重 `radial/linear-gradient`（这些是静态的、可缓存），
  合并进 L0 单一元素的 `background`。
- **veil（58px blur 圆）**：**删除实时 blur**，改为把柔和光晕直接写进径向渐变
  （`radial-gradient(... transparent 68%)` 本就能表达柔光，无需 `filter: blur`）。
- **grid**：保留为静态 `background-image` 线纹（当前已移除位移动画，L115-117），
  用 `mask-image` 做地平线淡出。静态、可缓存。
- **grain**：从 `mix-blend-mode: overlay` 的全屏层，改为**一张低透明噪点贴图**
  （data-uri / 小 PNG，`opacity: .03`，`background-repeat`），**不用 blend**。
- **vignette**：保留（纯静态径向渐变，无 blend）。

### 4.2 粒子/装饰的去留

- 粒子当前已是静态（上一轮 `9f624f6` 移除动画，L136-137）。保留为**静态纹理**，
  移入 `decor/DecorLayer`，`mix-blend-mode: screen` → 改为 `opacity` 叠加（禁全屏 blend，[01](01_TARGET_ARCHITECTURE.md) §5.2）。
- Tier=static 时装饰整体不渲染；Tier=acrylic 时静态；仅 Tier=full 允许 1 个 transform-only 漂移动画。

### 4.3 主题大气变体

现有 `data-atmosphere="verdant|abyss|ember|rose|parchment|daybreak"` 变体
（[`ambient-stage.css:191-217`](../../src/styles/ambient-stage.css)）**保留**，
但只改静态几何/透明度/渐变色，**不得**引入 blur 动画或 blend。rose/parchment 当前的 `blur(66px/70px)`
（L209-211）改为渐变表达。

---

## 5. ImageBackdrop

- 静态 `<img>` / `background-image`，`object-fit: cover`，一次性绘制，零动画、零 rAF。
- dim/scrim 用静态叠加层。
- 与自适应取色解耦：取色改为单帧、移出渲染路径（见 [05](05_ADAPTIVE_AND_MOTION.md)）。

---

## 6. DecorLayer（L1）

- 承接原 `ThemeEffects`（aurora/particles/rotating-blocks）与氛围粒子，统一受 RenderTier 降级（[06](06_THEME_EFFECTS.md)）。
- **全屏禁 blend**；旋转立方体等 3D CSS 在 acrylic/static 档降级为静态贴图或关闭。
- 任意时刻 L1 最多 1 个 transform-only 动画层（[01](01_TARGET_ARCHITECTURE.md) §5.1）。

---

## 7. 状态与生命周期

| 事件 | 行为 |
|---|---|
| 用户切壁纸（图/视频/无） | `BackdropLayer` 互斥切换 → `setBackdropKind()` → RenderTier 重算 → 表面原语随之切档（[04](04_SURFACE_TOKENS_CSS.md)） |
| 页面隐藏（`visibilitychange`） | 视频：**不暂停**（保留现有 macOS 复现结论，BackgroundVideo.tsx L293-303），但主路径本就零 rAF；兜底路径暂停绘制 |
| 窗口失焦 | 主路径无动作；兜底路径停绘制循环 |
| 组件卸载 | 停一切 rAF/interval/回调（现有 `onCleanup` L379-391 保留并简化） |

---

## 8. 验收目标

- [ ] 视频壁纸在原生面正常时：主路径运行，**canvas 兜底未激活**，静止页 0 rAF。
- [ ] 人为制造黑屏（或 macOS 复现）时：兜底自动接管，画面恢复。
- [ ] 氛围层：全屏层 0 `mix-blend-mode`、0 `filter: blur` 动画；grain 为静态贴图。
- [ ] 三背景互斥：任意时刻 DOM 中只有 1 个全屏背景层。
- [ ] 切换壁纸时 RenderTier 与卡片表面档位联动正确（video → acrylic）。
