# Phase 6 — Agent 平台

> **对应指南**：§10，§17 Phase 6。**状态**：**M7 达成（2026-10-24）**：6.1/6.2/6.3/6.6 + 6.5 第一批
>（执行循环 + 审批链 + IPC/UI）完成；接线前的审计债（P2-17/28/29）已清。6.4 注册表持久化随 Phase 7。
> **前置**：Phase 1 Core Service + Phase 2/5 权限系统。**ADR-009**（凭据存储）+ **ADR-010**（Agent 会话/审计/隐私）已于 2026-10-22 accepted。

## 1. 目标

通过统一 Core API 和插件工具执行可审查任务。先实现**只读工具 + 计划预览**，再逐步开放可审批写操作。Agent 不越权执行；敏感信息不被无意发送给远程模型。

## 2. 与当前状态的对应

- **6.1 / 6.2 / 6.3 / 6.6 已落地**（`crates/rmccc-agent`，63 项测试）；`ModDependency` 现在有真实解析器与消费方（审计 P3-2 销账）。
- **6.5 第一批完成（2026-10-24）**：`rmccc-agent::runtime`——`AgentService` 执行循环
  （脚本化假 provider 的 8 项端到端测试：直接调用回灌 / 审批批准与拒绝 / 取消 / 迭代上限）
  + `ApprovalBroker`（`proposed→approved/rejected` 快照推进 + `approval_resolved` 事件留痕）；
  IPC `agent_*` 9 条命令（注册表 77→86）+ `/agent` 页面（配置 / 对话 / 审批队列 / 历史）。
  仍是**只读工具**：写操作即使审批通过也被注册表拒绝，写工具在后续切片注册。
- **接线前审计债已清（2026-10-24）**：P2-17（home 脱敏接线、目录 0700、内置工具改为注入固定根目录，
  模型不再提供路径）、P2-28（unparsed jar 不再计入 installed）、P2-29（审计 fail-closed、JSONL 互斥、clear_all 记审计）。
- **6.4 / 6.7 残留**：注册表持久化随 Phase 7；隐私的"按需上下文范围 UI"随下一批切片。

### 已完成切片（2026-10-22）

| 切片 | 状态 | 证据 |
|------|------|------|
| 6.1 LLM Provider Adapter | ✅ | `rmccc-agent::llm`：`LlmProvider` / `OpenAiCompatibleProvider`；SSE 增量解析、总超时、指数退避重试（429/5xx/网络）、最小间隔限流、`CancelToken` 逐 chunk 检查；本地 TCP 服务器覆盖流式/重试/取消/超时/429 路径 |
| 6.2 凭据存储（ADR-009） | ✅ | `rmccc-agent::credentials`：`CredentialStore` trait + `FileCredentialStore`（AES-256-GCM，`private/agent.key` 0600，与账号 vault key 隔离）；公开视图无 `api_key`；端点策略（HTTPS / loopback HTTP）。Keychain/Credential Manager 记为 Phase 7+ 可替换后端 |
| 6.3 Session Store | ✅ | `rmccc-agent::session`：`SessionStore` 写 `agent/sessions/<id>.jsonl`（9 种白名单事件，提示词/输出只存 `sha256` + 字符数）、`agent/tasks/<id>.json`（原子写 + 脱敏 `TaskSnapshot`）、`list` / `read` / `read_task` / `list_tasks`、`purge_expired`（默认 30 天）与 `clear_all`（保留审计）；id 只允许 `[A-Za-z0-9_-]{1,64}`（路径安全唯一保证点）；`rmccc-config::paths` 补 `agent_*` 路径 |
| 6.6 只读工具 + 首个消费方 | ✅ | `rmccc-agent::mods`：扫描 `mods/*.jar` 的 `fabric.mod.json`（`depends`/`suggests`/`breaks`）→ `DependencyReport` + dry-run `ModInstallPlan`；`ToolRegistry` 校验 JSON Schema、拒绝非 `read_only`、未知字段与相对路径；内置 `versions.installed` / `mods.installed` / `mods.dependency_scan` |
| 6.4 Tool Registry 持久化 | 🟡 部分 | 描述符/四级风险/确认策略/重复 id 拒绝已在 6.6 完成；**跨进程的工具注册持久化未做**（插件工具来源属于 Phase 7） |
| 6.5 Loop + Approval | ✅ 第一批 | `AgentService` 执行循环 + `ApprovalBroker` + IPC `agent_*` 9 条 + `/agent` 页面；只读工具 + 计划预览 + 审批可审计（M7）；Agent 已接入主程序（主可执行 8.16 MiB，ADR-011 ✅） |

> **偏离说明**：`mods.toml`（Forge/NeoForge）未解析——这类 jar 进入 `unparsed` 并在计划里显式声明，
> **且不再计入已安装集合**（P2-28 修复）。
> **已接线（2026-10-24）**：`src-tauri` 依赖 `rmccc-agent`，IPC 9 条 + `/agent` 页面；主可执行 8.16 MiB（ADR-011 ✅）。

## 3. 任务切片分解

### 6.1 LLM Provider Adapter
- 统一流式消息/错误/模型能力/上下文长度；中止/超时/重试/限流。
- 不同提供商请求字段由 adapter 封装。
- API Key 经 `CredentialStore`（ADR-009），不进普通 JSON/localStorage/日志。

### 6.2 凭据存储升级（ADR-009）
- 评估 macOS Keychain/Windows Credential Manager；当前文件 vault 作受控回退。
- 插件不得读其他插件凭据；只请求某服务受控使用授权。

### 6.3 Session Store + Audit Log ✅（2026-10-22）
- `agent/sessions/<id>.jsonl` + `tasks/<id>.json`（指南 §12.1）。
- 记录意图/计划/工具调用/审批/输出/最终状态；不记密钥/敏感 prompt。
- **实现**：`SessionEventKind` = `session_started` / `user_message` / `model_response` / `tool_call` /
  `tool_result` / `approval_requested` / `approval_resolved` / `error` / `session_ended`；
  `tool_result` 只存 `ok` / `duration_ms` / `output_sha256` / `output_bytes`；错误与审批摘要截断后入库。
- **保留期**：`purge_expired(retention, now)` 是纯 mtime 裁剪（不接定时器），`clear_all` 删会话与任务、保留 `audit.jsonl`。
- **测试**（16 项单测 + 2 项端到端）：prompt 正文不上盘、工具输入脱敏、输出只存哈希、id 穿越拒绝（`../`/`/`/`..`/空串/超长）、
  未知会话 `NotFound`、任务快照原子写、列表排序、保留期只删旧文件、`clear_all` 保留审计。

### 6.4 Tool Registry + 风险分级
- `AgentToolDescriptor`（id/pluginId/name/inputSchema/outputSchema/requiredCapabilities/riskLevel/timeout/confirmationPolicy）。
- read_only / reversible_write / destructive / external_side_effect 四级。
- 模型不能自动扩大工具权限。

### 6.5 Planning/Execution Loop + Approval Broker
- 模型响应 → 受校验工具请求 → 权限判定 → 规范化结果 → 审计。
- 危险操作前呈现计划/目标实例/文件变更摘要/风险给用户确认。
- 取消/超时/失败恢复。

### 6.6 只读工具先行
- 查询版本/已装 Mod/兼容性；生成 `ModInstallPlan`（不执行写入）。
- "检查实例模组兼容性并修复缺失前置"流程（指南 §10.6）作为首个端到端场景，写操作经用户确认 + Core 再校验。

### 6.7 隐私
- Agent 按需访问上下文（Profile ID/版本/Loader/Java/已装 Mod 元数据）；日志脱敏用户名/路径/账号 ID。
- 不默认扫描全盘/上传全量日志/存档/配置。

## 4. 验收标准

- [ ] Agent 不越权执行工具；安装/删除类先形成方案。
- [ ] 工具结果可追踪；发起者可审计。
- [ ] 模型输出不能直接取得额外权限或任意执行命令。
- [ ] 发送给远程模型的数据范围清晰且可由用户控制。
- [ ] 超时/取消/重复调用/失败恢复有测试。
- [ ] ADR-009/010 accepted；Windows = deferred。

## 5. 平台策略

- Keychain 后端 macOS 实测；Windows Credential Manager 后端 deferred。
- 本地模型端点与远程 HTTPS provider 网络策略分开处理。

## 6. 风险与回滚

- Prompt Injection / 工具参数污染 → 参数 schema 校验 + 审批 + 不把模型输出当指令。
- 凭据泄露 → 强制走 CredentialStore；CI 静态扫描敏感字段。

## 7. 文档同步

- `ARCHITECTURE.md` 增"Agent 平台"章节；ADR-009/010 入库。
- `AUDIT` §1.3"Agent"更新；`PROJECT_STATUS` 回填。
