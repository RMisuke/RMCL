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
  - **浏览**：打开对话框列出游戏目录 / 运行目录 / mods 的完整路径，每行可复制。
    （旧实现是向右弹出的菜单，会被侧栏 overflow 裁切，已改对话框。）
  - **管理**：打开对话框提供重命名 / 浏览 Mod / 删除实例；删除走确认对话框。
    重命名、删除不再出现在设置或维护里。

## 3. Tab 1 · 概览（默认）

```
┌───────────────────────────────────────────────┐
│ [icon] sodium 实验存档                          │
│ [1.20.4] [Fabric 0.15.x] [4096 MiB] [Java 17]  │  ← 只读 chips
│ 上次游玩 3 小时前 · 创建于 2026-10-01            │
├───────────────────────────────────────────────┤
│ 📁 路径                                         │
│  游戏根目录  /…/game        [复制]              │
│  运行目录    /…/game/versions/1.20.4-isolated  │  ← isolation 生效时
│  Mods 目录   /…/mods        [复制]              │
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
  确认 → `modloader_install`，进度由 `modloader://progress`（started/completed 阶段事件）
  驱动卡片进入 loading 态，完成 toast + 卡片刷新。
- 不兼容置灰规则（前端静态表）：已装 fabric → forge/neo_forge/quilt/cleanroom/optifine 置灰
  （HMCL 同款约束）；灰卡 tooltip「当前实例已安装 Fabric」。更换 loader 建议「新建实例」
  （patch 叠加语义复杂，不在本实例上换），对话框文案明确提示。
- 「移除」依赖 `modloader remove` IPC（后端 P2，未就绪前灰置 + tooltip「即将支持」）。
- 版本兼容提示：`game_version` 为 1.20.4 时过滤 modloader_list 结果中不适配项由后端保证，前端不再过滤。
- 展示「本地版本实际可用的 loader」（P6 `modloader_analyze`）：概览 chips 处显示补注
  「检测到 Fabric 补丁」——P6 就绪前仅显示 profile.mod_loader。

## 6. Tab 4 · 设置（继承锁模型）

### 6.1 布局与保存

垂直分区卡片（HMCL GameSettingsPage 模式），每个可继承字段行结构：

```
字段名                    [RmInheritLock 🔓/🔒] 生效值副标题
[控件：输入/滑条/选择]
```

- 继承态（未覆盖）：控件显示**全局默认值**（只读灰态）+ 副标题「继承全局默认：4096 MiB」；
  点锁 → 加入 `override_properties` + 控件可编辑；
- 覆盖态：控件可编辑 + 副标题显示「覆盖全局（4096）」；再点锁 → 弹确认「恢复继承，当前自定义值将丢弃」。
- 生效值统一以 `instance_resolved_settings` 为准显示（后端已实现 resolve）。
- **自动保存**：没有「保存」按钮。选择/开关切换立即提交；文本与数字输入 450ms 防抖后提交；
  统一走 `instance_update_settings`（继承锁变化也立即提交覆盖名单）。页脚显示
  「修改后自动保存 / 正在保存… / 已自动保存」，失败 toast 并保留下次编辑重试。

### 6.2 分区与披露级别

| 分区 | 一级直出 | 三级折叠（RmAdvancedSection） |
|---|---|---|
| 运行环境 | Java（自动/检测列表 `java_detect`（含版本+厂商+64bit）/自定义路径）、最大内存（滑条 512–32768，步进 512；提示全局 auto_memory 时显示「自动」态） | 最小内存、Metaspace |
| 窗口与进程 | 窗口宽高（默认 854×480 占位）、全屏开关、进程优先级（low/normal/high） | — |
| 启动参数 | — | **JVM 参数**（textarea，一行一条；`override_jvm_args` 追加模式说明）、**游戏参数**（textarea）、**Java Agents**（列表增删）、**环境变量**（key/value 表格） |
| 进程包装 | — | **wrapper**（输入框 + 安全说明：仅进程级前缀，不会写入 JVM argfile——引用 P1-7 修复语义）、前置命令、后置命令 |
| 本地库 | — | use_native_glfw / use_native_openal / use_custom_natives（macOS 默认关，tooltip 解释何时需要） |
| 调试 | — | enable_debug_log_output / no_generated_jvm_args / no_generated_optimizing_jvm_args |

- Java 选择器的**版本预检**（前端规则表，仅提示不阻断）：game_version ≥ 1.20.5 → 建议 Java 21；
  ≥ 1.17 → Java 17+；选中 Java 的 `major_version` 不满足 → 选择器行内黄条警告
  （启动失败时后端会报 `java_version_mismatch`，前端提前人话化）。
- 完整性体检与隔离策略在本 tab 底部（原维护 tab 已删除）：隔离策略选择后立即保存，
  运行中禁用。

## 7. 完整性与隔离（设置 tab 内，原维护 tab 已删除）

```
┌──────────────────────────────────────────────┐
│ 🩺 完整性体检                                  │
│  [检查完整性]  ▶ 结果列表：                     │
│    ⚠ libraries/com.google…jar  缺失           │
│    ⚠ assets/objects/ab/….png   哈希不匹配      │
│  （MissingFileInfo: kind/description/path）    │
│  [修复全部缺失（N）]  → instance_repair         │
│  → toast「已修复 N 个文件」                     │
├──────────────────────────────────────────────┤
│ 📂 隔离策略                                    │
│  ○ 共享目录（默认，多实例共用 .minecraft）       │
│  ● 独立目录（isolation = "<id>-isolated"）      │
│  实时预览（instance_paths 重新解析）：            │
│    run_dir: /…/versions/1.20.4-isolated        │
│  ⚠ 切换隔离会改变存档/mods 读取位置，确认弹窗强提示 │
├──────────────────────────────────────────────┤
│ 🧨 危险区（danger 色分区）                      │
│  [重命名实例] [删除实例] [删除实例并清理版本文件]  │
└──────────────────────────────────────────────┘
```

- 检查完整性 = `instance_check_completion`（Hash 级全量复核，按钮上注明「约需数十秒，
  大型实例更久」+ 运行中状态）；修复 = `instance_repair`。
- 危险区（重命名/删除）已移入「管理」对话框；维护 tab 整体删除。
- `instance_refresh`（重扫仓库）：目录切换/刷新按钮已自动触发。

## 8. 页面级状态与错误

- 实例不存在（URL 手输/已删除）：整页 `RmEmptyState`「实例不存在或已删除」+ 返回列表。
- 运行中的实例：设置 tab 部分字段（隔离策略）禁用 + tooltip。
- 自动保存失败：toast 错误；下一次改动会重试（无未保存草稿拦截，切 tab 不弹确认）。

## 9. 接口与事件映射（本页全量）

| Tab | 接口 |
|---|---|
| 全局 | `profiles.list/update/delete`、`launch.start/kill/status`、`instance.remove`（清理版本） |
| 概览 | `instance.paths`、`instance_resolved_settings`、`instance.check_completion`（摘要） |
| Mod 管理 | `mod.list/toggle/remove/pick_and_install/install_local`、`mods_dependency_scan/apply_plan`（设计新增，P14） |
| 加载器 | `modloader.list/install`（+ P2 remove、P6 analyze） |
| 设置 | `settings.detectJava`、`instance_resolved_settings`、`instance_update_settings`、`instance.check_completion/repair`、`instance_paths` |
| 事件 | `launch://started/output/exited`、`modloader://progress`、`instances://refreshed` |

## 10. 渐进披露自查

一级：启动/停止、4 个 tab、Mod 列表、加载器卡片、Java/内存/窗口；
二级：行内 ⋯ 菜单、浏览/管理对话框、完整性体检；
三级：JVM/游戏参数、wrapper/命令、native 开关、调试开关、Metaspace（全部 RmAdvancedSection）。

## 11. i18n key 提案（节选）

`instanceDetail.tab.overview|mods|loaders|settings|maintenance` /
`instanceDetail.browse.gameDir|mods|saves|logs|screenshots|config` /
`instanceDetail.launch` `instanceDetail.stop` /
`instanceDetail.mods.disabled` `instanceDetail.mods.dropHint` `instanceDetail.mods.missingScan` /
`instanceDetail.mods.missingResult.required` `instanceDetail.mods.missingResult.conflict {n}` /
`instanceDetail.mods.missingResult.unparsed {n}` `instanceDetail.mods.applyPlan` /
`instanceDetail.mods.applied {n}` /
`instanceDetail.loader.installed {version}` `instanceDetail.loader.incompatible {current}` /
`instanceDetail.settings.inherit {value}` `instanceDetail.settings.override {global}` /
`instanceDetail.settings.javaVersionHint {need}` /
`instanceDetail.maintenance.check` `instanceDetail.maintenance.repair {n}` /
`instanceDetail.maintenance.isolationWarning` …

## 12. 验收要点

- 继承锁：每个可继承字段的三态（继承/覆盖/恢复）+ 生效值与
  `instance_resolved_settings` 一致（抽 5 字段做断言）。
- 22 个 override 字段全部可达且保存后 `profile_list` 读回一致。
- 拖拽 jar 到 Mod 列表 → `mod_install_local` 落盘成功；启停 mod 文件在两个目录间移动。
- 前置补装闭环（纯逻辑，无 AI 配置也应全通）：删掉 indium → 检查缺失前置 →
  一键补装 → 列表刷新且再次检查为零缺失。
- modloader 安装：fabric on 1.20.4 全流程（列表→选择→进度→完成→卡片 ✓）。
- 设置-完整性：人为删除一个 library 文件 → 体检发现 → 修复 → 再体检干净。
