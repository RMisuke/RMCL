# 04 · 实例详情页

> 路由 `/instances/:id`（tab 用 hash：`#overview` `#mods` `#loaders` `#settings`）。
> 对标 HMCL GameInstancePage：左侧 tab 栏 + 底部工具条；这是全应用信息量最大的页面，
> 渐进披露原则在此页接受最严格的检验。

## 1. 设计目标

1. 一个实例的一切：是什么（概览）、装了什么（Mod 管理）、怎么跑（加载器/设置，含完整性与隔离）。
2. 高频路径 ≤ 2 击：启动（工具条常驻）、装 Mod（Mod 管理 tab → 前往仓库）。
3. 22 个可覆盖字段的继承模型必须可视：每个字段看得到「当前生效值」与「是否覆盖」。

## 2. 总布局

```
┌──────────────────────────────────────────────────────────────┐
│ PageHeader：‹实例› / sodium 实验存档         [▶ 启动] [⋯]      │
├──────────┬───────────────────────────────────────────────────┤
│ SideTabs │                                                   │
│ ▤ 概览    │                                                   │
│ ▦ Mod 管理│                当前 tab 内容                        │
│ ⚙ Mod加载器│              （独立滚动）                          │
│ 🔧 设置   │  （含完整性体检与隔离策略，设置改动自动保存）          │
│──────────│                                                   │
│ [▶ 启动]  │                                                   │
│ [浏览 ▾] │                                                   │
│ [管理 ▾] │                                                   │
└──────────┴───────────────────────────────────────────────────┘
```

- PageHeader 的 [▶ 启动] 与侧栏底部 [▶ 启动] 同一动作（运行中均变 [■ 停止]）——
  双入口是刻意的（滚动到任何位置都能启动）。
- 侧栏底部工具条（HMCL 模式）：
  - **浏览**：打开 HMCL 同款文件夹菜单（实例运行文件夹 / 模组 / 资源包 / 世界 /
    原理图 / 光影包 / 截图 / 配置 / 日志 / 崩溃报告）。每一项都是图标 + 文字按钮，
    点击**直接**在 Finder/资源管理器中打开对应目录，不再先弹路径对话框、不再需要复制。
    菜单通过 body portal + fixed 定位，避免被侧栏的 `overflow` 裁切。
  - **管理**：打开对话框提供重命名 / 浏览 Mod / 删除实例；删除走确认对话框。
    重命名、删除不再出现在设置或维护里。

## 3. Tab 1 · 概览（默认）

```
┌───────────────────────────────────────────────┐
│ [icon] sodium 实验存档                          │
│ [1.20.4] [Fabric 0.15.x] [4096 MiB] [Java 17]  │  ← 只读 chips
│ 上次游玩 3 小时前 · 创建于 2026-10-01            │
├───────────────────────────────────────────────┤
│ 📁 文件夹                                       │
│ [实例运行文件夹] [模组文件夹] [资源包文件夹]      │
│ [世界文件夹] [原理图文件夹] [光影包文件夹]        │
│ [截图文件夹] [配置文件夹] [日志文件夹]            │
│ [崩溃报告文件夹]                                  │
│ 点击任意按钮直接在系统文件管理器中打开。            │
├───────────────────────────────────────────────┤
│ 🩺 完整性                                      │
│  [检查完整性]  上次检查: 未检查 / 缺失 12 个文件   │
│  （结果与修复动作详见设置 tab 底部，此处仅摘要 + 跳转）  │
├───────────────────────────────────────────────┤
│ 快速操作                                        │
│ [前往 Mod 仓库] [检查缺失前置] [管理设置]           │
└───────────────────────────────────────────────┘
```

- 数据：`profiles.list`（profile）+ `instance_paths` + `instance_resolved_settings`（Java/内存生效值）。
- 文件夹：`instance_reveal(key, folder)`；`folder` 是受限枚举（不允许前端传任意路径），
  缺失的叶子目录会先 `create_dir_all` 再打开，新实例点「日志/崩溃报告」也有反馈。
- 完整性摘要：进页不自动扫描；显示设置 tab 缓存的结果（同实例会话内）。

## 4. Tab 2 · Mod 管理

对标 HMCL ModListPage（去掉当前后端不支持的部分，留扩展位）。

```
工具条：[⟳ 刷新] [+ 添加本地文件] [→ 前往 Mod 仓库] [🩺 检查缺失前置] [🔍 搜索___]
列表（RmListRow ×44px）：
┌──────────────────────────────────────────────────┐
│ ☑ [🧩] sodium-fabric-0.5.8-mc1.20.4.jar    [⋯]    │
│      mods/ · 1.2 MiB                              │
├──────────────────────────────────────────────────┤
│ ☐ [🧩] indium-1.0.30.jar                   [⋯]    │
│      mods/disabled/ · 860 KiB                     │
├──────────────────────────────────────────────────┤
│ 分隔线「已禁用」以下列出 disabled 的 mod            │
└──────────────────────────────────────────────────┘
行 [⋯] 菜单：启用/禁用（mod_toggle）/ 删除（mod_remove，确认）/ 复制文件名
```

- 数据：`mod_list`（返回 enabled[]/disabled[]/mods_dir——**只有文件名**），
  大小/日期用前端 `stat`？无 IPC → 大小显示依赖后端扩展（P7），首版只显示文件名 + 目录分组；
  文档保留列位，接口就绪后填充。
- 启停：checkbox 即 `mod_toggle`（移动 mods/ ↔ mods/disabled/），乐观更新 + 失败回滚 + toast。
- 添加本地：`mod_pick_and_install`（原生选择器，jar 过滤）；同时支持**拖拽 jar 文件到列表**
  → Tauri onDragDrop 拿到 path → `mod_install_local`（这是该命令的唯一 UI 入口）。
- 前往 Mod 仓库：`/download#mods?instance=<id>`（Mod 仓库锁定版本+loader 筛选，见 05 §4）。
- 检查缺失前置（**纯本地+网络逻辑，不依赖 AI**，ADR-013）：
  - `mods_dependency_scan(profile_id)`（fabric.mod.json 依赖图 × 已装集合对比）→
    结果对话框：缺失必需（mod/约束/被谁需要）、声明式冲突、未解析 jar（说明不纳入判断）；
  - [一键补装缺失] → 展示下载计划（文件/来源/目标目录）确认 →
    `mods_apply_plan(profile_id, plan)`（Modrinth 匹配 + SHA1 下载）→
    任务中心进度 → toast「已补装 N 个前置」+ 列表刷新；
  - 外部 AI 工具经 MCP 触发同一能力（`mods.dependency_scan/apply_plan`）时，
    进度带 Agent 归因徽标（07 §3）。
- 未来扩展位（后端 P7）：每行 mod 名/modid/loader tag/兼容性红行警告（数据来自 fabric.mod.json 解析）。

## 5. Tab 3 · Mod 加载器

对标 HMCL InstallersPage 的卡片网格：

```
┌──────────┐ ┌──────────┐ ┌──────────┐ ┌──────────┐
│ ▣ Fabric  │ │ ▣ NeoForge│ │ ▣ Forge  │ │ ▣ Quilt  │
│ ✓ 0.15.11 │ │   安装    │ │   安装    │ │ (灰置)   │
│ [移除*]   │ │           │ │           │ │ 与当前冲突 │
└──────────┘ └──────────┘ └──────────┘ └──────────┘
┌──────────┐ ┌──────────┐ ┌──────────┐ ┌──────────┐
│ OptiFine  │ │ LiteLoader│ │ Cleanroom│ │LegacyFab.│
└──────────┘ └──────────┘ └──────────┘ └──────────┘
```

- 当前 loader（`profile.mod_loader`）：高亮卡显示「✓ 版本」。
- 点「安装」→ `RmDialog` 版本列表（`modloader_list(loader_type, game_version)`）：
  每行 `version` + stable tag（stable=RmTag success / beta=warning），默认选中最新 stable；
  确认 → `modloader_install`，进度由统一任务的 `task://updated`（`modloader-install`）
  驱动卡片进入 loading 态，完成 toast + 卡片刷新。
- 不兼容置灰规则（前端静态表）：已装 fabric → forge/neo_forge/quilt/cleanroom/optifine 置灰
  （HMCL 同款约束）；灰卡 tooltip「当前实例已安装 Fabric」。更换 loader 建议「新建实例」
  （patch 叠加语义复杂，不在本实例上换），对话框文案明确提示。
- 「移除」依赖 `modloader remove` IPC（后端 P2，未就绪前灰置 + tooltip「即将支持」）。
- 版本兼容提示：`game_version` 为 1.20.4 时过滤 modloader_list 结果中不适配项由后端保证，前端不再过滤。
- 展示「本地版本实际可用的 loader」（P6 `modloader_analyze`）：概览 chips 处显示补注
  「检测到 Fabric 补丁」——P6 就绪前仅显示 profile.mod_loader。

## 6. Tab 4 · 设置（HMCL 紧凑行 + 分组继承）

> 2026-10-31 整页重写：不再使用「字段名在上、控件在下、锁在左侧」的纵向表单，
> 改为 HMCL GameSettingsPage 的「分区标题 → 卡片 → 左标签 / 右控件」紧凑行；
> 同时保留 RMCL 的卡片表面、渐变强调与分组锁语义。

### 6.1 布局与尺寸

```
┌ 基础 ────────────────────────────────────────────────┐
│ 实例信息                                      [继承] │
│ ┌ 64×64 图标 ┬ 名称输入 18px/680 ┬ [更换图标] [清除] ┐ │
│ │            └ 版本 / loader / 隔离 chips           │ │
│ │              实例目录（等宽 11px，点击打开）      │ │
│ └ 支持 PNG/JPG，最大 512 KB                         ┘ │
├ 游戏 ────────────────────────────────────────────────┤
│ Java 运行时                                 [继承]   │
│ 选择方式     [自动 | 指定版本 | 已检测 | 自定义]      │
│ ...（按模式只显示一个内联编辑器）                     │
│ 内存                                          [继承] │
│ 最大堆内存   [━━━━━━●━━━━] [4096] MiB  [使用 8 GiB]  │
│              ▓▓▓▓▓▓░░░░░░ 系统已用 12.4/32 GiB · 实例 4 GiB
│ 最小堆内存   [0] MiB                                 │
│ Metaspace    [0] MiB                                 │
│ 窗口 / 快速进入 ...                                   │
├ 启动 ────────────────────────────────────────────────┤
│ 进程 / 自定义命令（工作目录、包装器、前后钩子）        │
├ 高级（默认折叠）─────────────────────────────────────┤
│ JVM 参数 / 游戏参数与环境变量 / Agents / 本地库 / 调试 │
├ 维护 ────────────────────────────────────────────────┤
│ 隔离策略 / 完整性体检                                 │
└ 修改后自动保存 · 已自动保存 ──────────────────────────┘
```

- 内容最大宽度 **780px**；卡片圆角 16px、1px border、`--rmccc-shadow-card`。
- 分区标题 15px/680，图标 26×26；卡片标题 13.5px/650，副标题 11.5px muted。
- 标准行：`grid-template-columns: minmax(0,1fr) auto`，最小高 **52px**，
  内边距 `10px 16px`，行间 1px 分隔线；标签 13.5px/545，说明 11.5px。
- 右控件基准宽 **300px**；数字框 96px；分辨率预设下拉 152px；
  长文本/路径用整行（`stacked`），不再把输入框挤在标签右侧。
- ≤760px 时所有行自动改为上下布局，避免窄窗口下标签被压扁。

### 6.2 控件选择（与 HMCL 的对应关系）

| 设置 | HMCL 参考 | RMCL 采用 | 理由 |
|---|---|---|---|
| Java 选择 | `RadioChoiceList` + 内联 payload | 分段选择 + 按模式内联编辑器 | 4 种模式比纵向 radio 更省空间；指定版本用数字框，已检测用下拉，自定义用路径输入 + 目录选择 |
| 内存上限 | `JFXSlider` + 数字框 + `MemoryStatusBar` | 滑条 + `NumberField` + 4px 系统/分配占用条 | 滑条负责粗调，数字框负责精确值，占用条回答“这台机器还剩多少” |
| 窗口尺寸 | 可编辑 `JFXComboBox` | 常用尺寸下拉 + 宽/高数字框 | 下拉覆盖主流分辨率，数字框保留精确值；全屏时隐藏尺寸行 |
| 快速进入 | `RadioChoiceList` + 内联文本框 | 分段选择 + 单行文本框 | 只有一种目标需要填写，避免同时露出三个无关输入框 |
| 进程优先级 / 隔离 | `LineSelectButton` | 原生下拉 | 选项少且互斥，下拉比卡片列表更紧凑 |
| 布尔项 | 开关/切换行 | `RmSwitch` | 与全应用一致 |
| 长参数 | `JFXTextField`（400px） | 整行 textarea / input | 参数可能很长，整行比 400px 更可读 |
| 完整性 | 独立页面 | 维护卡片内按钮 + 缺失列表 | 保留原功能，减少一级导航 |

### 6.3 分组继承（替代逐字段锁）

- 每张设置卡是一个**继承组**（Java / 内存 / 窗口 / 快速进入 / 进程 / 命令 /
  JVM / 游戏参数 / Agents / 本地库 / 调试），卡头右侧一个锁。
- 未覆盖：控件显示 `instance_resolved_settings` 的生效值，标签为「继承」；
  **控件保持可编辑**，首次修改任意项时自动把整组字段写入
  `override_properties` 并冻结当前生效值——不再让用户先点锁、再面对灰掉的控件。
- 已覆盖：标签「已覆盖」，锁高亮；点击锁恢复继承，整组字段从 override 列表移除，
  后端重新解析后回填全局值。
- 组内可空字段（min/metaspace/running dir）冻结时写入当前生效值；清空时按后端
  约定写 `0` / `""`。
- 只暴露启动器实际消费的字段；`auto_memory`、`launcher_visibility`、
  `graphics_backend` 等尚未接入启动链路的字段不放入 UI，避免出现“看起来能配、
  实际不生效”的假控件。

### 6.4 自动保存

- 选择/开关/分段/下拉立即提交；数字/文本 **420ms 防抖**；切换 tab 时
  `onCleanup` 强制 flush 最后一次编辑。
- 直接使用 ts-rs 的 `InstanceSettingsPatch`，不再经过旧 `InstanceView` 适配器
  （旧适配器把 `max_memory_mb` 等字段名映射丢失，是本次重写的直接原因之一）。
- 请求串行：飞行中的请求完成后合并下一次 patch；失败 toast + 底部状态变红，
  下一次编辑会重试。底部状态条 sticky，显示「修改后自动保存 / 正在保存… /
  已自动保存 / 保存失败」。

### 6.5 内存占用条

- 新增 `system_memory` IPC（`rmccc-platform::SystemMemory`）：macOS 用
  `sysctl hw.memsize` + `vm_stat` 得到总量 / 可用量；不可用时前端只显示
  「实例分配」，不伪造系统数字。
- 进度条 4–5px：灰色段 = 系统已用，accent 渐变段 = 本实例分配；
  下方左右分列「系统已用 X / Y」与「实例分配 Z」。
- 分配超过物理内存 80% 时显示黄条警告；建议值默认物理内存的 50%，
  上下限 2–8 GiB，并受 512 MiB 栅格约束。

## 7. 完整性与隔离（维护卡片）

- 隔离策略是普通字段（非 override 组）：下拉提供「跟随全局 / 共享目录 /
  仅模组化隔离 / 始终隔离」，保存后刷新 `instance_paths` 的 `run_dir` 预览；
  运行中禁用并显示「运行中」标签。
- 完整性体检按 `instance_check_completion` 返回的 `MissingFileInfo[]`
  （kind / description / path）逐行展示；有缺失时出现「修复 N 个文件」，
  走 `instance_repair` 统一任务，完成后自动复检。
- 原「危险区」仍只在「管理」对话框，不回到设置页。

## 8. 页面级状态与错误

- 实例不存在（URL 手输/已删除）：整页 `RmEmptyState`「实例不存在或已删除」+ 返回列表。
- 运行中的实例：设置 tab 部分字段（隔离策略）禁用 + tooltip。
- 自动保存失败：toast 错误；下一次改动会重试（无未保存草稿拦截，切 tab 不弹确认）。

## 9. 接口与事件映射（本页全量）

| Tab | 接口 |
|---|---|
| 全局 | `instance_list/refresh/get`、`launch_start/kill/status`、`instance_remove` |
| 概览 | `instance.paths`、`instance_resolved_settings`、`instance.check_completion`（摘要） |
| Mod 管理 | `mod.list/toggle/remove/pick_and_install/install_local`、`mods_dependency_scan/apply_plan`（设计新增，P14） |
| 加载器 | `modloader.list/install`（+ P2 remove、P6 analyze） |
| 设置 | `java_detect`、`system_memory`、`instance_resolved_settings`、`instance_update_settings`、`instance_check_completion`、`instance_repair`、`instance_paths` |
| 事件 | `launch://started/output/exited`、`task://updated`、`instances://refreshed` |

## 10. 渐进披露自查

一级：启动/停止、4 个 tab、Mod 列表、加载器卡片、Java/内存/窗口；
二级：行内 ⋯ 菜单、浏览/管理对话框、完整性体检；
三级：「高级」折叠区内的 JVM 参数、游戏参数、Agents、本地库、调试；
Metaspace 属于内存卡内的精确输入，不藏在三级。

## 11. i18n key 提案（节选）

`instanceDetail.tab.overview|mods|loaders|settings|maintenance` /
`instanceDetail.browse.gameDir|mods|saves|logs|screenshots|config` /
`instanceDetail.launch` `instanceDetail.stop` /
`instanceDetail.mods.disabled` `instanceDetail.mods.dropHint` `instanceDetail.mods.missingScan` /
`instanceDetail.mods.missingResult.required` `instanceDetail.mods.missingResult.conflict {n}` /
`instanceDetail.mods.missingResult.unparsed {n}` `instanceDetail.mods.applyPlan` /
`instanceDetail.mods.applied {n}` /
`instanceDetail.loader.installed {version}` `instanceDetail.loader.incompatible {current}` /
`instanceDetail.settings.section.*` `instanceDetail.settings.card.*` /
`instanceDetail.settings.group.inherited|overridden` /
`instanceDetail.settings.java.mode|auto|version|detected|custom|tooOld` /
`instanceDetail.settings.memory.max|min|metaspace|tooHigh` /
`instanceDetail.settings.window.*` `instanceDetail.settings.quickPlay.*` /
`instanceDetail.maintenance.check` `instanceDetail.maintenance.repair {n}` …

## 12. 验收要点

- 分组继承：编辑组内任意字段会自动覆盖整组；锁回继承后，界面值与
  `instance_resolved_settings` 一致（Java / 内存 / 窗口 / 快速进入各抽 1 组）。
- 直接 `instance_update_settings` 往返：`max_memory_mb`、`width`、`height`、
  `quick_play`、`jvm_args`、`env_vars`、`running_directory` 保存后读回一致。
- 内存条：可用 `system_memory` 时显示系统已用/总量；不可用时只显示实例分配，
  控件仍可正常保存。
- 拖拽 jar 到 Mod 列表 → `mod_install_local` 落盘成功；启停 mod 文件在两个目录间移动。
- 前置补装闭环（纯逻辑，无 AI 配置也应全通）：删掉 indium → 检查缺失前置 →
  一键补装 → 列表刷新且再次检查为零缺失。
- modloader 安装：fabric on 1.20.4 全流程（列表→选择→进度→完成→卡片 ✓）。
- 设置-完整性：人为删除一个 library 文件 → 体检发现 → 修复 → 再体检干净。
