# 02 · 页面与组件层审计

> 审计对象：`src/pages/`（8 页）+ `src/components/`（9 个全局组件）+ `src/components/rm/`（3 个基础组件库文件）
> 交叉对照 `src/ipc/api.ts`（后端能力边界）与 `src/styles/app.css`。
> 每节含：功能清单（用于 HMCL 对比基线）→ 分级问题（带 `文件:行号`）。

---

## 0. 路由与 Home / HomeDashboard 并存判定（核心结论）

- `src/main.tsx:21-38`：`/` 路由到 `./pages/Home`——**Home.tsx 在用**；`HomeDashboard.tsx` 全仓 grep 仅剩自身定义（:28），**无任何 import 引用，是 645 行死代码**。遗留路由 `/login→/accounts`、`/mods→/download#mods`、`/agent→/settings#ai`、`*→/` 均为 Navigate 重定向，正常。
- TitleBar Dock 导航六项（`TitleBar.tsx:16-23`）：主页 / 实例 / 下载 / 账号 / 插件 / 设置，与路由一致。

**死代码版本反而保留了 5 项在用版丢失/退化的实现**（重构时值得回捞）：

| 功能 | Home.tsx（在用） | HomeDashboard.tsx（死代码） |
|---|---|---|
| 卡片布局持久化 | localStorage `rmccc.heroLayout.v2`（:53），仅顺序+隐藏 | 后端 `dashboard.getLayout/saveLayout` 12 列网格（:40/:51/:127），含位置/尺寸/断点 + corrections 提示 |
| 卡片编辑 | 上移/下移/隐藏/恢复（一次仅恢复第一张，:250）/重置 | 宽度循环 2/3/4/6/8/12（:61）/隐藏/批量恢复（:108） |
| 启动入口 | 大按钮 + 实例下拉 | 五态机：无实例 CTA 引导态（:356）/运行中停止+日志+设置（:364）/最近游玩 3 条（:434） |
| 无实例空态 | 无专门空态 | 有引导卡（:358-361） |
| 下载卡 | 进度列表**无取消按钮**（:637-663） | 每行取消按钮（:633-637）+ 空态 CTA |
| 账号卡 | 无换肤入口 | 正版"更换皮肤"跳官网按钮（:509-516） |
| 版本卡 | 8 个 chip | 4 chip + **粘贴版本号输入框**（:589-596） |
| 插件卡 | 固定 200px iframe（app.css:1656） | 网格宽度可调 + **ErrorBoundary 包裹**（损坏卡片可重试/隐藏，:189-206） |

另有死代码连带的页面级问题：`.dashboard-grid`（app.css:879,1497-1498）、`.quick-launch`、`.dash-*` 等仅被死页使用。

---

## 1. Home.tsx（在用首页，716 行）

**功能清单**：卡片编辑模式（添加卡片对话框/恢复隐藏/重置布局/每卡上移下移隐藏）；内置卡：服务器信息卡（地址输入+连接按钮+四态+MOTD/人数/延迟/版本）、版本比对卡（客户端 vs 服务端+更新按钮）、公告卡（3 条）；隐藏卡：实例卡、版本卡（8 个主版本 chip）、下载卡（批次进度）、运行游戏卡（日志/停止/跳转）；插件卡（rmcccplugin:// iframe）；账号卡（头像/provider/最后游玩/实例数/id 前 8 位；切换/管理/登出）；启动卡（大按钮+实例选择+离线用户名兜底对话框 :187-201）。

| 级别 | 问题 | 位置 |
|---|---|---|
| **P0** | **三张默认卡片全部为假数据且无任何"演示"标识**：服务器卡 mock MOTD/人数/延迟（注释自认"演示数据，后端未接入"）、版本比对卡服务端版本硬编码 "26.2"、公告卡 3 条硬编码。用户会把假在线状态/假公告当真实功能 | `Home.tsx:423-480,484-541` |
| P1 | 版本卡跳 `/download#install?version=xxx`：`?version=` 写在 hash 内，`useSearchParams` 读不到（详见 Download 页 BUG-1），版本预选失效 | `Home.tsx:617,627` |
| P1 | 停止运行中游戏无确认 | `Home.tsx:681` |
| P1 | 登出无确认 | `Home.tsx:225` |
| P1 | `dashboard.listWidgets`、`versions.list` 失败被静默吞掉（无 toast、无错误态） | `Home.tsx:85,598` |
| P2 | 首页整体无实例时无引导空态（死代码版本有）；首帧无 skeleton（profiles/auth/catalog 均无 loading 态） | 全页 |
| P2 | "恢复隐藏卡片(n)"标签显示数量 n 但一次只恢复 `hidden[0]`，具误导性 | `Home.tsx:250` |
| P2 | `createMemo` 内执行副作用（默认选中）；字符串比较版本号（"9.9" > "26.2" 误判，演示数据但逻辑即错） | `Home.tsx:88,488` |
| P3 | 硬编码中文：`"欢迎来到 Example 生存服务器！"`(:430)、`"26.2 · Fabric 0.20.1"`(:433)、公告文案(:538-540)、默认地址(:426)、卡片副标题 "RMCCC"(:171) | 同左 |
| 布局 | `.hero-stack` 固定 `min(460px,42%); min-width:300px`（app.css:1640-1641）+ `.hero-corner` 固定 340px 绝对定位右下（app.css:1687-1698），**无断点**：800px 最小窗宽下重叠遮挡 | `app.css:1640-1698` |

---

## 2. Instances.tsx（实例列表，446 行）

**功能清单**：新建实例按钮；工具栏（搜索/游戏目录过滤下拉/目录管理弹窗/排序下拉[最近游玩/名称/创建时间，持久化]/卡片-列表视图切换/刷新）；Profile 卡片（图标[首字母兜底]/版本+loader 标签/最后游玩/运行中圆点/启动停止/更多菜单）；每实例菜单（启动/详情/浏览 Mod/重命名[prompt]/删除[typeToConfirm]）；目录浏览态（只读版本清单+缺失数标签）；目录管理弹窗（主目录/额外目录增删过滤）。

| 级别 | 问题 | 位置 |
|---|---|---|
| — | 三态完整性：Profile 列表用 `RmContentState`（loading skeleton/error/empty+CTA）齐全，**全项目最规范的一页** | `Instances.tsx:281-303` |
| P1 | 停止游戏无确认 | `Instances.tsx:326,367` |
| P2 | 目录列表加载失败仅 toast，界面显示 "0 条"（`?? 0` 兜底），易误判为"无版本"，无错误重试 UI | `Instances.tsx:89,252` |
| P2 | 实例图标 `<img>` 无 onError 兜底（仅 null 时有首字母兜底，损坏文件显示裂图）；长实例名无 ellipsis 截断 | `Instances.tsx:443` |
| P2 | 无"打开游戏目录"入口（HMCL 有；本项目 InstanceDetail 也只有"复制路径"） | — |
| P3 | `" · modded"` 英文直出 | `Instances.tsx:262` |
| 布局 | `instance-grid` 用 `auto-fill minmax(220px,1fr)`、toolbar flex-wrap——窄窗可用，本页布局健康 | `app.css:1012-1016` |

---

## 3. InstanceDetail.tsx（实例详情，1072 行，5 个 hash tab）

**功能清单（按 tab）**：
- 公共：面包屑/标题/启动停止/查看日志；RmSideTabs（overview/mods/loaders/settings/maintenance）+ 侧栏工具区（启动停止/Browse 菜单[游戏/运行/mods 目录——仅复制路径]/Manage 菜单[重命名/浏览 Mod/删除]）
- Overview：身份卡（版本/loader/内存/最后游玩）、路径卡（三路径+复制）、快捷操作卡（Mod 商店/缺失扫描/设置）
- Mods：刷新/添加本地 mod/去商店/缺失依赖扫描/搜索；启用列表（启停[乐观更新]/禁用/删除[确认]/复制文件名）；禁用分区；扫描结果对话框（缺失必需列表+一键补装+冲突/未解析计数）
- Loaders：8 种 loader 卡、已装标记、安装进度、fabric/forge 系互斥提示、版本选择对话框（稳定/beta）
- Settings：图标卡（≤512KB 选择/替换/清除）；运行时卡（Java 下拉[自动+探测]/最大内存）；窗口卡（分辨率宽×高/全屏/进程优先级）；高级参数折叠（JVM 参数/游戏参数/wrapper 命令）；高级调试折叠（6 个布尔开关：native glfw/openal/custom natives/debug log/禁用自动 JVM 参数×2）；**字段级继承锁**（每字段独立，未锁定显示继承值）；底部保存按钮（dirty 才可用）
- Maintenance：完整性检查（缺失文件列表+修复）/隔离模式（never/always/modded）/危险区（重命名/删除）

| 级别 | 问题 | 位置 |
|---|---|---|
| **P1** | **Overview 快捷按钮跨 tab 跳转失效**：模块级 `setTab()` 只做 `history.replaceState + dispatchEvent`，组件内 `tab` signal 只初始化一次、从不监听 hash 变化（全仓无 hashchange 监听者）。"缺失扫描"/"设置"按钮点击后 URL 变了但界面停留在 Overview | `InstanceDetail.tsx:285,287,296-299,57` |
| **P1** | **分辨率"高"编辑被静默丢弃**：宽/高两个输入框都只受 `overridden("width")` 控制（disabled 与 hint 均用 width 锁），保存时只遍历 `draftOverrides()`——只锁 width 时 height 可编辑但不写入 `opts.height`；只锁 height 时显示继承值无法编辑。改了不生效且无提示 | `InstanceDetail.tsx:888-890` vs `:797` |
| **P1** | SettingsTab 草稿丢失无警告：`dirty()` 存在但切 tab（Switch 卸载组件）或离开页面时静默丢弃 | `InstanceDetail.tsx:127-134,734` |
| P1 | `resolvedSettings`/`detectJava` 失败被吞（`r.ok &&` 无 else），无 loading、无错误提示，继承值显示 "—" | `InstanceDetail.tsx:762-763` |
| P2 | Loaders 版本对话框打开后若 `modLoader.list` 失败，弹窗停留空列表无错误态（仅 toast） | `InstanceDetail.tsx:558,609-635` |
| P2 | BrowseMenu 在 `instances.paths` 加载完成前点击菜单项静默无效（无禁用态） | `InstanceDetail.tsx:160-162` |
| P2 | `deleteProfile(profile, alsoVersion)` 的 `alsoVersion` 参数被 `void` 忽略——死参数 | `InstanceDetail.tsx:220` |
| P2 | Mods 搜索只过滤启用列表，禁用列表不参与；扫描结果 `conflicts`/`unparsed` 只有计数看不到明细；Mods 列表无虚拟化 | `InstanceDetail.tsx:329-331,513-516,426-477` |
| P2 | **后端有字段 UI 无控件**：`min_memory_mb`/`metaspace_mb`（在 OVERRIDABLE_FIELDS 名单与 draft 里但无输入控件，已存在的覆盖值无法查看/取消）、`env_vars`（完全缺失）、`pre_launch_command`/`post_exit_command`（draft 有 :751-752 但高级区无输入框）、`override_jvm_args`（被 override_properties 机制替代未用） | `types.ts:412-422`、`InstanceDetail.tsx:642-663,741-742,751-752,918-931` |
| P3 | 进程优先级下拉选项 `low/below_normal/normal/above_normal/high` 英文直出（其他枚举均有 i18n） | `InstanceDetail.tsx:907-911` |
| P3 | 图标预览 `<img>` 无 onError 兜底 | `InstanceDetail.tsx:705` |
| 布局 | 侧栏固定 200px（app.css:663-665）+ 内容滚动，窄窗内容区被压；分辨率输入固定 100px×2；Java 下拉无 max-width，长路径撑破字段控件；无断点 | `app.css:663-691` |
| 选项分布 | 5 tab 结构清晰，Settings 4 卡+2 折叠渐进披露良好；危险区在 Maintenance 与 ManageMenu 双入口（均为确认弹窗，可接受） | — |

---

## 4. Download.tsx（下载中心，861 行，4 tab）

**功能清单**：
- Tab：安装向导 / Mod 商店 / 整合包（**纯占位 EmptyState** :59-65）/ 缓存管理
- 安装向导 4 步+完成页：Step1 版本搜索+类型过滤（正式/快照/旧版/全部）+绑定本地版本对话框；Step2 跳过原版+8 种 loader 卡+loader 版本单选；Step3 实例名（必填/≤64/查重）；Step4 摘要+执行（不定进度条+"后台进行"）；Step5 查看实例/去 Mod 商店
- Mod 商店：实例上下文条（⚓+退出）；源切换（Modrinth 可用/**CurseForge 禁用+提示**）；搜索（300ms 防抖）+游戏版本下拉+loader 下拉（仅 fabric/forge/quilt）；结果列表（图标三级回退/名称/作者/下载量/分类/详情）；分页（上/下页+页码）；详情页（描述/⚓推荐版本行/版本列表/安装/复制链接/浏览器打开/依赖提示）；无上下文安装弹"选择目标实例"（兼容性警告标签）
- 缓存管理：开关/容量上限（MiB）/当前大小/条目数/清理（危险确认）

| 级别 | 问题 | 位置 |
|---|---|---|
| **P0** | **hash 内查询参数整体失效**：项目大量跳转使用 `/download#mods?instance=x`（Instances.tsx:155、InstanceDetail.tsx:178/281/415）与 `/download#install?version=x`（Home.tsx:617），而 @solidjs/router 的 `useSearchParams` 解析的是 path 的 search——hash 内的 `?instance=` 恒为 undefined。**"实例→仓库上下文（锁定过滤+推荐版本+直装目标）"与"版本预选"两条链路实际全断**。旁证：同文件 ModDetail:701-705 专门手写 `window.location.hash.split("?")` 解析——作者已知此坑但只修了详情层 | `Download.tsx:411-413,86-96` 及上述跳转源 |
| **P1** | 同页 hash 跳转不切 tab：`tab` signal 仅初始化一次（:42-44），Step5 "去 Mod 商店"（:397）在 Download 已挂载时点击后无任何可见变化。Settings 页同款（:37） | `Download.tsx:42-44,397` |
| P1 | 版本列表 `slice(0, 100)` 硬截断（:237）、Mod 详情版本列表 `slice(0, 30)`（:734），均无"加载更多"/虚拟化/提示；选 old/all 类型时靠后的版本纯浏览永远看不到 | `Download.tsx:237,734` |
| P1 | Step1 无 loading 态（版本未加载时列表就是空的，无 skeleton）；错误态 `RmErrorState` 无重试按钮（onRetry 未传） | `Download.tsx:236-260` |
| P2 | `copyLink` 失败无任何提示（else 缺失）；缓存清理 `clearAll()` 返回 false 时静默失败 | `Download.tsx:511-517,820-822` |
| P2 | 向导执行中不可取消（底层 batch 可在 TaskCenter 取消，但向导 UI 不引导） | `Download.tsx:368-388` |
| P2 | loader 安装失败仅 toast，profile 已创建不回滚、不阻止进入 Step5，用户可能没注意 modloader 没装上 | `Download.tsx:195` |
| P2 | 游戏版本下拉选项未去重（多实例同版本重复项）；商店 loader 筛选缺 neo_forge/optifine（向导支持 8 种，商店仅 3+空） | `Download.tsx:573,580-585` |
| P2 | 缓存管理无 loading/error 态（stats 为 null 时开关默认 true、大小 0） | `Download.tsx:800-861` |
| P3 | `${local.missing_count} missing` 前中后文混排；`"Modrinth · MO"`/`"CurseForge · CF"` 品牌硬编码；Step1 刷新按钮无文字无 title | `Download.tsx:275,550,558,234` |

---

## 5. Accounts.tsx（账号页，256 行）

**功能清单**：无账号欢迎卡（微软/离线大按钮）；会话卡（头像/名称/provider/刷新登录）；账号列表（设为当前/删除[确认]）；添加卡；安全说明折叠；微软设备码对话框（打开链接/验证码+复制/验证码事件到达自动复制 auth.ts:151-168/关闭即清除）；离线用户名对话框（校验+回车）；右侧 3D 皮肤预览（SkinViewer）；登录成功回放挂起启动。

| 级别 | 问题 | 位置 |
|---|---|---|
| P2 | 页面数据无 loading 态（首帧闪现"欢迎卡"再跳列表）、refresh 失败静默 | `Accounts.tsx` 全页 |
| P2 | "刷新登录"对离线账号也显示，点击必然失败弹错误 toast——入口应按 provider 隐藏 | `Accounts.tsx:118` |
| P2 | 设备码流程无法主动取消后端轮询（关闭对话框只清前端状态；authStore 无取消 API——能力缺失） | `Accounts.tsx:191-194` |
| P2 | 删除"当前使用中"账号的确认文案无特殊警示；`subtitle={account.type}` 与 provider 标签重复展示两处 | `Accounts.tsx:116,140` |
| P3 | 步骤序号 `"1. "` `"2. "` 直拼 | `Accounts.tsx:202,206` |
| 布局 | `.accounts-split` 固定 `minmax(0,1fr) 360px` **无断点**；SkinViewer 画布 <50×80 不初始化，极窄时右侧整块只剩"加载中"占位 | `app.css:1959-1963`、`SkinViewer.tsx:146,264-266` |
| 选项分布 | 无账号时"欢迎卡"与"添加卡"同时出现两份登录入口，重复 | `Accounts.tsx:95-107,171-180` |

---

## 6. Settings.tsx（设置页，883 行，7 个 hash 分区）

**功能清单（按分区）**：游戏默认值（根目录/默认 Java/自动内存+min-max range/默认窗口宽高+全屏/高级参数折叠[JVM/游戏参数]）；通用（语言/主题卡片网格+主题 JSON 查看/动效开关/日志卡[刷新/复制/清空+200 行视图]/更新卡[自动检查开关+"立即检查"**永久禁用**]）；下载（镜像官方/BMCLAPI 单选+并发数 1-64）；AI 与控制（AI：BaseURL+探测+模型下拉+API Key+测试保存；MCP：开关+端口+token 显示/复制/重新生成+HTTP/stdio 配置复制+会话列表+断开+**工具权限矩阵（只读/操作/破坏性 × 默认/允许/确认/拒绝）**+最近调用 8 条）；插件宿主（iframe 嵌入）；高级-版本仓库（重扫描+版本清单[重命名/复制/删除]）；关于（版本/体积/仓库/致谢）。

| 级别 | 问题 | 位置 |
|---|---|---|
| **P1** | **createEffect 用后端返回覆盖用户编辑中的输入**：监听 config() 把 `rootDir/autoMemory` 重置为后端值——每次任意其他设置保存返回新 config 时，正在编辑、尚未保存的根目录输入框被覆盖（用户输入丢失） | `Settings.tsx:80-85` |
| **P1** | **全部数字/文本输入 onInput 即刻保存**：并发数/内存/窗口尺寸/JVM 参数 textarea 每敲一个字符发一次 IPC `settings_set`，无防抖、无保存按钮、失败仅 toast；内存 range 越界仍会提交。与 InstanceDetail 的"显式保存+dirty"模式并存，全局心智不一致 | `Settings.tsx:138-170,405` |
| P2 | 配置加载无 loading/错误 UI：config 为 null 时控件显示默认值（并发 16/内存 1024-4096），真实配置到达前用户改动会以默认值+改动合并保存（竞态窗口） | `Settings.tsx` 全页 |
| P2 | 修改 MCP 工具策略为 deny/allow 无确认（一键改变外部 Agent 权限）；断开 MCP 会话无确认 | `Settings.tsx:629-634,657-660` |
| P2 | PluginsHost 空态按钮用 `window.location.hash = "#/plugins"` 导航（脆弱写法，应使用 navigate()） | `Settings.tsx:710` |
| P2 | "立即检查更新"永久禁用（title=即将推出）——功能缺失却占位 | `Settings.tsx` |
| P3 | `${current} (saved)`(:440)、`● ${session.client}`(:623)、outcome 标签 `ok/denied/failed` 英文直出(:685)、`" · modded"`(:801) | 同左 |
| 布局 | 侧栏 200px 固定 + 内容滚动，无断点 | `app.css` |
| 选项分布 | 7 分区结构合理；但 AI+MCP 合在一页过重（MCP 卡自身 6 个区块，可拆子页）；`instances.rename/duplicate` 收在"高级-版本仓库"而非实例上下文，语义藏得深 | — |

---

## 7. Plugins.tsx（插件管理，581 行，master-detail）

**功能清单**：安装（文件选择器+页面级拖拽 .rmcpkg）+刷新；诊断横幅；列表（名称/版本/状态/错误红点）；详情（启用禁用/移除[确认]）；详情 5 tab：概览（发布者/描述/路径/安装时间/最近错误）、权限（待审批[批准/忽略-持久化 localStorage]/已授权[撤销]/能力目录风险分级）、配置（Schema 表单+原始 JSON+保存/重置）、贡献（卡片/主题/设置页计数+跳转）、逻辑（调用下拉+JSON 输入+运行/取消+结果）。

| 级别 | 问题 | 位置 |
|---|---|---|
| **P1** | **拖拽安装依赖 `file.path`**：Chromium/WKWebView 已移除 `File.path`，该属性常为 undefined → 拖拽安装大概率永远走"非 rmcpkg"toast 分支，功能疑似整体失效（需真机验证） | `Plugins.tsx:142` |
| P2 | 移除插件 `pluginsApi.remove(id, false)` 恒不删数据，UI 没有"同时删除数据"选项（后端参数存在未暴露） | `Plugins.tsx:104` |
| P2 | 权限"忽略"只是 localStorage 隐藏，无"显示已忽略"入口，被忽略的待审批项无法找回 | `Plugins.tsx:54-59` |
| P2 | 配置"重置"无确认；`configGet`/`contributions` 失败静默 | `Plugins.tsx:374-380,411-421,512-515` |
| P2 | SchemaForm 的 `enum` 分支渲染成自由文本框而非下拉，枚举约束未生效 | `Plugins.tsx:453-463` |
| P2 | `plugin_rollback` 后端 API（api.ts:313）UI 无任何入口；`previousVersion` 字段（types.ts:520）未展示 | `api.ts:313` |
| P3 | 列表行副标题 `v{version} · {state}` state 原文英文 + 行尾又有 i18n 状态标签——重复且前者未翻译 | `Plugins.tsx:183` |
| 布局 | master-detail 主列固定 300px + 详情 `flex:1`，无断点，窄窗主列挤压详情 | `app.css:1262` 一带 |

---

## 8. 全局组件

| 组件 | 状态 | 问题 |
|---|---|---|
| **TitleBar** | 良好 | onCleanup noop 泄漏（见 01 ARC-11）；关闭按钮无"仍有游戏运行"确认（HMCL 有） |
| **TaskCenter** | 良好 | 覆盖下载（暂停/恢复[乐观标记，与后端事件无对账 downloads.ts:228]/取消）、loader 安装、运行游戏、MCP 破坏性确认卡（120s 倒计时）、最近结果 3 条；确认卡 createMemo 内 setInterval 空转（01 ARC-13）；停止无确认 |
| **LogDrawer** | 良好 | 自动滚动/复制/过滤+级别 chip/行级着色/崩溃保留缓冲/退出码横幅/AI 分析（egress 二次确认+可取消+结果独立窗可拖宽）；关闭抽屉非崩溃即丢缓冲（误关日志丢失）；上限 1000 行静默截断；`levelOf` 靠字符串包含判断（脆弱） |
| **Toast** | 良好 | 栈式右下/最多 5 条/5s 自动消失/动作按钮/aria-live；注释写 4s 与实现 5000ms 不符（ui.ts:52-55）；长错误信息无折叠 |
| **SkinViewer** | 良好 | 正版 sessionserver 皮肤+披风、离线 Steve、姿势循环、单例 viewer、ResizeObserver、WebGL 丢失诊断；**init 失败 UI 永远停留"加载中"**（:190-192,264-266）无错误态；blob 不 revoke（01 ARC-14） |
| **AccountAvatar** | 优秀 | 三级回退链完整（缓存协议→直连→首字母），localStorage 首帧直读；仅纹理缓存不失效（01 ARC-15） |
| **LaunchBubble** | 良好 | 五阶段进度/repair 显示缺失数/失败显示错误；failed 5s 自动消失，用户不在场错误信息即丢（LogDrawer 不因启动失败自动打开） |
| **PageHeader / icons / rm 三件套** | 良好 | `BackLink` 死导出；RmMenu 菜单项无键盘导航；RmDialog body 无 max-height 溢出策略；RmContentState 三态容器设计规范（surfaces.tsx:183-199）；RmInheritLock aria-label 硬编码英文（surfaces.tsx:236-237） |

---

## 9. 布局形态专项结论

1. **导航形态**：左侧 Rail 已被标题栏中央 Dock 取代（六项+双角标），信息密度与可达性良好；但 Rail 的状态与 CSS 残留未清（01 ARC-23/24）。
2. **页面形态谱系**：Hero 双栏（Home）/ 工具栏+网格（Instances）/ SideTabs 5+4+7（三个详情类页面）/ 向导（Download install）/ 工具栏+列表+分页（Download mods）/ 双栏固定皮肤位（Accounts）/ master-detail（Plugins）。形态选择与内容匹配，无形态滥用。
3. **系统性缺口——响应式**：全项目在用布局**没有任何响应式断点**（唯一断点属于死代码 dashboard-grid）；三处固定宽度（hero-corner 340px / accounts-split 360px / sidetabs 200px）在 800px 最小窗宽下必然挤压或重叠。窗口最小尺寸（800×500）与设计文档（1280×720）互相矛盾（01 ARC-29）。
4. **溢出保护**：长文本（实例名、Java 路径、错误消息）普遍缺 ellipsis/max-width/tooltip 兜底；`<img>` 普遍无 onError 兜底。

## 10. 选项分布专项结论

1. **数量级**：Settings 全局分区约 34 个选项/操作 + InstanceDetail 实例级约 17 个暴露控件（后端另有 5 个字段无控件），对当前产品成熟度不算失衡；对照 HMCL（全局 ~38 + 实例 ~41，总计 ~110）差距主要在**缺项**而非组织（详见 03 对比文档）。
2. **保存模式不一致**（最重要的一致性问题）：Settings"onInput 即存（无防抖）"vs InstanceDetail"显式保存+dirty+继承锁"——同一产品两种心智，且前者有覆盖用户输入的 P1 缺陷。
3. **分布失衡点**：AI+MCP 挤在设置一个分区（MCP 卡 6 区块）应拆子页；`instances.rename/duplicate` 藏在"高级-版本仓库"；无账号时 Accounts 双重登录入口；危险操作入口重复（删除 ×2）与缺失（停止 ×0 确认）并存。
4. **渐进披露**：高级参数/高级调试折叠区、向导分步、维护危险区隔离——执行良好，是设计支柱"渐进披露"的正面落实。
