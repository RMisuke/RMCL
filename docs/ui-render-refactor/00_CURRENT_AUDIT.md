# 00 · 现状审计（前端特效渲染层）

> 本文是重构方案的**问题基线**。所有数字由本机命令实测（`wc -l` / `grep -c` / 源码逐行阅读），
> 证据以 `文件:行` 形式给出，可直接跳转核对。审计对象：`src/` 前端特效与渲染底层，
> **不含**页面业务逻辑、IPC、Rust 后端。
> 基线 commit：`dcc357f`（2026-11-03）。上一轮空闲帧优化 `9f624f6` 已并入本基线。

---

## 1. 渲染层栈（自下而上）

来源 [`src/App.tsx:116-128`](../../src/App.tsx)：

```text
.app-root
├── ThemeBackdrop            全屏：AmbientStage 氛围层 或 主题视频/图片壁纸
│   └── ThemeEffects(app)    全屏：aurora / rotating-blocks / particles 三选若干
├── BackgroundVideo          全屏：用户视频壁纸（原生 <video> + 全屏 <canvas> 逐帧绘制）
├── BackgroundImage          全屏：用户静态图片壁纸
├── .window-drag-strip / TitleBar / AccountDock / LaunchDock
└── .app-body > .app-content 内容 + N 个 backdrop-filter 玻璃卡片
```

**同屏最多 3 个全屏背景层 + 1 组全屏主题特效层 + N 个玻璃面**，全部参与每一帧合成。
这是所有性能问题的结构性根源：**背景是"活"的（逐帧变化），玻璃又实时采样背景**。

---

## 2. 规模盘点

| 指标 | 实测值 | 度量方式 |
|---|---|---|
| CSS 总行数 | **7654** 行 / 6 文件 | `find src -name '*.css' \| xargs wc -l` |
| `app.css` 单文件 | **4024** 行 / 139 KB | 同上（巨石文件） |
| `home-stage.css` | 1333 行 / 56 KB | 同上 |
| `theme-core.css` | 850 行 / 33 KB | 同上 |
| `backdrop-filter` 出现 | **61** 处 / 6 文件 | `grep -c backdrop-filter src/**` |
| 动画类原语（`animation:`/`transition:`/`@keyframes`/`will-change`/`filter:blur`） | **203** 处 | `grep -Ec` 合并统计 |
| `@keyframes` 定义 | app.css 23、theme-skin 4、theme-core 3、home-stage 2、instance-settings 1 | `grep -c @keyframes` |
| 常驻 rAF / 视频帧回调 | BackgroundVideo 逐帧绘制 + 1s watchdog | 见 §3.1 |

---

## 3. 五个高开销根因

### 3.1 视频壁纸：每帧全屏 canvas blit（GPU 最大单点）

**现象**：为绕开 WKWebView"能解码却丢失原生 video 合成面（黑屏）"的怪癖，
组件把**每一帧解码结果 `drawImage` 到一张全屏 `<canvas>`**，canvas 尺寸 = `window × dpr(≤1.5)`，
节流 33ms（≈30fps）。于是同一段视频被：原生合成一次 + 逐帧缩放上传到 canvas 一次。

**证据**：[`src/components/BackgroundVideo.tsx`](../../src/components/BackgroundVideo.tsx)
- `drawVideoFrame()` 全屏 `ctx.drawImage(el, …)`：L99-121
- `drawFallback()` 常驻 rAF 循环：L125-134
- `watchFrames()` `requestVideoFrameCallback` 每帧回调：L170-186
- `recoverSurface()` 三级恢复（load→remount）：L193-233
- `checkPlayback()` 1s watchdog：L235-258、`setInterval(…,1000)` L363
- 整文件 **461 行**，其中约 300 行是"绘制循环 + 看门狗 + 恢复"。

**代价**：4K 视频 → 解码 → 逐帧缩放 → 全屏纹理上传，**只要视频壁纸开着就常驻满负荷**；
watchdog + 帧回调 + focus/visibility 恢复逻辑长期持有定时器与回调链。

**根因**：用"最贵的通用兜底路径（canvas 逐帧）"去同时服务 macOS 与 Windows，
而 Windows WebView2 的原生 `<video>` 合成本身就便宜、根本不需要 canvas。

---

### 3.2 玻璃直接盖在"活"背景上（合成 pass 随卡片数放大）

**现象**：卡片普遍使用 `backdrop-filter: blur(16–30px) saturate(120–130%)`，
而其下方是逐帧变化的视频 canvas / 动画氛围层。**背景每变一帧，每个玻璃面都要重跑一遍全屏级模糊**。

**证据**：[`src/styles/home-stage.css`](../../src/styles/home-stage.css)
- `.soft-card` blur(16px)：L166-167
- `.soft-launch-card` blur：L327-328
- `.account-dock` blur(26px)：L794-795
- popover blur(30px)：L892-893
- 壁纸激活时 `.app-body` 额外 blur(22px)：L1000-1001
- 壁纸激活时 titlebar/dock 降到 blur(14px)：L1012
- 已在壁纸激活时把 home-card blur **减半**：L1003-1008

**已有的"止血"证据**（说明团队已在与成本搏斗）：
- 默认卡片 blur 从 24px 降到 **16px**：[`src/lib/glass.ts:26-28`](../../src/lib/glass.ts)
- 壁纸激活时 blur 减半、dock 降到 14px（上方 CSS）

**代价**：合成成本 ≈ `O(玻璃面数 × 单面模糊 pass)`，且**每帧重复**，因为背景不缓存。

---

### 3.3 多层全屏 `mix-blend-mode` + 大半径模糊（无法缓存，强制回读）

**现象**：氛围层由多个全屏子层叠加，且大量使用混合模式与大半径 `filter: blur()`。

**证据**：[`src/styles/ambient-stage.css`](../../src/styles/ambient-stage.css)
- `.ambient-veil` `filter: blur(58px) saturate(118%)` 全屏：L46-51
- `.ambient-particles` `mix-blend-mode: screen` 全屏：L120-126
- `.ambient-grain` `mix-blend-mode: overlay` 全屏：L179-187
- `.ambient-veil i` 三个 `mix-blend-mode: screen` 圆：L52-76
- 大气变体再叠 blur(66px/70px)：L209-211

**已删动画的证据**（上一轮 `9f624f6` 的止血注释，仍在源码里）：
- L59-60："Animating a full-screen 58px blur forced the compositor to rebuild the entire backdrop each frame"
- L115-117：grid 背景位移动画已移除
- L136-137：粒子已改静态

**代价**：`mix-blend-mode` 令合成器**回读整层再混合**，破坏 GPU 层隔离与缓存；
即便当前是静态，多层全屏 blend 仍使氛围层无法作为单一缓存层复用。

---

### 3.4 自适应取色在渲染路径上（切壁纸时 CPU 尖峰）

**现象**：为从壁纸提取主题色，会**轮询播放中的视频抓 5 帧**，每帧画到 112² canvas
`getImageData` 再跑 k-means 聚类，全在主线程。

**证据**：[`src/lib/adaptiveTheme.ts`](../../src/lib/adaptiveTheme.ts)
- `liveVideoFrames()` 轮询播放中的视频 5 次 / ~2s：L298-312
- `drawRegion()` `getImageData` + 逐像素亮度扫描：L222-262
- `sampleBackground()` 图/视频/主题媒体多路采样：L442-486
- 聚类在 [`src/lib/adaptivePalette.ts`](../../src/lib/adaptivePalette.ts)（**22 KB**）
- 触发点：视频 `onLoadedData` → `scheduleAdaptiveSample(250)`（BackgroundVideo.tsx L434）

**代价**：切换壁纸/主题时主线程阻塞尖峰；视频路径下与解码竞争同一帧时序。

---

### 3.5 五套冗余变量命名空间（体积 + 维护 + 级联失效）

**现象**：同一批语义色被发布到 **5 套** CSS 变量命名空间，JS 每次换肤重复写入约 40 个 legacy 变量。

**证据**：
- [`src/lib/glass.ts:70-89`](../../src/lib/glass.ts)：同时写 `--rmccc-panel-alpha` **和** `--soft-frame-opacity`、`--rmccc-card-alpha` **和** `--soft-card-opacity`
- [`src/theme/index.ts:104-115`](../../src/theme/index.ts)：`--rmccc-*` 之外再桥接 `--surface-*` / `--text-*` / `--color-blue` / `--border-*`
- [`src/lib/adaptiveTheme.ts:143-183`](../../src/lib/adaptiveTheme.ts)：`tokenVars()` 一次性发布 `--rmccc-*` + `--scene-*` + `--surface-*` + `--text-*` + `--color-blue` + `--soft-*` 六组共约 40 个变量

**命名空间清单**：`--rmccc-*`（新语义）、`--scene-*`（装饰色）、`--adaptive-*`（取色中间态）、
`--soft-*`（旧 dashboard 遗留）、`--surface-*`/`--text-*`/`--color-blue`（更旧遗留）。

**代价**：CSS 体积膨胀、改一处需同步多套、级联覆盖顺序脆弱（`disableAdaptive` 需手动重贴 base 变量，
见 adaptiveTheme.ts L583-600 的注释"否则 shell 变成透明黑底白字"）。

---

## 4. 平台差异（决定重构必须分级）

| 能力 | Windows WebView2 (Chromium) | macOS WKWebView |
|---|---|---|
| 原生 `<video>` 合成 | 便宜、稳定 | 存在"丢合成面 → 黑屏"怪癖（§3.1 的由来） |
| `backdrop-filter` over 视频 | 可接受 | 昂贵 |
| 逐帧 canvas blit | 无必要 | 当前唯一兜底，代价最高 |

**结论**：不能用一条路径服务两端。必须引入**能力分级**（见 [`02_RENDER_TIER_ENGINE.md`](02_RENDER_TIER_ENGINE.md)），
按平台/GPU/壁纸类型在启动时决定渲染档位。

---

## 5. 问题 → 解决方向对照（速查）

| 根因 | 解决方向 | 详细设计 |
|---|---|---|
| §3.1 视频逐帧 canvas | 默认原生 `<video>` + CSS，canvas 降级为"探测到黑屏才启用"的兜底 | [03](03_BACKGROUND_SUBSYSTEM.md) |
| §3.2 玻璃实时采样活背景 | 卡片默认"亚克力染色"（不采样背景）；真 backdrop-filter 只留给静态背景上的小浮层 | [04](04_SURFACE_TOKENS_CSS.md) |
| §3.3 全屏 blend + 大模糊 | 氛围层烘焙成单层静态渐变 + 静态噪点贴图，全屏层禁用 blend | [03](03_BACKGROUND_SUBSYSTEM.md) |
| §3.4 取色在渲染路径 | 单帧降采样 → Worker/idle 聚类 → 缓存，不轮询视频 | [05](05_ADAPTIVE_AND_MOTION.md) |
| §3.5 五套变量 | 收敛为 `--rmccc-*` + `--scene-*` 两套，删除全部 legacy 桥接 | [04](04_SURFACE_TOKENS_CSS.md) |
| 无分级 | `src/render/` 引擎 + `RenderTier` 启动决策，全局唯一开关 | [02](02_RENDER_TIER_ENGINE.md) |

---

## 6. 审计边界（本次不动的东西）

- 页面/业务组件、路由、stores、IPC（135 条）契约：**不动**
- 主题 JSON schema（`schemas/theme.schema.json`）与插件 manifest 契约：**不动**（见 [06](06_THEME_EFFECTS.md)）
- skinview3d 皮肤查看器：仅沿用已有的 `IntersectionObserver` + `visibilitychange` 暂停，不重写
- Rust 后端、视频转码代理（后端已做 H.264 hardware-safe 转换）：**不动**
