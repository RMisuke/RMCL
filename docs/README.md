# RMCCC 文档索引

> 本目录的分类导航。顶层协作规范见 [`../AGENTS.md`](../AGENTS.md)（设计文档优先、
> Phase 顺序、体积红线、macOS-only 约束）；文档间冲突时以 `ARCHITECTURE.md` 为最高权威。
> 本次索引重建于 **2026-11-03**，目录清单与数字全部按本机实盘核对（`find` / `grep` / `stat`），未采信文档自述。

## 按用途快速导航

| 我想… | 去读 |
|---|---|
| 理解整体架构（最高权威） | [`ARCHITECTURE.md`](ARCHITECTURE.md)（§11 IPC 表 **132 条**、§25 AI/MCP、§26 壁纸引擎） |
| 了解 AI / MCP 方向（ADR-013） | [`AI_AND_MCP_ARCHITECTURE.md`](AI_AND_MCP_ARCHITECTURE.md)（S1–S7 切片与进度表） |
| 改 AI 日志分析（S2 重构） | [`AI_ANALYSIS_REFACTOR.md`](AI_ANALYSIS_REFACTOR.md)（运行绑定 / 终止保证 / 提交模式 / 提示词；已实现） |
| 确认开发进度 / 接手开发 | [`PROJECT_STATUS.md`](PROJECT_STATUS.md)（轮次记录，最新在最上）→ [`DEVELOPMENT_PLAN.md`](DEVELOPMENT_PLAN.md)（Phase 1–5）→ [`roadmap/`](roadmap/)（Phase 0–7 细化） |
| 做前端重构 | [`ui-design/`](ui-design/)（16 份设计 + 验收清单 + 本索引） |
| 做前端特效/渲染层重构（性能） | [`ui-render-refactor/`](ui-render-refactor/)（现状审计 + 目标架构 + RenderTier 引擎 + 背景/表面/动效/主题分册 + 实现计划） |
| 写插件 | [`guides/PLUGIN_AUTHORING.md`](guides/PLUGIN_AUTHORING.md) → `plugin-sdk/` → [`adr/`](adr/)（ADR-003/004/006/007/022） |
| 做一套主题 | [`guides/THEME_PLUGIN_AUTHORING.md`](guides/THEME_PLUGIN_AUTHORING.md)（含 `../schemas/theme.schema.json`） |
| 部署与运维 | [`guides/OPERATIONS.md`](guides/OPERATIONS.md) |
| 查技术债 / 审计结论 | [`audits/`](audits/)（4 份仍在跟踪的台账） |
| 查插件平台去留的正式结论 | [`adr/ADR-022-plugin-platform-retention.md`](adr/ADR-022-plugin-platform-retention.md)（保留；两份收缩分析已归档） |
| 查历史决策由来 | [`adr/`](adr/)（21 条 ADR）与 [`archive/`](archive/)（24 份历史快照） |

## 目录结构

| 位置 | 内容 | 时效性 |
|---|---|---|
| `ARCHITECTURE.md` | 完整架构设计（最高权威） | 活文档 |
| `AI_AND_MCP_ARCHITECTURE.md` | AI 服务 + 外部 Agent 控制（唯一权威设计，含实施进度） | 活文档 |
| `AI_EDITING_RULES.md` | AI 编码规范（门禁 / 提交 / 测试规则） | 活文档 |
| `DEVELOPMENT_PLAN.md` | Phase 1–5 路线图与验收标准 | 活文档（Phase 完成态回填停在 2026-10-20，实况看 `PROJECT_STATUS.md`） |
| `PROJECT_STATUS.md` | 按轮次的变更与交接记录（最新轮在最上） | 活文档 |
| `RMCCC_PLUGIN_PLATFORM_ARCHITECTURE_GUIDE.md` | 插件平台目标架构（§1.4 macOS-only、§5.7 平台适配层、§10 AI/MCP、§20 ADR 触发条件） | 活文档 |
| `adr/` | 21 条 ADR + 索引：accepted = 003–007、009–011、013、014、016、018–022；proposed = 001、008、012；superseded = 002→016、017→020。**无 ADR-015**（序号空缺，不回填） | 活文档 |
| `audits/` | 仍在跟踪的 4 份台账：`AUDIT_2026-10-20.md`（横切全景 + 功能矩阵）、`AUDIT_2026-10-23.md`（增量台账，agent 部分已由 ADR-013 取代）、`PLUGIN_SECURITY_AUDIT_2026-10-27.md`、`AUDIT_2026-11-01_WINDOWS.md`（Windows 兼容 + 整改销账） | 台账（持续销账） |
| `guides/` | `OPERATIONS.md`（运维）+ `PLUGIN_AUTHORING.md`（插件编写）+ `THEME_PLUGIN_AUTHORING.md`（主题规范） | 活文档 |
| `roadmap/` | 13 份：`00_ROADMAP_OVERVIEW.md`、`01`–`09` 各 Phase、`EXECUTION_PROMPTS.md`（批次执行提示词）、`PLATFORM_ADAPTATION.md`（平台适配层归属）、`README.md`；`08_phase6_agent.md` 为历史记录 | 活文档 |
| `ui-design/` | 前端重设计：`00`–`15` 共 16 份（00 总览 … 15 主题系统 V4）+ `ACCEPTANCE_CHECKLIST.md`（手动走查清单）+ `README.md`，前端重构的唯一设计依据 | 活文档 |
| `ui-render-refactor/` | 前端特效/渲染底层重构方案（2026-11-03）：`README` 索引 + `00` 现状审计 + `01` 目标架构 + `02` RenderTier 引擎 + `03` 背景子系统 + `04` 表面/令牌/CSS + `05` 自适应+动效 + `06` 主题特效 + `07` 实现计划。针对玻璃/视频壁纸/主题动效的高 CPU/GPU 占用，**不含代码改动** | 活文档（设计方案） |
| `design/` | `mc-icons-src/`：3 张 Minecraft 物品图标原始素材（`book`/`crafting_table`/`oak_hanging_sign`，128px） | 素材 |
| `archive/` | 24 份历史快照：07-31 / 08-04 审计与整改台账、问题索引、HMCL 架构分析、Modloader 方案，**2026-11-03 新增** `BACKEND_AUDIT_2026-10-25/`、`FRONTEND_AUDIT_2026-10-25/`、`FRONTEND_AUDIT_2026-10-26/`、`PLUGIN_SYSTEM_REMOVAL_ANALYSIS_2026-10-28.md`、`TRIM_TO_THEMES_AND_CARDS_ANALYSIS_2026-10-28.md`（后两份的收缩建议由 ADR-022 否证） | 冻结（仅问题溯源，内链可能失效） |

## 与仓库其它位置的关系

| 位置 | 说明 |
|---|---|
| `../schemas/` | 公共协议 JSON Schema：`manifest`、`dashboard-layout`、`plugin-ui-bridge`、`theme` |
| `../src-tauri/src/handlers.rs` | **IPC 命令的唯一权威清单**（132 条，由 `../src-tauri/src/ipc_tests.rs:168` 断言锁定） |
| `../icon resource/` | 应用图标与彩色素材的入库位置（`app-icon.png` 是 `pnpm tauri icon` 的唯一再生成入口，见 `audits/AUDIT_2026-11-01_WINDOWS.md`）；`docs/design/mc-icons-src/` 只放物品图标原始素材 |
| `../plans/` | 2026-08 的 HMCL 对标计划三份，**全仓零引用**，尚未决定归档去向 |

## 已归档（2026-11-03，git 历史可查）

- `audits/BACKEND_AUDIT_2026-10-25/`、`audits/FRONTEND_AUDIT_2026-10-25/`、`audits/FRONTEND_AUDIT_2026-10-26/`
  → 移入 `archive/`：基线 commit 早于实例 v2 / 主题 v2–v3 / 任务中心 / 整合包安装 / SNGL 便携数据根多轮改动；
- `audits/PLUGIN_SYSTEM_REMOVAL_ANALYSIS_2026-10-28.md`、`audits/TRIM_TO_THEMES_AND_CARDS_ANALYSIS_2026-10-28.md`
  → 移入 `archive/` 并在文首标注建议未被采纳（见 ADR-022）；
- 所有指向旧 `docs/audits/<归档名>` 的引用（含 `../crates/rmccc-modloader/src/lib.rs:347` 的注释路径）已改指 `docs/archive/`。

## 已删除（2026-10-23，git 历史可查）

- `test_log.md` / `test-flow.md` — RMCLI 时代（2026-07-30）的 CLI 测试日志，项目已更名演进；
- `old/MIGRATION_CHECKLIST.md` / `old/MIGRATION_PLAN.md` — 2026-07-29 已完成的迁移存根。

## 维护规则

1. 新增 IPC 命令时同步 `ARCHITECTURE.md` §11、`ui-design/11_IPC_COVERAGE.md`、
   `src/ipc/api.ts` 与 `src-tauri/src/ipc_tests.rs` 的总数断言（见 `AI_EDITING_RULES.md` §6）。
2. 架构方向变更先落 ADR（`adr/README.md` 有模板），再改 `ARCHITECTURE.md` 与相关文档。
3. 点时性审计 / 计划完成后**不留在根目录**：仍有效的进 `audits/` 或并入活文档，
   纯历史的移入 `archive/` 并加横幅。
4. **文档中的数量必须由命令得出**：IPC 条数、crate 成员数、主可执行字节数、测试计数——
   写入时在同一段落注明度量方式；过期数字直接改，不做「约/目前」模糊化。
5. 已被后续轮次覆盖的结论不保留双口径：旧文进 `archive/` 并加否决横幅，指向它的新引用改指台账或 ADR。
