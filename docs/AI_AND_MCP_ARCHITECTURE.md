# RMCCC AI 与外部 Agent 控制架构（轻干预 · 轻 AI）

> 版本：v1.0（2026-10-23）。决策记录：ADR-013。
> 本文档是 RMCCC「AI 能力 + 外部 Agent 接口」的**唯一权威设计**，
> 取代 Phase 6 期间形成的"启动器内建 Agent 平台"方向（`roadmap/08_phase6_agent.md`
> 保留为历史记录）。UI 层设计见 `ui-design/07_AI_AND_MCP.md`。

> **实施进度（2026-10-23，代码以本表为准）**：
>
> | 切片 | 状态 | 说明 |
> |------|------|------|
> | S1 AI 服务 | ✅ 后端完成 | `rmccc-llm` + `ai_*` 5 条 IPC；设置页 AI 分区 UI 随前端重构落地 |
> | S2 日志分析 | ✅ 后端完成 | `ai_analyze_log/cancel` + `ai://delta/done/error` 事件；日志面板流式 UI 随前端重构落地 |
> | S3 缺失前置 | ✅ 后端完成 | `mods_dependency_scan/apply_plan` IPC；实例页 UI 随前端重构落地 |
> | S4 MCP 服务器 | ✅ 后端完成 | Streamable HTTP（JSON 模式）+ 回环 + Bearer + 会话/限速/审计/策略文件；`mcp_*` 9 条 IPC；**默认关闭**。设置页分区 UI 随前端重构落地 |
> | S5 操作+确认+stdio 桥 | ✅ 后端完成 | 31 工具（15 只读/11 操作/5 破坏性确认）+ `mcp://call` 归因事件 + 两阶段确认流（120s 超时即拒）+ `rmccc mcp` stdio 桥（`--launch`）。事件 payload `origin` 字段与 GUI 徽标/toast 随前端重构落地（当前以 `mcp://call` 替代归因数据源） |
> | S6 插件桥 | ✅ 后端完成 | `ai.chat` 能力（medium）+ 桥方法 `llm.chat`（限流 6/min、流式推送、密钥代持）；`plugin://push` 前端中继随前端重构落地 |
> | S7 旧面下线 | ✅ 完成 | `agent_*` 9 条移除、`/agent` 路由重定向、前端无引用 |
>
> **实现与设计 §4.2/§5.2 的已记录偏差**：
> 1. `ai_config_set` 三字段可选（留空沿用已存值）——与 §4.2 一致，已按此实现；
> 2. `mods.apply_plan` 工具入参只有 `{profile_id}`（plan 由最新扫描自动生成）——§5.2 表中
>    的 `plan?` 参数暂不开放（应用器总是重扫）；
> 3. 新增事件 `mcp://call`（操作类工具的 started/finished 归因）——设计 §8 未列出，
>    用于在不动下载管线 payload 的前提下支撑 GUI 归因徽标（S5 的 origin 字段方案延后）。

## 0. 一句话定位

**启动器不内建 Agent；启动器把自己变成工具。**
AI 编排交给外部成熟工具（ZCode 等，经 MCP 接入）；启动器内部只保留一个
**轻量 OpenAI 兼容 AI 服务**（供启动器自身与插件做简单智能功能），
以及一个 **MCP 服务器**（把启动器的配置与操作能力开放给外部 Agent），
所有外部发起的操作在 GUI 上**实时可见、可归因、可确认**。

## 1. 背景与动机

Phase 6 按"受控编排层"思路在启动器内实现了 Agent 执行循环、工具注册表、
审批代理与会话存储（`crates/rmccc-agent`，9 条 IPC，`/agent` 页面）。实践暴露的问题：

1. **重复造轮子**：对话循环、工具编排、上下文管理，外部 AI 工具（ZCode/Claude/Cursor）
   已经做得更好，且用户已经在用；
2. **重 UI 负担**：完整 Agent 需要 dedicated 页面、会话回放、审批队列——
   与启动器"干净简洁"的产品目标冲突；
3. **能力错配**：很多被归到"Agent 能力"下的功能（缺失前置检测、完整性体检）
   本质是**纯本地逻辑 + 网络请求**，根本不需要 AI。

新方向（用户决策，2026-10-23）：

- **控制权反转**：外部 AI 工具是"大脑"，启动器是"手"。启动器经 MCP 提供工具，
  外部 Agent 调用工具来读配置、装版本、下 Mod、启动游戏；
- **用户在场可视化**：外部 Agent 的每个操作在启动器 GUI 上实时呈现
  （下载进度条 + "正在由 Agent 执行"标识），破坏性操作弹确认卡；
- **内部轻 AI**：一个全局配置的 OpenAI 兼容端口（地址/密钥/自动检测模型/流式），
  供启动器内置功能（如启动失败一键日志分析）和插件调用，**不设独立 Agent UI**。

## 2. 设计原则

1. **轻干预**：启动器永远不打断用户去"代理"用户；外部 Agent 操作默认放行并可视化，
   只有破坏性操作才需要用户在 GUI 点确认。
2. **AI 是能力不是主体**：AI 调用 = 一次函数调用（发文本 → 拿流式回复），
   没有循环、没有工具注册表、没有会话状态机。
3. **不是什么都要 AI**：能用确定性逻辑解决的（依赖缺失、完整性、版本兼容）
   一律纯逻辑实现；AI 只用于"解释性"任务（日志分析、文案、建议）。
4. **一次配置全局使用**：AI 端口在设置页配置一次，启动器内置功能与插件共用同一凭据。
5. **外部操作 = 用户操作 + 归因**：MCP 工具调用走与用户点击**完全相同**的
   内部命令层与下载管理器（因此 GUI 天然可见），仅多一个 origin 标记与审计记录。

## 3. 总体架构

```
┌──────────────────────────────────────────────────────────────────┐
│  外部 AI 工具（ZCode / Claude Desktop / Cursor / 脚本）              │
│     │ MCP 客户端                                                   │
└─────┬───────────────────────────────────────┬──────────────────────┘
      │ ① stdio（工具直接拉起 `rmccc mcp`）      │ ② Streamable HTTP
      ▼                                        │   http://127.0.0.1:<port>/mcp
┌─────────────────┐  本机回环 + Bearer Token   │
│ rmccc mcp 桥进程  │──────────────────────────▶│
└─────────────────┘                            ▼
┌──────────────────────────────────────────────────────────────────┐
│ RMCCC 启动器进程（Tauri）                                          │
│                                                                  │
│  ┌───────────────┐   ┌────────────────┐   ┌───────────────────┐  │
│  │ MCP Server     │   │ 内部命令层       │   │ AI 服务(rmccc-llm) │  │
│  │ (JSON-RPC 2.0) │──▶│ (与 GUI IPC 同源) │   │ OpenAI 兼容客户端   │  │
│  │ 31 个工具       │   │ pipeline/config │   │ 流式/模型探测       │  │
│  │ 确认代理/审计    │   │ /download/...   │   └────┬──────────────┘  │
│  └───────┬───────┘   └───────┬────────┘        │    │               │
│          │                   │                 │    │               │
│          ▼                   ▼                 ▼    ▼               │
│  ┌────────────────────────────────────────┐  ┌──────────────────┐  │
│  │ 事件总线（download://、launch:// …）      │  │ 插件 UI 桥         │  │
│  │ payload 携带 origin(user/agent/plugin)  │  │ llm.chat 方法      │  │
│  └──────────────────┬─────────────────────┘  └──────────────────┘  │
│                     ▼                                            │
│              GUI：任务中心/进度条/确认卡/归因徽标/日志抽屉 AI 分析        │
└──────────────────────────────────────────────────────────────────┘
```

三个组成部分：

| 部分 | 归属 | 一句话 |
|---|---|---|
| AI 服务 | `rmccc-llm`（新 crate，纯 Rust 库） | OpenAI 兼容客户端：配置/探测模型/流式对话 |
| MCP 服务器 | `src-tauri` 内模块（复用内部命令层） | 把启动器能力以 31 个 MCP 工具开放，带归因/确认/审计 |
| GUI 呈现 | ui-design 07/10 文档 | 设置分区 + 任务中心归因徽标/确认卡 + 日志 AI 分析 |

## 4. 内部 AI 服务（`rmccc-llm`）

### 4.1 能力定义（对插件开发者承诺的"AI 端口基本能力"）

| 能力 | 形式 |
|---|---|
| 配置 | `base_url` + `model` 存配置 JSON（非敏感），`api_key` 存凭据保险库（ADR-009，service=`llm`） |
| 自动检测模型 | `list_models()` → `GET {base_url}/models`（OpenAI 兼容端点；输入地址后 UI 自动拉取模型下拉） |
| 输入输出 | `chat(messages, opts) -> ChatReply`（阻塞式，单轮） |
| **流式输出** | `chat_stream(messages, opts) -> Stream<Delta>`（SSE 解析，支持取消/超时） |
| 错误契约 | 统一 `LlmError`：网络/401/429/超时/内容截断，人话化文案由 UI 层做 |
| 脱敏 | 发送前 `redact`（复用 rmccc-agent 的 redact 模块：home→`~`、token 模式遮蔽） |

- 兼容目标：任何 OpenAI 兼容端（OpenAI / DeepSeek / GLM / Ollama `/v1` / LM Studio / OneAPI 中转）。
- 超时：首字节 30s、整轮 180s；取消 = drop stream。
- **明确不做**：多轮编排、工具调用（function calling 透传可选，v1 不暴露）、
  会话存储、Token 计费——都留给外部 Agent 或插件自己。

### 4.2 IPC 面（Tauri，供 GUI 与内置功能）

| 命令 | 入参 | 返回/事件 |
|---|---|---|
| `ai_config_get` | — | `{configured, base_url, model, has_key}` |
| `ai_config_set` | `{base_url?, api_key?, model?}` | `()`（key 入 vault） |
| `ai_models_probe` | `{base_url?, api_key?}` | `string[]`（设置页输入地址后自动检测；未传参用已存配置） |
| `ai_analyze_log` | `{profile_id?}` | `{analysis_id}`；事件 `ai://delta {analysis_id, text}` / `ai://done` / `ai://error`（流式） |
| `ai_analyze_cancel` | `{analysis_id}` | `()` |

- `ai_analyze_log` 是**内置参考消费方**：取该实例最近一次游戏会话日志尾部
  （launchStore 缓冲 + 日志文件兜底）→ redact → 组装分析提示词（崩溃/异常退出优先截取
  ERROR 附近窗口）→ `chat_stream` → 事件推送 GUI 流式面板（UX 详见 ui-design 07）。
- **数据外发提示**：每个 AI 消费功能首次使用时一次性确认
  「日志将发送至 {host}（你的 AI 服务）进行分析」，记忆偏好；端点域名来自用户配置。

### 4.3 插件消费（plugin-ui 桥扩展）

- 新桥方法 `llm.chat`（能力 `ai.chat`，风险 medium，需显式授予）：
  - 请求 `{messages: [{role, content}], stream?: bool}`；
  - 非流式：直接返回完整回复（≤512KiB 既有响应上限）；
  - 流式：立即返回 `{streamId}`，随后宿主推
    `{type:"llm.delta", streamId, text}` / `{type:"llm.done", streamId}` /
    `{type:"llm.error", streamId, message}`（复用桥的宿主→插件推送通道，与 theme/revoke 同机制）；
- 限流：每插件 6 次/分钟（防失控插件烧 key）；插件用**宿主配置的端点与密钥**，
  自己永远拿不到 key 本体（凭据经 `credentials.use:llm` 语义由宿主代持）；
- 既有能力目录追加：`ai.chat`（use host LLM service）。

### 4.4 不是 AI 的功能（划清边界）

| 功能 | 实现 | 说明 |
|---|---|---|
| 缺失前置检测 | `mods_dependency_scan`（纯本地：fabric.mod.json 依赖图 + 已装集合对比） | 原 rmccc-agent 扫描逻辑直接复用，去 Agent 化 |
| 一键补装缺失前置 | `mods_apply_plan`（Modrinth 搜索 → 版本匹配 → SHA1 下载） | 原 PlanApplier/ModrinthPlanApplier 逻辑复用 |
| 完整性体检/修复 | 既有 `instance_check_completion/repair` | 与 AI 无关 |
| 版本/loader 兼容判断 | 既有 modloader / version 逻辑 | 与 AI 无关 |

## 5. 外部 Agent 控制：MCP 服务器

### 5.1 传输与部署（两种接入方式）

**① Streamable HTTP（推荐，服务器跑在启动器进程内）**

- 端点 `http://127.0.0.1:<port>/mcp`，**仅回环**，默认关闭，设置页开启；
- 鉴权：`Authorization: Bearer <token>`（32 字节随机 hex，存 `<data>/mcp-token`，0600）；
- 端口：开启时自动挑选空闲端口（设置页显示），可固定；
- 协议：MCP（JSON-RPC 2.0）——`initialize` / `tools/list` / `tools/call` /
  `notifications/progress` / 会话头（`Mcp-Session-Id`）；
- 实现约束（体积红线）：基于既有依赖 `hyper`（reqwest 已引入，启用 server feature）
  自建轻量 JSON-RPC + SSE，**不引入 axum/tower 全家桶**；合入前跑 `cargo bloat`
  评估，strip 后主可执行须仍 < 10 MiB（ADR-011）。

**② stdio 桥（兼容"工具自己拉起 MCP 服务器"的主流集成方式）**

- `rmccc mcp` 子命令（src-cli）：stdin/stdout 说 MCP，把调用转发到①的 HTTP 端点
  （token 从数据目录读取，同机即同用户）；
- 启动器未运行时：返回明确错误「RMCCC 未运行，请先启动启动器」；
  `rmccc mcp --launch` 变体：先拉起 GUI 再连（等待就绪 ≤ 10s）；
- 适合 Claude Desktop / ZCode / 任何按 `command + args` 配置 MCP 的客户端。

**设置页一键复制配置**（部署便捷性）：

```jsonc
// 方式① HTTP（ZCode / Cursor 等支持 URL 型 MCP 的工具）
{ "mcpServers": { "rmccc": {
    "url": "http://127.0.0.1:47610/mcp",
    "headers": { "Authorization": "Bearer <在设置页复制的令牌>" } } } }

// 方式② stdio（Claude Desktop / ZCode 等支持 command 型 MCP 的工具）
{ "mcpServers": { "rmccc": {
    "command": "/path/to/rmccc", "args": ["mcp"] } } }
```

### 5.2 工具目录（31 个，`域.动作` 命名，输入输出均 JSON Schema 定义）

**只读（15，默认 allow）**：

| 工具 | 说明 |
|---|---|
| `system.status` | 版本、运行中游戏、活动下载概览 |
| `system.audit_recent {limit?}` | 最近操作审计（脱敏），Agent 自查历史 |
| `profiles.list` / `profiles.get {profile_id}` | 实例列表/详情 |
| `instances.list` | 版本仓库（磁盘 versions/） |
| `versions.available {release_type?}` | 可安装游戏版本（Mojang manifest） |
| `modloader.list {loader_type, game_version}` | loader 可用版本 |
| `mods.installed {profile_id}` | 已装 Mod（enabled/disabled） |
| `mods.dependency_scan {profile_id}` | **纯逻辑**依赖扫描：缺失必需/冲突/未解析 |
| `mods.search {query, game_version?, loader?, page?}` | Modrinth 搜索 |
| `mods.versions {mod_id}` | Mod 版本列表 |
| `instance.check_completion {profile_id}` | 完整性体检（工具描述注明耗时数十秒） |
| `java.detect` | Java 探测 |
| `settings.get` | 全局设置（**脱敏：密钥类字段永不返回**） |
| `logs.game {profile_id?, lines?}` | 最近游戏日志尾部（脱敏） |

**操作（11，默认 allow，全程 GUI 可见 + 归因）**：

| 工具 | 说明 |
|---|---|
| `profiles.create {name, game_version}` | 创建实例 |
| `profiles.update {profile_id, fields}` | 实例设置变更（可逆） |
| `game.install {name?, game_version, loader?}` | 编排：建实例+装版本(+loader)，对应 GUI 安装向导 |
| `modloader.install {profile_id, loader_type, loader_version}` | 安装 loader |
| `mods.install {profile_id, mod_id, version_id?}` | 装 Mod（缺 version_id 自动选兼容推荐版） |
| `mods.install_local {profile_id, path}` | 从本地路径装 Mod |
| `mods.toggle {profile_id, mod_filename, enabled}` | 启停 Mod |
| `mods.apply_plan {profile_id, plan?}` | 补装缺失前置（缺 plan 时按最新扫描自动生成） |
| `instance.repair {profile_id}` | 修复缺失文件 |
| `downloads.control {batch_id, action}` | 暂停/恢复/取消下载 |
| `launch.start {profile_id}` | 启动游戏 |

**破坏性（5，默认 confirm——必须用户在 GUI 确认卡放行）**：

| 工具 | 说明 |
|---|---|
| `launch.kill {profile_id}` | 停止运行中游戏 |
| `profiles.delete {profile_id}` | 删除实例 |
| `instances.remove {version_id}` | 删除版本文件 |
| `mods.remove {profile_id, mod_filename}` | 删除 Mod 文件 |
| `settings.set {fields}` | 修改全局设置 |

规则：

- 每工具的 `risk` 与默认策略可被设置页**逐工具覆盖**（allow / confirm / deny）；
- 工具描述（description）即外部 Agent 的使用说明书，必须写清参数语义与副作用；
- 未列出的能力（插件管理、凭据、AI 服务本身）**不开放 MCP**。

### 5.3 确认流（破坏性操作的两阶段协议）

```
外部 Agent                     MCP Server                    GUI（任务中心）
    │ tools/call(profiles.delete)  │                              │
    │─────────────────────────────▶│ 创建确认记录 Confirmation      │
    │                              │ (复用 TaskSnapshot 形态:       │
    │ ◀── progress 通知「等待用户确认」│  kind=mcp:profiles.delete,    │
    │                              │  status=proposed, 120s 倒计时) │
    │                              │──────确认卡 + toast + 徽标────▶│
    │                              │              用户点 [允许]/[拒绝] │
    │                              │◀─────────resolve─────────────│
    │ ◀── result {status:"ok", …}  │（允许→执行；拒绝→               │
    │    或 {status:"denied"}       │  {status:"denied", reason?}；  │
    │    或 {status:"timeout"}      │  超时→denied）                 │
```

- 复用 Phase 6 已验证的两阶段语义（proposed→approved→applied/failed，防重放），
  但**主体从"内建 Agent 工具"换成"MCP 工具调用"**，UI 从 Agent 页审批队列换成任务中心确认卡；
- 确认卡显示：来源会话（哪个外部工具）、工具名、参数人话化、影响（"将删除实例 X 的配置"）、
  倒计时；允许/拒绝即时生效并写审计。

### 5.4 归因与 GUI 实时可见性

- 内部命令层所有操作携带 `Origin { User, Agent{session}, Plugin{plugin_id}, Internal }`；
- **既有事件 payload 增加 `origin` 字段**（additive，不破坏旧消费者）：
  `download://progress|complete|error`、`launch://started|output|exited`、`modloader://progress`；
- GUI 呈现（详见 ui-design 07/10）：
  - 任务中心/主页下载卡：origin=Agent 的任务带「Agent」徽标；
  - 每次 Agent 发起操作：toast「外部 Agent（ZCode）开始下载 1.20.4…」；
  - 新事件：`mcp://session {kind: connected|disconnected, session}`、
    `mcp://confirmation {kind: requested|resolved, id}`；
- 窗口最小化时：任务指示器红点提示待确认数（不做系统通知，v1 简化）。

### 5.5 会话、审计与安全

| 项 | 设计 |
|---|---|
| 开关 | MCP 服务**默认关闭**，设置页显式开启；总开关关闭即断开全部会话 |
| 监听 | 仅 127.0.0.1；绑定非回环地址（局域网远程控制）为 v2 议题，v1 明确不做 |
| 令牌 | Bearer token；「重新生成」使全部会话立即失效；token 文件 0600 |
| 会话 | initialize 时登记（MCP clientInfo 名称，如 "ZCode"/"claude"）；设置页列出在线会话 + 「断开」；限并发 ≤ 4 |
| 限速 | 每会话 60 次调用/分钟；慢工具（check_completion）并发 1 |
| 审计 | 每次 tools/call 写审计（工具、脱敏参数、结果、会话），复用 ADR-010 的脱敏/目录/保留规则；设置页「最近 Agent 操作」视图 |
| 密钥 | MCP 层**永不**返回任何密钥；`settings.get/set` 字段白名单（镜像/并发/目录等，不含凭据） |

## 6. 边界与非目标

1. **不做内建 Agent**：AgentService 执行循环、启动器内工具注册表（对外）、会话存储退役；
2. **不做局域网/远程 MCP**：v1 仅本机回环；
3. **插件不经 MCP**：MCP 是宿主级能力，插件能力仍走插件平台（桥/能力目录）；
   未来若插件要注册 MCP 工具，另立 ADR；
4. **CLI 并行存在**：`rmccc` CLI 仍是脚本化控制方式（profile/download/launch 子命令），
   MCP 与 CLI 共享同一命令层，不重复实现；
5. **AI 服务不做编排**：只有 chat/stream/probe，没有循环与工具。

## 7. 从 Phase 6 迁移（代码层）

| 现状 | 处置 |
|---|---|
| `rmccc-agent::mods`（依赖扫描、PlanApplier、Modrinth 执行器） | **保留并上移**：扫描/补装改为宿主级能力（IPC `mods_dependency_scan` / `mods_apply_plan`，MCP 工具 `mods.dependency_scan/apply_plan`），去 Agent 语义 |
| `rmccc-agent::redact` | 保留：AI 服务与 MCP 日志工具共用 |
| `rmccc-agent::audit` | 保留并扩展：记录 MCP 工具调用与 AI 调用（origin 维度） |
| `rmccc-agent::session`（会话/任务存储） | **会话存储退役**；TaskSnapshot 结构保留，改作 MCP 确认记录（kind=`mcp:<tool>`） |
| `rmccc-agent::runtime`（AgentService/ApprovalBroker） | 退役：编排职责交外部 Agent；确认职责简化进 MCP 服务器（§5.3） |
| `rmccc-agent::tools`（ToolRegistry） | 拆解：内置工具描述/Schema 定义迁往 MCP 工具目录；`invoke_approved` 防重放校验逻辑迁移到确认流 |
| IPC `agent_*` 9 条 | 移除，替换为 `ai_*` 5 条 + `mcp_*` 9 条 + `mods_dependency_scan/apply_plan` 2 条（86→93） |
| `/agent` 页面（Agent.tsx） | 移除；能力归入设置页「AI 与外部控制」分区 + 任务中心（见 ui-design 07） |
| ADR-010 | 审计/脱敏/目录权限/保留策略**继续有效**；会话部分废止（ADR-013 标注） |

## 8. 新 IPC 命令与事件汇总

命令（16 条新增，9 条 agent 命令移除）：

- AI：`ai_config_get/set`、`ai_models_probe`、`ai_analyze_log/cancel`
- MCP：`mcp_status`、`mcp_set_enabled`、`mcp_regenerate_token`、`mcp_tools_list/set`、
  `mcp_sessions_disconnect`、`mcp_confirmations_list`、`mcp_confirmation_resolve`、`mcp_recent_calls`
- Mod 纯逻辑：`mods_dependency_scan`、`mods_apply_plan`

事件（新增 5 + 既有 10 条 payload 扩展 origin）：

- `ai://delta` / `ai://done` / `ai://error`（analysis_id 键）
- `mcp://session`、`mcp://confirmation`

## 9. 实施切片（建议顺序）

| 切片 | 内容 | 验收 |
|---|---|---|
| S1 | `rmccc-llm` crate + `ai_*` IPC + 设置页 AI 分区（地址→自动探测模型→流式测试） | 对 OpenAI 兼容端点完成一次流式对话 |
| S2 | 日志一键分析（`ai_analyze_log` + 日志抽屉流式面板 + 外发确认） | 人为制造启动失败 → 分析结论可读 |
| S3 | `mods_dependency_scan/apply_plan` IPC + 实例详情「检查缺失前置/一键补装」 | 缺 indium 实例补装成功（纯逻辑，无 AI） |
| S4 | MCP 服务器：HTTP 传输 + 只读 15 工具 + 会话/令牌/审计 + 设置页 MCP 分区 | ZCode 经 MCP 列实例、查依赖 |
| S5 | MCP 操作 11 工具（归因/事件 origin/GUI 徽标/toast）+ 破坏性 5 工具确认流 + stdio 桥 | 外部 Agent 装一个 Mod 并在 GUI 看到进度；删除实例弹确认卡 |
| S6 | 插件桥 `llm.chat`（含流式推送）+ `ai.chat` 能力 + 限流 | server-dashboard 样例插件调用 AI 成功 |
| S7 | agent_* 命令与 `/agent` 页下线、ADR/文档收尾 | 命令注册表 93 条，门禁全绿 |

每切片独立可合入；S4 之前 UI 不出现任何 MCP 入口（避免空占位）。

## 10. 验收标准（总）

1. 外部闭环：ZCode 配好 MCP → 「帮我把 sodium 实例补上缺失前置并启动」→
   用户在 GUI 看到依赖扫描结果 toast → 下载进度条（Agent 徽标）→ 破坏性操作无 → 游戏启动；
2. 确认闭环：同一会话请求删除实例 → GUI 确认卡 → 拒绝 → MCP 返回 denied，无文件变化；
3. AI 闭环：拔掉网络/错 key → 人话错误；日志分析流式呈现且路径脱敏；
4. 审计闭环：`system.audit_recent` 与设置页视图一致，无明文凭据/绝对 home 路径；
5. 体积门禁：hyper server feature 合入后 strip 主可执行 < 10 MiB（ADR-011）；
6. 旧 Agent 面：`/agent` 路由 404→重定向设置页；agent_* IPC 移除后前端无引用。
