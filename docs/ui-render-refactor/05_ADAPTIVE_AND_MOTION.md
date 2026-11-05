# 05 · 自适应取色 + 集中式动效系统

> 目标：消除审计根因 §3.4（取色在渲染路径上），并把散落在各 CSS/组件的动画
> 收敛成**一套集中式、合成器友好、受 RenderTier 门控**的动效系统（[01](01_TARGET_ARCHITECTURE.md) P4）。

---

## Part A · 自适应取色（Adaptive Palette）

### A.1 现状问题

| 问题 | 证据 | 后果 |
|---|---|---|
| 轮询播放中的视频抓 5 帧 | [`adaptiveTheme.ts:298-312`](../../src/lib/adaptiveTheme.ts) | ~2s 内反复读活视频 |
| 每帧 112² canvas `getImageData` + 逐像素亮度扫描 | 同上 L222-262 | 主线程阻塞 |
| k-means 聚类在主线程 | [`adaptivePalette.ts`](../../src/lib/adaptivePalette.ts)（22 KB） | 切壁纸时 CPU 尖峰 |
| 触发点绑在视频 `onLoadedData` | [`BackgroundVideo.tsx:434`](../../src/components/BackgroundVideo.tsx) | 与解码争时序 |
| 清除时需手动重贴 base 变量（脆弱） | adaptiveTheme.ts L583-600 | 双命名空间后遗症 |

### A.2 设计思路：单帧 + 离线 + 缓存

1. **单帧降采样**：不再轮询 5 帧。取**一帧**（视频用一次 `requestVideoFrameCallback`，
   图片用 `naturalWidth` 就绪后一次），降采样到小尺寸（如 64²）再读。
   视频路径**不再**创建第二个隐藏 `<video>` 克隆去 seek（当前 `sampleCorsVideo` L402-440 的克隆路径删除或仅留极端兜底）。
2. **移出主线程**：`getImageData` 后的聚类计算放进 **Web Worker**（`adaptivePalette` 的纯计算部分），
   主线程只做"抓帧 → 传 ImageBitmap/像素 → 收结果"。Worker 不可用时回退 `requestIdleCallback`。
3. **不绑渲染路径**：触发点从视频 `onLoadedData` 改为"背景层稳定后一次"，
   由 [03 背景子系统](03_BACKGROUND_SUBSYSTEM.md) 在 `setBackdropKind` 落定后调度，且 `scheduleAdaptiveSample` 去抖。
4. **缓存**：结果按 `source + path/mode` 缓存到 `localStorage`（现有 `PALETTE_KEY` 保留），
   同一壁纸不重复取色。
5. **单命名空间**：取色结果只写 `--rmccc-*`（覆盖主题值）+ `--scene-*`，
   `--adaptive-*` 降为 Worker 内部中间态（[04](04_SURFACE_TOKENS_CSS.md) §3.1）。
   清除自适应 = 移除覆盖，主题令牌自然复位，**删除** L583-600 的手动重贴逻辑。

### A.3 数据流

```text
背景稳定 → 抓 1 帧（64²）→ transferToImageBitmap / 像素数组
        → postMessage 到 Worker → k-means + 角色选择 + 对比度校验
        → 回传 palette → 主线程写 --rmccc-* / --scene-* → 缓存 localStorage
```

### A.4 降级

- `RenderTier == "static"` 或用户关自适应：跳过取色，直接用主题令牌。
- Worker 不可用：`requestIdleCallback` 兜底；再不可用则 `setTimeout(0)` 分片，绝不阻塞首帧。
- 跨域/解码失败：回退 `paletteFromThemeAccent`（现有 L504-507 保留）。

### A.5 验收

- [ ] 切换壁纸时主线程无 > 16ms 的取色阻塞（聚类在 Worker）。
- [ ] 视频路径不再轮询 5 帧、不再创建隐藏克隆 video（极端兜底除外，需注释说明）。
- [ ] 同一壁纸二次进入不重新取色（命中缓存）。
- [ ] 关闭自适应后 shell 不复现"透明黑底白字"（单命名空间自然复位）。

---

## Part B · 集中式动效系统（`src/render/motion.ts`）

### B.1 现状问题

- 动画散落：`@keyframes` 分布在 app.css(23)、theme-skin(4)、theme-core(3)、home-stage(2)、instance-settings(1)。
- 门控分散：`data-motion="off"` 与 `prefers-reduced-motion` 在每个文件各写一遍
  （ambient-stage.css L236-248、theme-skin.css L276-287、theme-core.css L758-774）。
- 存在被动画的**非合成器属性**历史（上一轮已删 grid 位移、veil blur、粒子动画——
  ambient-stage.css L59-60/L115-117/L136-137 的注释即证据），说明缺乏统一约束。

### B.2 设计思路：一套令牌 + 一个门控 + 属性白名单

1. **动效令牌**（写进 `tokens.css`，[04](04_SURFACE_TOKENS_CSS.md) §3.2）：
   ```text
   --rmccc-duration-{instant,fast,base,slow}   例：80/140/220/320ms
   --rmccc-ease-{out,in,out-soft}              现有 --rmccc-ease-out 已存在，收敛为固定几条
   ```
   所有 `transition`/`animation` 只引用令牌，禁止裸写 `ms`/`cubic-bezier`。
2. **单一门控**：`data-motion`（由 [02 RenderTier](02_RENDER_TIER_ENGINE.md) 输出）+ `prefers-reduced-motion`
   在 `motion.ts` 统一处理，CSS 只在**一处**（primitives.css 顶部）写全局降级规则，
   各组件文件**不再**各写一遍 `@media (prefers-reduced-motion)`。
3. **属性白名单**（[01](01_TARGET_ARCHITECTURE.md) §5.4）：动画只允许 `transform` / `opacity`。
   入场动画（page-in / dialog-in / menu-in，theme-core.css L758-762）保留但确保只用这两属性 + `will-change` 预算内。
4. **禁动画清单**：`filter` / `backdrop-filter` / `background-position` / `box-shadow` / 几何属性。
   dev 期由 [02 compositing 守卫](02_RENDER_TIER_ENGINE.md) §4 扫描告警。

### B.3 各 Tier 的动效预算

| Tier | 入场过渡 | 常驻动画 | 装饰漂移 |
|---|---|---|---|
| full | 允许（transform/opacity） | ≤ 1 个全屏 transform-only 层 | 允许 1 个 |
| acrylic | 允许（更短 duration） | 0 常驻 | 静态 |
| static | 关闭（或仅 opacity 极短） | 0 | 关闭 |

### B.4 保留的动效（观感不降级）

- 页面/对话框/菜单入场（现有 noctis-page-in / dialog-in / menu-in）——保留，统一走令牌。
- 按钮/行 hover 过渡——保留（color/border/background 过渡属廉价属性，白名单外但成本极低，
  可豁免；**禁**的是 hover 触发 `transform`/`box-shadow` 动画导致重排，theme-core.css L767-769 已把 hover transform 设为 none，保留该结论）。
- 任务进度条动画——保留（transform 驱动）。

### B.5 验收

- [ ] 全仓 `@keyframes` 集中管理，组件 CSS 不再各自定义重复动画。
- [ ] `prefers-reduced-motion` / `data-motion` 降级规则只在一处声明。
- [ ] 无动画使用白名单外属性（dev 守卫 0 告警）。
- [ ] static 档下全前端 0 常驻动画、0 rAF。

---

## 6. 与其它模块关系

| 模块 | 关系 |
|---|---|
| [02 RenderTier](02_RENDER_TIER_ENGINE.md) | 提供 `data-motion` / tier，本模块据此门控 |
| [03 背景](03_BACKGROUND_SUBSYSTEM.md) | 背景稳定后调度取色；氛围粒子动画受本模块门控 |
| [04 令牌/CSS](04_SURFACE_TOKENS_CSS.md) | 动效令牌写入 tokens.css；取色结果写 `--rmccc-*`/`--scene-*` |
| [06 主题特效](06_THEME_EFFECTS.md) | 特效动画受本模块 tier/motion 门控 |
