# ADR-010: Agent 会话、工具审计与隐私边界

> **2026-10-23 修订（ADR-013）**：AI 方向调整为「内部轻量 AI 服务 + 外部 MCP 控制」。
> 本 ADR 的**会话存储（sessions/）与内建 Agent 会话概念废止**；
> **审计、脱敏、目录权限（0700）、保留策略继续有效**，适用对象改为
> MCP 外部工具调用与 AI 服务调用记录（origin 维度）。后续以 ADR-013 与
> `docs/AI_AND_MCP_ARCHITECTURE.md` 为准。

- **状态**：accepted（部分被 ADR-013 修订）
- **日期**：2026-10-22
- **阶段**：Phase 6
- **关联**：指南 §10.1/§10.2/§10.5、§12.1；ADR-009（凭据）；ADR-007（逻辑插件权限复核）；`docs/roadmap/08_phase6_agent.md` 6.3/6.7

## 背景

Agent 是"受控编排层"，不是"能任意调用工具的聊天窗口"。要让用户事后能回答
"这个 Agent 为什么装了这个 Mod / 改了什么 / 用了哪些工具"，同时又不能把
凭据、完整日志、私人存档当成默认上下文上传给远程模型。

指南 §12.1 已给出目录约定：

```text
<ApplicationData>/rmccc/agent/
  sessions/<session-id>.jsonl
  tasks/<task-id>.json
```

需要定的是：**记录什么、不记录什么、留多久、谁能读、如何脱敏**。
这一层如果先做错，后面接审批（6.5）与写操作（Phase 7）时无法补救。

## 选项

### A. 只记内存、不落盘

优点：没有隐私负担。缺点：崩溃/重启后无法审计；用户无法回看 Agent 做过什么；
无法满足"发起者可审计"的验收标准。排除。

### B. 全量记录（含完整提示词、工具原始输入输出、完整日志）

优点：调试与复盘最强。缺点：把密钥、用户名、绝对路径、服务器地址都写进磁盘；
一旦日志被上传（用户手动发 issue）就是数据泄露。违反指南 §10.5。排除。

### C. 分层的、默认脱敏的事件日志（选中）

会话存**事件流**（append-only JSONL），审计存**工具调用记录**（append-only JSONL），
两者字段都是白名单枚举；提示词正文默认不落盘，只存哈希与长度；工具输入输出先脱敏再落盘；
保留期默认 30 天。

## 决策

1. **目录与文件**
   - `agent/sessions/<session-id>.jsonl`：一行一个事件。
   - `agent/audit.jsonl`：跨会话的工具调用审计（与插件审计并列，互不覆盖）。
   - `agent/tasks/<task-id>.json`：`ModInstallPlan` 等**待用户确认的计划**（不执行写入）。
   - 目录创建后经 `PlatformFs::restrict_owner_only` 收紧（`0600`；Windows ACL 待原生 CI）。
2. **事件白名单**（`SessionEvent`）：`session_started`、`user_message`（仅哈希+长度）、
   `model_response`（仅哈希+长度）、`tool_call`、`tool_result`、`approval_requested`、
   `approval_resolved`、`error`、`session_ended`。
   **没有** `raw_prompt` 字段；开启"记录提示词"是未来的显式 opt-in，不在本 ADR 的默认路径里。
3. **审计条目**（`AuditRecord`）：`ts`、`session_id`、`tool_id`、`risk_level`、
   `input_redacted`、`outcome`（`ok` / `denied` / `error`）、`duration_ms`、`error_kind`。
   **不记** api_key、完整命令输出、文件内容。审计写入失败不得让工具调用"假装成功"：
   只读工具失败可见，写操作（未来）必须 fail-closed。
4. **脱敏规则**（`redact` 模块，纯函数、可单测）
   - 形如 `sk-…` / `Bearer …` / `api_key=…` 的凭据串 → `«redacted»`；
   - 用户 home 目录前缀 → `~`；
   - 邮箱与 Minecraft 用户名（上下文已知时）→ `«user»`；
   - 会话 UUID → 仅在审计内部保留，导出文本默认遮蔽。
   - 脱敏是**写盘前**执行，不是展示时执行：磁盘上不应存在未脱敏原文。
5. **隐私上下文边界**
   - 默认只提供：所选 profile 的 `id/name/game_version/loader/java_major` 与已装 Mod 的
     `mod_id/version/file` 元数据。
   - 日志/崩溃报告只在用户显式选择范围后读取，且先脱敏。
   - 不扫描磁盘、不遍历存档、不读取 `accounts.json` / `private/`。
   - 远程模型调用前，UI 说明"将发送哪些字段"；本地 loopback 端点与远程 HTTPS 分开提示。
6. **保留与删除**
   - 默认保留 30 天，按天裁剪；用户可"清除 Agent 历史"一键删除 `agent/` 下会话与任务文件
     （审计保留策略与插件审计一致：可清除，但清除动作本身记一条审计）。
   - 不自动上传任何会话内容；导出是显式动作，导出前再次脱敏。

## 后果

**正面**

- 满足"工具结果可追踪、发起者可审计"，同时默认不把敏感内容写盘。
- 事件与审计都是 JSONL，崩溃后仍可读；不需要数据库，符合项目 JSON 存储风格。
- 脱敏是纯函数，可像其它安全边界（zip-slip、路径校验）一样用正负向单测锁住。

**负面 / 代价**

- 默认不存提示词正文，排障时需要用户临时开启记录（后续切片）。
- 审计写入增加一次同步 I/O；只读工具可接受，写操作路径需要异步队列（6.5 处理）。
- 保留期裁剪需要后台任务或启动时扫描；本切片只提供纯函数，不接定时器。

## 迁移影响

- 新增 `crates/rmccc-agent` 的 `session` / `audit` / `redact` 模块；无既有调用方受影响。
- `agent/` 目录加入数据目录约定（`docs/ARCHITECTURE.md` §15 与 `rmccc-config::paths`）。
- 不涉及插件注册表；Agent 的工具调用**不能**绕过 `authorize()`（Phase 2/5 规则不变）。

## 跨平台验证方式

- macOS ARM64（本机）：JSONL 追加/读取、脱敏正负向、保留期裁剪纯函数、审计写入失败可见。
- Windows：目录/文件权限已由 `windows.rs` 的 `icacls` ACL 实现；原生 Windows 编译/实测 deferred。
- Linux：目录权限收紧仍为 `UnsupportedPlatform` → deferred。

## 状态变更记录

- 2026-10-21：proposed（Phase 6 前置，指南 §10.5）
- 2026-10-22：accepted（分层事件日志 + 脱敏审计；提示词正文默认不落盘）
