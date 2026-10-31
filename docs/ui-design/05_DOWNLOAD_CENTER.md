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

- **单命令**：`instance_install(directoryId, mcVersion, instanceId, loaderType?, loaderVersion?)`
  立即返回，后台完成「下载原版 → 提交实例 →（可选）安装 loader → 改名/选中」。
  - 无 loader：原版资源直接写入 `versions/{实例名}`，实例名可以不是 MC 版本号。
  - 有 loader：原版先装到 `versions/{mcVersion}`（loader 的父版本必须可解析），
    loader 产物（如 `1.21.1-fabric-0.16.9`）成功后再改名为向导里填的实例名。
  - 命令立即返回 `{ task_id }`；任务在**后端同步创建**，前端不再创建 pending 行，
    也不需要按 batch id 重新绑定。loader 阶段是同一任务的第 3 步，不另起一行。
- 执行页显示阶段清单（下载游戏文件 → 安装 Modloader → 提交实例），进度数据来自
  统一任务快照（任务中心）；本步可离开页面，不强制等待。
- 完成页三按钮：**立即启动**（launch_start）/ 查看实例（→ /instances/:id）/ 去装 Mod
  （→ `#mods?instance=<id>`，带上下文）。完成后 profilesStore.refresh()。

### 2.6 备用入口：绑定本地已有版本

步骤 ① 顶部折叠入口「使用本地已有版本文件（高级）」：下拉列出 `instance_list` 的
InstanceInfo（game_version + has_jar/has_json + modded）→ 选中后跳过下载，
`profile_create` 直接绑定（version_install 对已完整版本走秒过 fast path）。
面向从旧启动器迁移磁盘文件的用户；默认折叠。

## 3. 分类二：Mod 仓库（Modrinth）

> 2026-10-29 重构（对齐 HMCL 下载页）：搜索卡片压缩为「来源 / 结果数 / 翻页」+ 单行筛选，
> 结果行直接展示兼容游戏版本与加载器；详情按游戏版本折叠分组，显示版本类型、日期、文件大小
> 与已安装标记。以下为现行设计。

### 3.1 列表视图

```
┌──────────────────────────────────────────────────────┐
│ Modrinth  CurseForge(即将)  14 个 Mod   ‹ 第1/1页 ›   │
│ 🔍 [sodium______] [游戏版本▾] [加载器▾] [排序▾] [搜索] │
├──────────────────────────────────────────────────────┤
│ ⚓ 正在为实例「sodium 实验存档」浏览 · 1.20.4·Fabric ✕ │  ← 有 instance 参数时
├──────────────────────────────────────────────────────┤
│ [🖼] Sodium  CaffeineMC             [fabric]          │
│      现代化的渲染引擎…      支持 [1.20.4] [1.20.3] +2  ↓46M › │
│ [🖼] Iris Shaders …                                  │
└──────────────────────────────────────────────────────┘
```

- 搜索参数映射 `mod_search(query, game_version, loader, page, limit, index)`；**筛选变化即自动搜索**
  （300ms 防抖 + 请求序号防竞态，HMCL 模式）。
- 实例上下文（`?instance=`）：版本/loader 筛选锁定为实例值（下拉禁用 + 一行 ⚓ 提示条）；
  「退出上下文」回全库模式；筛选版本在结果行版本标签中置顶。
- 结果行：icon（缺省占位字母块）、name、authors、description 一行截断、下载量、
  **兼容游戏版本（前 2 个 +N）与 loader 标签**；categories 仅在详情展示。
- 分页在搜索卡片右上角（`‹ 第 x/y 页 ›`），不再占用结果区底部一行。
- 搜索卡片与结果面板为 82% 磨砂玻璃（`backdrop-filter`），壁纸仍可见；密集小字使用
  比 `--rmccc-text-muted` 更深的 `--store-muted`，避免在视频背景上失去对比度。
- 搜索命中新增 `game_versions`（Modrinth `versions`）与 `loaders`；后者缺失时从
  `categories` 中提取 loader 分类（`fabric/forge/neoforge/quilt/liteloader/…`）。

### 3.2 详情视图（点击结果进入，页内二级视图，面包屑 ‹返回结果）

```
[🖼] Sodium                                 [项目主页 ↗]
     CaffeineMC · 46.1M 下载 · optimization / fabric
     现代化渲染引擎…（两行，超出省略）
────────────────────────────────────────────
⚓ 推荐版本 · Minecraft 1.21.1              [目标实例]
  ●稳定版 0.6.9+mc1.21.1  ✓已安装 推荐   [安装] [链接] [↗]
────────────────────────────────────────────
版本列表 5 个版本   [全部版本▾] [全部加载器▾] [全部|稳定|测试|快照]
▾ Minecraft 1.21.5   2 个版本              2026-06-04
  ●稳定版 0.6.9+…     日期·大小·[Fabric]   支持[1.21.4] [安装]…
  ●测试版 0.7.0-beta… …
▸ Minecraft 1.21.1（默认收起，点开）
▸ Minecraft 1.20.4（默认收起）
```

- 版本数据 `mod_get_versions(mod_id)`；新增 `published` / `version_type` / `file_size`。
  推荐版本 = 首个满足 `game_versions ∋ 实例版本 && loaders ∋ 实例 loader` 的版本（前端筛选）。
- 版本按一个主游戏版本折叠分组（默认展开最新 2 组；有游戏版本筛选时聚焦该版本），
  组内按发布日期倒序；顶部提供游戏版本 / 加载器 / 版本类型筛选。
- 版本行显示发布通道（稳定 / 测试 / 快照）、版本名/版本号、日期、文件大小、loader、
  额外兼容版本；有实例上下文时对照 `mod_list` 显示 ✓ 已安装。
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
- 安装失败（`task://updated` 的 `failed` 终态）：任务中心失败详情 + 本页 toast
  「安装失败：{人话原因}」，
  完成页提供「重试缺失文件」（对已建 profile 再发 version_install，幂等补缺）。

## 6. 接口与事件映射

`versions.list/install`、`profiles.create`、`modloader.list/install`、`mods.search/get_versions/
install/get_download_url`、`instance.list`（绑定本地版本）、`open_external_url`、
`task_list` / `task_cancel` / `task_pause` / `task_resume`；
事件 `task://updated` / `task://sync`、`instances://refreshed`。

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
