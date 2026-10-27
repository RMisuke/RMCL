# RMCCC 文档索引

> 本目录的分类导航。顶层协作规范见 [`../AGENTS.md`](../AGENTS.md)（设计文档优先、
> Phase 顺序、体积红线、macOS-only 约束）；文档间冲突时以 `ARCHITECTURE.md` 为最高权威。

## 按用途快速导航

| 我想… | 去读 |
|---|---|
| 理解整体架构（最高权威） | [`ARCHITECTURE.md`](ARCHITECTURE.md)（含 §11 IPC 表 84 条、§25 AI/MCP） |
| 了解 AI / MCP 方向（ADR-013） | [`AI_AND_MCP_ARCHITECTURE.md`](AI_AND_MCP_ARCHITECTURE.md)（S1–S7 切片与进度表） |
| 确认开发进度 / 接手开发 | [`DEVELOPMENT_PLAN.md`](DEVELOPMENT_PLAN.md)（Phase 1–5）→ [`roadmap/`](roadmap/)（Phase 0–7 细化）→ [`PROJECT_STATUS.md`](PROJECT_STATUS.md)（轮次记录） |
| 做前端重构 | [`ui-design/`](ui-design/)（15 份：风格 / 各页面 / IPC 覆盖 / 注意事项） |
| 写插件 | [`guides/PLUGIN_AUTHORING.md`](guides/PLUGIN_AUTHORING.md) → `plugin-sdk/` → [`adr/`](adr/)（ADR-003/004/006/007） |
| 做一套主题 | [`guides/THEME_PLUGIN_AUTHORING.md`](guides/THEME_PLUGIN_AUTHORING.md)（含 `schemas/theme.schema.json`） |
| 部署与运维 | [`guides/OPERATIONS.md`](guides/OPERATIONS.md) |
| 查技术债 / 审计结论 | [`audits/`](audits/)（10-20 横切全景 + 10-23 增量台账） |
| 查历史决策由来 | [`adr/`](adr/)（决策记录）与 [`archive/`](archive/)（历史快照） |

## 目录结构

| 位置 | 内容 | 时效性 |
|---|---|---|
| `ARCHITECTURE.md` | 完整架构设计（最高权威，2026-10-23 同步 ADR-013） | 活文档 |
| `AI_AND_MCP_ARCHITECTURE.md` | AI 服务 + 外部 Agent 控制（唯一权威设计，含实施进度） | 活文档 |
| `AI_EDITING_RULES.md` | AI 编码规范（门禁 / 提交 / 测试规则） | 活文档 |
| `DEVELOPMENT_PLAN.md` | Phase 1–5 路线图与验收标准 | 活文档 |
| `PROJECT_STATUS.md` | 按轮次的变更与交接记录（最新轮在最上） | 活文档 |
| `RMCCC_PLUGIN_PLATFORM_ARCHITECTURE_GUIDE.md` | 插件平台目标架构（§1.4 macOS-only、§5.7 平台适配层、§10 AI/MCP 约束） | 活文档 |
| `adr/` | 架构决策记录：ADR-002–007、009–011、**013** accepted；001/008/012 proposed；**014** accepted（核心切片） | 活文档 |
| `audits/` | `AUDIT_2026-10-20.md`（横切全景）+ `AUDIT_2026-10-23.md`（增量台账）+ `PLUGIN_SECURITY_AUDIT_2026-10-27.md`（插件/主题安全审计）、`PLUGIN_SYSTEM_REMOVAL_ANALYSIS_2026-10-28.md`（插件系统去留分析）、`TRIM_TO_THEMES_AND_CARDS_ANALYSIS_2026-10-28.md`（只留主题+卡片：体积/启动/内存收益实测归因）；agent 部分已标注被 ADR-013 取代 | 台账（持续销账） |
| `guides/` | `OPERATIONS.md`（运维）+ `PLUGIN_AUTHORING.md`（插件编写）+ `THEME_PLUGIN_AUTHORING.md`（主题规范） | 活文档 |
| `roadmap/` | Phase 0–7 细化执行计划 + `PLATFORM_ADAPTATION.md`；`08_phase6_agent.md` 为历史记录 | 活文档 |
| `ui-design/` | 前端重设计 15 份（00 总览 … 13 注意事项），前端重构的唯一设计依据 | 活文档 |
| `archive/` | 历史快照：07-31 / 08-04 审计、整改台账、问题索引、HMCL 架构分析、Modloader 方案 | 冻结（仅问题溯源，内链可能失效） |

## 已删除（2026-10-23，git 历史可查）

- `test_log.md` / `test-flow.md` — RMCLI 时代（2026-07-30）的 CLI 测试日志，项目已更名演进；
- `old/MIGRATION_CHECKLIST.md` / `old/MIGRATION_PLAN.md` — 2026-07-29 已完成的迁移存根。

## 维护规则

1. 新增 IPC 命令时同步 `ARCHITECTURE.md` §11、`ui-design/11_IPC_COVERAGE.md`、
   `src/ipc/api.ts`（见 `AI_EDITING_RULES.md` §6）。
2. 架构方向变更先落 ADR（`adr/README.md` 有模板），再改 `ARCHITECTURE.md` 与相关文档。
3. 点时性审计 / 计划完成后**不留在根目录**：仍有效的进 `audits/` 或并入活文档，
   纯历史的移入 `archive/` 并加横幅。
