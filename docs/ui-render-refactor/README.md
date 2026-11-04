# 前端特效渲染层重构方案（ui-render-refactor）

> 本目录是**前端特效/渲染底层的完整重构设计**，为后续开发提供路线与目标。
> 触发原因：主界面玻璃质感渲染导致高 CPU/GPU 占用、主题动效资源占用、动态视频壁纸高占用。
> 重构前提：**高性能低开销 + macOS/Windows 双端兼容**；保留玻璃质感、主界面布局、
> 动态/视频背景主题、动画/毛玻璃观感；从零重设底层架构、删除冗余配置。
>
> 基线 commit `dcc357f`（2026-11-03）。所有审计数字由本机命令实测，证据以 `文件:行` 给出。
> **本方案不含代码改动**——是设计文档，供照此开发。

---

## 阅读顺序

| 序 | 文档 | 内容 |
|---|---|---|
| 0 | [`00_CURRENT_AUDIT.md`](00_CURRENT_AUDIT.md) | 现状审计：渲染层栈、规模盘点、**五个高开销根因**（带 `文件:行` 证据）、平台差异 |
| 1 | [`01_TARGET_ARCHITECTURE.md`](01_TARGET_ARCHITECTURE.md) | 目标架构总纲：五条设计原则、分层模型、**RenderTier 能力分级**、硬性合成约束、目标目录结构、收益量化 |
| 2 | [`02_RENDER_TIER_ENGINE.md`](02_RENDER_TIER_ENGINE.md) | 渲染引擎 `src/render/`：决策中心、`RenderTier` 决策表、响应式接口、dev 合成守卫 |
| 3 | [`03_BACKGROUND_SUBSYSTEM.md`](03_BACKGROUND_SUBSYSTEM.md) | 背景子系统 `src/backdrop/`：唯一 L0 背景层、视频原生主路径 + 黑屏兜底、氛围静态烘焙 |
| 4 | [`04_SURFACE_TOKENS_CSS.md`](04_SURFACE_TOKENS_CSS.md) | 表面原语（glass/acrylic/solid）+ 统一令牌（5 套→2 套）+ `app.css` 4024 行巨石拆分 |
| 5 | [`05_ADAPTIVE_AND_MOTION.md`](05_ADAPTIVE_AND_MOTION.md) | 自适应取色移出渲染路径（单帧+Worker+缓存）+ 集中式动效系统 |
| 6 | [`06_THEME_EFFECTS.md`](06_THEME_EFFECTS.md) | 主题特效降级矩阵 + 主题配置精简（**JSON 契约不变**） |
| 7 | [`07_IMPLEMENTATION_PLAN.md`](07_IMPLEMENTATION_PLAN.md) | 模块依赖图、实现顺序（S1–S8）、分阶段路线（Phase A–D）、门禁、风险与取舍、影响清单、起手建议 |

---

## 一句话方案

**把"背景"拍平成一层、只模糊一次、可缓存的表面；让玻璃默认不再实时采样背景。**
用一个**能力分级**的渲染引擎（`RenderTier`：full / acrylic / static）取代散落在 CSS/组件里的 ad-hoc 特效逻辑。

## 核心收益（详见 [01](01_TARGET_ARCHITECTURE.md) §8、[07](07_IMPLEMENTATION_PLAN.md) §6）

| 维度 | 现状 → 目标 |
|---|---|
| 视频壁纸 GPU | 逐帧全屏上传（常驻满负荷）→ 解码 + 一次合成 |
| 玻璃合成 | `O(卡片数 × 全屏模糊)/帧` → `O(1 背景层)` |
| 全屏混合层 | veil/particles/grain 多层 blend → 0 |
| CSS 变量命名空间 | 5 套 → 2 套 |
| `backdrop-filter` | 61 处 → 个位数 |
| `app.css` | 4024 行巨石 → 拆分，< 40% 体积 |

## 起手建议

先做 **Phase A 的 S1 + S3**（RenderTier 引擎 + 视频背景主路径 + 氛围静态化）——
**见效最快、风险最低**，单独交付即可消除审计 §3.1（GPU 最大单点）与 §3.3（全屏 blend），
且不触碰主题契约与业务组件。详见 [07](07_IMPLEMENTATION_PLAN.md) §8。

## 边界（本次不动）

页面/业务组件、路由、stores、IPC（132 条）契约、主题 JSON schema、插件 manifest、
Rust 后端、视频转码代理、skinview3d——**全部不动**。重构严格限定在特效/渲染底层。

## 与仓库其它文档的关系

- 遵循 [`../AGENTS.md`](../AGENTS.md)（设计文档优先、本地只构建 `.app`、体积红线、macOS-only 约束）
  与 [`../AI_EDITING_RULES.md`](../AI_EDITING_RULES.md)（门禁/提交/§3.5 产物编译期内嵌）。
- 前端视觉走查用 [`../ui-design/ACCEPTANCE_CHECKLIST.md`](../ui-design/ACCEPTANCE_CHECKLIST.md)。
- 插件平台保留结论见 [`../adr/ADR-022-plugin-platform-retention.md`](../adr/ADR-022-plugin-platform-retention.md)（主题特效降级不收缩插件平台）。
- 完成后按 [`../README.md`](../README.md) 维护规则，把轮次记录写入 [`../PROJECT_STATUS.md`](../PROJECT_STATUS.md)。
