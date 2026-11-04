# 01 · 目标架构总纲

> 本文定义重构后的**顶层架构、分层模型、硬性合成约束与目录结构**。
> 各模块的详细设计见后续分册；实现顺序见 [`07_IMPLEMENTATION_PLAN.md`](07_IMPLEMENTATION_PLAN.md)。
> 问题基线见 [`00_CURRENT_AUDIT.md`](00_CURRENT_AUDIT.md)。

---

## 1. 一句话目标

**把"背景"拍平成一层、只模糊一次、可缓存的表面；让玻璃默认不再实时采样背景。**
在保留玻璃质感、主界面布局、动态/视频壁纸、动画/毛玻璃观感的前提下，
用一个**能力分级**的渲染引擎取代当前散落在 CSS/组件里的 ad-hoc 特效逻辑。

设计优先级（从高到低）：**低资源开销 > 双端兼容 > 观感清爽 > 代码简洁**。

---

## 2. 五条设计原则

| # | 原则 | 含义 | 针对的审计根因 |
|---|---|---|---|
| P1 | **背景单层化** | 任意时刻只有 **1 个**全屏背景层参与合成，且它自身是"可缓存"的（静态或原生合成） | §3.1 §3.3 |
| P2 | **玻璃去实时化** | 玻璃面默认用"亚克力染色"（半透明色+高光边+静态噪点），**不采样背景**；真 `backdrop-filter` 只留给静态背景上的有界小浮层 | §3.2 |
| P3 | **能力分级** | 启动时一次性决定 `RenderTier`，全前端只认这一个开关，不再散落 `data-*` 特判 | §4 平台差异 |
| P4 | **合成器友好动画** | 所有动画只用 `transform` / `opacity`；禁止动画 `filter`/`blur`/`background-position`/全屏 `mix-blend-mode` | §3.3 |
| P5 | **单一令牌源** | 颜色/圆角/阴影/间距收敛为一套 `--rmccc-*` + 一套装饰 `--scene-*`，删除全部 legacy 命名空间 | §3.5 |

---

## 3. 目标分层模型

```text
.app-root
├── [L0] BackdropLayer        唯一全屏背景层（三选一，互斥）
│         ├─ AmbientBackdrop  静态烘焙渐变（默认主题壁纸，零常驻动画）
│         ├─ VideoBackdrop    原生 <video>（canvas 仅黑屏兜底）
│         └─ ImageBackdrop    静态图片
├── [L1] DecorLayer           可选装饰（粒子/极光），受 RenderTier 降级；全屏层禁 blend
├── [L2] Shell                titlebar / dock / drag-strip（结构，不含全屏模糊）
└── [L3] Content              页面 + 表面原语卡片
          ├─ .surface-glass    真 backdrop-filter（仅 Tier=full 且背景静态时启用）
          ├─ .surface-acrylic  染色玻璃（默认，不采样背景）← 绝大多数卡片
          └─ .surface-solid    实色面（Tier=static / 减动效）
```

**关键不变量**：L0 是"活"的（视频）时，L3 一律走 `.surface-acrylic`/`.surface-solid`，
**绝不**启用 `.surface-glass` 的 backdrop-filter。这条规则由 RenderTier 统一裁决（见 §4）。

---

## 4. RenderTier 能力分级（架构核心）

启动时（一次）由 [`02_RENDER_TIER_ENGINE.md`](02_RENDER_TIER_ENGINE.md) 定义的引擎计算：

| Tier | 触发条件 | 背景 | 玻璃 | 装饰动画 |
|---|---|---|---|---|
| **full** | Windows/Chromium **且** 背景静态 **且** 未减动效 | 原生合成 | 允许真 `backdrop-filter`（少量浮层） | 允许 1 个 transform-only 层 |
| **acrylic**（默认） | macOS，**或** 任意端的视频壁纸 | 原生 `<video>`/静态 | 一律染色亚克力，不采样背景 | 静态装饰，无常驻动画 |
| **static** | `prefers-reduced-motion` / 低配探测 / 用户关动效 | 静态 | 实色面 | 全部关闭 |

Tier 通过 `<html data-render-tier="…">` 暴露，CSS 只按此一个属性分支。
**取代**当前的 `data-video-wallpaper` / `data-image-wallpaper` / `data-cards-transparent` /
`data-panel-transparent` / `data-motion` 等多套并行特判（保留 `data-motion` 作为 static 的输入之一）。

---

## 5. 硬性合成约束（写进 lint / review 清单）

1. **全屏层数量 ≤ 1 个带动画**；其余全屏层必须静态且可缓存。
2. **全屏层禁用 `mix-blend-mode`**（grain/粒子改为低透明静态贴图，或用 `opacity` 叠加）。
3. **`backdrop-filter` 只允许**：(a) 有界小面（对话框/菜单/popover），且 (b) 其背景在该 Tier 下是静态的。禁止全屏 `.app-body` 级 backdrop-filter。
4. **动画属性白名单**：`transform`、`opacity`。禁止动画 `filter`、`backdrop-filter`、`background-position`、`box-shadow`、`width/height/top/left`。
5. **`will-change` 预算**：全屏层最多 1 处，且仅在动画进行时挂载，静止时移除。
6. **常驻定时器/rAF 预算**：背景层静止时**零** rAF、零 setInterval（当前视频路径的 watchdog 需按 [03](03_BACKGROUND_SUBSYSTEM.md) 收敛）。

---

## 6. 目标目录结构

新增单一渲染引擎模块，特效逻辑从组件/CSS 收敛到这里：

```text
src/render/                      ← 新增：渲染引擎（唯一特效决策中心）
├── tier.ts                      RenderTier 探测 + 决策 + 订阅（P3）
├── compositing.ts               合成约束的运行时守卫（可选：dev 期断言）
├── surfaces.ts                  表面原语的令牌计算（glass/acrylic/solid 的变量）
├── motion.ts                    集中式动效令牌 + data-motion 门控（见 05）
└── index.ts                     对外唯一入口

src/backdrop/                    ← 新增：背景子系统（从 components 抽出）
├── BackdropLayer.tsx            L0 唯一全屏背景层（三选一互斥）
├── VideoBackdrop.tsx            原生 video + 黑屏兜底 canvas（见 03）
├── ImageBackdrop.tsx
├── AmbientBackdrop.tsx          静态烘焙氛围（见 03）
└──Decor/                        L1 装饰（粒子/极光，受 Tier 降级）

src/styles/                      ← 重组（见 04）
├── tokens.css                   唯一令牌源（--rmccc-* + --scene-*）
├── primitives.css               .surface-* / .glass-* 原语
├── layout.css                   app-shell / dock / titlebar 布局
├── components/                  按组件拆分的样式（取代 app.css 巨石）
└── (删除) ambient-stage.css 的动画与 blend 部分
```

**保留不动**：`src/pages/`、`src/stores/`、`src/ipc/`、`src/components/rm/`（结构组件）、
主题 JSON schema、插件契约。重构严格限定在**特效/渲染底层**。

---

## 7. 与现有模块的关系（映射表）

| 现有 | 去向 | 说明 |
|---|---|---|
| `components/BackgroundVideo.tsx`（461 行） | → `backdrop/VideoBackdrop.tsx` | 删逐帧 canvas 主路径，保留为兜底（[03](03_BACKGROUND_SUBSYSTEM.md)） |
| `components/AmbientStage.tsx` + `styles/ambient-stage.css` | → `backdrop/AmbientBackdrop.tsx` | 静态烘焙，删 blend/大模糊/粒子动画（[03](03_BACKGROUND_SUBSYSTEM.md)） |
| `components/ThemeSurface.tsx` | → `backdrop/` + `render/` | 主题背景并入 L0，主题特效并入 L1 并受 Tier 降级（[06](06_THEME_EFFECTS.md)） |
| `lib/glass.ts` | → `render/surfaces.ts` | 保留三个用户偏好（panel/card opacity、blur），删 `--soft-*` 双写（[04](04_SURFACE_TOKENS_CSS.md)） |
| `lib/adaptiveTheme.ts` + `adaptivePalette.ts` | → `render/` + Worker | 移出渲染路径（[05](05_ADAPTIVE_AND_MOTION.md)） |
| `theme/index.ts` 变量桥接 | 瘦身 | 删 `--surface-*`/`--text-*`/`--color-blue` legacy 桥（[04](04_SURFACE_TOKENS_CSS.md)） |
| `styles/app.css`（4024 行） | 拆分 | 令牌/原语/布局/组件四层（[04](04_SURFACE_TOKENS_CSS.md)） |

---

## 8. 预期收益（量化目标）

| 指标 | 现状 | 目标 |
|---|---|---|
| 视频壁纸空闲帧 GPU | 逐帧全屏上传，常驻满负荷 | 仅解码 + 一次合成；静止页零 rAF |
| 玻璃合成 pass | `O(卡片数 × 全屏模糊)` / 帧 | `O(1 背景层)`；卡片不参与背景重模糊 |
| 全屏混合层 | veil/particles/grain 多层 blend | 0 全屏 blend |
| CSS 变量命名空间 | 5 套 | 2 套（`--rmccc-*` + `--scene-*`） |
| `backdrop-filter` 处数 | 61 | 个位数（仅小浮层） |
| app.css | 4024 行巨石 | 拆分，去重后目标 < 40% 体积 |

**取舍**（详见 [07](07_IMPLEMENTATION_PLAN.md) §风险）：视频壁纸上卡片是"染色亚克力"而非像素级透出模糊——
肉眼接近，但非真 backdrop 采样；这是消除 §3.2 的根本代价。Windows 静态壁纸档（Tier=full）仍给真玻璃。
