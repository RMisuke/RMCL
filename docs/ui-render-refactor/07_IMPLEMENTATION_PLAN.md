# 07 · 实现计划：模块关系、顺序、阶段、门禁与风险

> 本文把 [00](00_CURRENT_AUDIT.md)–[06](06_THEME_EFFECTS.md) 的设计落成**可执行的开发路线**：
> 模块依赖、实现顺序、分阶段验收、门禁、风险与取舍。供后续开发直接照此推进。

---

## 1. 模块依赖图

```text
                 ┌──────────────────────────┐
                 │  02 RenderTier 引擎        │  ← 决策中心，最先落地
                 │  src/render/tier.ts        │
                 └────────────┬─────────────┘
            data-render-tier / data-motion / setBackdropKind
          ┌───────────────┬───┴────────┬────────────────┐
          ▼               ▼            ▼                ▼
 ┌────────────────┐ ┌───────────┐ ┌──────────────┐ ┌──────────────┐
 │ 03 背景子系统    │ │ 04 表面/   │ │ 05 自适应+   │ │ 06 主题特效   │
 │ src/backdrop/  │ │ 令牌/CSS   │ │ 动效         │ │ (并入 03/05) │
 │ L0 唯一背景层   │ │ 原语+拆app │ │ Worker取色   │ │ 降级矩阵     │
 └───────┬────────┘ └─────┬─────┘ └──────┬───────┘ └──────┬───────┘
         │  setBackdropKind │  .surface-*   │ 写 --rmccc-*   │ L1 decor
         └────────► 02 ◄────┴───────► 04 ◄──┘               │
                                                            ▼
                                                     03 DecorLayer
```

**读法**：02 是根，先落地并广播档位；03/04/05/06 都消费 02 的输出。
03 与 02 双向（背景 kind 反过来触发 tier 重算）。04 依赖 02 的 tier 决定表面原语。
05 取色结果写 04 的令牌。06 的背景并入 03、特效并入 03 的 DecorLayer 并受 05 门控。

---

## 2. 实现顺序（拓扑排序）

| 序 | 模块 | 为什么这个位置 | 可独立交付 |
|---|---|---|---|
| **S1** | 02 RenderTier 引擎 | 所有模块的决策根；先有档位开关，后续才能按档分支 | ✅ 可先只广播属性，暂不改视觉 |
| **S2** | 04 令牌收敛（先删 legacy 变量） | 令牌是 CSS/表面的地基；先统一命名空间，避免后续返工 | ✅ 视觉不变，纯内部收敛 |
| **S3** | 03 背景子系统（视频主路径 + 氛围静态化） | **最大性能收益**（§3.1/§3.3）；依赖 S1 的 tier、S2 的 scene 令牌 | ✅ 见效最快 |
| **S4** | 04 表面原语（glass/acrylic/solid）+ CSS 拆分 | 依赖 S1 tier + S3 背景 kind；消除 §3.2 | ✅ |
| **S5** | 05 动效系统集中化 | 依赖 S1 门控；收编散落动画 | ✅ |
| **S6** | 05 自适应取色移 Worker | 依赖 S3 背景稳定信号 + S2 单命名空间 | ✅ |
| **S7** | 06 主题特效降级 + 主题背景并入 L0 | 依赖 S3（背景路径）+ S5（门控）；契约不变最后做 | ✅ |
| **S8** | CSS 巨石拆分收尾 + 死代码清理 + 守卫 | 全部模块稳定后做结构性拆文件，风险最低 | ✅ |

> **建议起手**：S1 + S3（视频主路径）——**见效最快、风险最低**，
> 单独交付即可消除审计 §3.1 的 GPU 最大单点，且不触碰主题契约与业务组件。

---

## 3. 分阶段路线与验收

### Phase A（S1–S3）：引擎 + 背景，拿掉最大 GPU 开销

**交付**：`src/render/tier.ts` 广播 `data-render-tier`；`src/backdrop/` 唯一背景层；
视频默认原生 `<video>`，canvas 降为黑屏兜底；氛围层静态烘焙（删 veil blur / 全屏 blend）。

**验收**：
- [ ] 静止 ambient 页：0 rAF、0 setInterval（dev 守卫断言，[02](02_RENDER_TIER_ENGINE.md) §4）。
- [ ] 视频壁纸原生面正常时：canvas 兜底未激活；人为黑屏时兜底自动接管。
- [ ] 氛围层：0 全屏 `mix-blend-mode`、0 `filter:blur` 动画。
- [ ] 三背景互斥，DOM 中仅 1 个全屏背景层。
- [ ] 三门禁绿（`pnpm typecheck` / `pnpm test` / `pnpm build`）。
- [ ] **macOS 实机视觉验收**：`pnpm tauri build --bundles app` 后在 `.app` 确认氛围观感 + 壁纸首帧/恢复不黑屏
      （遵守 [`AI_EDITING_RULES.md`](../AI_EDITING_RULES.md) §3.5：产物编译期内嵌，`pnpm build` 不影响已构建 `.app`）。

### Phase B（S4）：表面原语 + 令牌落地，拿掉每帧重模糊

**交付**：`.surface-{glass,acrylic,solid}` 三原语；卡片改挂 `.surface.surface-auto`；
删除全屏 `.app-body` backdrop-filter；`--soft-*`/`--surface-*`/`--text-*`/`--color-blue` 全删。

**验收**：
- [ ] `backdrop-filter` 从 61 处降到个位数，全在 full 档小浮层。
- [ ] `grep` 全仓 legacy 命名空间 0 命中（除文档）。
- [ ] 卡片档位随 `data-render-tier` 联动；视频壁纸下卡片为 acrylic（不采样背景）。
- [ ] Settings 三个玻璃偏好仍可调且生效。
- [ ] 三门禁绿 + macOS 实机视觉验收。

### Phase C（S5–S6）：动效集中 + 取色离线

**交付**：`render/motion.ts` 单门控 + 动效令牌；取色改单帧 + Worker + 缓存。

**验收**：
- [ ] `prefers-reduced-motion`/`data-motion` 降级只在一处声明。
- [ ] 无动画使用白名单外属性（守卫 0 告警）。
- [ ] 切壁纸主线程无 > 16ms 取色阻塞；同壁纸二次进入命中缓存。
- [ ] 关闭自适应不复现"透明黑底白字"。
- [ ] 三门禁绿。

### Phase D（S7–S8）：主题特效降级 + CSS 拆分收尾

**交付**：主题背景并入 L0、特效并入 L1 并降级；`app.css` 拆散删除；死代码清理。

**验收**：
- [ ] 主题 JSON schema / manifest **零改动**；4 个示例插件仍可加载。
- [ ] acrylic 档 rotating-blocks 不渲染 3D 立方体。
- [ ] `app.css` 已删除；单 CSS 文件 < 400 行；CSS 总体积 < 当前 40%。
- [ ] 三门禁绿 + macOS 实机视觉验收 + 主题相关单测通过。

---

## 4. 门禁（每阶段必过，沿用 AGENTS.md / AI_EDITING_RULES.md）

前端：`pnpm typecheck`、`pnpm test`、`pnpm build`。
本机视觉验收：`pnpm tauri build --bundles app` → 在 `target/release/bundle/macos/SNGL.app` 内确认观感
（**不生成 DMG**，遵守 AGENTS.md 本地构建约束）。
Rust 三门禁（fmt/clippy/test）：本重构**零 Rust 改动**时可免跑；若触及（如背景 prepare IPC）则必跑。
体积红线：主可执行 < 20 MiB（strip 后）；前端重构不应显著影响，但 `pnpm build` 后核对 chunk 体积（skinview3d 515 kB 既有告警非本次引入）。

---

## 5. 风险与取舍

| 风险/取舍 | 说明 | 缓解 |
|---|---|---|
| **亚克力非真透出模糊** | 视频壁纸上卡片是"染色"而非像素级 backdrop 采样，肉眼接近但非等价 | Windows 静态壁纸档（full，且需独显）仍给真玻璃；用户可通过 cardBlur 偏好在 full 档调节 |
| **黑屏兜底探测误判** | WKWebView 黑屏探针若误判，可能该走兜底却没走 | 探针保守：不确定时**倾向启用兜底**（宁可多一次 canvas，不可黑屏）；保留现有 recover 逻辑 |
| **删 legacy 变量破坏历史 skin CSS** | 若有第三方手写 CSS 引用 `--soft-*` | 主题契约本就不含 CSS（themes never provide CSS）；影响面仅限历史手写 skin，属可接受清理 |
| **CSS 巨石拆分回归** | 4024 行拆散可能漏样式 | 放最后阶段（S8）；拆分前后 `pnpm build` 产物比对 + macOS 实机逐页走查（[`ui-design/ACCEPTANCE_CHECKLIST.md`](../ui-design/ACCEPTANCE_CHECKLIST.md)） |
| **Worker 取色兼容性** | 个别 WebView Worker/ImageBitmap 支持差异 | 三级回退：Worker → requestIdleCallback → setTimeout 分片 |
| **视觉验收未做即宣称完成** | 上一轮 `9f624f6` 就遗留"动画移除/blur 降级未实机验收" | 每阶段强制 macOS `.app` 实机走查，验收项写进 PROJECT_STATUS 轮次记录 |
| **Windows 实测缺失** | 本机 macOS-only（AGENTS.md §1.4 约束），full 档无法本机验证 | Windows 相关项记 **deferred / release-blocked**，不得宣称已验收 |

---

## 6. 重构后的优势（总结）

1. **GPU**：视频壁纸从"逐帧全屏上传"降到"解码 + 一次合成"；玻璃合成从 `O(卡片数 × 全屏模糊)/帧` 降到 `O(1 背景层)`。
2. **CPU**：取色移出主线程；静止页 0 rAF / 0 interval。
3. **可维护**：5 套变量 → 2 套；61 处 backdrop-filter → 个位数原语；4024 行巨石 → 分层 < 400 行/文件。
4. **防回退**：RenderTier 单一决策 + dev 合成守卫，性能约束变成"违规即告警"，不再靠人肉记忆。
5. **双端兼容**：能力分级让 Windows 走便宜的原生合成 + 真玻璃，macOS 走保守的 acrylic，不再用一条最贵路径兜两端。
6. **观感**：统一令牌 + 三表面原语，去掉过度叠加的 veil/grain/particle，界面更清爽。
7. **契约稳定**：主题 JSON schema、插件 manifest、IPC、业务组件、路由、stores 全部不动——重构严格限定在特效/渲染底层。

---

## 7. 对现有代码的影响清单

| 文件/模块 | 影响 | 阶段 |
|---|---|---|
| `src/render/`（新增） | 新建引擎（tier/surfaces/motion/compositing） | A–C |
| `src/backdrop/`（新增） | 背景子系统，从 components 抽出 | A |
| `components/BackgroundVideo.tsx`（461 行） | 删逐帧 canvas 主路径，保留为兜底分支 | A |
| `components/AmbientStage.tsx` + `styles/ambient-stage.css` | 静态烘焙，删 blend/大模糊/粒子动画 | A |
| `components/ThemeSurface.tsx` | 背景并入 L0，特效并入 L1 降级 | D |
| `lib/glass.ts` | → `render/surfaces.ts`，删 `--soft-*` 双写 | B |
| `theme/index.ts` | 删 legacy 变量桥接（L104-115） | B |
| `lib/adaptiveTheme.ts` + `adaptivePalette.ts` | 单帧 + Worker + 缓存，删轮询/克隆 | C |
| `styles/app.css`（4024 行） | 拆散删除 | D |
| `styles/home-stage.css` / `theme-core.css` / `theme-skin.css` | 玻璃样式收敛到原语；门控收敛到 motion.ts | B–D |
| **不动** | `pages/` `stores/` `ipc/` `components/rm/` `schemas/` 插件契约 Rust 后端 skinview3d | — |

---

## 8. 起手建议

先做 **Phase A 的 S1 + S3**（RenderTier 引擎 + 视频背景主路径 + 氛围静态化）：
- 见效最快：直接消除审计 §3.1（GPU 最大单点）与 §3.3（全屏 blend）。
- 风险最低：不触碰主题契约、业务组件、CSS 巨石拆分。
- 可独立交付验收：静止页 0 rAF + 视频原生合成 + 氛围静态，即可在 macOS `.app` 实测确认。

确认本路线后，可据此拆成逐模块的可执行任务清单（每模块一个开发批次 + 门禁 + 实机验收）。
