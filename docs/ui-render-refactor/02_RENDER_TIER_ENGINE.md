# 02 · 渲染引擎模块 `src/render/`（RenderTier）

> 本模块是整个重构的**决策中心**：把"用哪种背景、玻璃采不采样、装饰动不动"这三件事
> 从散落的 CSS/组件特判收敛成**一个启动期决策 + 一个全局属性**。
> 上层原则见 [`01_TARGET_ARCHITECTURE.md`](01_TARGET_ARCHITECTURE.md) §4；问题基线见 [`00`](00_CURRENT_AUDIT.md) §3、§4。

---

## 1. 模块职责与边界

**负责**：探测运行环境 → 计算 `RenderTier` → 通过 `<html data-render-tier>` 广播 →
提供表面原语令牌（[04](04_SURFACE_TOKENS_CSS.md)）与动效门控（[05](05_ADAPTIVE_AND_MOTION.md)）的取值。

**不负责**：具体背景渲染（[03](03_BACKGROUND_SUBSYSTEM.md)）、主题特效内容（[06](06_THEME_EFFECTS.md)）、
业务组件。引擎只输出"档位与令牌"，不持有 DOM 层。

---

## 2. `RenderTier` 决策

### 2.1 输入信号

| 信号 | 来源 | 用途 |
|---|---|---|
| `platform` | `navigator.userAgent`（已在 [`App.tsx:94`](../../src/App.tsx) 用过） | 区分 Windows(Chromium) / macOS(WKWebView) |
| `reducedMotion` | `matchMedia('(prefers-reduced-motion: reduce)')` | 强制 static |
| `userMotion` | 用户设置 `settingsStore.motionEnabled()`（现有） | 关动效 → static |
| `backdropKind` | 当前背景是 `ambient` / `image` / `video`（来自 [03](03_BACKGROUND_SUBSYSTEM.md) 的状态） | 视频 → 至多 acrylic |
| `gpuTier`（可选） | 一次性轻量探测（见 §2.4） | 低配 → 降档 |

### 2.2 决策表（纯函数，便于单测）

```text
decideTier(input) ->
  if reducedMotion or !userMotion            => "static"
  else if gpuTier == "low"                    => "static"
  else if backdropKind == "video"             => "acrylic"   // 活背景：玻璃禁止采样
  else if platform == "macos"                 => "acrylic"   // WKWebView：backdrop over 任意背景偏贵
  else if platform == "windows" && backdropKind in {ambient,image} => "full"
  else                                        => "acrylic"   // 保守默认
```

**不变量**：`tier == "full"` ⇒ 背景一定是静态的（ambient/image）。这条被 [04](04_SURFACE_TOKENS_CSS.md)
的 `.surface-glass` 依赖——只有 full 档才允许真 `backdrop-filter`。

### 2.3 输出与广播

- 写 `document.documentElement.dataset.renderTier = tier`。
- 保留 `data-motion="on|off"` 作为 static 的可读别名（现有 CSS 已大量引用，见 theme-core.css L758-774）。
- **删除**并替换以下旧特判属性（迁移见 [07](07_IMPLEMENTATION_PLAN.md)）：
  `data-video-wallpaper` / `data-image-wallpaper`（改为 `data-backdrop="video|image|ambient|theme"`）、
  `data-cards-transparent` / `data-panel-transparent`（并入表面令牌，见 [04](04_SURFACE_TOKENS_CSS.md)）。

### 2.4 GPU 探测（可选，保守）

不引入重型 benchmark。用一次性 `OffscreenCanvas`/WebGL `getParameter` 读 renderer 字符串 +
一个 ≤16ms 的合成探针即可；探测失败一律回退 `acrylic`（宁可保守，不可误判成 full 导致卡顿）。
探测结果缓存到 `localStorage`（`rmccc.gpuTier.v1`），跨启动复用，不重复探测。

---

## 3. 响应式接口（SolidJS）

```ts
// src/render/tier.ts（签名草案，非最终代码）
export type RenderTier = "full" | "acrylic" | "static";
export type BackdropKind = "ambient" | "image" | "video" | "theme";

export function renderTier(): RenderTier;        // 响应式读取
export function backdropKind(): BackdropKind;    // 响应式读取
export function setBackdropKind(kind: BackdropKind): void;  // 背景子系统写入，触发重算
export function initRenderTier(): void;          // App 启动调用一次
export function onTierChange(fn: (t: RenderTier) => void): () => void;
```

**重算触发**：`backdropKind` 变化（用户切壁纸）、`userMotion`/`reducedMotion` 变化。
背景 kinds 由 [03](03_BACKGROUND_SUBSYSTEM.md) 的背景子系统在挂载/卸载时调用 `setBackdropKind`。

---

## 4. 合成守卫（`compositing.ts`，dev 期）

开发构建下提供一个可选的运行时断言器，把 [01](01_TARGET_ARCHITECTURE.md) §5 的硬约束变成"违规即告警"：

- 扫描新增全屏元素是否带 `mix-blend-mode`（`getComputedStyle`）→ 违规告警。
- 统计当前 `backdrop-filter` 元素数，超阈值告警。
- 静止页（无 video、无动画）若检测到常驻 rAF/interval → 告警。

生产构建 tree-shake 掉（`import.meta.env.DEV` 门控）。这是"防止性能回退"的护栏，不是运行时开销来源。

---

## 5. 与其它模块的契约

| 消费方 | 依赖本模块的什么 |
|---|---|
| [03 背景子系统](03_BACKGROUND_SUBSYSTEM.md) | 读 `renderTier()` 决定视频是否走原生/canvas 兜底；写 `setBackdropKind()` |
| [04 表面/令牌/CSS](04_SURFACE_TOKENS_CSS.md) | 读 `data-render-tier` 决定 `.surface-glass` vs `.surface-acrylic` vs `.surface-solid` |
| [05 自适应/动效](05_ADAPTIVE_AND_MOTION.md) | 读 `renderTier()`/`data-motion` 门控动画与取色频率 |
| [06 主题特效](06_THEME_EFFECTS.md) | 读 `renderTier()` 决定特效降级到静态/关闭 |

**单一事实源**：任何"要不要动、要不要真模糊"的判断都只能来自本模块，
组件与 CSS 不得再自行 `navigator.userAgent` 特判或叠加 `data-*`。

---

## 6. 设计目标验收

- [ ] `decideTier` 为纯函数，单测覆盖 §2.2 全部分支（含 reducedMotion 优先级）。
- [ ] 启动后 `<html>` 上只有 `data-render-tier` + `data-motion` + `data-backdrop` 三个渲染相关属性，旧特判属性全部移除。
- [ ] 静止 ambient 页：0 rAF、0 setInterval（用 dev 守卫断言）。
- [ ] GPU 探测失败时回退 `acrylic`，不出现 `full`。
