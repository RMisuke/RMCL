# 历史快照索引（docs/archive/）

> 本目录只用于**问题溯源与设计由来**。这里每一份的「当前状态」结论都已过期，
> **现状一律以 [`PROJECT_STATUS.md`](../PROJECT_STATUS.md) 顶部轮次与 [`audits/`](../audits/) 台账为准**；
> 决策口径以 [`adr/`](../adr/) 为准。内链可能失效，按本索引的基线 commit 回查更可靠。

## 单篇快照

| 文件 | 基线 / 日期 | 内容 | 为什么冻结 |
|---|---|---|---|
| [ARCHITECTURE_IMPROVEMENT_PLAN.md](ARCHITECTURE_IMPROVEMENT_PLAN.md) | 2026-07-29 | 基于 HMCL v3.0 源码分析的对照改进方案 | 多数条目已由后续轮次实现或推翻 |
| [MODLOADER_IMPLEMENTATION_PLAN.md](MODLOADER_IMPLEMENTATION_PLAN.md) | 2026-07-30 | Modloader 安装的 HMCL 分析与 Rust 设计 | 落地形态改为 `crates/rmccc-modloader`（installer/patch + 7 loader），最终口径看 `ARCHITECTURE.md` |
| [ARCHITECTURE_AUDIT_REPORT.md](ARCHITECTURE_AUDIT_REPORT.md) | 2026-07-31 | 后端全量审计（10 crate + Tauri 层 + CLI 层） | 被同日 08-04 版取代，部分判断（如 response file 清理时机）被其推翻 |
| [BACKEND_ISSUES_INDEX.md](BACKEND_ISSUES_INDEX.md) | 07-31 审计索引 | 后端问题结构化清单 | **无状态列**，不能用来判断进度 |
| [ARCHITECTURE_AUDIT_REPORT_2026-08-04.md](ARCHITECTURE_AUDIT_REPORT_2026-08-04.md) | 2026-08-04 | 后端全量审计：28 IPC 命令、`cargo test --workspace` 实测 122 项全绿 | 5 个 P0 阻断点均已修复；命令数与测试规模严重过期（现 132 / 755） |
| [FIX_PLAN_2026-08-04.md](FIX_PLAN_2026-08-04.md) | 2026-08-04/05 | 53 项整改执行清单（P0×5 / P1×9 / P2×25 / P3×14） | 「✅ 已修复」多为文档自述，个别条目与当前代码不符 |
| [PLUGIN_SYSTEM_REMOVAL_ANALYSIS_2026-10-28.md](PLUGIN_SYSTEM_REMOVAL_ANALYSIS_2026-10-28.md) | 2026-10-28 | 插件系统价值与移除收益分析（方案 B：只留主题） | **建议被 [`ADR-022`](../adr/ADR-022-plugin-platform-retention.md) 否证**，不得作为删 crate 依据 |
| [TRIM_TO_THEMES_AND_CARDS_ANALYSIS_2026-10-28.md](TRIM_TO_THEMES_AND_CARDS_ANALYSIS_2026-10-28.md) | 2026-10-28 | 只留主题 + 卡片的体积/启动/内存收益符号级归因 | 「为性能裁剪不成立」的测算结论仍有效；收缩建议已作废（ADR-022） |

## 审计目录（2026-11-03 自 `docs/audits/` 归档）

| 目录 | 基线 | 内容 | 为什么冻结 |
|---|---|---|---|
| [BACKEND_AUDIT_2026-10-25/](BACKEND_AUDIT_2026-10-25/README.md) | commit `92d8743`（分支 `ui/home-hero-dock`） | 6 份：crate 层 / Tauri 层 / 前端 / HMCL 盘点 / 审计意见 / 总览 | 基线早于实例 v2、主题 v2–v3、任务中心、整合包安装、SNGL 便携数据根；仍开放的功能缺口以 [`audits/AUDIT_2026-10-20.md`](../audits/AUDIT_2026-10-20.md) 与 [`audits/AUDIT_2026-11-01_WINDOWS.md`](../audits/AUDIT_2026-11-01_WINDOWS.md) 为准 |
| [FRONTEND_AUDIT_2026-10-25/](FRONTEND_AUDIT_2026-10-25/README.md) | 分支 `ui/home-hero-dock` | 6 份：架构横切 / 页面组件 / HMCL 对照 / 问题清单 / 建议 | 51 项问题已由 10-26 复核，页面名与行号（`HomeDashboard.tsx`、`src/routes/`）已过期 |
| [FRONTEND_AUDIT_2026-10-26/](FRONTEND_AUDIT_2026-10-26/README.md) | main @ `3b863da` | 4 份：51 项复核 / 增量审计 / 整改计划 | 整改计划已被后续 UI 轮次覆盖；文中引用的 `docs/qa/JOURNEYS.md` 从未建立，走查口径改用 [`ui-design/ACCEPTANCE_CHECKLIST.md`](../ui-design/ACCEPTANCE_CHECKLIST.md) |

## 归档规则（与 `docs/README.md` 维护规则一致）

1. 点时性审计/计划完成后：仍有效的结论并入活文档或 `audits/` 台账，纯历史快照移入本目录。
2. 每份入档文档必须在标题下加 `⚠️ 历史快照` 横幅，写明**基线日期/commit**、**过期了什么**、**现状看哪里**。
3. 用 `git mv` 保留历史；入档后必须全仓 grep 旧路径并改指（代码注释里的文档路径也算引用）。
4. 本目录文档**不作为改代码的依据**；需要据此改代码时，先出新 ADR。
