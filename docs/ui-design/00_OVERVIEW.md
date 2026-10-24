# 00 · 总设计架构

> 前端重构的顶层设计。读完本文件应能回答：新界面有哪些页面、如何导航、
> 数据从哪来、旧界面为什么被替换、实施还需要后端补什么。

## 1. 设计目标与原则

### 1.1 目标

1. **打开即玩**：启动器首屏 5 秒内可完成「选实例 → 点启动」。
2. **干净简洁**：主界面只保留高频必要元素；视觉降噪（见 01_STYLE_GUIDE）。
3. **主页卡片化**：保留并强化 Dashboard 卡片网格（布局引擎、插件卡片、可编辑），这是
   RMCCC 区别于 HMCL 的核心形态——主页是「可定制的总览」，不是固定列表。
4. **全接口覆盖**：86 条 IPC 命令、10 个事件全部有明确界面归属（见 11_IPC_COVERAGE）。
5. **插件生态一等公民**：插件卡片、插件主题、插件设置页、logic 插件都有宿主入口。
6. **安全与隐私内建**：凭据永不展示、外部 Agent 破坏性操作走 GUI 确认卡、
   AI 数据外发透明提示、插件 UI 隔离，安全提示不可被主题覆盖。

### 1.2 核心原则：渐进披露（一级只放高频必要项）

```
一级（默认可见）   高频：启动/停止、实例切换、安装、Mod 搜索、账号切换、语言与外观
二级（页面内分区） 中频：实例设置（Java/内存/窗口）、Modloader、完整性维护、下载镜像
三级（折叠/高级）  低频：JVM 参数、wrapper、环境变量、前后置命令、native 开关、
                  下载并发、版本仓库管理、事件节流、data_dir
```

执行规则（每个页面文档都有对应「渐进披露」小节自查）：

- 一个控件的默认可见性由「普通玩家每月用几次」判定：≥ 数次 → 一级；偶尔 → 二级；
  基本只有排障/高级用户用 → 三级折叠，**折叠区必须能搜到、有说明**，不是删除。
- 危险操作（删除实例/账号/版本、卸载插件）即使高频也必须二次确认。
- 高级区用统一的 `AdvancedSection` 折叠组件，不各搞一套。

### 1.3 设计输入（现状审计结论，2026-10-23）

现有 7 个路由的问题，是本次推倒重来的直接动因：

| 现状 | 问题 |
|---|---|
| `/` Dashboard（1186 行） | 一个页面塞下：账号检查、Profile 增删、版本浏览安装、启动/停止、游戏日志、下载进度、布局编辑——职责严重过载 |
| 无全局导航 | 页面间只能靠零散按钮 `navigate()`，新用户不知道还有什么功能 |
| `/mods` ModBrowser（217 行） | 半成品：无 loader/版本筛选 UI、无详情页、无来源切换 |
| 「实例」概念混乱 | Profile 与磁盘 versions/ 版本仓库两套概念混用；`instance_list/rename/duplicate/remove` 4 条命令连 wrapper 都没有 |
| 15 条命令无 UI 入口 | `download_pause/resume`、`instance_*` 4 条、`plugin_contributions`、`logic.invoke/cancel`、`plugin_install(path)`、`dashboard_get_theme_json`、`plugin_config_get`、`mod_get_download_url`、`mod_install_local` |
| 设置页缺字段 | `mirror_url`、`download_concurrency`、`check_updates` 后端支持但未渲染 |
| 插件设置页无宿主 | manifest `settingsPages` 全链路可用，前端无渲染入口 |
| AI/Agent 方向调整 | 内建 Agent 页与 9 条 `agent_*` IPC 将被「轻量 AI 服务 + 外部 MCP 控制」取代（ADR-013，见 07） |
| 2 处 wrapper 类型错误 | `instances.refresh/repair` 返回类型（实施时修正）；`AgentRunOutput` 大小写错误随 agent 命令移除而消解 |

### 1.4 从 HMCL 借鉴什么（以及刻意不借鉴什么）

**采纳并变形**：

1. 全局左侧导航 + 每个管理页「左子导航 + 右内容」的同构布局 DNA；
2. 实例详情：左侧 tab 栏 + 底部「浏览/管理」工具条；Mod 管理 tab 与 Mod 下载页的上下文跳转
   （从实例进入自动按实例版本+loader 预设筛选，详情页「推荐版本」置顶）；
3. 字段级**继承/覆盖锁**模型（实例 → 全局默认，副标题永远显示生效值）；
4. 安装新游戏 = 版本选择 → loader 卡片网格 → 命名 → 安装的向导流；
5. 统一任务反馈：任务分组进度 + 实时速度 + 可取消；
6. 三态内容区（加载 / 失败点重试 / 空态）标准化；搜索即筛选、即时校验；
7. 语义化设计 token（M3 角色色的思路，映射到 RMCCC 现有 8 token 主题系统）。

**刻意不采纳**：

1. HMCL 把所有任务做成**模态对话框**（无法并行浏览）→ RMCCC 用**非模态任务中心**
   （导航栏底部指示器 + 可展开面板），这是可以超越 HMCL 的点；
2. HMCL 主页「只有启动按钮」的极简 → RMCCC 保留 Dashboard 卡片网格（产品定位不同：
   RMCCC 主页是插件可扩展的总览面板）；但吸收其精神——**快速启动卡是主页第一焦点**；
3. 标题栏随页面变形的复杂协议 → RMCCC 用常驻 Rail 导航 + 页面内 PageHeader，实现成本低且不易错。

## 2. 信息架构总图

```
RMCCC 窗口
│
├─ TitleBar（自定义标题栏：应用名 + 窗口控制，保留现有实现）
│
├─ Rail（左侧全局导航，200px，可收起为 64px 图标模式）
│   ├── 主页      /              （Dashboard 卡片网格）
│   ├── 实例      /instances     （实例列表）
│   │             /instances/:id （实例详情，页内 tab）
│   ├── 下载      /download      （下载中心，页内分类）
│   ├── 账号      /accounts
│   ├── 插件      /plugins
│   ├── 设置      /settings      （页内子导航）
│   └── [底部] 任务指示器（活动任务徽标，点击展开任务中心面板）
│
└─ 内容区（每个页面 = PageHeader + PageBody）
    ├── PageHeader：面包屑 + 页面标题 ·············· 页面级动作（48px）
    └── PageBody：滚动内容区（管理页内部再分「左子导航 + 右内容」）
```

页面树与文档对应：

```
/                    → 02_HOME_DASHBOARD.md
/instances           → 03_INSTANCE_LIST.md
/instances/:id       → 04_INSTANCE_DETAIL.md
   ├─ #overview        概览（默认）
   ├─ #mods            Mod 管理
   ├─ #loaders         Mod 加载器
   ├─ #settings        游戏设置（含高级折叠区）
   └─ #maintenance     维护
/download             → 05_DOWNLOAD_CENTER.md
   ├─ #install         安装游戏（向导，默认）
   ├─ #mods            Mod 仓库（Modrinth）
   └─ #modpack         整合包（占位，后端缺口）
/accounts             → 06_ACCOUNTS.md
/plugins              → 09_PLUGINS.md
/settings             → 08_SETTINGS.md
   ├─ #game-defaults   游戏默认（全局继承基线）
   ├─ #general         通用（语言/外观/更新/日志）
   ├─ #download        下载（镜像/并发）
   ├─ #ai              AI 与外部控制（AI 服务 + 外部 Agent·MCP，07_AI_AND_MCP.md）
   ├─ #plugins         插件（插件设置页宿主）
   ├─ #advanced        高级（版本仓库/data_dir）
   └─ #about           关于
（跨页面）任务中心面板 + Toast + 游戏日志抽屉 → 10_TASKS_AND_FEEDBACK.md
```

> tab 用 URL hash（`#mods`）而非路由参数，保证刷新/分享能回到同一 tab，且实例 id 变化时 hash 保留。

## 3. 应用 Shell 设计

### 3.1 布局与尺寸

```
┌────────────────────────────────────────────────────────────┐
│ TitleBar 40px：RMCCC ───────────────────────── [─] [□] [✕] │
├──────────┬─────────────────────────────────────────────────┤
│          │ PageHeader 48px：‹实例 / 1.20.4-Fabric›  [刷新]…  │
│  Rail    ├─────────────────────────────────────────────────┤
│  200px   │                                                 │
│ (64px*)  │                 PageBody                        │
│          │           （页面自身管理滚动）                    │
│          │                                                 │
│ ─────────│                                                 │
│ 任务 (2) │                                                 │
│ ‹收起导航 │                                                 │
└──────────┴─────────────────────────────────────────────────┘
  * 收起状态只显示图标，悬停显示 tooltip；记忆在前端本地偏好（localStorage，见 §8）
```

- 最小窗口 1280×720（与 Dashboard 断点 wide 对齐）；Rail 收起后内容区 ≥ 1200px。
- PageHeader：左侧面包屑（一级页面只有标题），右侧**页面级动作**（如主页「编辑布局」、
  实例详情「启动」、Mod 仓库「刷新」）。动作按钮数量 ≤ 3，其余进 `⋯` 菜单。
- 滚动策略：PageBody 独立滚动（`overflow-y: auto`），Rail/TitleBar/PageHeader 固定；
  长列表（版本列表、Mod 结果）用虚拟滚动（沿用 `components/VirtualList.tsx`）。

### 3.2 Rail 导航项定义

| 项 | 图标（Material Symbols） | 徽标 | 说明 |
|---|---|---|---|
| 主页 | `home` / `home_fill` | — | Dashboard |
| 实例 | `sports_esports` | 运行中实例数 | 实例列表 |
| 下载 | `download` | 进行中下载数 | 下载中心 |
| 账号 | `person` | —（未登录显示 `!` 点） | 账号 |
| 插件 | `extension` | 有诊断 error 时黄点 | 插件管理 |
| 设置 | `settings` | — | 设置 |
| 任务指示器 | `progress_activity`（旋转） | 聚合进度环；MCP 待确认数红点 | 底部固定，点击展开任务中心 |

### 3.3 导航模型

- **一级导航**：Rail 常驻，6 项 + 任务指示器，永远可见、永远可达（对比现状的按钮迷宫）。
- **二级导航**：实例详情 / 下载中心 / 设置 用**页内左侧子导航**（同构组件 `SideTabs`，
  200px，图标 + 文字，选中态高亮），不进 URL 路由、用 hash。
- **上下文跳转**（HMCL 模式，跨页携带状态）：
  - 实例详情 → Mod 管理 tab「前往 Mod 仓库」→ `/download#mods?instance=<profile_id>`
    （Mod 仓库顶部显示「正在为实例 X 浏览」，锁定版本+loader 筛选）；
  - 实例详情 → Mod 管理「检查缺失前置」→ 纯逻辑扫描 + 一键补装（04 §4，**不依赖 AI**）；
  - 外部 AI 工具（ZCode 等）经 MCP 触发启动器操作时，GUI 显示 Agent 归因徽标与
    破坏性操作确认卡（07 §3）；
  - Mod 仓库 → 安装成功 toast「查看实例」→ 实例详情；
  - 插件页 → 贡献 tab「前往主页」/「前往外观设置」。
- **空态引导**：全局唯一的「零实例 / 零账号」引导逻辑在主页快速启动卡（见 02），
  各列表页空态只有一个大 CTA 指向下载中心 / 账号页，不重复造引导页。

## 4. 路由表

| 路由 | 页面 | 主要接口组（详见 11_IPC_COVERAGE） |
|---|---|---|
| `/` | 主页 Dashboard | dashboard 9 条、profiles.list、launch 3 条、auth.get_session、versions 2 条、downloads 3 条 |
| `/instances` | 实例列表 | profiles.list / delete、launch 3 条、instance_refresh |
| `/instances/:id` | 实例详情 | profiles.update、instance 9 条、modloader 2 条、mods 9 条、settings.detectJava |
| `/download` | 下载中心 | versions 2 条、profiles.create、modloader 2 条、mods.search/getVersions/install/getDownloadUrl、open_external_url |
| `/accounts` | 账号 | auth 8 条、open_external_url |
| `/plugins` | 插件管理 | plugin 16 条、plugin_ui、plugin_logic 2 条 |
| `/settings` | 设置 | settings 6 条、dashboard 主题 4 条、instance 版本仓库 5 条、logs 2 条、open_external_url、AI/MCP 16 条（设计新增，07） |
| （全局） | 任务中心/日志抽屉 | download 3 条、launch 3 条、全部事件订阅 |

旧路由兼容：`/login` → 重定向 `/accounts`（首启零账号时 `/accounts` 自动进入引导态）；
`/mods` → 重定向 `/download#mods`；`/instances/:id`（旧 InstanceSettings）→ 新实例详情 `/instances/:id#settings`。

## 5. 状态与数据层

### 5.1 Domain Stores（替换现状的页面局部 signal）

遵循 `ARCHITECTURE.md` §9.2「一个领域一个 store」的设计（现状未执行，本次落地）：

| Store | 持有状态 | 数据来源 | 消费页面 |
|---|---|---|---|
| `authStore` | 当前会话、账号列表、登录流程状态 | auth 8 条 + `auth://device-code` | Rail、主页、账号页、快速启动 |
| `profilesStore` | 实例列表、当前选中实例 id（内存态） | profiles 4 条 + `instances://refreshed` | 主页、实例列表/详情、下载中心、Agent |
| `launchStore` | 运行中进程（profile→pid/exit）、日志环形缓冲 | launch 3 条 + `launch://started/output/exited` | Rail、任务中心、实例列表/详情、日志抽屉 |
| `downloadStore` | 活动批次（进度/状态）、历史结果 | download 3 条 + `download://progress/complete/error` | Rail、任务中心、主页下载卡、下载中心 |
| `modloaderStore` | 安装中的 loader 任务 | `modloader://progress` | 任务中心、实例详情 |
| `settingsStore` | AppConfig + UserSettings + 主题 | settings 6 条 + dashboard 主题 4 条 | 设置页、所有页面（token） |
| `aiStore` | AI 配置状态、模型列表、进行中的日志分析 | `ai_*` 5 条（设计新增，ADR-013） | 设置-#ai、日志抽屉 AI 分析 |
| `mcpStore` | MCP 开关/端口/会话/权限矩阵/待确认 | `mcp_*` 9 条（设计新增）+ `mcp://session/confirmation` | 设置-#ai、任务中心确认卡、Rail 红点 |
| `pluginsStore` | 插件列表 + 诊断 | plugin_list（进入插件页时拉取） | 插件页、Rail 徽标 |

规则：

- store 是**唯一**事件订阅点（`events.ts` 常量 → store action），页面只读 store 派生数据，
  杜绝多处 `listen()` 泄漏；
- store 提供 `refresh()` 惰性拉取 + 事件驱动更新双通道；页面切换不重复请求（带 TTL，
  profiles 30s、accounts 60s、插件列表进页必刷）；
- 选中实例 id：本期仅内存态（会话内保持），持久化依赖后端新增 IPC（见 §9-P5）。

### 5.2 事件接线总表

| 事件 | → Store | UI 反应 |
|---|---|---|
| `download://progress` | downloadStore | 任务中心进度、Rail 徽标、主页下载卡 |
| `download://complete` | downloadStore + profilesStore.refresh | toast「安装完成」+ 实例列表刷新 |
| `download://error` | downloadStore | 任务中心失败态 + toast（可重试=再次 version_install） |
| `auth://device-code` | authStore | 账号页设备码卡片 |
| `launch://started` | launchStore | Rail/实例列表运行徽标、日志抽屉开始 |
| `launch://output` | launchStore（环形缓冲/1000 行） | 日志抽屉、任务中心 |
| `launch://exited` | launchStore | 运行徽标消失、异常退出时 toast + 日志抽屉提示 |
| `modloader://progress` | modloaderStore | 任务中心、实例详情 loader 卡状态 |
| `instances://refreshed` | profilesStore / 版本仓库视图 | 列表刷新 |
| `download://paused` / `resumed`（预留） | downloadStore | 当前后端不发射；UI 预留状态位（§9-P3） |
| `ai://delta` / `ai://done` / `ai://error`（设计新增） | aiStore | 日志抽屉 AI 分析流式面板（07 §4） |
| `mcp://session` / `mcp://confirmation`（设计新增） | mcpStore | 设置会话列表、任务中心确认卡（07 §3） |
| 既有任务事件 payload 增加 `origin` 字段（设计新增） | 各任务 store | Agent/插件发起操作的归因徽标与 toast（07 §3） |

### 5.3 实施时必须修正的 api.ts 问题

| 问题 | 修正 |
|---|---|
| `agent_*` 9 条命令与 `/agent` 页面 | 随 ADR-013 迁移移除，替换为 `ai_*` 5 条 + `mcp_*` 9 条 + `mods_dependency_scan/apply_plan`（见 07 与架构文档 §7） |
| `instances.refresh` 声明返回 `number`，实际 `Vec<InstanceInfo>` | 修正类型 |
| `instances.repair` 声明返回 `void`，实际 `usize` | 修正类型（展示「已修复 N 个文件」） |
| `mod_install_local` 等 5 条命令无 wrapper | 新增 wrapper（见 11_IPC_COVERAGE） |

## 6. 概念澄清：实例（Profile） vs 版本仓库

RMCCC 后端有两套「实例」概念，必须在 UI 层面统一口径，否则用户必然困惑：

| 概念 | 后端 | 用户认知 | UI 归属 |
|---|---|---|---|
| **实例 Profile** | `profile_list` 等（profiles.json） | 「我玩的游戏」——名字、版本、loader、设置 | 全局：主页/实例列表/实例详情 |
| **版本仓库条目** | `instance_list`（磁盘 versions/ 目录，含 modloader patch 产物） | 高级概念：「本地已下载的版本文件」 | **只在** 设置 → 高级 → 版本仓库 暴露 |

规则：面向普通用户的一切「实例」均指 Profile；版本仓库页顶部放固定说明框
（「这是磁盘上的版本文件，不是你的游戏实例；普通用户无需关心」）。
实例详情的维护 tab 通过 `instance_check_completion/repair/paths`（接 profile id，后端自动回落版本 id）
间接使用仓库，用户无感。

## 7. 安全与隐私基线（全页面生效）

1. **凭据永不渲染**：`AuthSession.access_token/refresh_token`、Agent api_key 任何界面不显示、
   不入日志、不进 DOM（`log_get` 查看器同理，若后端日志含 token 由后端 skip 字段保证）。
2. **外部操作可确认**：MCP 破坏性工具调用一律走 GUI 确认卡（来源/参数/影响可见，
   超时默认拒绝）；操作类调用全程归因徽标可见（ADR-013，07 §3）。
3. **插件 UI 不可信**：iframe 卡片/设置页只经 `plugin_ui_invoke` 桥通信，错误态/权限态由宿主渲染；
   宿主安全提示色（danger/success/warning）不被主题插件覆盖（插件指南 §9.5）。
4. **外链只走 `open_external_url`**（仅 http/https）；「打开本地目录」按钮依赖新增 `open_path` IPC（§9-P1）。
5. **数据目录权限**：agent 数据目录 0700 的既有约束不在 UI 重复实现；MCP/AI 的审计
   与确认记录沿用 ADR-010（部分修订，见 ADR-013）的脱敏与保留规则。
6. **AI 数据外发透明**：任何把用户数据（游戏日志等）发送至 AI 端点的功能，
   首次使用需确认并标注目标 host；发送内容先经后端脱敏。

## 8. 前端偏好存储

现有 `UserSettings` 只有 `locale/theme/check_updates`。本次新增**前端本地偏好**（不进 AppConfig）：
Rail 收起状态、实例列表视图模式（卡片/列表）、各页折叠区展开态、日志抽屉宽度。
存储：`localStorage`（key 前缀 `rmccc.ui.`），随「清除 Agent 历史」无关、随 WebView 数据清除而消失——
可接受，均为可再生的界面偏好。**不**往 UserSettings 加字段（避免后端 schema 变更）。

## 9. 后端配套改动清单（实施依赖，按优先级）

本设计默认 86 条命令可用；以下为设计引用到但后端尚缺的最小配套集，
每项独立小、不阻塞整体（阻塞关系见 12_SUMMARY 阶段表）：

| # | 改动 | 依赖它的 UI | 工作量预估 |
|---|---|---|---|
| P1 | 新增 `open_path` IPC（`rmccc-platform` `ShellBackend::open_path` 已实现，仅缺命令封装） | 实例详情「浏览 ▾」、设置-日志目录 | 极小 |
| P2 | `modloader 卸载` IPC（CLI `modloader remove` 已有逻辑） | 实例详情-Mod 加载器 tab | 小 |
| P3 | 发射 `download://paused/resumed` 事件（常量已预留） | 任务中心暂停/恢复状态回显 | 小 |
| P4 | Dashboard 内置 widget 注册表新增 `downloads-card`（rmccc-dashboard widget.rs） | 主页下载任务卡 | 小 |
| P5 | `selected_profile_id` 持久化 IPC（`LauncherState` 已有字段） | 主页/实例列表选中态跨重启 | 小 |
| P6 | `modloader_analyze` IPC（`crates/rmccc-modloader/src/analyzer.rs` 已有） | 实例详情-加载器 tab 显示「本地版本实际可用的 loader」 | 小 |
| P7 | `ModVersion` DTO 增加 `dependencies` 字段；`mod_list` 增加元数据（复用 rmccc-agent 的 fabric.mod.json 解析） | Mod 仓库依赖提示、Mod 管理兼容警告 | 中 |
| P8 | CurseForge `ModSource` 实现（trait 已抽象） | Mod 仓库来源切换 | 中 |
| P9 | 启动器更新流程（tauri-plugin-updater 已装，`check_updates` 已有字段） | 设置-通用-更新按钮 | 中 |
| P10 | 整合包导入（Modrinth .mrpack / CurseForge zip） | 下载中心-整合包 tab（当前占位） | 大，Phase 7+ |
| P11 | 服务器列表 / SLP ping | 不做入核，规划为插件能力（生态方向） | — |
| P12 | AI 服务：`rmccc-llm` crate + `ai_*` 5 条 IPC + `ai://delta/done/error` 事件（架构文档 S1/S2） | 设置-#ai、日志抽屉 AI 分析 | 中 |
| P13 | MCP 服务器：回环 HTTP + `rmccc mcp` stdio 桥 + 31 工具 + 确认流 + `mcp_*` 9 条 IPC（S4/S5） | 设置-#ai、任务中心确认卡/归因徽标 | 大 |
| P14 | `mods_dependency_scan` / `mods_apply_plan` 2 条 IPC（依赖扫描去 Agent 化，S3） | 实例详情-检查缺失前置 | 小 |
| P15 | 既有任务事件 payload 增加 `origin` 字段（S5） | 任务中心/主页归因徽标 | 小 |

> P1–P6 建议随前端实施同批落地；P7–P9 可迭代；P10/P11 明确为后续阶段，UI 先占位不虚假承诺。
> P12–P15 属 AI/MCP 架构（ADR-013），权威设计与实施顺序见
> `docs/AI_AND_MCP_ARCHITECTURE.md`（S1–S7 切片），UI 侧见 07_AI_AND_MCP.md。

## 10. 页面模块清单（文档分工）

见 README.md 索引表。每个页面文档统一包含：设计目标 / 布局示意（ASCII）/ 模块与组件拆解 /
交互流程 / 渐进披露自查 / 接口与事件映射 / 状态·空态·错误 / i18n key 提案 / 验收要点。
