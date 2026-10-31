# 08 · 设置页

> 路由 `/settings`（子导航 hash：`#game-defaults` / `#general` / `#download` / `#storage` / `#plugins` /
> `#advanced` / `#about`）。对标 HMCL LauncherSettingsPage 的三组左导航结构，
> 并补齐现状缺失的镜像源/并发/更新检查/版本仓库管理。

## 1. 设计目标

1. 全局配置唯一入口；与实例设置共享同一套字段组件（继承模型的「被继承层」）。
2. 高频项（语言、外观）直出；下载类中频项成区；危险/底层项（版本仓库、data_dir）下沉到「高级」。
3. 插件贡献的设置页（settingsPages）在此获得宿主——补上全链路缺的最后一环。
4. AI 服务与外部 Agent 控制（MCP）的全局配置入口：一次配置全局使用（ADR-013，
   UX 详见 [07_AI_AND_MCP.md](07_AI_AND_MCP.md)）。

## 2. 布局

```
┌──────────────┬──────────────────────────────────────────┐
│ 游戏          │                                          │
│ ▤ 游戏默认    │            当前分区内容                    │
│ ────────────│         （表单单列 max-width 720px）        │
│ 启动器        │                                          │
│ ▣ 通用        │                                          │
│ ⤓ 下载        │                                          │
│ 🤖 AI 与控制   │
│ ▦ 插件        │                                          │
│ ────────────│                                          │
│ ⚙ 高级        │                                          │
│ ⓘ 关于        │                                          │
└──────────────┴──────────────────────────────────────────┘
```

## 3. 分区 · 游戏默认（#game-defaults）

顶部说明条：「以下值是所有实例的**继承基线**；实例详情中可逐项覆盖」+ [查看实例如何覆盖 ›]
（跳实例文档说明，不做弹窗教学）。

| 区块 | 一级直出 | 三级折叠 |
|---|---|---|
| 目录 | 游戏根目录 `game_root_dir`（输入 + 「修改后新实例生效，已有实例路径不变」提示） | — |
| Java | 默认 Java（自动 / `java_detect` 列表：版本+厂商+64bit+路径 / 自定义路径） | — |
| 内存 | 自动内存开关 `auto_memory`；手动时 min/max 滑条（`default_min/max_memory_mb`） | — |
| 隔离 | 默认隔离策略 `default_isolation`（共享/独立 二选一 + 说明） | — |
| 窗口 | 默认宽高（`default_window_width/height`）、全屏 `default_fullscreen` | — |
| 进程 | 默认优先级 `default_process_priority` | — |
| 参数 | — | 默认 JVM 参数 `default_jvm_args`、默认游戏参数 `default_game_args`（textarea，一行一条） |

保存：变更即 `settings_set`（后端防抖写盘）；校验错误行内。

## 4. 分区 · 通用（#general）

| 区块 | 内容 |
|---|---|
| 语言 | `settings_supported_locales` 下拉（显示 native_name）；切换即时生效（现有 i18n 机制），无需重启 |
| 外观 | 主题卡列表（`dashboard_list_themes`：8 套内置 Noctis 主题 + 插件主题 mode="custom" 标注来源）+ 首卡「自适应」；不再提供“跟随系统”卡（旧 `null` 偏好仍按系统解析；选择任一主题后写入显式 key）。每卡显示主题场景缩略图（背景/三色角色/surface 条），选中即切换；「查看主题 JSON」（`dashboard_get_theme_json`，调试入口，折叠）。 |
| 背景 | 宿主默认氛围层（`AmbientStage`）：没有图片/视频壁纸时按主题渲染渐变 wash、缓动 veil、地平线、透视网格与主题粒子；图片/视频壁纸或 v2 主题背景存在时该层不创建。图片壁纸卡位于动态视频卡上方：本地 PNG/JPEG/WebP/GIF/BMP/AVIF（≤32 MiB）经 `dialog_pick_background_image` + `background_image_prepare` 授权后只读显示；遮罩 0–80%。图片与 MP4 动态壁纸互斥（启用/选择其一会关闭另一个），图片层位于视频层之上。 |
| 玻璃 | 主面板不透明度 0–100%（`--rmccc-panel-alpha`）、卡片不透明度 0–100%（`--rmccc-card-alpha`）、主面板卡片模糊度 0–40 px（`--rmccc-card-blur`）。0% 时对应层真正透明；图片/视频壁纸下主页卡片模糊度自动取设置值的 55%。 |
| 更新 | 检查更新开关 `check_updates`（settings_set_user）+ [立即检查]（后端 P9 就绪前灰置「即将支持」） |
| 日志 | 内嵌日志查看器：[刷新] 拉取 `log_get(lines)`（默认 200，可调 500）等宽字体滚动区 + 级别着色（ERROR=danger）+ [复制] [清空]（`log_clear` 确认）；「打开日志目录」待 open_path（P1） |

### 外观 · 自适应主题

- 采样图片的 4 个区域（整图/中心/左上/右下）或视频整段的 5 帧（8%/30%/52%/74%/90%）
  加可见当前帧；跨域或 seek 失败时退回当前帧，不向网络发送像素。
- 多帧 OKLab k-means（k=6）后按覆盖度 × 色度 × 中心偏置选出代表色；primary 优先
  有色度的代表色，secondary/tertiary 至少相隔 30°。
- 生成 dark/light 两套 surface stack，**跟随当前主题明暗模式**；所有关键对 ≥4.5:1，
  卡片显示来源、采样帧/像素数、模式和置信度；失败时回退当前主题 accent。
- 存储键 `rmccc.adaptivePalette.v2`；v1 结果自动失效。关闭自适应恢复 8-token 主题。

## 5. 分区 · 下载（#download）

| 项 | 控件 | 说明 |
|---|---|---|
| 下载源 `mirror_url` | 单选卡：官方源（Mojang/Modrinth 直连）/ BMCLAPI 镜像（国内加速） | 补齐现状缺失字段；卡片副文案解释适用人群（「国内网络显著提速；镜像数据可能略有延迟」） |
| 并发数 `download_concurrency` | 滑条 1–64，默认 16 | 副文案「过高可能被源站限速」 |
| 事件节流 `event_throttle_ms` | 数字输入（RmAdvancedSection 内） | 进度事件频率，默认 100ms |

> 代理设置：后端 AppConfig 无此字段（HMCL 有）。不放假控件；在分区底部灰字
> 「代理支持规划中」。缺口记录于 00 §9。

## 6. 分区 · 存储管理（#storage）

> 2026-10-28 新增。用于回答「空间被谁占了」，并提供唯一的安全清理入口。

```
┌ 存储概览 ────────────────────────────────────────────────┐
│ 总占用 3.2 GiB │ 启动器数据 820 MiB │ 游戏目录 2.3 GiB │ 可清理 816 MiB │
│ [一键清理缓存]  只清理可再生成的下载/图标缓存                 │
├ 启动器目录与配置 ─────────────────────────────────────────┤
│ 🗑 下载与图标缓存   ~/…/rmccc/cache        816 MiB  [可清理] │
│ 📁 启动器配置       ~/…/rmccc/config        12 KiB          │
│ 📁 运行日志         ~/…/rmccc/logs          1.1 MiB         │
│ 📁 已安装插件 … / 插件数据 … / 运行状态 … / WebView 缓存 …   │
├ 游戏目录（可展开） ───────────────────────────────────────┤
│ ▸ Default   ~/…/rmccc/instances     2.3 GiB   [打开目录]     │
│   ▸ 展开：                                                  │
│     共享资源 libraries/assets/…      1.8 GiB                 │
│     1.20.4-Fabric   420 MiB          [打开实例]              │
│     …                                                       │
└───────────────────────────────────────────────────────────┘
```

- 数据来自 `storage_overview`：启动器数据按 data dir 一级条目分类（配置/缓存/日志/插件/
  状态/Agent/Dashboard/私有数据/旧 profile/顶层配置文件），另加 macOS WebView 缓存；
  游戏目录单独统计，避免与 `{data_dir}/instances` 重复计数。
- 每个游戏目录可展开显示 `versions/{id}` 的逐实例占用与「共享资源」
  （libraries/assets/其他未归入实例的文件）；实例行可直接进详情页删除。
- 「一键清理缓存」只删除 `{data_dir}/cache`（下载对象、版本清单、图标缓存），
  不触碰存档、模组、实例、插件、配置；确认对话框写明后可执行，成功 toast 释放量并刷新。
- 大目录扫描在 `spawn_blocking` 中执行，不阻塞 IPC/主线程。

## 7. 分区 · AI 与外部控制（#ai）

全局唯一入口：**AI 服务**（OpenAI 兼容端口：地址/密钥/自动检测模型/流式）与
**外部 Agent 控制**（MCP 服务器：开关/令牌/会话/工具权限/最近操作）。
一次配置全局使用——启动器内置功能（日志一键 AI 分析）与插件（桥 `llm.chat`）
共用同一 AI 端口；外部 AI 工具（ZCode 等）经 MCP 控制启动器。

本分区的完整交互规格（布局示意图、检测流、令牌管理、权限矩阵、渐进披露、
i18n、验收）统一维护在 [07_AI_AND_MCP.md](07_AI_AND_MCP.md)，此处只做入口声明：

- AI 服务子区：地址输入即检测模型（`ai_models_probe`）→ 模型下拉自动填充 →
  密钥入凭据保险库 → 「保存并测试」；
- MCP 子区：总开关（默认关）+ 端口/令牌 + 一键复制接入配置（HTTP 与 stdio 两种）+
  在线会话 + 工具权限矩阵（31 工具按 只读/操作/破坏性 分组）+ 最近 Agent 操作；
- 运行期反馈不在本页：归因徽标/确认卡在任务中心（10 §3），日志 AI 分析在日志抽屉（10 §4）。

## 8. 分区 · 插件（#plugins）—— 插件设置页宿主

```
┌──────────────────────────────────────────────┐
│ server-dashboard 插件提供的设置                 │
│ ┌──────────────────────────────────────────┐ │
│ │ (iframe: rmcccplugin://server-dashboard/  │ │
│ │  ui/index.html?page=settings)             │ │
│ │  400px 高，与 Dashboard 卡片同一桥协议       │ │
│ └──────────────────────────────────────────┘ │
│ 🔒 该插件已获得 settings.read 能力…            │
└──────────────────────────────────────────────┘
```

- 数据：`plugin_list` 过滤 `settingsPages > 0` 的插件（每插件一卡）；
  iframe 懒加载（进入分区才渲染）；桥复用 `plugin_ui_invoke`（与卡片同一实现，`?page=` 区分）。
- 空态：「暂无插件提供设置页」+ [管理插件 ›] → /plugins。
- 权限撤销/插件禁用时：卡片显示占位（同 02 §3.5 权限态）。

## 9. 分区 · 高级（#advanced）

### 8.1 版本仓库管理（instance_list 等命令的唯一 UI）

```
顶部固定说明框：「版本仓库 = 磁盘 versions/ 下已下载的版本文件，不是你的游戏实例。
普通用户无需关心；删除实例不会删除这些文件，可在此清理。」
工具条：[⟳ 重新扫描]（instance_refresh → instances://refreshed 自动刷新表）
表格（RmListRow）：
│ 1.20.4            │ [正式] [已补丁] │ 完整        │ [⋯] │
│ 1.20.4-fabric-0.15│ [fabric 补丁]   │ 缺失 3 个    │ [⋯] │
│ 1.21              │ [正式]          │ 仅 JSON      │ [⋯] │
列 = InstanceInfo: id / game_version / modded / has_jar / has_json / missing_count
行 [⋯]：重命名（dialog 校验合法 id → instance_rename）/ 复制（输入新 id →
instance_duplicate）/ 删除（确认「实例引用该版本时可能无法启动」→ instance_remove）
```

- 「缺失 N」列直接用 InstanceInfo.missing_count（后端轻量统计），
  深度体检去实例详情维护 tab。
- 被任何 profile 引用的版本行尾显示「被 N 个实例引用」（前端对照 profiles.list 的
  game_version/mod_loader 推断），删除前确认弹窗列引用实例名。

### 8.2 其他

- 数据目录 `data_dir`：只读显示 + 复制 + 「在系统中打开」（待 P1）；
  修改引导到 CLI/配置文件说明（折叠说明，不给 UI 输入——改 data_dir 是重运维操作）。
- 前端诊断：frontend_log 机制说明（只读说明卡，无操作）。

## 10. 分区 · 关于（#about）

应用名 + 版本 + 构建信息（前端读 package version）；
体积承诺说明（「主程序 < 10 MiB」当前实测值由 CI 写入的常量显示——若无则省略）；
开源地址/文档链接（`open_external_url`）；[立即检查更新]（同 §4，P9 依赖）。

## 11. 状态与错误

- settings_get/set 失败：分区顶部行内错误条 + 重试；不做全局弹窗。
- 版本仓库操作失败（重名/非法 id/文件占用）：dialog 内行内错误，不关弹窗。
- 主题切换失败（损坏主题）：回退内置 + toast（现有降级逻辑）。

## 12. 接口映射（本页全量）

`settings_get/set/get_user/set_user/supported_locales/detectJava`、
`dashboard_list_themes/get_theme_tokens/set_theme_preference/get_theme_preference/get_theme_json`、
`instance_list/refresh/rename/duplicate/remove`、`storage_overview/storage_clean_cache/instance_sizes`、
`log_get/clear`、`open_external_url`、
`plugin_list`（插件分区）、`ai_*` 5 条 + `mcp_*` 9 条（设计新增，见 07 §8）；
事件 `instances://refreshed`、`ai://delta/done/error`、`mcp://session/confirmation`。

## 13. 渐进披露自查

一级：语言、外观模式与主题、下载源、并发、Java 默认、内存默认、AI 地址/密钥/模型、
MCP 总开关与接入配置；
二级：主题 JSON、日志查看器、插件设置页、MCP 会话与令牌管理、最近 Agent 操作；
二级补充：存储管理的游戏目录展开、逐实例占用；
三级：默认 JVM/游戏参数、事件节流、版本仓库、data_dir（整个「高级」分区本身就是三级）、
MCP 逐工具权限矩阵。

## 14. i18n key 提案（节选）

`settings.nav.gameDefaults|general|download|storage|plugins|advanced|about` /
`settings.storage.overview|total|launcher|games|cleanable|clean|cleanTitle|cleanBody|cleaned` /
`settings.game.intro` `settings.game.rootDir` `settings.game.rootDirNote` /
`settings.general.language` `settings.general.appearance.mode` `settings.general.theme {name}` /
`settings.general.updates` `settings.general.updates.checkNow` /
`settings.general.log.viewer` `settings.general.log.clear` /
`settings.download.mirror.official` `settings.download.mirror.bmclapi` /
`settings.download.concurrency` `settings.download.proxyFuture` /
`settings.plugins.host.empty` `settings.plugins.host.permissionNote {capability}` /
`settings.advanced.repo.title` `settings.advanced.repo.intro` `settings.advanced.repo.rescan` /
`settings.advanced.repo.rename` `settings.advanced.repo.duplicate` `settings.advanced.repo.remove` /
`settings.advanced.repo.referencedBy {n}` `settings.advanced.dataDir` / `settings.about.*`

`settings.ai.*` / `settings.mcp.*` 命名空间的完整清单见 07_AI_AND_MCP.md §10。

## 15. 验收要点

- `AppConfig` 15 个可写字段全部有控件（对照 SettingsUpdate 逐项 checklist）。
- 存储管理：`cache/` 有内容时显示可清理量；清理后 cache 消失、实例/存档不变；
  展开游戏目录能看到每个实例的体积。
- 版本仓库：复制→重命名→删除链路 + 引用计数显示正确。
- 主题切换即时生效且「跟随系统」在系统外观切换时联动（matchMedia 监听，现有机制）。
- 镜像切换后新下载走 BMCLAPI（抓包或后端日志验证，人工验收项）。
- 插件设置页：server-dashboard 样例的设置页在本分区正常渲染并可交互。
- AI 与外部控制分区与 07 文档一致（AI 配置三步走、MCP 接入配置一键复制、
  权限矩阵），验收细则见 07 §11。
