# 10 · 任务中心与全局反馈

> 跨页面模块：Rail 任务指示器、任务中心面板、游戏日志抽屉、Toast 规范、三态内容标准。
> 这是 RMCCC 相对 HMCL（全模态任务对话框、无法并行浏览）的**超越点**：
> 任务非模态化，用户可以边下载边逛 Mod 仓库。

## 1. 任务模型（前端聚合）

后端没有统一任务模型，前端把三类「进行中的事」聚合成统一视图：

| 任务类型 | 生命周期 | 数据源 | 操作 |
|---|---|---|---|
| 下载批次 | version_install 发起 → complete/error | downloadStore（`download://progress/complete/error`，batch_id 键） | 暂停/恢复/取消（`download_pause/resume/cancel`） |
| Modloader 安装 | modloader_install 发起 → progress(stage) | modloaderStore（`modloader://progress`，profile_id+loader_type 键） | 无（阶段级事件，无取消 IPC） |
| 游戏运行 | launch_start → exited | launchStore（`launch://started/output/exited`，profile_id 键） | 停止（`launch_kill`）、看日志 |

> 完整性修复（instance_repair）与 mod 安装是同步短命令，直接 toast 反馈，不进任务中心。

**归因（ADR-013）**：所有任务行携带 origin（user/agent/plugin）。origin=agent 的任务显示
`🤖 Agent` 徽标（tooltip：来源工具名，如 ZCode）；origin=plugin 显示插件名徽标。
数据源：既有任务事件的 `origin` 字段（设计新增）。Agent/插件发起操作另有专属 toast（07 §3.2）。

## 2. Rail 任务指示器

```
Rail 底部固定项：
  ◌ (环形进度环 20px，聚合所有下载批次的字节进度加权)
  任务 (2)   ← 数字 = 活动下载数 + 运行中游戏数（loader 安装并入下载计数）
  ⚠ 红点     ← MCP 待确认数（外部 Agent 破坏性操作确认卡，07 §3.3）
点击 → 展开任务中心面板（Popover，宽 420px，右下弹出）
无任务时：图标静止灰色，点击展开空面板（含最近完成 3 条）
```

## 3. 任务中心面板

```
┌─ 任务中心 ────────────────────────────┐
│ ⤓ 下载                                 │
│ ┌───────────────────────────────────┐ │
│ │ 1.20.4 · sodium 实验存档            │ │
│ │ ▓▓▓▓▓▓▓▓░░░░░░░  54%               │ │
│ │ 2,180/4,023 个文件 · 986 MiB/1.8G  │ │
│ │ 正在下载: libraries/guava-….jar     │ │
│ │              [⏸ 暂停] [✕ 取消]      │ │
│ └───────────────────────────────────┘ │
│ ⚙ Modloader                            │
│ ┌───────────────────────────────────┐ │
│ │ Fabric → sodium 实验存档   ⏳ 安装中 │ │
│ └───────────────────────────────────┘ │
│ ▶ 运行中的游戏                          │
│ ┌───────────────────────────────────┐ │
│ │ ● sodium 实验存档  PID 12345 · 12分  │ │
│ │        [📄 日志] [■ 停止]           │ │
│ └───────────────────────────────────┘ │
└───────────────────────────────────────┘
```

- **待确认置顶（有待确认时）**：外部 Agent 确认卡——破坏性 MCP 工具调用的两阶段确认
  （来源工具/工具名/参数人话化/影响/倒计时，[允许][拒绝]，超时自动拒绝），见 07 §3.3；
- origin=agent 的下载/loader 行带 `🤖 Agent` 徽标 + 来源工具 tooltip（§1 归因）；
- 下载行双进度：文件数（completed/total）与字节（downloaded/total）同时显示，
  取自 `download://progress` payload（current_file 截断中段显示）。
- 暂停/恢复按钮随批次状态切换；P3（paused/resumed 事件）未就绪前，
  操作后按钮短暂禁用 + 下一条 progress 恢复刷新（乐观 UI 兜底）。
- 取消 → 确认弹窗（「已下载的部分文件将保留为 .part，可续传」——与后端续传语义一致）。
- 完成的批次移入「最近完成」折叠区（保留 3 条：成功绿 / 失败红 + 「查看错误」），
  failure 详情 = download://error 的 error/failed_files 人话化。
- 下载速度：单批当前速度前端按 progress 事件差分计算（EMA 平滑），面板底部汇总。

## 4. 游戏日志抽屉

```
┌─ 日志 · sodium 实验存档 ──────────── [🤖 AI 分析] [暂停自动滚动] [复制] [✕] ─┐
│ [13:45:02] [main/INFO]: Loading Minecraft 1.20.4 with Fabric …  │
│ [13:45:03] [Render thread/INFO]: …                               │
│ [13:45:10] [main/ERROR]: …（ERROR 行 danger 色，WARN 黄色）       │
│ …（环形缓冲 1000 行，launch://output 流入）                       │
└──────────────────────────────────────────────────────────────────┘
```

- 右侧滑入抽屉（宽 560px，可拖拽调整），来源：任务中心 [📄 日志] / 快速启动卡「查看日志」/
  实例详情运行态。
- 每个运行中实例独立缓冲（launchStore 按 profile_id 分桶）；实例退出后缓冲保留至关闭抽屉
  （异常退出 exit_code ≠ 0 时自动弹开抽屉 + 顶部红条「游戏异常退出（代码 -1）」）。
- 过滤输入框（子串匹配）+ 级别过滤芯片（INFO/WARN/ERROR）。
- **AI 分析**（ADR-013 内置参考功能）：[🤖 AI 分析] → `ai_analyze_log` →
  `ai://delta/done/error` 流式事件在抽屉底部渲染分析面板（可能原因/建议/复制/重新分析）；
  未配置 AI 时按钮灰置并引导设置；异常退出自动弹开时按钮高亮。完整 UX 见 07 §4。
- 无崩溃分析（HMCL GameCrashWindow 等价物不存在）——设计留扩展位：exited 异常时
  尾部显示「复制全部日志用于求助」按钮。

## 5. Toast 规范（何时用什么）

| 反馈类型 | 载体 | 例 |
|---|---|---|
| 轻结果（成功/已取消/已复制） | Toast（4s） | 「已安装 sodium 到 mods/」 |
| 可跟进的结果 | Toast + 「查看」动作 | 「安装完成」→查看实例 |
| 进行中的过程 | 任务中心（不 toast） | 下载进度 |
| 表单/字段错误 | 行内红字 | 实例名重名 |
| 流程级失败 | Toast error + 任务中心详情 | 「安装失败：镜像不可达」 |
| 破坏性确认 | RmConfirmDialog | 删除实例/账号/插件 |

规则：一个动作最多一条 toast；连续同类成功合并（「已安装 3 个文件」）；
绝不 toast 循环刷屏（事件去重 1s 窗口）。

## 6. 三态内容标准（RmContentState）

所有异步数据区（列表、卡片、表格）必须三态齐备（HMCL SpinnerPane 模式）：

- **loading**：骨架屏（列表用行骨架 ×5，卡片用块骨架），首次加载 ≥ 300ms 才显示（防闪烁）；
- **error**：人话原因 + 「重试」按钮，整块点击即重试；
- **empty**：图标 + 一句话 + 一个 CTA（各页面文档已定义具体文案）。

## 7. 全局键盘

- `Esc`：关闭最上层浮层（抽屉 → 面板 → 弹窗 → 菜单，逐层）；
- `Ctrl/Cmd + K`：预留命令面板（后续阶段，不在本期范围，避免空占位）。

## 8. 接口与事件映射

`download_cancel/pause/resume`、`launch_kill`、`launch_status`（App 启动恢复运行态）、
`mcp_confirmations_list/confirmation_resolve`（确认卡，设计新增）；
事件：`download://progress/complete/error(/paused/resumed 预留)`、`launch://started/output/exited`、
`modloader://progress`、`ai://delta/done/error`、`mcp://session/confirmation`，
以及既有任务事件的 `origin` 字段（设计新增）。

## 9. i18n key 提案（节选）

`tasks.title` / `tasks.empty` / `tasks.download.{fileCount|bytes|current|pause|resume|cancel}` /
`tasks.download.recentDone` `tasks.download.recentFailed` /
`tasks.download.speed {speed}` `tasks.download.totalSpeed {speed}` /
`tasks.loader.installing {loader} {instance}` /
`tasks.game.running {instance}` `tasks.game.pid` `tasks.game.stop` `tasks.game.log` /
`tasks.game.exitedAbnormally {code}` / `tasks.cancelConfirm.body` /
`tasks.log.autoscroll` `tasks.log.copy` `tasks.log.filter` `tasks.log.copyForHelp` /
`tasks.agentBadge` `tasks.agentToast.start` `tasks.confirm.*` `logs.ai.*`（完整见 07 §10） /
`common.retry` `common.cancel` `common.confirm` `common.copy` `common.done`

## 10. 验收要点

- 下载 + 浏览并行：发起 1.20.4 安装 → 立即去 Mod 仓库搜索 → Rail 环与面板进度持续更新。
- 暂停/恢复/取消三操作对同一批次的 UI 状态机正确（P3 前的兜底态也要自洽）。
- 游戏日志：启动真实实例 → 抽屉滚动跟随 → 停止 → 缓冲关闭清理（内存无泄漏，事件取消订阅）。
- 异常退出自动弹抽屉（用 `exit_code` 模拟）。
- Toast 合并与去重规则（快速连续安装 3 个 mod 只见一条汇总）。
- Agent 归因与确认：外部工具经 MCP 发起下载 → 任务行徽标 + toast；
  确认卡允许/拒绝/超时三路径（07 §11 联测）。
