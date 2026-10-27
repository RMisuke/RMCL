# 03 · 实例列表页

> 路由 `/instances`。实例（Profile）的总览与管理入口，主页快速启动卡的「完整版」。

## 1. 设计目标

1. 一屏看全：所有实例的版本、loader、上次游玩、运行状态。
2. 常用动作一步可达：启动（行内）、进详情（点击行）、删除（右键/⋯）。
3. 新建实例只有一个出口：右上「新建实例」→ 下载中心向导（不在本页弹表单，
   因为创建实例必然涉及版本/loader 选择，向导才是正确形态）。

## 2. 布局（2026-10-27 目录抽屉版）

```
┌────────────────────────────────────────────────────────────────────┐
│ PageHeader：实例                                  [+ 新建实例]      │
├──────────────────┬─────────────────────────────────────────────────┤
│ 游戏目录   +  ⌕   │ 目录名 [外部] [默认] [HMCL] [PCL]   [打开][⟳][删] │
│ ──────────────── │ 路径…                                            │
│ ▸ 默认目录     3  │ ┌─────────────────────────────────────────────┐ │
│ ▸ HMCL 目录    7  │ │ [🔍 搜索] [排序 ▾]        [▦ 卡片 | ≡ 列表] │ │
│ ▸ 整合包目录   1  │ ├─────────────────────────────────────────────┤ │
│                  │ │  卡片模式 / 列表模式（同一批实例，同一滚动区） │ │
│ [目录管理]        │ └─────────────────────────────────────────────┘ │
└──────────────────┴─────────────────────────────────────────────────┘
```

- 左侧目录抽屉与设置/下载页的 `RmSideTabs` 同构：显示名称、路径省略、实例数，
  外部目录带 `external` 角标，当前目录高亮；底部「目录管理」。
- 右侧固定目录头（名称、来源标记、路径、打开目录/刷新/移除）+ 工具条 + 内容。
- 视图模式：卡片网格（默认，3–4 列自适应）/ 表格化紧凑列表；两者共用 `items()`，
  切换只换呈现层（`Switch`），不重新请求、不丢滚动位置；偏好存 localStorage。
- 排序：最近游玩（默认，`last_played` 降序、null 沉底）/ 名称 / 创建时间。
- 搜索：前端本地过滤（实例数量级 < 100，无需后端）。
- 主页：快捷启动的实例选择器与「实例」卡片只列**当前选中目录**的实例，并显示目录名与数量。

## 3. 卡片/行信息结构

```
[icon 40px（profile.icon，缺省用版本首字母色块）]
主文字：实例名（14px）
副文字：游戏版本 tag + loader tag（fabric/forge/… 均为 RmTag accent/neutral）
元信息：last_played 相对时间 / 「从未游玩」
尾随：  运行中 → 「■ 停止」+ 绿色呼吸点；否则 hover 显示「▶ 启动」
角标：   完整性异常（missing_count > 0，来自 instance_check_completion 惰性抽查？
        否——列表页不做完整性扫描（4000 文件太贵），仅显示后端 Profile 自带字段；
        完整性体检入口在实例详情-维护）
```

> 注意：列表页不调用 `instance_check_completion`（Hash 级全量扫描约 20s），
> 这是后端审计确认的成本约束；完整性状态只在实例详情维护 tab 按需计算。

## 4. 交互

| 动作 | 行为 |
|---|---|
| 点击左侧目录 | `game_directory_select` → 重新加载该目录实例；旧选中实例若不属于新目录则清空，主页自动重选最近游玩 |
| 「+ 添加目录」 | 原生目录选择器 → `game_directory_add`（空目录补建 `versions/`）→ 切换过去 |
| 「扫描本机」 | `game_directory_scan` → 勾选候选（HMCL / PCL / 官方）→ 逐个 `game_directory_add`；只登记不复制 |
| 「目录管理」 | 重命名、打开目录、移除（仅取消登记，不删文件）、点击行设为默认 |
| 点击卡片/行标题 | → `/instances/:id`（详情默认概览 tab；面包屑含目录名） |
| 行内「▶ 启动」 | `launch_start`；运行中卡变「■ 停止」（`launch_kill`）；失败 toast（含 Java 缺失等错误码人话） |
| 右键卡片 / `⋯` 菜单 | 启动 / 打开实例详情 / 在 Mod 仓库中浏览（→ `/download#mods?instance=`）/ 重命名（dialog → `profile_update.name`）/ 删除 |
| 删除 | `RmConfirmDialog`：说明「只删除实例配置，不删除已下载的游戏文件（可在 设置→高级→版本仓库 清理）」；确认 → `profile_delete` |
| 「+ 新建实例」 | → `/download#install`（向导） |
| ⟳ 刷新 | profilesStore.refresh()；`instances://refreshed` 事件到达时自动刷新（版本仓库重扫后实例 loader 状态可能变化） |
| 运行中实例删除 | 菜单中禁用删除/重命名，tooltip「游戏运行中」 |

## 5. 状态与空态

- 空态（`RmEmptyState`）：图标 `sports_esports` +「还没有实例」+「安装一个 Minecraft 版本，
  几分钟就好」+ CTA「去安装」→ `/download#install`。
- 加载：`RmContentState` 骨架（卡片骨架 ×3）。
- 错误：profiles.list 失败 → 整页 `RmErrorState` 可重试。

## 6. 接口与事件映射

`game_directory_overview` / `game_directory_scan` / `game_directory_reveal` /
`game_directory_add|update|remove|select`（目录）；
`instance_list` / `instance_get` / `instance_update_settings` / `instance_rename` /
`instance_duplicate` / `instance_remove`（实例）；`launch_start/kill/status`（运行态）。
事件 `launch://started/exited`、`instances://refreshed`（同时刷新目录计数）。

## 7. 渐进披露自查

一级：搜索/排序/启动/新建/进详情；
二级：右键菜单（重命名、跳 Mod 仓库）；
三级：无（版本仓库清理去设置页）。

## 8. i18n key 提案（节选）

`instances.title` / `instances.search.placeholder` / `instances.sort.lastPlayed|name|created` /
`instances.new` / `instances.empty.title` / `instances.empty.cta` / `instances.neverPlayed` /
`instances.delete.title` / `instances.delete.body` / `instances.running` / `instances.cannotModifyRunning`

## 9. 验收要点

- 0 / 1 / 多实例三态；运行中实例的启动→停止循环在卡片与主页快速启动卡之间状态一致（同一 launchStore）。
- 删除实例后主页快速启动卡若指向该实例，自动切到下一个实例或引导态。
- 排序与视图偏好刷新后保持（localStorage）。
