# ADR-013: 轻干预 AI 架构——内部 LLM 服务 + 外部 MCP 控制

- **状态**：accepted
- **日期**：2026-10-23
- **关联**：ADR-009（凭据）、ADR-010（审计/脱敏，部分修订）、ADR-011（体积）、
  `docs/AI_AND_MCP_ARCHITECTURE.md`（完整设计）、`docs/ui-design/07_AI_AND_MCP.md`（UI）、
  `docs/roadmap/08_phase6_agent.md`（被调整的历史计划）

## 背景

Phase 6 按「受控编排层」在启动器内实现了 Agent 执行循环、工具注册表、审批代理、
会话存储与 `/agent` 页面（9 条 IPC）。实践发现三个结构性问题：

1. 启动器内做对话编排是重复造轮子——外部 AI 工具（ZCode/Claude/Cursor）
   已具备更强的编排、上下文与审批 UX，且用户已在用；
2. 完整 Agent 界面（对话/回放/审批队列）与启动器「干净简洁」的产品目标冲突；
3. 多数被归入「Agent 能力」的功能（缺失前置检测、完整性体检）是纯逻辑，
   用 AI 承载属于能力错配。

用户决策（2026-10-23）：启动器**不内建 Agent**。改为：

- 把启动器能力经 **MCP 服务器**开放给外部 AI 工具，外部 Agent 调用工具
  控制启动器配置与操作，操作过程在 GUI 实时可见、可归因、破坏性可确认；
- 内部保留**轻量 AI 服务**（OpenAI 兼容端口：地址/密钥/自动探测模型/流式），
  供启动器内置功能（如启动失败日志一键分析）与插件调用，无独立 Agent UI；
- AI 与 MCP 均为设置页全局配置，一次配置全局使用。

## 决策

1. **内部 AI 服务**：新建 `rmccc-llm` crate，仅提供
   `list_models` / `chat` / `chat_stream`（OpenAI 兼容，SSE 流式，可取消）；
   配置（base_url/model）入配置 JSON，api_key 入凭据保险库（service=`llm`）；
   插件经桥方法 `llm.chat`（能力 `ai.chat`）消费，宿主代持密钥。
   不做多轮编排、工具调用、会话存储。
2. **外部控制 = MCP**：启动器进程内 MCP 服务器（JSON-RPC 2.0），
   双传输：本机回环 Streamable HTTP（Bearer token，默认关闭）+
   `rmccc mcp` stdio 桥；31 个工具按三级风险策略
   （只读 allow / 操作 allow+归因 / 破坏性 confirm）。破坏性调用走两阶段确认：
   GUI 确认卡放行/拒绝/超时默认拒绝，复用 Phase 6 验证过的
   proposed→approved→applied 快照语义。
3. **GUI 实时可见**：内部命令层操作携带 origin（user/agent/plugin），
   既有事件 payload 扩展 origin 字段；任务中心/主页卡片显示「Agent」徽标，
   Agent 操作 toast 提示，破坏性操作确认卡。
4. **迁移**：`rmccc-agent` 的依赖扫描/补装执行器/脱敏/审计保留并去 Agent 化
   （上移为宿主能力 `mods_dependency_scan`/`mods_apply_plan`）；
   AgentService 循环、对外工具注册表、会话存储、`agent_*` 9 条 IPC、
   `/agent` 页面退役（86→93 条 IPC）。缺失前置检测为纯逻辑功能，
   不依赖 AI。
5. **体积约束**：MCP HTTP 端点基于既有 hyper 依赖（reqwest 已引入）启用
   server feature 自建，禁止引入 axum 全家桶；合入须过 ADR-011 门禁。

## 后果

**正面**：AI 编排复用外部工具的成熟度；启动器 UI 回归简洁（去掉一整个页面）；
纯逻辑功能（依赖补装）不再绕道 LLM；插件获得零成本 AI 能力（宿主代持凭据）；
外部 Agent 天然获得比内建 Agent 更强的推理与审批 UX。

**负面/代价**：需要维护 MCP 工具目录（31 个工具的 Schema/描述/权限）与
两种传输；外部 Agent 体验依赖 MCP 客户端质量；hyper server feature 有体积成本
（须实测）；网络型安全面从「出站 HTTPS」扩展出「本机入站回环端口」。

**对既有决策的修订**：

- ADR-010：会话存储（sessions/）与「Agent 会话」概念废止；
  审计、脱敏、目录权限、保留策略**继续适用**于 MCP 调用与 AI 调用记录；
- `docs/roadmap/08_phase6_agent.md`：保留为历史记录，后续工作以
  `docs/AI_AND_MCP_ARCHITECTURE.md` 的 S1–S7 切片为准；
- 插件指南 §10 改写为「轻干预 AI 与外部 Agent 控制」。

## 验收要点

见 `docs/AI_AND_MCP_ARCHITECTURE.md` §10（外部闭环 / 确认闭环 / AI 闭环 /
审计闭环 / 体积门禁 / 旧面下线）。
