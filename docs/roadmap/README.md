# RMCCC 细化执行计划（roadmap/）

> **本目录的定位**：把 [`RMCCC_PLUGIN_PLATFORM_ARCHITECTURE_GUIDE.md`](../RMCCC_PLUGIN_PLATFORM_ARCHITECTURE_GUIDE.md) 的目标架构（Phase 0–7）与 [`AUDIT_2026-10-20.md`](../AUDIT_2026-10-20.md) 的当前事实逐条对接，为**当前 macOS-only 开发阶段**产出可执行、可验收、与代码现状一致的细化计划。
>
> **本目录与其它文档的关系**：
>
> | 文档 | 角色 | 本目录关系 |
> |------|------|-----------|
> | `AGENTS.md` | 顶层协作规范 | 本目录受其约束（设计文档优先、Phase 顺序、体积红线、编码规范） |
> | `ARCHITECTURE.md` | 设计基线（最高权威） | 本目录计划不得与之冲突；冲突时先提 ADR |
> | `RMCCC_PLUGIN_PLATFORM_ARCHITECTURE_GUIDE.md` | 目标架构 / 插件平台方向 | 本目录是该指南 Phase 0–7 的细化执行版，服从其 §1.4 与 §5.7 |
> | `AUDIT_2026-10-20.md` | 横切全景 + 技术债台账（P0–P3） | 本目录每条任务都标注对应的审计条目（P1-x / P2-x / §x） |
> | `PROJECT_STATUS.md` | 轮次变更记录 | 本目录的"当前状态"以审计快照为准；完成后回写 PROJECT_STATUS |
> | `DEVELOPMENT_PLAN.md` | Phase 1–5 路线图 | 本目录是其向后延伸（插件平台 + Agent + 发布） |
| [`docs/adr/`](../adr/) | 架构决策记录（ADR） | 本目录 Phase 0 产出 ADR-001/011/012；Phase 2 产出 ADR-003/004 |

## 阅读顺序

1. [`00_ROADMAP_OVERVIEW.md`](00_ROADMAP_OVERVIEW.md) — 总体分段、依赖、里程碑、当前状态锚点。
2. 当前阶段的细化计划（按编号）。
3. [`PLATFORM_ADAPTATION.md`](PLATFORM_ADAPTATION.md) — 平台适配层能力域与 `#[cfg]` 迁移映射（跨阶段引用）。

## 当前执行约束（来自指南 §1.4）

- 本机仅 macOS Apple Silicon 开发；**不在本机做跨平台交叉编译，不做 Windows/Linux 实测**。
- 架构须保留全平台能力：平台差异集中在适配层，领域核心只定义 trait/DTO，不内联 `#[cfg]` 执行。
- 非 macOS 的"必须验证"项一律记 deferred / release-blocked，交付报告不得填 pass。

## 计划文档清单

| 文档 | 对应指南阶段 | 主要解决的审计欠账 |
|------|------------|--------------------|
| [00_ROADMAP_OVERVIEW.md](00_ROADMAP_OVERVIEW.md) | 总览 | — |
| [01_phase0_baseline.md](01_phase0_baseline.md) ✅ | Phase 0 | §15.5 环境回归、P2-7 strip、体积基线缺口 |
| [02_phase0.5_product_gaps.md](02_phase0.5_product_gaps.md) ✅ | 既有产品缺口补齐 | P1-1 Mod 浏览器、P1-2 多账号 UI、P1-5 本地 mod 安装、P2-6 |
| [03_phase1_core_convergence.md](03_phase1_core_convergence.md) ✅ | Phase 1 | P1-3 pipeline 绕过、P1-4 loader 双体系、P2-1 context.rs、P2-2 IPC 测试、P2-4 zip-slip、P2-8/P2-9/P2-10/P2-11、ADR-002 |
| [04_phase2_plugin_protocol.md](04_phase2_plugin_protocol.md) ✅ | Phase 2 | ADR-003/004、Manifest/权限/生命周期契约 |
| [05_phase3_dashboard.md](05_phase3_dashboard.md) | Phase 3 | ADR-005、Widget/布局/主题 |
| [06_phase4_isolated_ui_sdk.md](06_phase4_isolated_ui_sdk.md) | Phase 4 | ADR-006、隔离 UI + SDK |
| [07_phase5_logic_runtime.md](07_phase5_logic_runtime.md) | Phase 5 | ADR-007、运行时 PoC |
| [08_phase6_agent.md](08_phase6_agent.md) | Phase 6 | ADR-009/010、Agent 工具管线 |
| [09_phase7_distribution.md](09_phase7_distribution.md) | Phase 7 | ADR-008/011/012、签名/公证/发布 |
| [PLATFORM_ADAPTATION.md](PLATFORM_ADAPTATION.md) | 跨阶段（指南 §5.7 细化） | 平台执行散落在领域 crate 的迁移 |

## 计划文档统一结构

每个阶段文档包含：目标 / 前置条件 / 与当前状态的对应（引用审计条目）/ 任务切片分解 / 验收标准 / 验证命令 / 平台策略（macOS 本机 vs deferred）/ 风险与回滚 / 文档同步要求。

## 维护约定

- 计划是**活文档**：阶段推进时更新"状态"标记（pending / in_progress / done / blocked）。
- 每完成一个切片，把对应审计条目（P1-x / P2-x）在 `AUDIT` 的台账中销账、在 `PROJECT_STATUS` 回填一轮记录。
- 不直接把目标架构当成已实现；"当前状态"以源码与实测为准。
