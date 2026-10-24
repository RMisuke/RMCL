# 05 · 下载中心（安装游戏 · Mod 仓库 · 整合包占位）

> 路由 `/download`（分类用 hash：`#install` 默认 / `#mods` / `#modpack`）。
> 落实「安装游戏里包括安装游戏本体、下载模组、下载 Modloader」的中心化入口。
> 布局对标 HMCL DownloadPage：左侧分类导航 + 右侧内容。

## 1. 设计目标

1. **安装游戏 = 一条向导流**：版本（本体）→ Modloader（可选）→ 命名 → 安装执行，
   不让用户在多个页面间自己拼装。
2. **Mod 仓库是全库浏览器**，但任何入口带实例上下文进入时自动锁定筛选并置顶推荐版本
   （HMCL「实例 → 下载页」模式）。
3. 下载进行中不打断浏览：全部进度进任务中心（见 10），本页只在按钮上显示轻状态。

```
┌──────────┬────────────────────────────────────────────────┐
│ 分类导航   │                                                │
│ ▣ 安装游戏│                                                │
│ ▦ Mod 仓库│              当前分类内容                        │
│ ▤ 整合包  │                                                │
│ (即将推出)│                                                │
└──────────┴────────────────────────────────────────────────┘
```

## 2. 分类一：安装游戏（向导，默认）

### 2.1 流程总图

```
① 选择版本 ────▶ ② Modloader ────▶ ③ 实例信息 ────▶ ④ 安装执行 ────▶ 完成
  version_list     可跳过(vanilla)    profile_create    version_install
  搜索/类型过滤     modloader_list     (+内存默认)       (+modloader_install)
  虚拟滚动         8 卡片→版本弹窗                      任务中心跟踪
                                                        [立即启动][查看实例][装 Mod]
```

- 向导用**页内步骤条**（Step 1–4 顶部指示器），不是 modal——用户可中途返回上一步修改；
  每步「下一步」按钮做即时校验。
- 深链：`/download#install?version=1.21.1&loader=fabric` 预填步骤 ①②（主页速装卡使用）。

### 2.2 步骤 ①：选择游戏版本

```
┌────────────────────────────────────────────────┐
│ [🔍 搜索版本号________]  [类型: 正式版 ▾]  [⟳]    │
├────────────────────────────────────────────────┤
│ ○ 1.21.4        2026-xx-xx   [正式版]           │  ← RmListRow 44px
│ ○ 1.21.3        2026-xx-xx   [正式版]           │     虚拟滚动（版本 700+）
│ ○ 1.21.4-rc1    …           [预览版]            │
│ ○ 24w14potato   …           [愚人节]             │
└────────────────────────────────────────────────┘
```

- 类型过滤：正式版（默认）/ 快照 / 远古版本（old_beta/old_alpha 合并）/ 全部。
- 已在本地的版本（对照 profilesStore/版本仓库缓存）行尾显示「已安装」tag，仍可选（多实例）。
- 选中即高亮，「下一步」激活。

### 2.3 步骤 ②：Modloader（可跳过）

```
○ 不安装（纯净版）                    ← 默认选中
┌────────┐┌────────┐┌────────┐┌────────┐
│ Fabric ││NeoForge││ Forge  ││ Quilt  │
│ 8.x    ││        ││        ││        │
└────────┘└────────┘└────────┘└────────┘
…（OptiFine / LiteLoader / Cleanroom / LegacyFabric，同 04 §5 卡片）
选中 loader → 弹出版本列表（modloader_list，stable 优先）→ 选定回填
```

- 无 loader 卡片在当前 game_version 无可用版本时（`modloader_list` 返回空）置灰 + 「该版本不支持」。
- Fabric/Quilt 安装提示（HMCL 模式）：选中后下方黄条「大多数 Fabric Mod 还需要 Fabric API，
  安装完成后可在 Mod 仓库一键安装」——**不**自动装（后端无依赖安装链，P7 后升级为自动建议）。

### 2.4 步骤 ③：实例信息

```
实例名称   [1.21.4-fabric________]     ← 默认名规则：纯版本 = 版本号；
                                        有 loader = {版本}-{loader}
最大内存   默认跟随全局（4096 MiB） [自定义…]   ← 90% 用户不动
图标       [默认]（首字母色块）              ← 阶段一不做自定义图标上传
```

- 名称即时校验：非空 / ≤64 / 与现有实例不重名（行内红字）。
- 「自定义内存」点开才展开滑条（渐进披露）。

### 2.5 步骤 ④：安装执行 → 完成页

- 提交序列：`profile_create(name, game_version)` → 拿到 profile.id →
  `version_install(version, profile_id)` →（若选了 loader）等 `download://complete`
  后 `modloader_install(profile_id, loader_type, loader_version)`。
  > 后端约束：`version_install` 立即返回、后台执行；loader 安装需在游戏文件就绪后发起，
  > 前端按 `download://complete(batch)` 事件串联（无 loader 则跳过）。
- 执行页显示阶段清单（下载游戏文件 → 安装 Modloader → 完成），进度数据来自任务中心；
  本步可离开页面（进度进 Rail 徽标），不强制等待。
- 完成页三按钮：**立即启动**（launch_start）/ 查看实例（→ /instances/:id）/ 去装 Mod
  （→ `#mods?instance=<id>`，带上下文）。完成后 profilesStore.refresh()。

### 2.6 备用入口：绑定本地已有版本

步骤 ① 顶部折叠入口「使用本地已有版本文件（高级）」：下拉列出 `instance_list` 的
InstanceInfo（game_version + has_jar/has_json + modded）→ 选中后跳过下载，
`profile_create` 直接绑定（version_install 对已完整版本走秒过 fast path）。
面向从旧启动器迁移磁盘文件的用户；默认折叠。

## 3. 分类二：Mod 仓库（Modrinth）

### 3.1 列表视图

```
┌──────────────────────────────────────────────────────┐
│ 🔍 [sodium________]  游戏版本[1.20.4▾] 加载器[fabric▾] │
│    来源: Modrinth（CurseForge 即将支持）   [搜索]      │
├──────────────────────────────────────────────────────┤
│ ⚓ 正在为实例「sodium 实验存档」浏览 · [退出上下文 ✕]    │  ← 有 instance 参数时
├──────────────────────────────────────────────────────┤
│ [🖼] Sodium                    [性能] [fabric]        │
│      CaffeineMC · 1.2M 下载                          │
│      现代化的渲染引擎，大幅提升帧率…                    │
│ [🖼] Iris Shaders              [光影] [fabric]        │
│ …                                                    │
├──────────────────────────────────────────────────────┤
│ [|‹] [‹]  第 1/12 页 [›] [›|]     每页 20 ▾           │
└──────────────────────────────────────────────────────┘
```

- 搜索参数映射 `mod_search(query, game_version, loader, page, limit)`；**筛选变化即自动搜索**
  （300ms 防抖 + 请求序号防竞态，HMCL 模式）；结果虚拟滚动。
- 实例上下文（`?instance=`）：版本/loader 筛选锁定为实例值（下拉禁用 + ⚓ 提示条）；
  「退出上下文」回全库模式。
- 结果行：icon（缺省占位字母块）、name、authors、download_count 缩写、description 一行截断、
  categories → RmTag。

### 3.2 详情视图（点击结果进入，页内二级视图，面包屑 ‹返回结果）

```
[🖼] Sodium
     CaffeineMC · 1,234,567 下载 · fabric 性能
     完整描述（可滚动）…                    [在浏览器打开 ↗]
────────────────────────────────────────────
⚓ 推荐用于 1.20.4 · fabric          ← 有实例上下文时置顶
  ▸ sodium-fabric 0.5.8+mc1.20.4   2026-xx-xx [fabric]
     [安装到本实例] [复制链接] [浏览器下载]
────────────────────────────────────────────
▸ 1.21.x（折叠组，按游戏版本倒序）
  ▸ sodium-fabric 0.6.x+mc1.21  …
▸ 1.20.x
  ▸ …
```

- 版本数据 `mod_get_versions(mod_id)`；推荐版本 = 首个满足
  `game_versions ∋ 实例版本 && loaders ∋ 实例 loader` 的版本（前端筛选）。
- 版本行操作：
  - **安装到实例**：有上下文直接装；无上下文 → `RmDialog` 选实例（列表 + 兼容性标注：
    `version.loaders` 与 `profile.mod_loader` 不匹配的实例行内黄条警告，仍可强行装
    ——vanilla mod loaders 为空视为兼容）；
  - 装即 `mod_install(profile_id, version)`（SHA1 校验由后端保证），toast「已安装到 X 的 mods/」
    + 「查看实例」动作；重复文件名后端处理，前端 toast 提示；
  - **复制链接**：`mod_get_download_url(version_id)` → 剪贴板；
  - **浏览器下载**：`open_external_url(download_url)`。
- 「已安装」标记：对照目标实例 `mod_list` 文件名（上下文模式），行尾显示 ✓。
- 依赖提示（后端 P7 前的替代方案）：版本行下方灰字「依赖信息暂不可用；装完可在
  实例详情『检查缺失前置』中核对」（纯逻辑功能，跳实例详情，见 04 §4）。

## 4. 分类三：整合包（占位）

`RmEmptyState`：图标 `inventory_2` +「整合包安装规划中」+ 说明（将支持 Modrinth .mrpack 与
CurseForge zip 一键导入：选包 → 预览内容 → 下载全部文件 → 生成实例）+「关注版本更新」。
不做假按钮。后端缺口见 00 §9-P10。

## 5. 状态与错误

- 搜索失败（网络）：结果区 `RmErrorState`（「Modrinth 不可达，检查网络或稍后重试」）。
- `version_list` 失败：步骤 ① 空态 + 重试（后端有 1h 缓存兜底，命中时顶部提示「离线缓存，
  数据可能滞后」）。
- modloader 列表加载失败：卡片网格骨架 + 重试。
- 安装失败（download://error）：任务中心失败详情 + 本页 toast「安装失败：{人话原因}」，
  完成页提供「重试缺失文件」（对已建 profile 再发 version_install，幂等补缺）。

## 6. 接口与事件映射

`versions.list/install`、`profiles.create`、`modloader.list/install`、`mods.search/get_versions/
install/get_download_url`、`instance.list`（绑定本地版本）、`open_external_url`；
事件 `download://progress/complete/error`、`modloader://progress`、`instances://refreshed`。

## 7. 渐进披露自查

一级：版本搜索/类型过滤、loader 卡片、实例命名、Mod 搜索三筛选、安装按钮；
二级：每页条数、绑定本地版本（折叠）、无上下文装 mod 的实例选择弹窗；
三级：无（镜像/并发在设置页）。

## 8. i18n key 提案（节选）

`download.tab.install|mods|modpack` /
`download.wizard.step.version|loader|info|execute` /
`download.wizard.versionType.release|snapshot|old|all` /
`download.wizard.loader.skip` `download.wizard.loader.apiHint` /
`download.wizard.nameDefault {version} {loader}` /
`download.wizard.bindLocal` /
`download.wizard.done.launch` `download.wizard.done.view` `download.wizard.done.mods` /
`modStore.search.placeholder` `modStore.contextBar {name}` `modStore.exitContext` /
`modStore.recommended {version} {loader}` `modStore.installTo` `modStore.installed` /
`modStore.copyLink` `modStore.openInBrowser` `modStore.dependencyHint` /
`download.modpack.placeholder.*`

## 9. 验收要点

- 完整向导：1.20.4 + Fabric → 实例可启动（端到端一次）。
- 深链预填：主页速装卡芯片进入后步骤 ①② 已选中。
- 上下文浏览：从实例进入 → 版本/loader 锁定 + 推荐版本置顶；退出上下文恢复。
- 搜索防抖与竞态：快速连续改筛选不出现旧结果覆盖新结果。
- 无上下文安装弹窗的 loader 兼容警告正确（fabric mod + forge 实例 → 黄条）。
