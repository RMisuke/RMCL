# ADR-019: 统一任务中心与下载事务模型

- **状态**：accepted（2026-10-30）
- **日期**：2026-10-30
- **阶段**：Phase 4+（安装 / 下载体验）
- **关联**：[ADR-013](ADR-013-lightweight-ai-and-external-mcp.md)（origin 归因）、[ADR-016](ADR-016-instance-management-v2.md)（实例 key）、[ADR-018](ADR-018-modpack-installation.md)（整合包编排）；`crates/rmccc-tasks`；`src/components/TaskCenter.tsx`

## 背景

任务中心此前是**前端聚合**：`download://progress`（下载批次）、
`modloader://progress`（loader 阶段）、`modpack://progress`（整合包阶段）三套事件，
分别落到 `downloadsStore`、`modloadersStore`、`modpacksStore` 三个 store，再由
`TaskCenter` 拼成四组行。这套结构带来了无法靠样式修补的问题：

1. **任务身份不稳定**：向导先创建 `wizard-{key}` 合成行，后端 batch id 到达后靠
   “恰好只有一个 pending”猜着 re-key；并发安装会错配。
2. **进度语义不完整**：下载行只有批次，loader 行只有阶段百分比，整合包行只有
   0–100 的硬编码权重；用户看不到“现在第几步 / 这步在做什么 / 后面还有几步”。
3. **同一事务被拆成多行**：实例安装的“原版 → loader”在下载 100% 后看起来卡住，
   loader 事件要伪造进下载行；整合包安装期间内部的实例安装事件可能另起一行。
4. **控制面按实现细节暴露**：`download_pause/resume/cancel` 接收 batch id；loader
   安装没有取消；整合包有独立的 `modpack_cancel`。UI 需要知道三套 id。
5. **无法恢复**：前端刷新后活动任务全部丢失；最近完成也只存在内存里，且分散在
   三个 store。
6. **归因字段无处安放**：origin（user/agent/plugin）在设计里存在，但每个事件都要
   各自加一份。

继续给三套事件加字段只会让状态机更碎。任务中心需要自己的领域模型。

## 目标

1. 一个**任务** = 一次用户可感知的安装/下载事务；下载批次、loader 阶段、整合包
   文件下载都只是任务内部的步骤/计数器。
2. 任务在**后端同步创建**，id 在命令返回前就确定；前端不再合成行、不再 re-key。
3. 快照包含完整过程：加权步骤时间线、当前步骤、文件/字节计数、速度、ETA、
   最近细节日志、警告、错误、结果、可执行操作。
4. 控制面只认 `task_id`：`task_pause` / `task_resume` / `task_cancel`。
5. 前端刷新后通过 `task_list` 恢复活动任务与最近完成；广播落后时用
   `task://sync` 整板重发。
6. 所有现有安装入口（实例、整合包、loader、Mod、依赖补装、修复）都接入同一模型，
   不保留“短命令直接 toast、不进任务中心”的例外。

**不解决**：任务持久化到磁盘（应用重启后历史清空）、任务重试（`can_retry` 预留为
false）、跨设备同步、运行中游戏的进程管理（仍由 `launchStore` 负责，任务中心只
展示运行态）。

## 选项

### A. 继续前端聚合，补字段

- 优点：改动最小。
- 缺点：三套 id/生命周期仍在；并发 re-key、刷新恢复、统一取消都无法干净解决。

### B. 后端统一任务注册表（采纳）

- 新增纯 Rust crate `rmccc-tasks`：`TaskRegistry` + `TaskHandle` + `TaskSnapshot`。
- 编排层（Tauri commands / core 调用方）创建任务，把下载回调、loader 阶段、
  整合包阶段都写进同一个 record。
- Tauri 只做一件事：把快照广播成 `task://updated`。

### C. 引入通用工作流引擎 / 持久化队列

- 优点：可恢复、可重试、可跨进程。
- 缺点：当前没有“应用重启后继续下载”的需求，持久化队列会引入 schema 迁移、
  文件锁、崩溃恢复与额外体积；先把事务模型做对，持久化留作后续独立 ADR。

## 决策

采纳 **B**。

### 1. 任务模型

`rmccc_tasks::TaskSpec` 由编排层声明：

```rust
TaskSpec::new(TaskKind::InstanceInstall, "安装 1.20.4")
    .subtitle("1.20.4 · fabric")
    .target("default/my-instance")
    .tag("fabric")
    .cancellable(true)
    .step("prepare", "准备", 1)
    .step("game", "游戏文件", 10)
    .step("loader", "加载器", 6)
    .step("commit", "提交实例", 1)
```

- **weight 是相对权重**，不是百分比；总体进度 = Σ(step 权重 × step 完成度) / Σ权重。
  步骤完成度优先取步骤自己的 `percent`，否则取 `completed/total`，完成/跳过记为 1。
- 步骤状态：`pending / running / done / failed / skipped`。`begin_step` 自动把上一个
  running 步骤收为 done，因此编排层只需按顺序 `begin_step`。
- 任务状态：`queued / running / paused / cancelling / succeeded / failed / cancelled`。
- 快照包含 `currentStep/currentStepLabel`、`download`（文件数、字节、EMA 速度、ETA、
  当前文件）、`details`（最近 8 行）、`warnings`、`error`、`result`、`capabilities`。
- 完整日志通过 `task_log(task_id)` 拉取（每任务 240 行环形缓冲），避免每 100ms
  把整段安装器 stdout 塞进事件。

### 2. 生命周期与恢复

- `TaskRegistry::create` 同步插入 active 并广播初始快照；命令随后返回 `task_id`。
- 高频进度在注册表内按 **100ms/任务** 节流；阶段变化、暂停/取消、完成立即广播。
- 终态任务移入 `history`（容量 32），`task_list` 返回 `{active, recent}`。
- 广播通道（容量 256）落后时，Tauri 桥发 `task://sync` 整板；前端用整板替换，
  不依赖“每个事件都收到”。
- 应用重启后历史清空；这是明确的产品边界，不是遗漏。

### 3. 控制面

| IPC | 参数 | 语义 |
|---|---|---|
| `task_list` | — | `{active, recent}` 初始/恢复 |
| `task_get` | `taskId` | 单个快照（深链/调试） |
| `task_log` | `taskId` | 完整细节日志 |
| `task_pause` | `taskId` | 注册表置 `paused` 并调用任务的 pause action |
| `task_resume` | `taskId` | 置 `running` 并恢复 pause action |
| `task_cancel` | `taskId` | 置 `cancelling`、置协作取消标志、调用 cancel action |
| `task_clear_history` | — | 清空最近完成 |

- `can_pause` 只有注册了 pause action 才为 true；当前实例安装的下载批次支持暂停，
  loader 安装器执行阶段不可暂停。
- `can_cancel` 由任务声明；整合包通过 `cancel_flag` 传给 core，下载批次由
  cancel action 立即取消；loader/Mod 安装当前不可取消，因此不显示按钮。
- 终态由 worker 调 `succeed/fail/cancelled` 落定；`task_cancel` 只进入
  `cancelling`，不会伪造“已取消”。

### 4. 事件与前端

- 事件只剩两个：`task://updated`（`TaskSnapshot`）与 `task://sync`（`TaskBoard`）。
- 前端 `tasksStore` 是唯一任务 store：`active` 细粒度 record + `recent` 数组；
  终态快照自动从 active 移入 recent。
- `TaskCenter` 不再区分“下载/loader/整合包”三套行：每个任务一张卡，固定展示
  总体进度、当前步骤、文件/字节、速度/ETA、步骤轨道；展开后是完整步骤列表 +
  安装器日志 + 警告。
- `TaskIndicator` 的环形进度 = 活动任务 `overallPercent` 均值；数字 = 活动任务 +
  运行中游戏；红点仍是 MCP 待确认。
- 完成副作用集中在 `tasksStore`：实例/整合包/loader 成功后刷新实例列表并选中结果；
  异步安装的成功/失败 toast 也由它统一发，避免每个调用点重复。

### 5. 各入口映射

| 入口 | TaskKind | 步骤 |
|---|---|---|
| `instance_install` | `instance-install` | prepare → game → loader? → commit |
| `modpack_install` | `modpack-install` | prepare → game? → loader? → libraries? → overrides → files? → metadata → finalize |
| `modloader_install` | `modloader-install` | prepare → download → installer → verify |
| `mod_install` | `mod-install` | download → write |
| `mods_apply_plan` | `dependency-install` | scan → install |
| `instance_repair` | `instance-repair` | check → download → commit |

`mod_install` / `mods_apply_plan` / `instance_repair` 仍是同步命令，但任务在命令
执行前创建，事件照常进入任务中心；完成后进入 recent。

## 后果

正面：

- 一个事务一行，进度语义与用户心智一致；并发安装不再错配。
- 刷新可恢复，控制面只认 task id，MCP 也不需要理解 batch id。
- 新增安装类型只需声明 `TaskSpec` + 调 `TaskHandle`，不再新增事件族/store。
- 体积：`rmccc-tasks` 无新增第三方依赖（复用 tokio/serde/ts-rs），新增代码约
  1k 行；对 20 MiB 红线影响可忽略。

负面：

- 同步命令（Mod/修复）在极快完成时可能只出现在 recent，用户看不到过程；这是
  可接受的，因为任务仍被记录。
- 应用重启后历史丢失；持久化留待后续 ADR。
- 旧的 `download_*` / `modpack_cancel` IPC 与 MCP `downloads.control` 被移除，
  外部脚本需要迁移到 `task_*` / `tasks.control`。

## 迁移影响

- 删除 `downloadsStore`、`modloadersStore`；`modpacksStore` 只保留 inspect/start。
- 删除 `download://*`、`modloader://progress`、`modpack://progress` 及
  `DownloadProgressPayload`、`ModpackProgressPayload`、`ThrottledEmitter`。
- `instance_install` 返回值从 `()` 变为 `TaskStart { task_id }`；
  `modpack_install` 仍返回 `{ task_id }`，但 id 由任务注册表生成。
- `modpack_cancel` 删除；前端与 MCP 改用 `task_cancel`。
- `src/ipc/types.ts` 由 ts-rs 重新生成；`bindings.rs` 导出任务类型。

## 跨平台验证方式

任务模型与 UI 是平台无关的纯 Rust/Web 代码；macOS 本机跑 `cargo test --workspace`、
`pnpm typecheck/test/build` 与 `.app` 构建。Windows/Linux 的 WebView 事件时序
（100ms 节流下的渲染）记 deferred，随平台适配阶段验证。

## 状态变更记录

- 2026-10-30：accepted，替换三套进度事件与前端聚合 store。
