# Phase 6 — Agent 平台

> **对应指南**：§10，§17 Phase 6。**状态**：pending。
> **前置**：Phase 1 Core Service + Phase 2/5 权限系统。**ADR-009**（凭据存储）+ **ADR-010**（Agent 会话/审计/隐私）须在实现前 accepted。

## 1. 目标

通过统一 Core API 和插件工具执行可审查任务。先实现**只读工具 + 计划预览**，再逐步开放可审批写操作。Agent 不越权执行；敏感信息不被无意发送给远程模型。

## 2. 与当前状态的对应

- 当前无 Agent 实现；`ModDependency` 已定义无解析器（P3-2），可作为 Agent 只读工具的早期消费方。

## 3. 任务切片分解

### 6.1 LLM Provider Adapter
- 统一流式消息/错误/模型能力/上下文长度；中止/超时/重试/限流。
- 不同提供商请求字段由 adapter 封装。
- API Key 经 `CredentialStore`（ADR-009），不进普通 JSON/localStorage/日志。

### 6.2 凭据存储升级（ADR-009）
- 评估 macOS Keychain/Windows Credential Manager；当前文件 vault 作受控回退。
- 插件不得读其他插件凭据；只请求某服务受控使用授权。

### 6.3 Session Store + Audit Log
- `agent/sessions/<id>.jsonl` + `tasks/<id>.json`（指南 §12.1）。
- 记录意图/计划/工具调用/审批/输出/最终状态；不记密钥/敏感 prompt。

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
