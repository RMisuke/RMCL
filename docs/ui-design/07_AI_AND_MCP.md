# 07 · AI 服务与外部 Agent 控制（MCP）

> **没有独立页面**。这是「轻干预 · 轻 AI」的 UX 落地（ADR-013）：
> AI 与外部控制都住在**设置页「AI 与外部控制」分区**（一次配置全局使用），
> 其运行反馈**融入既有界面**（任务中心归因徽标/确认卡、toast、日志抽屉 AI 分析），
> 不新开任何窗口。架构权威：`docs/AI_AND_MCP_ARCHITECTURE.md`。

## 1. 设计目标

1. 用户 30 秒配好 AI 端口：输入地址 → 自动检测模型 → 粘贴密钥 → 完成；
2. 外部 Agent（ZCode 等）接入是「复制粘贴一段配置」的难度；
3. 外部 Agent 干的每件事，用户在 GUI 上**看得见**：进度条带 Agent 徽标、
   toast 提示、破坏性操作弹确认卡；
4. AI 消费融入日常动线：游戏启动失败 → 日志抽屉里点「AI 分析」→ 流式看结论。

## 2. 设置页「AI 与外部控制」分区（/settings#ai）

子导航归属「启动器」组，位于「下载」与「插件」之间。两个子区：

```
┌─ 🤖 AI 服务 ────────────────────────────────────────────┐
│ API 地址   [https://api.deepseek.com/v1________] [检测]   │
│            ⟳ 检测中… / ✅ 检测到 12 个模型 / ⚠ 无法连接    │
│ 模型       [deepseek-chat ▾]      ← 检测成功后自动填充      │
│ API 密钥   [••••••••••••••]        ← 已保存则显示「已配置」  │
│ [保存并测试]   状态：✅ 已配置 · 上次调用成功                │
│ 🔒 密钥加密保存在本机（凭据保险库）；日志分析等功能会把       │
│    脱敏后的文本发送至该端点。                               │
└──────────────────────────────────────────────────────────┘
┌─ 🕹 外部 Agent 控制（MCP） ──────────────────────────────┐
│ [○ 关闭  ● 开启]   端口 127.0.0.1:47610（自动分配，可固定）  │
│ 访问令牌   ••••••••4f2a  [显示] [复制] [重新生成]           │
│                                                          │
│ ── 接入方式（复制即用）──                                   │
│ [ZCode / Cursor（HTTP）] [Claude Desktop / stdio（命令）]  │
│   ┌────────────────────────────────────────────┐        │
│   │ { "mcpServers": { "rmccc": {                │ 📋复制  │
│   │   "url": "http://127.0.0.1:47610/mcp",      │        │
│   │   "headers": { "Authorization": "Bearer …" }│        │
│   │ } } }                                       │        │
│   └────────────────────────────────────────────┘        │
│                                                          │
│ ── 在线会话 ──                                            │
│ ● ZCode  自 13:02 · 47 次调用                [断开]       │
│ （空态：暂无外部工具连接）                                  │
│                                                          │
│ ── 工具权限 ──  [全部重置为默认]                            │
│ 只读（15）     ✓ 允许      [展开逐项调整]                   │
│ 操作（11）     ✓ 允许·GUI 可见 [展开]                      │
│ 破坏性（5）    ⚠ 需确认     [展开：launch.kill / profiles… │
│                                                          │
│ ── 最近 Agent 操作 ── [查看全部]                           │
│ 13:05 ZCode · mods.apply_plan · ✅ 补装 1 个前置            │
│ 13:02 ZCode · game.install · ✅ 创建实例 1.20.4-fabric      │
└──────────────────────────────────────────────────────────┘
```

### 2.1 AI 服务子区交互

- **地址输入即检测**（300ms 防抖）`ai_models_probe(base_url)`；成功 → 模型下拉自动填充
  并默认选第一个（`ai_config_set` 保存）；失败 → 行内黄条（人话：「无法连接：401 未授权，
  请检查密钥」/「超时，检查地址是否 OpenAI 兼容（通常以 /v1 结尾）」）。
- 密钥输入框：password 型，已配置时占位「已配置（不回显）」，留空保存 = 不变更。
- 「保存并测试」：保存后发一次极小请求（`ai_models_probe` + 一句 ping），
  toast 结果；此按钮即「测试连接」。
- 未配置 AI 时的下游联动：日志抽屉「AI 分析」按钮灰置 + tooltip「先在设置中配置 AI 服务」+
  点击跳转本分区。
- **数据外发确认**：首次使用任一 AI 功能时弹一次性确认
  「日志分析将把脱敏后的游戏日志发送至 {host}」[允许][取消]，记忆偏好
  （偏好存 localStorage，重置入口在本区底部小字）。

### 2.2 MCP 子区交互

- 总开关关闭态：整个子区（会话/权限/操作视图）收起，只留一行说明
  「关闭时外部工具无法连接 RMCCC」+ 令牌不显示。
- 令牌「显示」需点击（明文短暂显示 30s）；「重新生成」确认弹窗说明
  「现有外部工具连接将全部失效，需要更新配置」。
- 工具权限三组卡片按架构文档 §5.2 的风险分组；逐项可设 允许 / 需确认 / 禁用
  （`mcp_tools_set`）；破坏性组默认全部「需确认」。
- 在线会话（`mcp_status` + `mcp://session` 事件驱动）：名称（MCP clientInfo）、
  连接时间、调用次数、[断开]（`mcp_sessions_disconnect`）。
- 最近 Agent 操作（`mcp_recent_calls`，审计脱敏视图）。

## 3. GUI 实时反馈（融入既有界面，本模块的跨页部分）

### 3.1 归因徽标（任务中心 / 主页下载卡）

所有 origin=agent 的任务（下载批次 / loader 安装 / 运行中游戏）在任务行与
主页下载卡显示 `🤖 Agent` 徽标（RmTag accent 变体 + 图标 smart_toy）；
origin=plugin 显示插件名徽标。数据：既有事件 payload 新增的 `origin` 字段。

### 3.2 Agent 操作 toast

- 外部 Agent 发起操作 → toast「ZCode 正在下载 1.20.4…」（info 型，带「查看」
  → 任务中心）；
- 操作完成/失败 → 常规结果 toast（含 Agent 归因文案）；
- 会话连接/断开（`mcp://session`）→ 设置页未打开时 toast「外部工具 ZCode 已连接 RMCCC」。

### 3.3 确认卡（任务中心新分区，破坏性操作）

```
任务中心面板顶部（有待确认时置顶显示，红点同步 Rail 任务指示器）：
┌─ ⚠ 外部 Agent 请求确认 ────────────────┐
│ ZCode 请求删除实例                      │
│ 目标：纯净生存（1.21）                  │
│ 影响：删除该实例的启动配置（不删游戏文件） │
│                            ⏳ 87s       │
│         [✅ 允许]   [✕ 拒绝]            │
└────────────────────────────────────────┘
```

- 数据：`mcp_confirmations_list`（轮询 2s，有 pending 时）+ `mcp://confirmation` 事件驱动；
- 允许/拒绝 = `mcp_confirmation_resolve`；倒计时归零自动拒绝（后端语义，前端只显示）；
- 确认卡信息按工具参数人话化渲染（每工具一行文案模板，未覆盖的回退 JSON 视图）；
- 窗口最小化时 Rail 任务指示器红点计数提示。

## 4. 日志一键 AI 分析（内置参考消费方）

入口：游戏日志抽屉（10 §4）——异常退出自动弹开时，或手动打开时，
抽屉头部出现 [🤖 AI 分析] 按钮：

```
┌─ 日志 · sodium 实验存档 ──── [🤖 AI 分析] [暂停滚动] [复制] [✕] ─┐
│ …日志流…                                                        │
├─ AI 分析 ────────────────────────────────────────────────────┤
│ ▸ 分析中（流式）：                                               │
│ │ 从日志看，游戏在加载 mod 'indium' 时抛出                        │
│ │ ClassNotFoundException…▌                                      │
│ ──────────────────────────────────────                         │
│ 结论卡（完成后）：                                               │
│ 可能原因: indium 版本与 sodium 不匹配（0.9.30 vs 0.5.8）          │
│ 建议: ① 在 Mod 管理中禁用或更新 indium  ② 检查缺失前置            │
│                [复制分析结果] [重新分析]                          │
└────────────────────────────────────────────────────────────────┘
```

- 交互：点击 → `ai_analyze_log(profile_id)` → 订阅 `ai://delta/done/error` 流式渲染
  （等宽区，安全 markdown：粗体/代码/列表，不渲染 HTML）；
- 未配置 AI：按钮灰置 + 跳设置引导（§2.1）；分析中可 [取消]（`ai_analyze_cancel`）；
- 首次使用过 §2.1 的数据外发确认；
- 发送内容 = 最近会话日志尾部（后端截取 ERROR 上下文窗口 + redact），抽屉顶部小字
  「已发送 213 行脱敏日志至 {host}」保证透明；
- 同一实例同一会话只保留最近一次分析结果；「重新分析」重新拉取最新日志。

## 5. 实例详情「检查缺失前置」（纯逻辑，不是 AI）

归位说明：原设计「依赖体检 → Agent」已删除。新流程（纯本地+网络逻辑）：

```
实例详情 · Mod 管理 tab → [🩺 检查缺失前置]
   → mods_dependency_scan(profile_id)
   → 结果对话框：
      缺失必需（1）：indium ≥1.0（sodium 需要）
      冲突（0） / 无法解析（2，Forge jar 未纳入判断）
                    [一键补装缺失] [取消]
   → 确认计划（列出将下载的文件）→ mods_apply_plan(profile_id, plan)
   → 任务中心显示下载进度（普通用户来源，无 Agent 徽标）
   → toast「已补装 1 个前置」+ Mod 列表刷新
```

详细 UI 归属 04_INSTANCE_DETAIL §4；此处只声明「此功能不依赖 AI 配置」。

## 6. 插件消费 AI（面向插件开发者）

- 插件设置页/卡片经桥方法 `llm.chat`（能力 `ai.chat`）调用宿主 AI 端口；
  流式经 `llm.delta/done/error` 推送（插件 UI 自己渲染）；
- 插件页（09）的权限 tab 中 `ai.chat` 出现在能力目录，风险 medium；
- 宿主未配置 AI 时桥返回结构化错误（`not_configured`），插件 UI 应引导用户去设置页。

## 7. 状态与空态

| 场景 | 呈现 |
|---|---|
| AI 未配置 | 设置分区正常可填；下游功能按钮灰置 + 引导 |
| AI 配置错（401/超时） | 设置区行内错误 + 测试按钮重试；下游功能报人话错误 |
| MCP 关闭（默认） | 分区收起 + 一句说明；GUI 无任何 Agent 反馈元素 |
| MCP 开启无会话 | 「暂无外部工具连接」+ 接入方式卡 |
| MCP 会话在线无操作 | 会话列表显示；无 toast 打扰 |
| 确认超时/拒绝 | 确认卡消失 + toast「已拒绝：超时未确认/用户拒绝」 |

## 8. 接口与事件映射（本模块全量）

| 组 | 接口 |
|---|---|
| AI 服务 | `ai_config_get/set`、`ai_models_probe`、`ai_analyze_log/cancel` |
| MCP | `mcp_status`、`mcp_set_enabled`、`mcp_regenerate_token`、`mcp_tools_list/set`、`mcp_sessions_disconnect`、`mcp_confirmations_list`、`mcp_confirmation_resolve`、`mcp_recent_calls` |
| 纯逻辑前置 | `mods_dependency_scan`、`mods_apply_plan` |
| 事件 | `ai://delta/done/error`、`mcp://session`、`mcp://confirmation`；既有任务事件的新 `origin` 字段 |

> 以上 16 条命令为**设计新增**（后端待实现，见架构文档 S1–S7 切片）；
> 旧 `agent_*` 9 条命令与 `/agent` 页面随迁移移除（11_IPC_COVERAGE §Agent 注记）。

## 9. 渐进披露自查

一级：AI 地址/密钥/模型三件套、MCP 总开关与接入配置复制、确认卡、日志 AI 分析按钮；
二级：令牌管理与重新生成、会话管理、最近 Agent 操作、数据外发偏好重置；
三级：31 工具逐项权限矩阵、原始审计视图。

## 10. i18n key 提案（节选）

`settings.ai.title` / `settings.ai.baseUrl` `settings.ai.baseUrlHint` /
`settings.ai.probing` `settings.ai.probeOk {n}` `settings.ai.probeFail` /
`settings.ai.model` `settings.ai.apiKey` `settings.ai.saved` `settings.ai.testAndSave` /
`settings.ai.privacyNote` `settings.ai.egressConfirm {host}` /
`settings.mcp.title` `settings.mcp.enable` `settings.mcp.port` /
`settings.mcp.token.show` `settings.mcp.token.copy` `settings.mcp.token.regenerate` /
`settings.mcp.regenerate.body` / `settings.mcp.connect.http` `settings.mcp.connect.stdio` /
`settings.mcp.sessions` `settings.mcp.sessions.empty` `settings.mcp.disconnect` /
`settings.mcp.perms.read` `settings.mcp.perms.operate` `settings.mcp.perms.destructive` /
`settings.mcp.perms.allow` `settings.mcp.perms.confirm` `settings.mcp.perms.deny` /
`settings.mcp.recent` / `settings.mcp.off` /
`tasks.confirm.title` `tasks.confirm.by {tool}` `tasks.confirm.timeout {s}` /
`tasks.confirm.allow` `tasks.confirm.deny` `tasks.confirm.denied` /
`tasks.agentBadge` `tasks.agentToast.start {tool} {action}` /
`logs.ai.analyze` `logs.ai.analyzing` `logs.ai.cause` `logs.ai.suggestion` /
`logs.ai.copy` `logs.ai.retry` `logs.ai.notConfigured` `logs.ai.sentNote {n} {host}` /
`instanceDetail.mods.missingScan` `instanceDetail.mods.applyPlan` …

## 11. 验收要点

- AI 配置三步走：输地址 → 自动出模型列表 → 存密钥 → 测试通过（真机对 OpenAI 兼容端点）。
- 日志分析流式呈现 + 中途取消 + 路径脱敏（无绝对 home 路径出现在分析文本来源日志里）。
- ZCode 接入：复制 HTTP 配置 → 连接成功 → 会话出现在设置页 → 调用 `mods.search`
  → GUI toast；发起下载 → 任务中心 Agent 徽标进度条。
- 确认流：外部请求删除实例 → 确认卡倒计时 → 拒绝 → MCP 返回 denied、无文件变化；
  允许 → 删除执行 + 审计记录。
- MCP 关闭时：本地端口无监听（`lsof` 验证），外部工具无法连接。
- 旧 `/agent` 路由重定向到 `/settings#ai`，`agent_*` 命令无前端引用。
