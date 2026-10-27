# RMCCC 插件平台架构与跨平台开发指南

> **文档类型：** 目标架构 / 技术决策基线 / Agent Vibe Coding 指南  
> **项目：** RMCCC（Rust Minecraft Core Client）  
> **文档基准日期：** 2026-10-20  
> **主要目标平台：** Windows x64、macOS Apple Silicon、macOS Intel；Linux x64 作为现有规划中的附加目标继续维护，除非项目负责人通过 ADR 明确调整。  
> **主要读者：** 后续参与项目的 AI 编码 Agent、维护者、插件 SDK 开发者。  
> **状态：** 目标设计指南，不代表本文中的目标模块、插件运行时和功能已经存在于当前代码中。

---

## 0. 本文的使用方法与权威性

本文为后续插件平台方向的**目标架构基线**，用于约束后续设计和 Vibe Coding。它需要被纳入仓库文档体系，但在正式取代任何现行规则之前，维护者应检查并更新 `AGENTS.md`、`ARCHITECTURE.md` 中的引用和冲突条款。

### 0.1 开始任何开发任务前，Agent 必须做的事情

1. 阅读仓库根目录 `AGENTS.md`（如存在）和 `docs/AI_EDITING_RULES.md`。
2. 阅读 `docs/ARCHITECTURE.md`，理解现存实现；阅读本指南，理解目标方向。
3. 阅读 `docs/audits/AUDIT_2026-10-20.md`、`docs/PROJECT_STATUS.md` 和 `docs/DEVELOPMENT_PLAN.md`，确认当前代码事实、已知问题和阶段状态。
4. 执行 `git status --short`，确认工作区状态；不得覆盖用户未提交修改。
5. 通过源码和 Cargo/npm 配置确认问题，不允许把本文中的目标模块当作已经实现的代码。
6. 先给出小范围执行计划和验收方法，再修改；一轮只解决清楚定义的一组问题。
7. 完成后运行与改动相匹配的验证，报告实际命令、成功/失败结果、未能运行的项目及原因。不得把“代码看起来正确”写成“测试通过”。

### 0.2 文档与代码的优先级

- 对**当前实现事实**：源码、配置和实际命令输出优先于旧文档的状态叙述。
- 对**目标架构**：本指南是目标设计草案；它不能覆盖项目负责人明确批准的决策，也不能自行把重大架构建议变成事实。
- 对安全、权限和跨平台兼容：采用更严格的约束；如现有代码与这些约束冲突，先报告风险，再提出最小迁移方案。
- 当本指南与 `ARCHITECTURE.md` 冲突时，Agent 不得静默选择其中一份。应指出冲突，提出 ADR；被批准后同步两份文档。
- 旧审计和整改文档用于问题溯源，不应直接当成当前 TODO 清单。

### 0.3 不允许的工作方式

Agent 不得：

- 一次性重写所有 crate、所有 IPC、全部前端或整个启动流程。
- 为了“插件化”而把所有现有功能立即改成外部可卸载插件。
- 让第三方插件直接持有 `CoreContext`、账户 Token、任意本地路径或原始 Tauri `AppHandle`。
- 把插件 JS 直接 `import()` 到可信主窗口中并赋予主窗口的全部权限。
- 仅在 macOS 上通过测试就宣称 Windows 可用。
- 用 `cfg!(windows)` 作为唯一平台分支设计手段；需要平台 API 的功能应由明确的模块/trait 封装。
- 把 `.unwrap()`、`expect()`、`panic!()` 用于生产路径以隐藏错误；遵守现有 clippy 门禁。
- 未经批准扩大依赖、引入庞大运行时或更改插件 API 契约。
- 对签名私钥、Token、LLM API Key、认证响应体或敏感路径进行日志输出。

---

# 1. 项目背景与当前起点

## 1.1 当前架构事实（以 2026-10-20 审计快照为准）

当前项目已经具备：

- Rust Cargo Workspace：10 个业务 crate，加 `src-tauri` 和 `src-cli`，共 12 个成员。
- Tauri 2 + SolidJS/Vite 桌面应用，CLI 与 GUI 共享大部分业务模块。
- Rust 业务层约 31,260 行，前端约 4,279 行。
- 认证、下载、版本索引、资源安装、Java 检测、游戏启动、Profile 设置继承、多个 Mod Loader 的较完整实现。
- Modrinth 查询、本地模组启用/禁用/删除相关功能。
- TS 类型桥 `ts-rs`，将部分 Rust 数据类型导出为 TypeScript。
- CI lint 门禁和 workspace 测试基础。

关键业务 crate：

| crate | 当前职责 |
|---|---|
| `rmccc-error` | 统一错误和错误码 |
| `rmccc-config` | 路径、配置、schema 迁移、防抖写盘、日志基础设施 |
| `rmccc-auth` | 微软 Device Flow、离线账号、Token vault、账户仓库 |
| `rmccc-download` | 并发下载、重试、Range 续传、SHA1 校验和缓存 |
| `rmccc-version` | Mojang 版本元数据、版本仓库、libraries/assets、完整性检查 |
| `rmccc-launch` | Java 探测、启动参数、natives、进程管理 |
| `rmccc-modloader` | Loader 版本与安装流程 |
| `rmccc-mods` | 模组源接口及 Modrinth 实现 |
| `rmccc-profile` | Profile JSON 存储与全局/实例设置继承 |
| `rmccc-core` | `CoreContext`、Pipeline、旧目录迁移及会话相关协调 |
| `src-tauri` | 桌面命令、事件、状态和 WebView 桥接 |
| `src-cli` | CLI 命令面、REPL、输出信封和业务门面 |

## 1.2 已知结构性欠账

下面是已有审计指出的事实，开始工作前必须复核其是否仍成立：

1. `rmccc-core` 中部分 Pipeline 函数尚未成为统一业务入口，Tauri 与 CLI 存在重复安装、修复或启动编排。
2. `profile` 与 `instance` 两套实体概念并存，部分 `instance_*` 命令面向版本目录 ID，而不是用户可见的 Profile。
3. Loader 安装存在旧 `ModLoaderProvider` 和新 `DependencyManager` 两套路径。
4. `src-cli/src/context.rs` 是承担多个域职责的大型上下文文件。
5. 前端路由的大部分命令调用绕过了 `api.ts` 分域 wrapper，直接以命令名字符串调用 `safeInvoke`，削弱了类型契约。
6. IPC 命令层测试非常少，前端测试缺失；历史测试结果不能替代当前代码的重跑结果。
7. `natives` 解压路径需要完整的 zip-slip 防护。
8. 当前审计记录的 GUI release 文件未剥离大小超过项目既有 `<10 MB` 目标；本地 `strip` 环境曾失败，体积目标尚未证实。
9. 最新审计指出本地 Rust 工具链路径和 Cargo 缓存发生环境回归。必须重新验证，不要直接引用旧测试结论。

## 1.3 重要区分：当前已实现 vs 未来目标

| 能力 | 当前起点 | 未来目标 |
|---|---|---|
| Minecraft 下载安装与启动 | 核心流程已有实现和历史实机记录 | 收敛为稳定、可测试的 Core Service API |
| Mod 管理 | Modrinth 与本地文件管理部分已实现 | 通用 Mod Source、依赖计划、可扩展来源和安全安装服务 |
| UI | Dashboard、登录、设置、实例设置等静态路由 | 注册式卡片、可定制布局、主题、可扩展导航和页面 |
| 插件系统 | 目前不能视为已有完整的第三方运行时 | Manifest、权限、生命周期、贡献注册表、隔离运行时、SDK |
| Agent | 作为未来产品方向 | 模型适配、工具注册、任务计划、权限审批、可审计执行 |
| 跨平台 | 文档规划 macOS/Windows/Linux；需复核当前 CI | macOS Apple Silicon/Intel、Windows x64 必须通过原生构建与验证；Linux x64 继续保持目标 |
| 发布 | workflow 已有基础配置，部分签名/公证/体积/E2E 未闭环 | 三平台产物、签名、升级、安装与卸载均通过真实验收 |

**禁止把尚未实现的插件体系直接加入现有启动主链路。** 应先定义服务契约，后通过适配层接入，再逐阶段开放给插件。

## 1.4 当前开发环境现实与跨平台策略（必须遵守）

> **本节是当前阶段的工作约束，优先级高于本文档其余关于"三平台必须验证"的描述。**
> 当本节与其余跨平台验收条款冲突时，按本节执行；其余条款作为"待 CI 原生 runner 触达后"的目标保留。

**当前事实（2026-10-20）：**

- 本项目**当前仅在本机 macOS（Apple Silicon）环境开发**。本机是唯一日常开发与手测机器。
- 本机**不进行跨平台交叉编译**：不在 macOS 上为 `x86_64-pc-windows-msvc` 等目标构建，也不在本机模拟 Windows 运行时。
- 本机**不进行 Windows 平台实际测试**：不宣称 Windows 可用、不把"macOS 编译通过"当作 Windows 验收、不在交付报告中把 Windows 标为已验证。
- Linux x64 同样不在本机实测，作为既有规划目标保留。

**架构层面必须保留的能力（即使当前不实测）：**

- 代码结构必须保持全平台可移植：平台差异集中在**平台适配层**（见 §5.7），领域 crate 与 Core Service 不得内联 `#[cfg(target_os=...)]` 执行逻辑。
- 平台特定能力（文件系统原子写入/权限/rename/锁、进程树终止与优先级、凭据存储、shell/打开 URL、Java 探测、安装器/更新、原生打包）一律由对应平台适配器承担具体执行；启动器核心只定义 trait 与 DTO，不承担具体执行。
- 任何新增系统依赖必须在 §5.3 记录 macOS/Windows/Linux 三侧安装步骤，即使当前只在 macOS 验证。

**验收与报告规则：**

- 各阶段验收中"Windows native CI 编译和测试通过"等条款，当前一律记为 **deferred（推迟到原生 Windows runner/CI 触达后）**，并标记为 release-blocked；不得在本机环境下填 pass。
- 本机可执行的验收限定为：macOS ARM64 的 `cargo fmt/clippy/test`、`pnpm typecheck/build`、`pnpm tauri dev` 手测、源码审计与单测。
- 凡涉及平台特有代码的改动，必须在交付报告中显式标注"当前缺口：仅 macOS 验证，Windows/Linux 待 CI"，不得隐瞒为完整通过。
- 不得因本机工具链或环境限制而提高体积红线或放宽安全约束。

---

# 2. 产品目标、非目标和基本原则

## 2.1 产品定位

RMCCC 是一个跨平台的 Minecraft 桌面应用平台：

1. 自带一套可靠、可复用的 Minecraft Core Engine。
2. 由官方内置功能和可选扩展共同构成用户可见的启动器。
3. 允许用户通过插件扩展 Mod 来源、兼容性检查、服务器信息、主题、Dashboard 卡片、工具页面及 Agent 工具。
4. 允许用户将 LLM 接入游戏管理工作流，完成环境检查、计划生成、模组建议与经授权执行的多步骤任务。
5. 让插件开发者可以独立开发、测试、打包和发布扩展，而无需为每个功能修改或重新编译主程序。
6. 在 Windows 与 macOS 上具有一致的功能语义、数据格式和升级体验，而不是只做到“代码能够编译”。

## 2.2 非目标（第一阶段不做）

- 不把所有核心 Rust crate 都立即变为独立动态插件。
- 不允许任意插件直接执行系统命令、任意读写文件或访问账户密钥。
- 不在首版构建完整的公开插件市场、评分系统、支付系统和社交平台。
- 不要求首版实现插件热重载；安全启停和进程重启加载优先。
- 不要求首版支持所有形式的 UI 框架。官方优先提供基于 Web 标准的 SDK 和声明式组件。
- 不把模型输出视为可信指令；Agent 的工具调用仍受宿主权限策略控制。
- 不为追求微内核纯粹性而牺牲游戏安装/启动主链路的稳定性。

## 2.3 设计原则

1. **Core 是业务数据和文件操作的唯一权威入口。**
2. **插件依赖公开契约，不依赖内部实现。**
3. **GUI、CLI、插件与 Agent 使用同一组 Core Service。**
4. **UI 扩展能力和系统权限分离。** 注册卡片不自动获得文件、网络或启动权限。
5. **默认最小权限。** 每项敏感能力都需要声明、授权、校验和可撤销记录。
6. **跨平台优先采用统一语义。** 平台差异在边界层处理，不散落到业务逻辑中。
7. **所有外部输入均不可信。** 包文件、网络响应、插件 Manifest、LLM 输出、Mod 文件和下载 URL 均须验证。
8. **失败隔离。** 插件或 Agent 出错不得阻断启动器基本功能或破坏用户游戏数据。
9. **迁移可恢复。** 配置、插件和布局升级需要 schema version、备份或回滚策略。
10. **小步可验证。** 每阶段都形成运行的增量版本，禁止跨多层的大规模无验收重构。

---

# 3. 目标系统架构

## 3.1 逻辑架构图

```text
┌────────────────────────────────────────────────────────────────┐
│                       Presentation Layer                       │
│ SolidJS Shell · Dashboard Layout · Pages · Theme · Agent UI     │
└───────────────────────────────┬────────────────────────────────┘
                                │ typed host bridge
┌───────────────────────────────▼────────────────────────────────┐
│                    Application / Host Layer                    │
│ Command adapters · IPC gateway · Event gateway · Task manager  │
│ Permission broker · Extension manager · UI contribution host    │
└───────────────────────────────┬────────────────────────────────┘
                                │ stable internal service API
┌───────────────────────────────▼────────────────────────────────┐
│                       Core Service Layer                       │
│ GameService · InstanceService · ModService · DownloadService   │
│ AuthService · SettingsService · JavaService · ProcessService   │
│ Install plans · Repair plans · Validation · Persistence         │
└───────────────────────────────┬────────────────────────────────┘
                                │
┌───────────────────────────────▼────────────────────────────────┐
│                         Domain Engines                         │
│ rmccc-version · rmccc-launch · rmccc-download · rmccc-auth     │
│ rmccc-modloader · rmccc-mods · rmccc-profile · rmccc-config    │
└───────────────────────────────┬────────────────────────────────┘
                                │
┌───────────────────────────────▼────────────────────────────────┐
│                    Platform Adapter Layer                      │
│ paths · credentials · process control · shell/open URL · Java   │
│ installer/update · filesystem durability · native packaging     │
└────────────────────────────────────────────────────────────────┘

Optional Extension Runtime (separate trust boundary):
  Manifest + Declarative Extensions + Sandboxed UI + WASM/Process
  Plugin SDK + Agent Tools + External Providers + Extension Bundles
```

以上为目标设计。它不是要求所有层分别拥有一个 crate；层的职责和依赖方向比 crate 数量更重要。

## 3.2 依赖方向

允许的依赖方向：

- 领域 crate 可以依赖 `rmccc-error` 和稳定基础类型，但不得依赖 Tauri、SolidJS 或 UI 代码。
- Core Service 层可以组合现有领域 crate。
- Tauri、CLI、Agent Host 和插件宿主都是服务消费者，不得分别复制游戏业务编排。
- 插件 API 只暴露稳定 DTO、枚举、版本规则、事件名称和能力接口；不能 re-export `CoreContext`、内部 repository 或原生句柄。
- 平台实现放在 `platform` / trait adapter 边界中；领域逻辑不通过到处散落的 `#[cfg(target_os = ...)]` 分叉运行。
- 插件只能通过 Host Broker 调用授权后的能力，不可直接访问 Core 内部对象。

## 3.3 建议的未来 Workspace 模块（不是立即全部创建）

```text
crates/
  rmccc-error/             # 现有
  rmccc-config/            # 现有
  rmccc-auth/              # 现有
  rmccc-download/          # 现有
  rmccc-version/           # 现有
  rmccc-launch/            # 现有
  rmccc-modloader/         # 现有
  rmccc-mods/              # 现有
  rmccc-profile/           # 现有
  rmccc-core/              # 现有；逐步收敛为稳定服务与编排层

  rmccc-platform/          # 将来：平台适配层。承载文件系统耐久、进程控制、凭据、
                           # shell/open、Java 探测、安装器/更新、原生打包等平台特定
                           # 执行；领域 crate 与 Core Service 只依赖其 trait，不内联 #[cfg] 执行
  rmccc-plugin-api/        # 将来：与运行时无关的版本化契约/DTO
  rmccc-plugin-host/       # 将来：Manifest、registry、生命周期、依赖/权限
  rmccc-plugin-runtime/    # 将来：WASM 或独立进程执行的抽象，等 ADR 通过再建
  rmccc-plugin-ui-api/     # 将来：UI 贡献和 host bridge 的协议类型
  rmccc-agent-api/         # 将来：模型、工具、上下文、审批接口
  rmccc-agent/             # 将来：会话、计划、工具执行和审计

src-tauri/                 # 桌面 shell 与 IPC adapter
src-cli/                   # CLI adapter，使用同一 Core Service
src/                       # SolidJS shell、dashboard、插件 UI 宿主
plugin-sdk/                # TypeScript SDK、示例和开发工具
schemas/                   # 公开 JSON Schema、协议快照
examples/plugins/          # 官方插件样例
```

可采用不同目录名，但职责应保持清楚。没有具体消费者和接口边界前，不要只为“看起来模块化”而拆 crate。

## 3.4 当前 `CoreContext` 的定位

`CoreContext` 保留为内部依赖注入容器。它可以持有共享 HTTP client、仓库、下载管理器和 provider，但不对外暴露给第三方。

对外服务应使用窄接口，例如：

- `GameService`：版本列表、安装规划、安装、完整性检查、修复和启动请求。
- `InstanceService`：Profile 列表、读取、设置解析、更新、删除策略。
- `ModService`：已装 Mod、来源搜索、版本查询、依赖解析、安装计划、启用/禁用/移除。
- `DownloadService`：提交任务、获取状态、暂停、恢复、取消与进度订阅。
- `AuthService`：获取有限的账号元数据、选择账号、登录/刷新工作流；绝不将刷新 Token 直接返回给插件。
- `SettingsService`：读取受限设置视图和更新允许的字段。
- `JavaService`：检测、识别版本和验证可执行文件。
- `AgentService`：注册/执行经授权的工具，记录会话和任务状态。

初期可以将这些 trait / facade 保留在 `rmccc-core`，而不是立刻拆出每一个单独 crate。

---

# 4. Core Service API 设计规范

## 4.1 API 分成三类

### A. 查询（Query）

读取版本、Profile、Mod、Java、下载状态和日志尾部。查询不能意外修改用户数据；必要的索引刷新应使用明确命名的命令。

### B. 计划（Plan）

返回尚未执行的操作方案，如 `InstallPlan`、`RepairPlan`、`ModInstallPlan`。计划应描述目标、将要新增/覆盖/删除的文件、下载资源、依赖变化、兼容性结论和风险。

### C. 执行（Command）

执行计划或受控操作。执行前必须重新校验计划所依赖的版本/实例状态，防止旧计划在用户状态变化后被盲目应用。

## 4.2 推荐关键 DTO

下面是概念级结构；实际字段需通过设计和测试定稿。

```text
InstanceSummary
ResolvedInstanceSettings
GameVersionSummary
InstalledMod
ModSearchQuery
ModSearchPage
ModDependencyEdge
CompatibilityReport
DownloadTaskSummary
InstallPlan
RepairPlan
OperationResult
PermissionRequest
PluginDescriptor
PluginRuntimeStatus
DashboardWidgetDescriptor
AgentToolDescriptor
```

DTO 原则：

- 使用稳定、可序列化的结构，不泄露 `PathBuf` 的平台内部细节或原生对象句柄。
- 对文件路径尽量传递宿主生成的 `PathId` / 资源句柄，而不是让插件传任意路径。
- 对可能不存在的数据使用 `Option` 或带状态的枚举，不把空字符串当成多义标记。
- 对长任务返回 `taskId`，通过查询与事件更新状态；不能长期占用 UI IPC 请求等待整个下载。
- 公开结构有 `schemaVersion` 或可说明的兼容策略；破坏性字段变更必须提高 API major 版本。

## 4.3 统一任务模型

游戏安装、修复、Mod 安装、Loader 安装及 Agent 工作流应尽量采用统一任务模型：

```text
TaskId
TaskKind
TaskState = queued | validating | running | waiting_for_approval |
            pausing | paused | cancelling | succeeded | failed | cancelled
Progress = completed_units / total_units + optional message
TaskResult
TaskError
CreatedAt / UpdatedAt
Initiator = user | plugin:<id> | agent:<session-id> | cli
```

需要支持取消、超时、退出时清理、重启后的状态恢复策略和失败可诊断性。是否支持暂停必须由具体任务声明；不支持暂停的任务不能伪装成可暂停。

## 4.4 错误契约

统一使用机器可读错误码和面向用户的消息分离：

```json
{
  "code": "plugin_permission_denied",
  "message": "The extension is not allowed to install mods.",
  "retryable": false,
  "details": {
    "extensionId": "top.example.extension",
    "capability": "mods.install"
  }
}
```

不要把 Rust Debug 字符串、绝对用户路径、密钥或 HTTP 认证响应直接当成用户消息返回。日志可以包含更细节信息，但必须脱敏。

## 4.5 事件契约

统一事件 envelope：

```json
{
  "eventVersion": 1,
  "eventId": "evt_...",
  "type": "download.progress",
  "timestamp": "2026-10-20T12:00:00Z",
  "source": { "kind": "core", "id": "download" },
  "taskId": "task_...",
  "payload": {}
}
```

要求：

- 事件名小写、域前缀清楚、命名有文档。
- 事件只是状态通知，不是唯一事实来源；客户端漏收事件后应能够通过查询状态恢复。
- 事件订阅必须返回取消订阅句柄；插件关闭、页面卸载或权限撤销后需要清理订阅。
- 事件必须限制 payload 大小、发射频率和日志敏感字段。
- 不把任意插件 event 名映射成可直接执行的宿主命令。

---

# 5. 跨平台目标与工程规范

## 5.1 目标平台矩阵

| Target | Rust target triple | 级别 | 必须验证的内容 |
|---|---|---|---|
| macOS Apple Silicon | `aarch64-apple-darwin` | 必须 | debug/release 构建、`.app`、DMG、Apple Silicon 实机启动、文件路径、Java 检测、进程关闭、代码签名/公证流程 |
| macOS Intel | `x86_64-apple-darwin` | 必须纳入目标；实际可用性需 CI runner/构建环境确认 | 对应架构构建与真实执行测试，插件包架构声明和 updater target 正确 |
| Windows x64 | `x86_64-pc-windows-msvc` | 必须 | 原生 Windows 构建、NSIS 或 MSI、WebView2、路径/进程/杀进程、文件锁、启动 Java、安装/卸载、Windows Defender/签名验证 |
| Linux x64 | `x86_64-unknown-linux-gnu` | 按现有项目规划继续维护 | 原生构建、系统依赖、文件权限、Java/进程、目标发行包烟测 |

**重要：** 不可把在 Apple Silicon 上交叉编译成功当作 Windows 验收。Rust 官方文档指出，非 Windows 主机到 `*-windows-msvc` 的交叉编译并非受支持的标准路径；Windows 目标必须由原生 Windows runner/机器进行构建和测试。macOS 产物则应在 macOS runner 构建和签名。参考：[Rust Windows MSVC target](https://doc.rust-lang.org/stable/rustc/platform-support/windows-msvc.html)、[Rust Apple Darwin targets](https://doc.rust-lang.org/rustc/platform-support/apple-darwin.html)。

**当前各平台本地验证状态（服从 §1.4）：**

| Target | 当前本地状态 | 目标状态 |
|---|---|---|
| macOS Apple Silicon | 本机开发与手测环境；`cargo fmt/clippy/test`、`pnpm typecheck/build`、`pnpm tauri dev` 可执行 | 保持 |
| macOS Intel | 本机不构建（Apple Silicon 单架构）；推迟到 macOS Intel/通用 CI runner | 原生构建 + 实测 |
| Windows x64 | 本机**不**交叉编译、**不**实测；推迟到原生 Windows CI runner | 原生构建 + 烟测 + 签名 |
| Linux x64 | 本机不实测；推迟到 Linux CI runner | 原生构建 + 烟测 |

> 当前阶段所有非 macOS Apple Silicon 的"必须验证"项一律记为 **deferred / release-blocked**，不得在本机填 pass。架构与适配层仍须按全平台目标组织代码（见 §5.7、§12），不得在领域逻辑中硬编码单一平台行为。

## 5.2 “跨平台兼容”的定义

不能只以 `cargo check` 成功衡量。每个平台至少要达到：

1. Rust 代码在原生平台编译通过。
2. 前端生产构建通过。
3. 安装包生成，签名/公证流程符合该平台发布方式（密钥未配置时只能标记为 release-blocked）。
4. 应用正常启动并首次创建数据目录。
5. 登录/离线账号、版本查询、下载安装、Java 探测、启动和停止游戏的关键流程完成烟测。
6. 配置和 Profile 可读写，路径含空格及非 ASCII 字符时工作正常。
7. 插件安装、禁用、恢复、升级失败回滚至少有自动化测试。
8. 打包后插件宿主/资源能正确定位，不依赖仓库源码相对路径。
9. 发布文件在干净环境中可安装、启动和卸载。
10. 发现平台不支持的能力时明确返回 `unsupported_on_platform`，不能静默失败。

## 5.3 开发环境策略

> 受 §1.4 约束：当前阶段本机仅 macOS 开发，不在本机做跨平台交叉编译或 Windows/Linux 实测。下列条款中"必须"指目标要求，"当前"指本机可执行范围。

- macOS 是**当前唯一**日常开发与手测环境：使用稳定 Rust toolchain（仓库 `rust-toolchain.toml` 锁定 `stable` + `rustfmt`/`clippy`）、Node/pnpm 锁文件及 Xcode Command Line Tools；不要依赖固定的 Homebrew Cellar 路径，也不要依赖本机 `~/.cargo` 缓存持久存在。
- **当前不在本机执行跨平台交叉编译**：不为 `x86_64-pc-windows-msvc`、`x86_64-apple-darwin`、`x86_64-unknown-linux-gnu` 在 macOS 上构建或跑测。这些目标的构建与测试推迟到对应原生 CI runner。
- Windows 验证使用原生 Windows 机器或 GitHub Actions Windows runner；构建依赖 Microsoft C++ Build Tools 和 WebView2。MSI 打包可能还依赖系统可选功能，按官方 Tauri prerequisites 配置。参考：[Tauri prerequisites](https://v2.tauri.app/start/prerequisites/)。
- 本机可执行的跨平台相关验证仅限：源码层平台适配审计（确认平台差异集中在适配层而非领域逻辑）、`cfg` 分支的目标平台测试在能编译时跑、不能编译时明示缺口。
- 开发机上工具链路径改变时，先修复可复现环境，再判断代码是否损坏。
- 使用仓库级 `rust-toolchain.toml` 锁定 channel/必要组件（经项目负责人批准）；避免脚本硬编码个人机器的 Rust 路径。
- Rust、Node、pnpm、Cargo.lock、pnpm-lock.yaml、Tauri 版本都要被锁定并在 CI 统一。
- 不因某次 CI runner 的预装软件而省略项目所需依赖说明；必要时显式安装组件。
- 任何新增系统依赖必须记录 macOS、Windows、Linux 对应安装步骤。

## 5.4 文件路径与文件系统差异

所有跨平台路径访问必须遵守：

- 路径使用 `Path` / `PathBuf` 与平台目录 API，禁止手工拼接 `"/"` 或硬编码 `C:\\Users`、`~/Library` 等用户目录。
- 不把 `current_dir()` 当作数据目录；程序可能从快捷方式、安装器、终端或不同工作目录启动。
- 分离 `resource_dir`、`app_config_dir`、`app_data_dir`、`cache_dir`、`log_dir`、`game_root_dir`、`instance_run_dir`、`plugin_dir` 和 `plugin_data_dir`。
- 测试必须包含路径空格、中文/非 ASCII 字符、只读目录、磁盘满/写入失败、已存在目标文件以及文件名大小写差异。
- 不假设 macOS/Linux 区分大小写而 Windows 不区分；Plugin ID、Mod 文件名和资源键要有明确的规范化规则。
- 不假设文件可被立即覆盖或删除；Windows 上运行中的文件可能被锁定，替换文件需采用 staging + rename/backup 策略并处理失败。
- 原子写入需考虑跨平台 rename 语义、同卷限制、文件同步、旧文件备份和 Windows 文件锁；写入失败不可先删除旧文件。
- 所有归档解压需 canonicalize/组件级校验目标路径，拒绝 `..`、绝对路径、盘符路径、UNC 路径、符号链接逃逸和重复冲突条目。
- 对路径字符串进行比较前先使用统一规则；不要在核心数据中存入仅在当前 OS 下有效的临时路径而不注明 platform scope。
- 避免把用户可见路径作为唯一业务 ID。使用稳定 ID，并在 UI 显示平台本地路径。

## 5.5 进程与命令行差异

现有启动链路需重点验证 Windows 行为：

- Java 可执行文件在 Windows 通常以 `java.exe` 结尾；不能硬编码 Unix 路径或 shell 语法。
- 不通过拼接字符串构造 shell 命令执行游戏。应将程序路径、参数数组、工作目录和环境变量分开传递。
- Windows、POSIX shell 和 PowerShell 的 quoting/escaping 不同；所有启动参数生成都应基于结构化参数，并按 Java response file 规范输出。
- 验证路径包含空格、中文、引号、反斜杠、`@`、百分号以及非 ASCII 用户名时能够正确启动。
- response file 的生命周期要在 JVM 已读取后再删除；测试正常退出、启动失败、强制终止和崩溃清理。
- Unix 进程组终止不能直接复用于 Windows；为进程树终止提供平台适配器。Windows 需要验证子进程是否残留，必要时评估 Job Object。
- `nice` 不适用于 Windows；进程优先级应该由 `ProcessPriority` 平台适配实现并返回明确结果。
- `sh`、`.bat`、`.ps1` 启动脚本是可选能力，必须各自测试；不能把 shell script 作为游戏启动核心链路的必要依赖。
- 文件名、扩展名和 `PATH` 搜索行为均有平台差异；不要用同一条未经验证的命令检测所有平台 Java。
- 进程关闭、stdout/stderr 管道耗尽、子进程继承句柄、应用退出时的收尾都应覆盖测试。

## 5.6 WebView 与 UI 差异

Tauri 在 macOS 使用 WebKit 系 WebView，在 Windows 使用 Microsoft Edge WebView2，因此同一 SolidJS UI 仍需处理渲染差异：

- 不依赖仅在某一系统字体中存在的字符或图标；关键图标使用随应用打包的资源。
- 测试高 DPI、窗口尺寸变化、系统缩放、滚动条差异、系统深色模式和字体回退。
- 拖拽、右键菜单、键盘快捷键、焦点恢复、输入法、文件选择器等行为按两个平台分别验证。
- CSS 避免不稳定的 WebKit/Chromium 专有行为作为布局基础；在最小支持 WebView 版本上验证。
- 应用外壳和插件卡片要有 loading、empty、error、permission denied 和 unsupported states。
- 生产 CSP 必须显式收紧。避免将不受信任的远程脚本或插件脚本载入可信主文档。参考：[Tauri CSP](https://v2.tauri.app/security/csp/)。

## 5.7 平台专属代码封装

建议的概念接口：

```rust
trait PlatformProcessControl {
    fn terminate_tree(&self, process_id: u32) -> Result<(), RmcccError>;
    fn set_priority(&self, process_id: u32, priority: ProcessPriority) -> Result<(), RmcccError>;
}

trait CredentialStore {
    fn get_secret(&self, key: &CredentialKey) -> Result<Option<SecretValue>, RmcccError>;
    fn set_secret(&self, key: &CredentialKey, value: &SecretValue) -> Result<(), RmcccError>;
    fn delete_secret(&self, key: &CredentialKey) -> Result<(), RmcccError>;
}
```

这里只展示边界，不要求立即采用完全相同的 trait 签名。平台实现可以通过 `#[cfg]` 选择，但业务调用方必须依赖统一接口。

凭据存储建议优先评估 macOS Keychain、Windows Credential Manager 等原生凭据设施；当前已有的文件加密 vault 不能直接假定适合所有新增 LLM Key 使用场景。应通过 `CredentialStore` 封装并保留受控回退方案。任何回退方案必须有明确安全说明，密钥不得出现在普通 JSON、前端 localStorage 或日志中。

### 5.7.1 归属规则（硬约束，优先级高于现有实现习惯）

1. **启动器核心（Core Service、领域 crate、`pipeline`）只定义 trait 与 DTO，不承担平台特定执行。** 凡是行为随 OS/arch 改变的"执行"（系统调用、进程信号、文件权限位、注册表、keychain、shell、PATH 搜索、原生打包），其实现代码必须归档到**平台适配层**（`platform/` 模块或未来 `rmccc-platform` crate），由 `#[cfg]` 在适配层内部选择具体后端。
2. **领域逻辑不得内联 `#[cfg(target_os=...)]` 执行分支。** 现有散落在 `rmccc-launch`（`nice`/`killpg`/`taskkill`/classpath 分隔符/Unix 权限位/symlink）、`rmccc-version`（OS/arch 规则筛选、natives 平台路径）、`rmccc-auth`（`.key` 权限 `icacls`/`chmod 600`）中的执行分支，属于待迁移技术债，应逐步迁入平台适配层。
3. **唯一例外：基于 Mojang 清单 `rules[].os` 的数据筛选**。判断某 library/asset 是否属于当前平台，是领域规则（应用 Minecraft 自身的平台规则），但应通过注入的 `PlatformDescriptor { os, arch, ... }` 描述对象判断，而非在领域函数里裸写 `cfg!(target_os=...)`。
4. **适配层 trait 必须返回归一化结果或 `unsupported_on_platform`**，不得静默失败或 panic。业务调用方拿到的总是 trait 返回的 DTO/枚举。
5. **平台适配器的选择在组装层（`CoreContext` 或 `AppState` 初始化）完成一次注入**，下游全链路依赖 trait，便于在测试中替换为 mock，也便于未来在 CI 上按目标注入不同后端。

### 5.7.2 必须归档到平台适配层的能力域清单

| 能力域 | 必须封装的接口（概念） | 现有承载位置（待迁移） |
|---|---|---|
| 路径解析 | `default_data_dir`/`config_dir`/`state_dir`/`log_dir`/`cache_dir`/`game_root_dir` | `rmccc-config/paths.rs`（已用 `dirs` crate，**良好范例**） |
| 文件系统耐久 | 原子写入（staging→rename/同卷限制）、flush/sync、旧文件备份、Windows 文件锁与 staging+rename、归档解压 zip-slip 防护 | `rmccc-config/file_utils.rs`、`rmccc-launch/natives.rs`（zip-slip 缺口）、`ProfileStore` 原子写 |
| 文件权限 | `.key`/argfile/敏感文件 0600（Unix）与 `icacls`（Windows）收紧 | `rmccc-auth/vault.rs`、`rmccc-launch/process.rs` |
| 进程控制 | `terminate_tree`（Unix `killpg`/Windows Job Object 或 `taskkill /T`）、`set_priority`（`nice`/Windows priority class）、进程组建立、stdout/stderr 管道、退出收尾 | `rmccc-launch/process.rs`、`managed.rs`、`default_launcher.rs`、`src-tauri/commands/launch.rs` |
| 进程/命令行生成 | 程序路径+参数数组+工作目录+env 分离传递、response file 生命周期、PATH 搜索 | `rmccc-launch/default_launcher.rs`、`process.rs` |
| Java 探测 | 各 OS 标准路径/注册表/`java_home`/`update-alternatives`、`java.exe` 扩展名、版本解析 | `rmccc-launch/java.rs`（已有 per-OS `mod`，**结构范例**，需迁入适配层 trait） |
| 凭据存储 | `CredentialStore`（get/set/delete），后端：Keychain/Credential Manager/文件 vault 回退 | `rmccc-auth/vault.rs` + `repository.rs` |
| shell / 打开 URL / 打开路径 | 受控 `open_external_url`（http(s) only）、打开文件夹、可选启动脚本 `sh`/`bat`/`ps1` | `src-tauri/commands/external.rs`、`rmccc-launch` 脚本生成 |
| 安装器 / 更新 | `.app`/DMG/NSIS/MSI/deb 或 AppImage、updater target 架构声明、签名/公证 | `release.yml`、`tauri-plugin-updater` 配置 |
| 平台描述符 | `PlatformDescriptor { os, arch, ... }` 供领域规则筛选使用 | 暂无统一抽象（领域内裸 `cfg!`） |

> 该清单的细化、当前 `#[cfg]` 散点审计与逐项迁移映射，见 [`docs/roadmap/PLATFORM_ADAPTATION.md`](roadmap/PLATFORM_ADAPTATION.md)。

### 5.7.3 当前 macOS-only 开发下的适配层落地策略

- 现阶段在本机只验证 macOS 后端，但 trait 与 `PlatformDescriptor` 必须先定义；Windows/Linux 后端可先留 `unimplemented!()` 占位或返回 `unsupported_on_platform`，待 CI runner 触达后补实现。
- 禁止用"先在领域里 `cfg!(windows)` 临时实现、以后再抽"作为理由推迟封装；新增平台相关能力一律直接进适配层。
- 适配层后端按目标平台分文件（`macos.rs`/`windows.rs`/`linux.rs`），同一 trait 只暴露一个 `pub` 接口；`#[cfg]` 只出现在这些后端文件内部和选择器中。

---

# 6. 插件系统总体设计

## 6.1 插件类型

同一插件包规范可以支持不同贡献类型，但执行权限必须分开：

| 插件类型 | 例子 | 默认执行能力 |
|---|---|---|
| Declarative / Data | 主题、图标、翻译、布局模板、静态卡片配置 | 无自定义代码 |
| UI Extension | Dashboard 卡片、独立页面、设置分区 | 只能使用 sandbox UI bridge 暴露的 API |
| Logic Extension | ModSource、依赖解析规则、复杂插件逻辑 | 隔离运行时，只获得显式 Host Import |
| Agent Extension | LLM provider、工具、context provider、审批策略扩展 | 通过 Agent Host 注册，经统一工具执行管线运行 |

“Agent 扩展”不是绕过插件宿主的特殊信任通道。Agent 插件仍遵循相同 Manifest、版本、权限和生命周期规范。

## 6.2 插件包与扩展代码的运行边界

推荐分阶段实现：

- **阶段 A：** 只支持数据型插件、内置 UI 贡献和官方示例插件；没有第三方任意代码执行。
- **阶段 B：** 自定义 UI 使用隔离的 iframe/WebView 容器，通过受控 `postMessage` 或等价协议与宿主通信。
- **阶段 C：** 逻辑插件的运行时通过 ADR 决定采用嵌入式 WASM Component Model，还是独立宿主进程。
- **阶段 D：** 在权限、性能、跨平台、打包体积和更新回滚均达标后才开放第三方逻辑插件。

不得将第三方 UI 脚本以 ES module 的方式直接导入主窗口，因为同一 JavaScript 环境中的代码可能获得主窗口可访问的全部 API。

对隔离 UI：

- 不向插件页面注入通用 Tauri IPC API。
- 使用最小的 sandbox 权限；需严格校验 `postMessage` 的来源、窗口引用、消息大小、协议版本及消息 Schema。
- 宿主每次处理请求时重新检查插件 ID、权限和作用域，不能仅信任前端隐藏按钮。
- CSP 禁止默认执行远程脚本；插件只使用安装包内静态资源，除非通过受控的网络 API 获取数据。
- 插件输出内容按不可信文本处理，不允许无验证地把 HTML 注入宿主 DOM。

## 6.3 逻辑运行时 ADR

建议优先做一份短小 PoC，对以下候选进行测量后再定：

**候选 A：嵌入式 WASM Component Model**

优点：语言无关、宿主定义显式导入、逻辑插件通常更易做跨平台包。Wasmtime 的 Component Model API 使用 WIT 描述宿主与插件世界，并生成绑定：[Wasmtime component API](https://docs.wasmtime.dev/api/wasmtime/component/index.html)。

风险：运行时增量、WIT/组件工具链成熟度、异步任务集成、内存/燃料限制、网络与文件 Host Import 开发成本。

**候选 B：独立插件宿主进程 + JSON-RPC/消息协议**

优点：进程崩溃隔离更强，可以独立升级/重启，插件语言选择空间更大。

风险：打包体积、子进程分发、IPC 认证、退出收尾、平台权限、进程管理和诊断复杂度都会增加。

**决策门槛：** 必须同时测量 Windows x64 与 macOS ARM64 的冷启动延迟、空插件体积、内存占用、调用延迟、权限隔离、跨平台构建和更新方式。不应仅按“Rust 插件可以编译”决定。

首版不建议原生 `.dll` / `.dylib` / `.so` 插件作为普通第三方扩展格式，因为它们具有 ABI、架构、系统依赖和签名问题。

## 6.4 插件 ID、版本与签名

- 插件 ID 使用稳定的反向域名形式，例如 `top.snow.server-dashboard`；安装目录、数据目录和日志都用该 ID 命名空间隔离。
- ID 一经公开不应随显示名变更；显示名可本地化。
- 插件版本使用 SemVer；插件 API 版本独立于应用版本和插件包版本。
- 插件源提供 package URL、SHA-256、体积、兼容性元数据和签名信息。
- SHA-256 只能证明文件与清单匹配；发布者身份需要签名/可信目录机制，不能将 hash 当作来源认证。
- 官方插件与第三方插件展示不同的发布者/信任状态；未签名开发插件仅在用户明确打开开发模式后安装，并显示风险提示。
- 对敏感插件权限进行审查或单独确认；即使插件通过签名验证，仍然需要运行时权限检查。
- 支持插件源切换，但默认只使用 HTTPS。拒绝不安全 HTTP，除非开发环境显式允许本地测试源。

---

# 7. 插件 Manifest 与包结构

## 7.1 建议包结构

```text
example-plugin.rmcpkg   # zip-based archive, fixed extension
  manifest.json
  README.md
  LICENSE
  CHANGELOG.md
  config.schema.json     # optional JSON Schema
  assets/                # optional static assets
  ui/                    # optional isolated UI bundle
    index.html
    index.js
    style.css
  logic/                 # optional component/runtime bundle
    plugin.wasm
  locales/               # optional
    en.json
    zh-CN.json
```

`ui/` 和 `logic/` 可以同时存在，也可以只有一个。仅主题或翻译包无需带代码。归档格式、大小限制、符号链接处理、签名格式在实现前应通过 ADR 确定。

## 7.2 Manifest 示例（设计草案）

```json
{
  "manifestVersion": 1,
  "id": "top.snow.server-dashboard",
  "name": "Server Dashboard",
  "version": "1.0.0",
  "description": "Display server status and announcements",
  "publisher": {
    "id": "top.snow",
    "name": "Snow"
  },
  "compatibility": {
    "pluginApi": "1.x",
    "core": ">=0.2.0 <0.3.0",
    "platforms": [
      "windows-x86_64",
      "darwin-aarch64",
      "darwin-x86_64"
    ]
  },
  "entry": {
    "ui": "ui/index.html"
  },
  "permissions": [
    {
      "capability": "dashboard.cards"
    },
    {
      "capability": "network.request",
      "hosts": ["status.example.net"]
    }
  ],
  "contributes": {
    "dashboardCards": [
      {
        "id": "server-status",
        "title": "Server Status",
        "defaultSize": { "w": 4, "h": 2 },
        "minSize": { "w": 2, "h": 1 },
        "settingsSchema": "#/definitions/serverSettings"
      }
    ],
    "settingsPages": [
      { "id": "server-dashboard-settings", "title": "Server Dashboard" }
    ]
  }
}
```

这是设计草案，不能在未定义正式 schema 的情况下直接当作最终协议。注意示例中的平台标识也需要由公共 schema 正式规定，且 Manifest 必须通过 JSON Schema 和额外语义校验。

## 7.3 Manifest 最低字段与验证

建议最低字段：

- `manifestVersion`
- `id`
- `name`
- `version`
- `publisher`
- `compatibility.pluginApi`
- `entry`（按插件类型决定）
- `permissions`
- `contributes`

验证必须包括：

1. 文件 schema 验证。
2. 插件 ID 与包目录 ID 一致。
3. 版本字符串合法。
4. entry 路径位于插件包目录内，无 `..`、绝对路径或 symlink escape。
5. 贡献 ID 在同插件内不重复；对全局注册项检测冲突。
6. 权限名属于已知能力列表；未知权限不能默认为允许。
7. 主机名列表必须精确匹配或使用明确、受控的通配规则；不得接受含义不清的任意 `*`。
8. 插件 API、Core 版本与当前平台匹配。
9. 依赖存在、版本范围满足且依赖图无循环。
10. 解压体积、文件数量、单文件大小、路径长度和压缩比有上限，防止 zip bomb。
11. 检查重复文件名、Windows 保留名称、大小写冲突和平台不允许的文件类型。
12. 在 staging 目录完成安装校验，再原子切换到正式目录；失败不能留下半安装状态。

## 7.4 插件配置 Schema

- 每个插件有独立配置文件：`plugin-data/<plugin-id>/config.json`。
- 配置通过 `config.schema.json` 校验，不接受任意写入全局设置。
- 用户密钥和认证凭据不能放在插件普通配置中；通过受限 Credential API 管理。
- 配置含 `schemaVersion`，迁移需要纯函数式迁移步骤和备份。
- 插件缺失/禁用时保留配置；明确卸载并选择“同时删除数据”后才删除用户数据。
- 插件升级时若新版本不兼容旧配置，应提示迁移失败并可回滚插件代码，不得把配置直接覆盖为默认值。

---

# 8. 插件能力与 Host Broker

## 8.1 能力命名空间

第一版建议先定义有限能力集，并在实现前固化公共文档与测试：

| Capability | 说明 | 默认策略 |
|---|---|---|
| `dashboard.cards` | 注册自己的 Dashboard 卡片 | 可由用户安装行为隐式允许，仍需 UI Host 隔离 |
| `ui.pages` | 注册插件页面和菜单项 | 安装后允许显示，不获得系统 API |
| `settings.read` | 读取公开的应用/实例设置子集 | 只读、按字段 allowlist |
| `instance.read` | 读取用户授权的实例概要 | 只读，避免泄露任意路径 |
| `mods.read` | 读取授权实例的模组元数据 | 只读 |
| `mods.search` | 调用已注册 Mod Source 查询 | 限速、网络走 Host |
| `mods.plan_install` | 生成安装计划 | 不执行文件写入 |
| `mods.install` | 请求执行安装计划 | 首次/批量操作需确认；按实例授权 |
| `mods.manage` | 启用、禁用、移除 Mod | 删除/批量变更需确认或预览 |
| `downloads.submit` | 创建受控下载任务 | 仅允许注册的资源来源和计划范围 |
| `downloads.manage` | 查询/暂停/恢复/取消自己的或授权任务 | 不得操作任意插件的任务 |
| `game.launch` | 启动指定实例 | 默认需要用户确认；可由用户设置为有限自动策略 |
| `logs.read` | 读取允许范围内的日志 | 脱敏、限量，默认不读取全量日志给插件 |
| `network.request` | 经宿主 HTTP client 对声明域名请求 | host allowlist、超时、响应体上限、禁止本地/私网探测默认策略 |
| `agent.tools.register` | 注册 Agent 工具 | 需结构化 schema 与执行入口，不等于获得工具运行权限 |
| `agent.context.read` | 读取授权的游戏环境上下文 | 按任务范围选择字段 |
| `credentials.use:<service>` | 使用特定服务凭据的受控能力 | 默认拒绝；不返回原始密钥 |

能力名称可以调整，但必须具有可审计的语义、最小作用域和明确拒绝行为。

## 8.2 权限授予模型

建议分为：

- **声明：** 插件 Manifest 声明需要的能力与作用域。
- **审查：** Host 验证能力是否存在、参数是否合法。
- **授予：** 用户按插件和能力进行授权；高风险能力可按每个实例/每次操作授权。
- **执行：** 每一次敏感调用都由 Host Broker 重新检查权限，不能只在插件启动时检查一次。
- **撤销：** 撤销后立即拒绝新调用，取消相关任务或使其进入安全终止状态，清理事件订阅和凭据句柄。
- **审计：** 记录调用发起者、能力、目标实例、操作计划摘要、用户审批和结果；不记录密钥及敏感内容。

权限决策不应由 UI 负责。前端按钮是否可见只影响界面，后端 Host Broker 才是授权的权威点。

## 8.3 Host Broker 负责的事情

- 根据 `pluginId` 找到已安装且启用的插件。
- 检查该插件的 Manifest、当前授权和调用目标作用域。
- 解析结构化参数并检查资源限制。
- 映射到 Core Service，而非内部 crate 函数。
- 为长任务返回 Task ID，附上 initiator。
- 对高风险操作进入确认流程。
- 做请求超时、并发限制、速率限制、响应大小限制和取消。
- 归一化错误，避免向插件泄露内部绝对路径、Token 或宿主敏感数据。

---

# 9. Dashboard 卡片、页面与主题

## 9.1 Dashboard 是布局引擎

主界面应从固定的 JSX 卡片排列，逐步迁移为“注册表 + 布局配置 + 受控渲染”的组合。

固定保留的 Shell：

- 应用标题栏/窗口操作。
- 核心导航与插件管理入口。
- 全局 Toast / 错误呈现。
- 可恢复的默认 Dashboard / 安全模式入口。
- 游戏状态或必要的系统级反馈（按后续 UX 决策确定）。

允许插件注册的贡献：

- Dashboard 卡片。
- 导航菜单项和页面。
- 设置分区。
- 命令面板动作（后续阶段）。
- Agent 工具或上下文提供者。
- 主题包与布局模板。

插件不能覆盖保留的安全入口、插件禁用页、权限管理页或恢复功能。

## 9.2 卡片注册描述

每个 Widget/卡片需要声明：

```text
WidgetDescriptor
  pluginId
  widgetId
  displayName / locale keys
  version
  entryRef
  defaultSize / minSize / maxSize
  supportedBreakpoints
  settingsSchemaRef
  requiredCapabilities
  lifecyclePolicy
```

布局实例至少保存：

```text
WidgetInstance
  instanceId
  pluginId
  widgetId
  enabled
  position (x, y)
  size (w, h)
  breakpointOverrides
  configuration
```

卡片配置只保存该卡片使用的非秘密配置。敏感数据必须通过凭据服务句柄读取。

## 9.3 布局规则

- 使用 CSS Grid 或类似响应式网格，不将像素绝对坐标当作唯一布局机制。
- 逻辑列数、间距、最小/最大尺寸应有 Schema 版本。
- 拖拽时显示有效放置区域；非法尺寸与越界坐标应在写入前归一化。
- 同位置冲突由确定性的布局算法处理并有单测，不能依赖 UI DOM 的偶然顺序。
- 拖动、调整大小、隐藏、删除卡片、添加卡片、恢复默认布局都是明确的用户命令。
- 用户可选择多个布局模板；模板是 `DashboardLayout`，不是 Minecraft `Profile`。
- 插件卸载后保留不可用占位信息与原设置，防止再次安装时布局丢失。
- 布局损坏时备份并恢复默认布局，不能导致应用无法启动。
- 至少测试 1280×720、小窗口、宽屏、高 DPI 和系统缩放变化；所有卡片必须具有 loading/empty/error 状态。

## 9.4 建议的布局文件

```json
{
  "schemaVersion": 1,
  "layoutId": "default",
  "grid": { "columns": 12, "gap": 16 },
  "widgets": [
    {
      "instanceId": "core-launcher-main",
      "pluginId": "org.rmccc.core-ui",
      "widgetId": "instance-launcher",
      "enabled": true,
      "position": { "x": 0, "y": 0 },
      "size": { "w": 8, "h": 3 },
      "configuration": {}
    }
  ]
}
```

这是格式草案；最终 Schema 必须存于 `schemas/`，并加入迁移和兼容测试。

## 9.5 主题系统

- 以设计令牌为第一层：`background` / `surface` / `surfaceElevated` / `text` / `textMuted` /
  `accent` / `accentContrast` / `border` 共 8 个颜色令牌（ADR-005 决策 7）。圆角/间距/字体/阴影
  属于宿主视觉系统，**不开放给主题**，以免破坏可读性与可访问性。
- 令牌映射到 `--rmccc-*` CSS variables，并桥接旧的 `--surface-*` / `--text-*` 变量；
  宿主安全语义色（danger/success/warning）由宿主按明暗固定注入，主题不得覆盖。
- 主题包只提供 JSON 与静态资源，**不执行 JS**；颜色只接受 `#rgb`/`#rrggbb`/`#rrggbbaa`，
  文件 ≤ 64 KiB，`schemaVersion = 1`。包检查、主题选择、运行期令牌三处复用同一校验。
- 支持浅色/深色/跟随系统；主题 JSON 可声明 `mode`，缺省按背景亮度推断。
- 主题无效时回退到内置安全主题，并在日志中记录校验失败；**选择时**即拒绝损坏主题，
  不允许"保存成功但界面回退"。
- 主题/布局与功能插件分别启用；UI 主题不能声明或继承业务权限。
- 作者规范：[`guides/THEME_PLUGIN_AUTHORING.md`](guides/THEME_PLUGIN_AUTHORING.md)；
  Schema：[`schemas/theme.schema.json`](../schemas/theme.schema.json)。
- 方向（**proposed**，未实现）：主题 v2 计划开放字体/圆角/间距/背景图/组件槽，
  采用"类型化令牌 + 宿主皮肤层"而不是任意 CSS，并新增 `themes.skin` 能力与用户覆盖，
  见 [ADR-014](adr/ADR-014-theme-v2-appearance-skin.md)。

## 9.6 插件 UI 技术约束

- 主程序 UI 继续使用 SolidJS；不为每个插件强制不同框架。
- 官方 SDK 提供少量稳定 UI primitives、设计令牌、卡片 Host API 和类型定义。
- 插件的 UI 与主界面版本解耦，但通过 UI API 版本约束兼容性。
- 不要求插件共享 SolidJS 内部对象，也不把宿主组件实例传给插件。
- 插件只能通过消息桥调用宿主服务；宿主必须验证每条消息。
- 外部 UI 不得加载任意远程 JS/CSS。更新 UI 包时需验证签名/完整性。
- 插件发生异常时展示该卡片的 error boundary；不得使整个 Dashboard 崩溃。

---

# 10. 轻干预 AI 与外部 Agent 控制

> 本节于 2026-10-23 按 ADR-013 重写：启动器**不内建 Agent 平台**。
> 完整设计见 `docs/AI_AND_MCP_ARCHITECTURE.md`；本节只保留与插件平台相关的约束。

## 10.1 定位反转：AI 是能力，Agent 在外部

- **编排在外**：对话循环、工具编排、上下文管理由外部 AI 工具（ZCode/Claude/Cursor）
  承担，它们经 MCP 服务器调用启动器能力（§10.3）。启动器内不做 Agent 循环、
  工具注册表（对外）与会话存储。
- **AI 在内但很轻**：启动器保留一个 OpenAI 兼容 AI 端口（地址/密钥/自动探测模型/
  流式输出），供内置功能（如启动失败日志一键分析）与插件调用（§10.2）。
  不设独立 Agent UI。
- **不是什么都要 AI**：缺失前置检测、完整性体检、版本兼容判断是
  **纯本地逻辑 + 网络请求**（`mods_dependency_scan` / `instance_check_completion` 等），
  禁止为了"AI 化"而绕道 LLM。

## 10.2 内部 AI 端口（插件消费面）

- 宿主经 `rmccc-llm`（OpenAI 兼容客户端）提供 `list_models` / `chat` / `chat_stream`；
  配置与密钥全局一份（密钥入凭据保险库，ADR-009，service=`llm`）。
- 插件通过桥方法 **`llm.chat`** 消费（新增能力 **`ai.chat`**，风险 medium，需显式授予）：
  非流式直接返回；流式先返回 `streamId`，宿主推 `llm.delta/llm.done/llm.error` 消息
  （复用桥的宿主→插件推送通道）。
- 插件**拿不到密钥本体**（宿主代持，`credentials.use:llm` 语义）；限流 6 次/分钟/插件。
- 向模型发送的内容必须先经宿主 redact（home→`~`、token 遮蔽）；
  模型输出的 Markdown/HTML/命令建议视为不可信，安全呈现（沿用原 §10.4 纪律）。

## 10.3 外部 Agent 经 MCP 控制（宿主级，不属于插件系统）

- MCP 服务器跑在启动器进程内，双传输：本机回环 Streamable HTTP（Bearer token，
  **默认关闭**）+ `rmccc mcp` stdio 桥；仅回环，不做局域网远程控制。
- 31 个工具按三级风险：只读（默认放行）/ 操作（放行 + GUI 归因徽标）/
  破坏性（GUI 确认卡，两阶段 proposed→approved→applied，超时默认拒绝）。
- 工具调用与用户操作走**同一内部命令层**，GUI 天然实时可见
  （下载进度、启动状态等既有事件 payload 增加 origin 字段）。
- 插件**不直接注册 MCP 工具**；如未来需要，另立 ADR。

## 10.4 沿用的纪律（原 ADR-010 有效部分）

- 审计：每次 MCP 工具调用与 AI 调用写审计（工具/脱敏参数/结果/来源会话）；
  会话存储（sessions/）废止，TaskSnapshot 结构转用作 MCP 确认记录。
- 凭据：API Key 只入保险库；不进配置 JSON、localStorage、日志、崩溃报告。
- 隐私：向远程模型发送环境信息（日志分析等）必须说明数据范围并经首次确认；
  不默认上传完整日志/存档/配置。

## 10.5 工作流示例：「检查并修复缺失前置」

纯逻辑路径（无 AI，v1 主路径）：

1. 用户（或外部 Agent）对实例调用 `mods_dependency_scan` → 缺失必需/冲突报告；
2. `mods.apply_plan` 生成/执行补装计划（Modrinth 搜索 → 版本匹配 → SHA1 下载）；
3. GUI 展示计划与进度（Agent 发起则带归因徽标）；执行后重扫描验证。

外部 Agent 路径（编排在外）：ZCode 等经 MCP 依次调用
`mods.dependency_scan` → `mods.apply_plan` → `launch.start`，
用户在启动器 GUI 全程看到进度与确认卡。

---

# 11. 生命周期、依赖、冲突与插件管理

## 11.1 生命周期状态机

建议至少有：

```text
not_installed
  -> downloading
  -> verifying
  -> staged
  -> installed_disabled
  -> enabled
  -> disabling
  -> disabled
  -> uninstalling
  -> removed

任何阶段均可能进入 failed，但必须保存错误原因和可恢复操作。
```

“已安装”不等于“已启用”。首次第三方插件安装后可以默认 disabled，或要求用户在审查权限后启用。高权限插件不应因下载完成就立即运行。

## 11.2 插件依赖

Manifest 可声明依赖：

```text
extensionDependencies:
  - id: org.rmccc.mod-api
    version: ^1.0.0
  - id: top.example.shared-ui
    version: ^2.1.0
```

规则：

- 依赖有版本范围；安装器先解析完整依赖图，再实际下载。
- 检查循环依赖、冲突 ID、平台不匹配和不可满足版本。
- 不能自动降级被其他插件使用的公共 API，而不提示影响。
- 升级失败必须恢复之前的版本目录和 registry state。
- 卸载存在依赖者时默认阻止，提供依赖链说明；不得默默级联卸载。
- 可选依赖必须与必需依赖区分。
- 用户自己的插件集合建议命名为 `ExtensionBundle` 或 `PluginSet`，不要复用 `Profile`，避免和 Minecraft 游戏实例混淆。

## 11.3 注册项冲突

- 每个插件内贡献 ID 唯一。
- 跨插件菜单、命令和卡片允许使用 namespaced ID：`<pluginId>/<localId>`。
- 受保护命名空间 `org.rmccc.*` 只允许官方核心贡献项使用。
- 一个插件失败时，只撤销该插件的注册项和订阅；不得清空所有插件注册表。
- 冲突报告要指出两个贡献项及冲突类型，不用静默覆盖来决定优先级。

## 11.4 热加载策略

第一版推荐应用启动时载入，并支持启用/禁用后重启重新加载。后续如需热加载，必须做到：

- 注册返回可撤销 handle / disposer。
- 所有事件订阅、计时任务、任务请求、Host Resource handle 能跟随插件生命周期清理。
- 正在执行的任务能够进入可控取消/等待状态。
- 卸载期间拒绝新的请求。
- 重载失败能够恢复上一个版本。

不允许用“删除插件文件夹”作为卸载完整实现。

---

# 12. 数据目录、配置、备份和迁移

## 12.1 数据目录分区

继续沿用平台目录 API 和现有 JSON 存储风格；建议逻辑分区如下：

```text
<ApplicationData>/rmccc/
  config/
    launcher-settings.json
  state/
    launcher-state.json
  user-settings.json
  accounts.json
  private/
    account-private-data.json
  profiles/
    <profile-uuid>.json
  plugins/
    <plugin-id>/<version>/...
  plugin-data/
    <plugin-id>/config.json
    <plugin-id>/data/...
  plugin-state/
    registry.json
    permissions.json
    enabled.json
  dashboard/
    layouts/<layout-id>.json
    themes/<theme-id>.json
  agent/
    sessions/<session-id>.jsonl
    tasks/<task-id>.json
  cache/
  logs/
  backups/
```

以上是目标布局。不要在一轮任务中迁移全部现存目录；应先用现有 `paths.rs` 解析实际数据路径，再制定单独、可回滚的数据迁移计划。

## 12.2 用户数据与资源路径区别

- 应用资源（只读）与用户数据（可写）必须分开。
- 插件可执行文件与插件用户数据分开，升级程序时不要覆盖用户配置。
- 缓存可以清理，用户数据不可因“修复安装”而隐式删除。
- 游戏公共资源根、Profile 运行目录和插件目录各自有不同权限和生命周期。
- 不依赖当前仓库或 `target/` 目录的相对路径定位运行资源。

## 12.3 原子写入与升级事务

对重要文件使用：

1. 校验新内容。
2. 在目标卷/目录写入唯一 staging 文件。
3. flush/sync（依平台和性能策略）。
4. 保留旧文件备份。
5. 原子替换或平台适配替换。
6. 验证完成后清理备份；失败时恢复。

插件安装和升级必须执行 staging → verify → commit，禁止在校验前直接覆盖当前可运行版本。

## 12.4 Schema 规则

- 每种持久化数据有独立 `schemaVersion`。
- 向后读取尽量兼容，未来版本数据默认只读，避免旧应用覆盖新字段。
- 迁移必须可重复（幂等）或有明确的一次性迁移标记。
- 每个迁移有旧/新 fixture 测试。
- 用户未知字段的处理方式应在 Schema 中明确；不能随意丢弃用户配置。
- 迁移前后关键用户数据必须保留；生成报告，失败不删除原文件。

---

# 13. 公开插件 SDK、API 文档和开发体验

## 13.1 SDK 目标

第三方开发者应能够仅靠公开文档和 SDK：

- 创建插件项目。
- 定义 Manifest 与配置 Schema。
- 使用稳定 TypeScript 类型或受支持的 WASM/WIT 接口。
- 注册卡片、页面、设置或 Mod Source 等贡献项。
- 调用允许的 Core API、订阅事件。
- 本地运行、热更新 UI（仅开发模式）、查看日志与权限拒绝原因。
- 打包 `.rmcpkg`、验证包结构、生成摘要/签名并本地安装。
- 在 API 兼容性检查工具中发现过期 API。

## 13.2 建议 SDK 内容

```text
plugin-sdk/
  README.md
  templates/
    declarative-theme/
    dashboard-card/
    agent-tool/
    mod-source/
  packages/
    @rmccc/plugin-api/
    @rmccc/plugin-ui/
    @rmccc/plugin-devtools/
  schemas/
    manifest.schema.json
    dashboard-layout.schema.json
    theme.schema.json
  docs/
    getting-started.md
    api-reference.md
    permissions.md
    lifecycle.md
    security.md
    cross-platform.md
    publishing.md
  examples/
    server-dashboard/
    theme-midnight/
    mod-source-example/
```

SDK 的实际包管理形式可以按仓库工作区决定，以上仅为建议结构。

## 13.3 API 文档生成要求

- 以 schema、Rust 类型、WIT 或 TypeScript 类型中的某一个明确来源为契约真相，避免同一结构手写多份。
- 对每个 API 记录：目的、输入、输出、权限、线程/异步语义、超时、错误码、平台限制、版本引入号、示例和弃用策略。
- 插件 Manifest、权限名、事件名、工具 Schema 和 UI 贡献结构都应有自动化 Schema 校验。
- 每个公开 API 至少一个正向示例和一个拒绝/错误示例。
- 插件 API 版本应独立于应用 semver；兼容矩阵在 SDK 发布说明中公开。
- 删除 API 前至少经过 deprecation 周期；开发版可提示，稳定版不可静默移除。

## 13.4 本地开发模式

建议提供开发期能力：

- `pnpm plugin:dev <path>`：对声明式/UI 插件运行 schema 验证和本地预览。
- `pnpm plugin:pack <path>`：验证文件清单并打包。
- `pnpm plugin:validate <file.rmcpkg>`：校验 Manifest、内容 hash、贡献 ID 和 API 兼容性。
- `rmcli plugin list|inspect|enable|disable|install|remove`：未来作为管理 API，待具体命令设计后实现。

命令名称是建议，不可在没有评估现有 clap 命令树冲突的情况下直接照抄。

开发模式可以加载未签名的本地包，但必须与生产模式分离；生产构建默认不降低签名和权限校验要求。

---

# 14. 安全设计与威胁模型

## 14.1 资产与威胁

需要保护的资产：

- Minecraft 账号、刷新 Token 和 LLM API Key。
- 游戏目录、存档、Profile 设置和已安装文件。
- 本地系统文件及用户隐私。
- 插件签名私钥、自动更新私钥和发布凭据。
- Agent 会话、日志、服务器凭据和用户提交给模型的内容。

主要威胁：恶意插件、供应链被篡改、伪造插件源、路径穿越、zip bomb、任意文件覆盖、XSS、越权 IPC、SSRF/本地网络探测、Prompt Injection、工具参数污染、数据泄露和升级失败。

## 14.2 安全硬约束

1. 插件无法直接获得原始账户 Token；调用登录/认证服务时只提供受控操作。
2. API Key/密钥不能出现在普通 JSON、浏览器 localStorage、崩溃日志或控制台输出。
3. 插件文件安装必须做路径规范化、文件数量/大小/压缩比限制和 staging 校验。
4. 拒绝 `..`、绝对路径、盘符路径、UNC 路径、符号链接逃逸、危险文件类型和重复路径覆盖。
5. 外部网络请求走 Host HTTP Service：必须有 URL scheme、host allowlist、重定向规则、DNS/IP 策略、超时、响应大小和并发限制。
6. 默认禁止插件访问 localhost、link-local、私网管理地址和云 metadata 地址；若确有服务器 LAN 查询用例，应设计特定能力并由用户授权，不开放任意 URL 代理。
7. 外部插件 JS 不得进入可信主窗口执行环境。
8. UI 收到插件字符串时默认按文本渲染；Markdown/HTML 要做安全处理。
9. Agent 不允许自己把自然语言解释成任意 shell 命令再无审批执行。
10. 删除、覆盖、账号操作、游戏启动和大范围安装必须经过 Core 权限策略和必要的用户确认。
11. 插件错误、超时或崩溃必须可以隔离；不能导致主程序 panic。
12. 日志必须对 Token、Authorization header、API key、私有路径和敏感 prompt 进行脱敏。
13. 插件权限可查看、可撤销；撤销后马上阻止后续能力调用。
14. 自动更新私钥与插件签名密钥不得进入 Git、日志或构建 artifact。

## 14.3 原生 Tauri ACL 与插件权限并存

Tauri capabilities/permissions 是宿主 WebView 能调用哪些 Tauri 命令的第一层安全边界；它不是 RMCCC 的第三方插件权限系统。

- 主窗口仅获得启动器需要的最小 Tauri 命令权限。
- 插件 UI 容器没有通用文件系统、shell、任意 window 或系统命令权限。
- RMCCC 插件权限由 Host Broker 实施，每次业务调用都检查。
- 不因已有 `tauri-plugin-*` 而自动把能力开放给插件。
- 配置权限、Scope 和平台特定能力时，分别审查 macOS、Windows 和 Linux 行为。

参考： [Tauri permissions](https://v2.tauri.app/security/permissions/)、[Tauri scopes](https://v2.tauri.app/security/scope/)、[Tauri CSP](https://v2.tauri.app/security/csp/)。

## 14.4 安装与升级事务

推荐流程：

1. 获取元数据和包文件，限定 HTTPS 与重定向范围。
2. 验证 registry 签名/发布者信息、包 SHA-256 与 Manifest 兼容性。
3. 将包下载到 staging 区，不对正式插件目录直接写入。
4. 验证解压路径、包大小、文件数量、内容类型和目录布局。
5. 解压到临时版本目录。
6. 安全扫描/策略验证；检查权限变化并在需要时重新征求授权。
7. 更新注册表事务，保留上一个可运行版本。
8. 在新的插件状态验证成功后提交启用状态。
9. 失败时恢复旧版本并输出可诊断错误。

更新时不能自动扩张旧插件权限。如果新版本请求更多权限，应在启用前让用户审查。

---

# 15. 版本兼容、数据兼容与升级策略

## 15.1 版本必须分开管理

- **Application Version**：RMCCC 本体 semver。
- **Core Service API Version**：插件可以调用的核心能力契约版本。
- **Plugin API Version**：Manifest、权限、事件和生命周期契约版本。
- **UI Extension API Version**：卡片、主题和页面宿主协议版本。
- **Agent Tool API Version**：工具 Schema、风险分级和执行协议版本。
- **Plugin Version**：每个插件自己的版本。
- **Data Schema Version**：配置、布局、状态和会话数据各自独立。

不要仅靠比较 RMCCC 的应用版本决定所有插件是否兼容。

## 15.2 兼容规则

- 兼容性检查发生在启用前，并在插件调用期间继续执行权限校验。
- Minor 版本应尽量向后兼容；破坏性变化提升相应 API major。
- 未知 Manifest major、未知强制字段或必需 capability 不支持时，拒绝启用并提供原因。
- 对可选贡献项不支持时可以局部降级，但必须提示哪些贡献被跳过。
- 插件所声明的平台不支持当前系统/架构时，不安装或保持 disabled。
- 应用升级后首次发现插件不兼容时，将其隔离为 `incompatible`，不要让整个应用启动失败。
- 数据 Schema 的未来版本应保持只读保护，避免旧客户端把新字段覆盖掉。

## 15.3 插件更新与回滚

- 下载新版本后先校验、staging，再切换 active version 指针/registry entry。
- 保存当前版本与配置备份，至少能在升级失败后回滚到前一可运行版本。
- 插件迁移执行失败时停止启用，不要继续以半迁移数据启动。
- 更新中断时下次启动能够检测并清理 staging 或恢复原版本。
- 需要网络的插件更新与 RMCCC 应用更新分开管理；用户应能单独关闭插件自动更新。

---

# 16. 构建、CI、测试和发布

## 16.1 原生平台 CI 原则

使用 GitHub Actions 的平台矩阵或等效 CI，但要按真实目标使用原生 runner。不要只在 macOS 上交叉编译并将产物当 Windows 发布件。

最低工作流建议：

1. **format / lint**：Rust fmt、Clippy（含生产代码 panic/unwrap 门禁）、前端 lint/typecheck。
2. **unit tests**：各 Rust crate、Manifest/Schema、路径安全、权限策略、布局算法、Agent 工具管线。
3. **integration tests**：Core Service 与文件系统、mock HTTP、插件安装事务、配置迁移、下载任务。
4. **native build matrix**：macOS ARM64、macOS Intel、Windows x64、Linux x64（Linux 若仍在支持范围）。
5. **UI build**：各目标的 `pnpm build` 和 Tauri build。
6. **E2E smoke tests**：应用启动、首次数据目录、基本路由、安装/禁用一个示例插件、主窗口恢复。
7. **release packaging**：产物归档、SHA-256、签名、公证/安装包和 updater 元数据。

GitHub Actions runner image 会变化，具体 `runs-on` 标签应以仓库当时实际可用的官方 runner 文档为准；不要永久依赖过期镜像标签。参考：[GitHub Actions runner images](https://github.com/actions/runner-images)。

## 16.2 测试分层

### Rust Unit

- Profile 设置合并和默认值。
- 安装计划、依赖图和版本约束。
- Manifest 解析、Schema 校验、ID 校验和权限合并。
- 插件贡献注册冲突与注销。
- Dashboard 布局规范化和迁移。
- 路径安全与 zip-slip 测试。
- 错误码和 API schema 快照。

### Rust Integration

- 插件安装成功、包损坏、签名失败、权限新增、磁盘不足、重复 ID、依赖循环。
- 插件升级中断和回滚。
- Core Service 同一业务路径被 GUI/CLI/插件调用时得到一致结果。
- 模拟 HTTP 超时、重定向到不允许的 host、过大响应和错误 MIME。
- 关闭/取消长任务时无遗留子进程或文件句柄。

### Frontend

- Widget registry、插件卡片 error boundary、权限拒绝状态、布局持久化和恢复。
- 多主题、窄窗口、高 DPI 和卡片最小尺寸。
- Host bridge 的 message schema、超时、取消和错误映射。
- 不应依赖真实插件市场或真实 LLM 才能跑的离线组件测试。

### E2E

每个平台至少覆盖：

1. 应用启动与关闭。
2. Profile 展示和设置持久化。
3. 安装声明式本地插件包。
4. 显示插件 Dashboard 卡片。
5. 禁用插件并确认卡片不再执行代码。
6. 恢复插件后布局与配置恢复。
7. 损坏插件不会阻止 RMCCC 启动。
8. 关键游戏安装/启动回归烟测。

## 16.3 发布目标与签名

建议产物策略：

- macOS ARM64：单独构建、`.app` / `.dmg`，按直接分发要求完成 Developer ID 签名与 notarization。
- macOS Intel：如继续承诺支持，生成并实测 x86_64 产物；之后再评估 universal binary 是否值得增加体积和构建复杂度。
- Windows x64：生成 NSIS 和/或 MSI 安装包；先选一个主发行格式，避免早期同时维护多个升级路径。正式公开发布应配置 Authenticode 签名。
- Linux x64：按目前项目的发行目标选定一种主要格式，避免无明确用户需求时同时支持过多打包格式。

Tauri updater 的更新签名与操作系统应用签名是不同事情。Updater 需要自己的签名材料；私钥不能提交到 Git，公钥可随应用发布；真实 endpoint 和签名密钥未配置时，必须将自动更新标记为不可发布。参考：[Tauri updater](https://v2.tauri.app/plugin/updater/)、[Tauri GitHub distribution pipeline](https://v2.tauri.app/distribute/pipelines/github/)、[Tauri distribution](https://v2.tauri.app/distribute/)。

## 16.4 二进制体积门禁

当前项目存在最终产物 `<10 MB` 的既有硬约束，当前 GUI 产物是否合格仍须实测。

- 先为现有版本测量并形成基线。
- 每次加入新 Runtime 或本地依赖时记录 release 体积变化。
- 分别记录 stripped executable、`.app`/安装包和 updater bundle 大小；这些不能混为同一指标。
- 如果 `<10 MB` 指单个可执行文件，必须在 `AGENTS.md` 中明确；如果指整个安装包，也应分别设阈值。
- 运行时 PoC 只有在其增量体积、启动延迟和内存数据被记录后才能进入决策。
- 禁止因为 CI 上一次 strip 环境失败就直接提高目标体积。

## 16.5 跨平台完成度报告

每个 release candidate 附一张实际结果表：

| 平台 | 编译 | 单测 | 集成测 | GUI 启动 | 关键功能 | 安装/卸载 | 签名/公证 | 产物大小 |
|---|---|---|---|---|---|---|---|---|
| macOS ARM64 | pass/fail | pass/fail | pass/fail | pass/fail | pass/fail | pass/fail | pass/fail/N/A | bytes |
| macOS Intel | pass/fail | pass/fail | pass/fail | pass/fail | pass/fail | pass/fail | pass/fail/N/A | bytes |
| Windows x64 | pass/fail | pass/fail | pass/fail | pass/fail | pass/fail | pass/fail | pass/fail/N/A | bytes |
| Linux x64 | pass/fail | pass/fail | pass/fail | pass/fail | pass/fail | pass/fail | pass/fail/N/A | bytes |

不适用项必须说明原因；缺少 runner/签名凭据不能填写 pass。

---

# 17. 分阶段实施路线与验收门槛

以下是建议路线，不要求一次执行完。每一阶段都应单独形成 PR/提交组和验证报告。

## Phase 0 — 建立可复现基线与跨平台差距清单

**目标：** 保证后续重构有可信的起点。

任务：

- 恢复并验证 Rust/pnpm 工具链和依赖缓存；检查锁文件。
- 重新运行 `cargo fmt --all -- --check`、Clippy 两类门禁、`cargo test --workspace`、`pnpm typecheck`、`pnpm build`。
- 审查 `.github/workflows/ci.yml` 和 `release.yml`，确认 Windows runner 是否真正构建/测试，而非只有 Linux/macOS 矩阵占位。
- 对 `rmccc-launch` 的 Windows 进程管理、Java 检测、参数文件、启动脚本做源码审计；列出未覆盖风险。
- 复核数据目录、路径拼接、ZIP 解压和文件原子替换实现。
- 记录当前 `.app`、DMG、Windows 安装包和可执行文件体积基线。Windows/DMG/Windows 安装包基线在无原生 runner 时标注为 deferred。

**验收：** 基线表有实际执行结果、明确环境限制、问题分级和责任归属；不以旧文档数字代替本次结果。本机仅产出 macOS Apple Silicon 基线，其余平台标注 deferred（服从 §1.4）。

## Phase 1 — 收敛 Core Service 和跨平台平台适配

**目标：** 建立插件可依赖的稳定底座。

任务：

- 统一 Tauri 与 CLI 的版本安装、资源修复、启动及 Loader 安装入口。
- 让 Pipeline 成为实际执行路径，移除重复业务编排。
- 定义 `GameService`、`InstanceService`、`ModService`、`DownloadService` 等窄门面，先留在现有 core crate。
- 明确 Profile 与版本仓库 instance 的语义和迁移/删除行为。
- 建立 Windows/macOS 的进程终止、凭据、路径和启动参数适配层（见 §5.7）。Windows 后端在本机不实现也不实测，留 trait + `unsupported_on_platform` 占位，待原生 Windows CI runner 触达后补实现。
- 用 ts-rs 或统一 schema 生成机制改进 IPC 契约，避免路由直接裸写字符串调用。
- 修复 zip-slip，并补齐关键安全测试。

**验收：** GUI、CLI 相同操作走同一 Core 实现；核心路径测试不依赖 Tauri WebView；适配层 trait 已定义且 macOS 后端可用，领域 crate 不再内联 `#[cfg]` 执行分支。Windows native CI 编译和测试通过一项记为 **deferred / release-blocked**（服从 §1.4），不得在本机填 pass。

## Phase 2 — 插件协议与声明式插件管理器

**目标：** 先实现插件管理，不先执行任意外部代码。

任务：

- 定稿 Manifest JSON Schema、Plugin ID、SemVer、兼容性和权限模型。
- 定义插件注册表、插件状态、配置命名空间和生命周期接口。
- 实现 `.rmcpkg` 本地安装、验证、启用、禁用、移除和失败恢复。
- 实现插件配置 schema 校验、权限查看/撤销和 API 兼容性提示。
- 提供一个纯数据型主题包与一个声明式官方插件作为第一个插件。

**验收：** 包损坏、ID 冲突、路径穿越、未知权限、依赖冲突和安装中断都有自动化测试；损坏插件不影响主应用启动。

## Phase 3 — Dashboard 卡片与主题引擎

**目标：** 实现用户能直接感知的模块化主界面。

任务：

- 把现有启动、实例等功能迁移为内置 WidgetDescriptor。
- 实现 Widget Registry、Layout Schema、拖放、调整尺寸、隐藏/添加、恢复默认。
- 实现主题令牌系统和 CSS variables 适配。
- 定义 Widget 生命周期、错误边界、配置界面和访问服务的权限。
- 做一个服务器状态/公告示例卡片；网络请求必须通过受控服务。

**验收：** 用户可以自定义布局并重启后保留；插件卡片失败或卸载不会损坏其它卡片或主程序。

## Phase 4 — 隔离 UI 扩展与 SDK

**目标：** 允许第三方开发自己的卡片和页面。

任务：

- 实现隔离 UI 容器与版本化 Host Bridge。
- 定义消息格式、请求 ID、超时、取消、错误、权限拒绝和消息大小限制。
- 公开 TypeScript SDK、样例、打包/验证工具和开发期本地加载模式。
- 为插件 UI 定义 CSP、资源白名单、UI API 版本和平台兼容要求。
- 对同一插件做 Windows/macOS UI 一致性测试。

**验收：** 外部样例可独立构建、打包、安装；无法直接调用未授权的 Tauri IPC 或宿主文件 API。

## Phase 5 — 逻辑插件运行时

**目标：** 扩展 Mod Source、依赖解析和工具逻辑。

任务：

- 对 embedded WASM 与独立进程做体积、性能、安全和维护成本 PoC。
- 通过 ADR 选择运行时，定义 WIT/JSON-RPC 等调用契约和资源限制。
- 实现 Host Import allowlist、超时、内存限制、并发限制和任务取消。
- 为权限隔离、网络代理、版本兼容、崩溃恢复提供测试。
- 首先开放一个低风险的 Mod Source 或只读兼容性扩展。

**验收：** 插件崩溃/超时不会崩溃主程序；宿主能强制执行权限与资源限制；Windows x64 和 macOS ARM64/Intel 有实际构建验证。

## Phase 6 — Agent 平台

**目标：** 通过统一 Core API 和插件工具执行可审查任务。

任务：

- 定义 LLM Adapter、Credential Store、Session Store、Tool Registry 和 Agent Event schema。
- 先实现只读工具和任务计划预览。
- 实现用户审批、取消、超时、执行日志和失败恢复。
- 逐步接入模组兼容性、依赖安装和启动日志分析。
- 将第三方工具调用纳入插件权限系统和审计日志。

**验收：** Agent 不会越权执行工具；安装/删除类行为先形成方案；工具结果可追踪；敏感信息不被无意发送给远程模型。

## Phase 7 — 插件目录、签名和生产分发

**目标：** 形成可维护的插件生态。

任务：

- 定义官方 registry 元数据和发布者身份。
- 加入签名验证、哈希校验、插件更新和回滚。
- 管理插件 API compatibility、权限新增提示和撤销。
- 制定插件审核、违规处理、撤回和安全更新流程。
- 完成 Windows/macOS 安装器、签名、公证、updater、体积和 E2E 验收。

**验收：** 公开发布件在干净系统安装和升级可用；插件更新失败可恢复；签名与密钥配置完成。

---

# 18. 给 Vibe Coding Agent 的执行方法

## 18.1 每次任务必须使用的流程

### Step 1：调查

- 检查 `git status --short`、当前分支和最近提交。
- 找到该功能的实际入口、依赖和测试；先阅读相关文件，不根据文件名猜实现。
- 识别任务属于 Core、Tauri adapter、CLI adapter、Frontend、Plugin API、Host、Runtime 还是 Agent。
- 写清当前行为、目标行为、平台差异和风险。

### Step 2：提出最小方案

Agent 先给出：

- 本轮目标和明确不做的事情。
- 受影响文件/模块。
- 数据或 API 契约是否变化。
- 是否涉及 Windows/macOS 差异。
- 测试与验收命令。
- 是否需要 ADR。插件 API、权限、运行时、存储 Schema、跨平台发布策略变更通常需要 ADR。

### Step 3：修改

- 优先增量扩展和兼容适配，避免无关重命名/格式化/目录重排。
- 每个功能先写契约或测试，再实现逻辑。
- 核心业务逻辑不直接依赖 Tauri。
- 插件能力经 Host Broker，不绕过权限检查。
- 对新增依赖记录功能、替代方案、体积影响和平台要求。
- 对 `cfg` 分支同时写目标平台测试或在当前环境无法运行时明示限制。

### Step 4：验证

根据变化范围运行：

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo clippy --workspace --lib --bins -- -D warnings -D clippy::panic -D clippy::unwrap_used
cargo test --workspace
pnpm typecheck
pnpm build
```

如果工具链或依赖缓存阻止命令运行，必须记录真实失败输出；不能继续声称完整验证通过。涉及平台特有代码时，必须追加 Windows/macOS 原生 CI 验证或说明当前缺口。

### Step 5：交付报告

每次任务最后必须报告：

1. 修改内容与理由。
2. 有影响的架构/契约。
3. 实际运行的验证命令及结果。
4. 未运行/未通过的验证与原因。
5. 剩余风险、迁移注意事项、下一步建议。
6. 文档是否需要同步。

不要自动进入下一阶段；完成用户要求的这一个切片后停下来给出可审查结果。

## 18.2 开发任务切片原则

建议每个 PR/提交组只包含以下一种主任务：

- Core API 门面建立。
- 单一平台 adapter 修复。
- 一个 Manifest/schema 契约。
- 一个插件生命周期阶段。
- 一种 Widget 注册能力。
- 一种权限与资源范围。
- 一项 SDK 命令。
- 一个 Agent 工具执行路径。
- 一组跨平台 CI/安装测试。

不要把“创建插件框架、修改 Dashboard、加 WASM、加 Agent、改安装包”放进同一轮任务。

## 18.3 代码生成和格式要求

- 修改公开 DTO 时同步更新 schema、Rust/TS 类型、示例和测试。
- 修改事件名称时同步更新 Host、前端、SDK 文档和测试。
- 新增配置字段时同步更新默认值、兼容读取、迁移逻辑、用户文档和类型。
- 新增插件能力时同步更新权限表、拒绝测试和 UI 中的权限说明。
- 所有错误路径必须释放订阅、任务句柄、临时文件和锁。
- 尽量避免过宽的 `pub`、全局可变状态和跨层循环引用。
- 不为减少几行代码牺牲错误信息或可测试性。
- 不用自动生成的文件手工覆盖源契约；按项目工具重新生成并检查 diff。

## 18.4 建议的 Agent 工作提示词模板

```text
你正在维护 RMCCC。请先阅读 AGENTS.md、docs/AI_EDITING_RULES.md、
ARCHITECTURE.md、RMCCC_PLUGIN_PLATFORM_ARCHITECTURE_GUIDE.md、
AUDIT_2026-10-20.md、PROJECT_STATUS.md 和 DEVELOPMENT_PLAN.md。

在修改前：
1. 检查 git status，不覆盖未提交改动。
2. 用源码确认现状，不把目标架构误认为现有实现。
3. 说明本轮目标、非目标、受影响的 crate/文件、跨平台风险、测试计划。
4. 如需改插件 API、权限、Manifest、持久化 Schema 或运行时选择，先提出 ADR；不要默默改协议。

实现约束：
- Core Service 是 GUI、CLI、插件和 Agent 共用的游戏业务入口。
- 不把 CoreContext 或原始 Tauri AppHandle 暴露给第三方插件。
- Windows x64、macOS ARM64、macOS Intel 是必需目标；Linux x64 按现有目标保留。
- 平台差异放入适配层；不只凭 macOS 上编译通过就宣称跨平台完成。
- 外部插件最小权限；不将插件脚本加载到可信主窗口。
- 遵循已有错误、日志、类型生成和测试规范。
- 进行最小范围的增量修改，不做无关重构。

完成后：运行与改动相关的格式、lint、测试、typecheck、build；
真实报告通过/失败/未执行项目，并同步必须的文档。
不要自动开始下一阶段。
```

---

# 19. Definition of Done（完成定义）

任何“已完成”的功能必须同时符合适用条件：

- [ ] 行为和非目标清晰，有对应设计说明。
- [ ] Core / Host / UI 的职责与依赖方向正确。
- [ ] 公开 API、Manifest、Schema、事件或配置变化已版本化并更新文档。
- [ ] 参数校验、权限拒绝和异常路径有测试。
- [ ] 对文件系统的修改有 staging/原子写入/失败恢复策略。
- [ ] 无敏感信息泄漏到日志或前端存储。
- [ ] macOS 行为已在本机检查；Windows/Linux 行为已标注 deferred（服从 §1.4）或在原生 CI 已验证——二者居其一，不得留空。
- [ ] Rust lint/test 和前端 typecheck/build 按实际能力通过。
- [ ] 跨平台特定验证未完成时明确标记，不隐瞒为完整通过。
- [ ] 新增依赖的二进制体积和发布影响有评估。
- [ ] `ARCHITECTURE.md`、本指南、开发计划和状态记录需要同步的内容已更新。
- [ ] 插件失败不会使核心启动链路不可用；有明确恢复方式。
- [ ] 用户数据迁移可重复、可追踪且不会静默丢失数据。

## 19.1 插件功能额外完成条件

- [ ] Manifest Schema 验证覆盖有效、无效和边界案例。
- [ ] 插件 ID、版本、依赖和贡献项冲突已检查。
- [ ] 权限最小化；未授权调用明确失败。
- [ ] 禁用后无持续订阅、后台任务或未关闭资源。
- [ ] 安装、升级、回滚、卸载与残余数据策略明确。
- [ ] 不能通过路径穿越、恶意归档或 Host Bridge 消息绕过权限。
- [ ] 平台不兼容时拒绝启用并给出原因。

## 19.2 Agent 功能额外完成条件

- [ ] 工具参数由 schema 校验。
- [ ] 工具的风险级别、权限和审批策略明确。
- [ ] 有超时、取消、重复调用和失败恢复行为。
- [ ] 工具执行结果与调用发起者可审计。
- [ ] 模型输出不能直接取得额外权限或任意执行命令。
- [ ] 发送给远程模型的数据范围清晰且可由用户控制。

---

# 20. 需要通过 ADR 逐项定稿的决策

这些决定不应通过 Agent 的个人偏好静默做出。

| ADR | 决策 | 建议时机 |
|---|---|---|
| ADR-001 | 定义正式支持的 OS 最低版本和架构 | Phase 0 |
| ADR-002 | 定稿 Core Service API 与 Profile/Instance 语义 | Phase 1 |
| ADR-003 | 插件包归档格式、目录结构、签名与信任模型 | Phase 2 |
| ADR-004 | Manifest JSON Schema、插件 ID 和版本兼容规则 | Phase 2 |
| ADR-005 | Dashboard Layout Schema、冲突布局算法和断点规则 | Phase 3 |
| ADR-006 | 插件 UI 的隔离容器和消息桥 | Phase 4 前 |
| ADR-007 | 逻辑插件运行时：WASM vs 独立进程 | Phase 5 前，需 PoC |
| ADR-008 | 插件目录/注册源和签名发布流程 | Phase 7 前 |
| ADR-009 | 凭据存储：Keychain/Credential Manager 与兼容回退 | Agent 接入前 |
| ADR-010 | Agent 会话存储、工具审计和隐私策略 | Agent Phase |
| ADR-011 | 小于 10 MB 的具体度量对象与门禁 | 运行时 PoC 前 |
| ADR-012 | macOS Intel 的 runner、打包和最低支持系统版本 | Phase 0 |

ADR 至少记录：背景、目标、考虑的选项、选择理由、后果、迁移影响、跨平台验证方式和状态（proposed/accepted/superseded）。

---

# 21. 参考资料

以下官方资料用于辅助技术决策；它们说明底层工具的行为，不会自动决定 RMCCC 的产品契约。外部文档可能更新，Agent 在实施相关能力时应重新核对当前版本。

1. DeepSeek Harness 架构（插件贡献服务、类型化事件、可逆注册、Profile/Bundle 分层的参考）：https://github.com/deepseek-ai/deepseek-harness/blob/master/docs/architecture.md
2. DeepSeek Harness 工具系统（工具定义与受控执行管线参考）：https://github.com/deepseek-ai/deepseek-harness/blob/master/docs/subsystems/tools.md
3. Tauri 插件开发（Tauri compile-time/plugin integration 与用户运行时插件系统的区别）：https://v2.tauri.app/develop/plugins/
4. Tauri CSP（生产 WebView 内容安全）：https://v2.tauri.app/security/csp/
5. Tauri Permissions（命令授权和能力配置）：https://v2.tauri.app/security/permissions/
6. Tauri Scope（能力作用域与拒绝优先规则）：https://v2.tauri.app/security/scope/
7. Tauri prerequisites（macOS Xcode、Windows C++ Build Tools/WebView2 等构建依赖）：https://v2.tauri.app/start/prerequisites/
8. Tauri updater（更新包签名、密钥和 endpoint）：https://v2.tauri.app/plugin/updater/
9. Tauri GitHub distribution pipeline：https://v2.tauri.app/distribute/pipelines/github/
10. Rust Windows MSVC targets：https://doc.rust-lang.org/stable/rustc/platform-support/windows-msvc.html
11. Rust Apple Darwin targets：https://doc.rust-lang.org/rustc/platform-support/apple-darwin.html
12. Rust platform support：https://doc.rust-lang.org/rustc/platform-support.html
13. Wasmtime Component API / WIT binding：https://docs.wasmtime.dev/api/wasmtime/component/index.html
14. GitHub Actions runner images：https://github.com/actions/runner-images

---

# 22. 最终方向总结

RMCCC 应逐步演进为 **稳定 Minecraft Core + 受控 Extension Host + 可组合 UI + Agent 工具平台**，而不是直接把所有代码改造成动态插件。

推荐实施顺序是：

1. 重建当前可信基线，尤其是原生 Windows 构建和测试能力。
2. 收敛安装、修复、启动和 Loader 等核心调用路径，建立可测试的 Core Service API。
3. 定义 Manifest、能力、权限、生命周期和插件配置契约，先实现声明式插件。
4. 把 Dashboard 迁移为卡片注册表与可持久化布局，增加主题令牌系统。
5. 引入隔离 UI、SDK 与插件开发工具。
6. 通过 PoC 决定逻辑插件的执行运行时，再开放更强的扩展能力。
7. 最后将 LLM/Agent 连接到稳定的工具执行管线，完成审批、审计和隐私约束。
8. 在实际 Windows/macOS/Linux 目标上完成安装、升级、回滚和发布验收。

**任何阶段都必须保持现有的 Minecraft 安装与启动能力可用。** 插件平台的目标是让新能力可以独立增加，而不是让核心业务变得更难调试、更难测试或更依赖某一个平台。
