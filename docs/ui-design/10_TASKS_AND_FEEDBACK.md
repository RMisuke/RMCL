# 10 · 任务中心与全局反馈

> 跨页面模块：任务指示器、任务中心面板、游戏日志抽屉、Toast 规范、三态内容标准。
> 2026-10-30 按 [ADR-019](../adr/ADR-019-unified-task-center.md) 重构：
> 后端统一任务模型，前端不再聚合三套下载/安装事件。

## 1. 任务模型（后端统一）

一个**任务** = 一次用户可感知的下载/安装事务。下载批次、loader 安装器阶段、
整合包文件下载都只是任务内部的步骤，不再各自成为一行。

| 字段 | 含义 |
|---|---|
| `id` | 后端同步创建，命令返回前确定；控制面唯一标识 |
| `kind` | `instance-install` / `modpack-install` / `modloader-install` / `mod-install` / `dependency-install` / `instance-repair` / `resource-download` |
| `state` | `queued / running / paused / cancelling / succeeded / failed / cancelled` |
| `title` / `subtitle` | 用户可读标题与目标（实例、版本、loader） |
| `target` / `tag` | `{directory}/{instance}` 与类型标记（loader 类型、mod id） |
| `origin` | `user / agent / plugin` + 来源标签（ADR-013 归因） |
| `steps[]` | 加权时间线：`id/label/weight/state/percent/completed/total/message` |
| `currentStep` | 当前 running（或 failed）步骤 |
| `overallPercent` | Σ(权重 × 步骤完成度) / Σ权重；成功恒为 100 |
| `download` | 当前下载批次的文件数、字节、EMA 速度、ETA、当前文件 |
| `details[]` | 最近 8 行安装器输出/阶段细节（完整日志走 `task_log`） |
| `warnings[]` / `error` / `result` | 警告、失败原因、实例 key/目录/安装文件数 |
| `capabilities` | `canPause / canResume / canCancel / canRetry` |

任务完成后移入 recent（最多 32 条）；WebView 刷新后由 `task_list` 恢复。

### 1.1 各入口的步骤

| 入口 | 步骤 |
|---|---|
| 安装游戏 | 准备 → 游戏文件 →（加载器）→ 提交实例 |
| 安装整合包 | 读取清单 → 游戏本体 →（加载器）→（前置库）→ 覆盖文件 →（整合包文件）→ 元数据 → 收尾 |
| 安装加载器 | 准备 → 下载安装器 → 运行安装器 → 校验产物 |
| 安装 Mod | 下载 → 写入 |
| 补装前置 | 扫描依赖 → 下载安装 |
| 修复实例 | 检查缺失 → 补全文件 → 提交 |

`Mod` 安装、补装、修复仍是同步命令，但任务在执行前创建，进度事件照常进入面板；
完成即转 recent。

## 2. 任务指示器

```
Dock 底部固定项：
  ◌ 环形进度（活动任务 overallPercent 均值）
  任务 (2)        ← 活动任务数 + 运行中游戏数
  ⚠ 红点          ← MCP 待确认数
点击 → 任务中心面板（500px，右下弹出）
无任务时：图标静止，点击展开空面板（含最近完成）
```

## 3. 任务中心面板

```
┌─ 任务中心 ────────────────────────────── [✕] ┐
│ 3 个任务 · 2.4 MB/s                          │
│ ┌─ 安装整合包 Better MC ─────────── 进行中 ─┐ │
│ │ 整合包安装 · CurseForge · 1.20.4          │ │
│ │ 游戏本体 · 下载 libraries/…      54%       │ │
│ │ ▓▓▓▓▓▓▓▓░░░░░░░░░░░░                     │ │
│ │ ●──●──●──○──○──○──○──○                   │ │
│ │ 第 3/8 步 · 1,234/2,180 文件 · 986M/1.8G  │ │
│ │ 1.2 MB/s · 剩余 1m 20s                    │ │
│ │ [安装过程]              [暂停] [取消]      │ │
│ └───────────────────────────────────────────┘ │
│ ▶ 运行中的游戏                                │
│ 最近完成  [清空]                              │
│ ✓ 安装 1.20.4       2m 13s        [查看]      │
└───────────────────────────────────────────────┘
```

- **一张卡一个事务**：不再有“下载 / loader / 整合包”三组；loader 阶段是实例安装
  的第 3 步，不会另起一行。
- **双进度**：总体进度条 + 当前步骤百分比；文件数与字节数、速度、ETA 同排展示。
- **步骤轨道**：每个步骤一个点，done/current/failed/skipped 四态；悬停显示标签。
- **安装过程（展开）**：完整步骤列表（状态 + 百分比 + 该步消息）、最近 40 行安装器
  日志、警告列表；日志来自 `task_log`，随任务 revision 刷新。
- **操作**：仅展示 `capabilities` 允许的按钮；暂停只对可暂停的下载阶段出现，
  取消进入 `cancelling` 而不是立刻伪装成已取消。
- **最近完成**：成功绿 / 失败红 / 取消灰，显示耗时；有实例结果时提供「查看」，
  跳转并选中实例（手动整合包则选中新建游戏目录）。
- **待确认置顶**：外部 Agent 的破坏性 MCP 调用确认卡，允许/拒绝/倒计时（07 §3.3）。

## 4. 游戏日志抽屉

```
┌─ 日志 · sodium 实验存档 ──────────── [🤖 AI 分析] [暂停自动滚动] [复制] [✕] ─┐
│ [13:45:02] [main/INFO]: Loading Minecraft 1.20.4 with Fabric …  │
│ [13:45:03] [Render thread/INFO]: …                               │
│ [13:45:10] [main/ERROR]: …（ERROR 行 danger 色，WARN 黄色）       │
│ …（环形缓冲 1000 行，launch://output 流入）                       │
└──────────────────────────────────────────────────────────────────┘
```

- 右侧滑入抽屉（宽 560px，可拖拽调整），来源：任务中心「日志」/ 快速启动卡 /
  实例详情运行态。
- 每个运行中实例独立缓冲（launchStore 按 profile_id 分桶）；实例退出后缓冲保留至
  关闭抽屉（异常退出 `exit_code ≠ 0` 时自动弹开 + 顶部红条）。
- 过滤输入框（子串匹配）+ 级别过滤芯片（INFO/WARN/ERROR）。
- **AI 分析**（ADR-013）：流式结果渲染在抽屉底部；未配置 AI 时按钮灰置。
- 无崩溃分析（HMCL GameCrashWindow 等价物不存在）——尾部提供「复制全部日志」。

## 5. Toast 规范

| 反馈类型 | 载体 | 例 |
|---|---|---|
| 轻结果（成功/已取消/已复制） | Toast（4s） | 「已安装 sodium 到 mods/」 |
| 可跟进的结果 | Toast + 「查看」动作 | 「安装完成」→ 查看实例 |
| 进行中的过程 | 任务中心（不 toast） | 下载/安装进度 |
| 表单/字段错误 | 行内红字 | 实例名重名 |
| 异步任务失败 | Toast error + 任务中心错误 | 「安装失败：镜像不可达」 |
| 破坏性确认 | RmConfirmDialog | 删除实例/账号/插件 |

规则：一个动作最多一条 toast；连续同类成功合并；绝不 toast 循环刷屏。
实例/整合包/loader 的完成副作用（刷新列表、选中实例、成功/失败 toast）集中在
`tasksStore`；同步命令（Mod/补装/修复）由调用点 toast，避免重复。

## 6. 三态内容标准（RmContentState）

所有异步数据区（列表、卡片、表格）必须三态齐备（HMCL SpinnerPane 模式）：

- **loading**：骨架屏（列表用行骨架 ×5，卡片用块骨架），首次加载 ≥ 300ms 才显示；
- **error**：人话原因 + 「重试」按钮，整块点击即重试；
- **empty**：图标 + 一句话 + 一个 CTA。

## 7. 全局键盘

- `Esc`：关闭最上层浮层（抽屉 → 面板 → 弹窗 → 菜单，逐层）；
- `Ctrl/Cmd + K`：预留命令面板（后续阶段，不在本期范围）。

## 8. 接口与事件映射

| 方向 | 名称 | 说明 |
|---|---|---|
| IPC | `task_list` | `{active, recent}`，启动/刷新恢复 |
| IPC | `task_get` / `task_log` | 单任务快照 / 完整细节日志 |
| IPC | `task_pause` / `task_resume` / `task_cancel` | 只认 `taskId` |
| IPC | `task_clear_history` | 清空 recent |
| 事件 | `task://updated` | `TaskSnapshot`，100ms/任务节流 |
| 事件 | `task://sync` | `TaskBoard`，广播落后时整板重发 |
| 事件 | `launch://*` / `ai://*` / `mcp://*` / `instances://refreshed` | 保持不变 |

已删除：`download://progress/complete/error`、`modloader://progress`、
`modpack://progress`、`download_cancel/pause/resume`、`modpack_cancel`。

## 9. i18n key（节选）

`tasks.title` / `tasks.activeSummary` / `tasks.now` / `tasks.recent` /
`tasks.clearRecent` / `tasks.completed` / `tasks.completedWarnings` /
`tasks.viewResult` / `tasks.showDetails` / `tasks.hideDetails` /
`tasks.stepProgress` / `tasks.eta` / `tasks.state.*` / `tasks.kind.*` /
`tasks.step.done|failed|skipped` /
`tasks.download.{pause|resume|cancel|cancelled}` /
`tasks.game.{running|stop|log|startedAt}` / `tasks.cancelConfirm.*` /
`tasks.confirm.*` / `logs.ai.*`

## 10. 验收要点

- 发起 1.20.4 安装 → 立即去 Mod 仓库搜索：Rail 环与面板持续更新；下载 100% 后
  loader 步骤继续推进，不出现“卡住”。
- 同时发起两个实例安装 / 一个整合包安装：三张卡互不串行、不串进度。
- 暂停/恢复/取消：仅允许的操作出现；取消后状态经 `cancelling` 落到 `cancelled`
  或 worker 的最终结果，不出现假成功。
- 刷新 WebView（dev）后 `task_list` 恢复活动任务与最近完成。
- 展开任意任务：步骤状态与 `task_log` 内容一致，日志不刷屏。
- 完成实例/整合包后自动刷新实例列表并选中结果；手动整合包选中新游戏目录。
- 广播落后（模拟慢前端）时 `task://sync` 能整板恢复。
