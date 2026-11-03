# 03 · 前端审计（SolidJS + TypeScript，约 1.5 万行）

> 审计对象：`src/`（TS/TSX 约 13,230 行 + app.css 2,023 行）。
> 基线：`docs/audits/FRONTEND_AUDIT_2026-10-25/`（51 项问题）发布后工作区又有大量未提交改动（SkinViewer/cache.ts/markdown.ts/Home 重写等），本审计在当前工作区逐项复核旧问题并覆盖新代码。`pnpm typecheck` 通过（0 类型错误）。
> 定级：P0 = 崩溃/数据丢失/用户被误导；P1 = 高风险/功能缺陷；P2 = 缺陷/冗余/性能；P3 = 小问题。总计：**2 P0 / 14 P1 / 37 P2 / 33 P3**。

---

## 第一节 · 前端架构现状概述

```
index.html（内联 boot guard：bundle 失败画错误面板 + frontend_log 上报 + unhandledrejection 捕获）
  └─ main.tsx  路由表：7 个活跃页全部 lazy()；/login /mods /agent → Navigate 重定向；* → /
       └─ App.tsx（揭窗握手 + ensureReachable 多显示器可达性 + initI18n/initTheme/initStores）
            ├─ TitleBar（品牌 + 中央 Dock 六项导航 + 运行中/下载角标 + 窗口控制）
            ├─ app-content（路由出口）
            └─ 全局浮层：TaskCenterPanel / LogDrawer(含 AI 分析双面板) / LaunchBubble
               / RmConfirmHost / RmPromptHost / ToastContainer
stores/（11 文件，createRoot 单例域 store）← 唯一 Tauri listen() 点（init.ts 集中 attach，幂等）
ipc/    safeInvoke 统一 never-reject 封装（99/100 后端命令有 wrapper）+ ts-rs 生成 types.ts
i18n/   zh-CN/en 各 558 key 双向对齐（编译期 Record 强制），t() 读 signal 自动响应切换
theme/  --rmccc-* token 体系 + 旧变量桥接 + 系统模式跟随
plugin-ui/ rmcccplugin:// iframe 桥（source 校验 + 配额 + 超时 + 推送中继）
lib/    format/exitCodes/markdown(自研 escape-first)/prefs/loaders
styles/ app.css 2,023 行（token 化、!important 仅 2 处正当使用、含 hero 皮肤主题层）
```

**与 ui-design 的符合度**：分层纪律（stores 唯一 listen 点、safeInvoke 全覆盖、ts-rs 类型桥、rm 组件库、RmContentState 三态容器、渐进披露折叠区）是忠实落实且质量高。**偏差集中在 Home**：在用 `Home.tsx`（hero + localStorage `rmccc.heroLayout.v2`）仍偏离 `02_HOME_DASHBOARD.md`（后端布局协议 + 五态 QuickLauncher + ErrorBoundary），且死代码版 `HomeDashboard.tsx`（645 行）反而保留了 ErrorBoundary/下载取消/空态引导等待回捞实现——"先改文档再改代码"约束仍未满足。窗口最小尺寸 800×500（tauri.conf.json:20-21）与 00_OVERVIEW.md:145 的 1280×720 冲突仍未裁决。

## 第二节 · 结构与架构

- [P1] src/pages/Settings.tsx:716-722 — 插件宿主分区用**裸 iframe**（`sandbox="allow-scripts"`）嵌 `rmcccplugin://…?page=settings`，未走 `PluginFrame`/`PluginBridge` — 插件设置页发 `ready` 永远无人应答，`settings.get`/`theme.get` 全部不可用，该分区实际不可用；Home.tsx:700-716 的 PluginCard 是正确写法，应统一改用 PluginFrame。
- [P1] src/App.tsx:61-75 — **全应用零 ErrorBoundary**（唯一使用在死代码 HomeDashboard.tsx:189-206）— 任一页面/组件渲染抛错 = 内容区白屏（boot guard 只兜首挂载前）；建议至少在 `<main class="app-content">` 外包一层 + plugin 卡片局部边界。
- [P2] src/pages/Settings.tsx:710 — 插件宿主空态按钮 `window.location.hash = "#/plugins"` — @solidjs/router 只监听 popstate/click 等（dist 源码核实无 hashchange），path 不变 → **点击无任何效果**；改 `navigate("/plugins")`。
- [P2] src/pages/HomeDashboard.tsx（整文件 645 行）— 零引用整页死代码，其中 ErrorBoundary（:189-206）、下载行取消按钮（:633-637）、无实例引导空态、五态 QuickLauncher、粘贴版本号优于在用版 — 删除前应回捞。
- [P2] src/stores/ui.ts:36,94-99,125-126 — `railCollapsed`/`toggleRail`/`rail.collapsed` pref 死状态（Rail 已被 Dock 取代，全库无消费者）。
- [P2] src/ipc/api.ts:333-338 — `dashboard.getLayout/saveLayout/resetLayout` 仅被死页消费（跨层冗余：后端命令 + DTO + wrapper 三层供养死代码）。
- [P3] src/pages/InstanceDetail.tsx:220 — `deleteProfile(profile, alsoVersion)` 的 `alsoVersion` 被 `void` 忽略，死参数。
- [P3] src/styles/app.css:949 — `.account-avatar-img` 死类。

**组件库纪律检查**：未发现绕过 rm 组件库的页面级重复样式——Instances/InstanceDetail/Download/Settings/Plugins 均统一走 rm kit，唯一直写 `<input class="rm-input">` 的场景（数字类型输入）属 rm kit 未提供 RmNumberInput，可接受但值得补一个组件收敛。

## 第三节 · SolidJS 反应式正确性

- [P1] src/components/TitleBar.tsx:37-42 — `onCleanup(unlisten)` 注册在 async onMount 的 `await appWindow.onResized(...)` **之后**，无 active owner 时为 noop，`onResized` 监听永不注销 — 同文件同坑 SkinViewer.tsx:268-277 已正确修复并留注释，TitleBar 是漏网点。
- [P1] src/pages/InstanceDetail.tsx:284-287,296-299,57 — Overview 快捷按钮调**模块级** `setTab()`（replaceState + `dispatchEvent(new HashChangeEvent("hashchange"))`），但全仓无任何 hashchange 监听者，组件内 `tab` signal 只初始化一次 — "缺失扫描/设置"按钮点击后 URL 变了界面不动；tab 应改为派生自 hash。
- [P1] src/pages/Download.tsx:42-44,397,528 与 src/pages/Settings.tsx:37 — 同页 hash 跳转不切 tab（`tab/section` signal 只读一次 hash）：向导 Step5"去 Mod 商店"、ModStore"退出上下文"（`navigate("/download#mods")`）、Settings 已挂载时改 hash，均无可见变化。
- [P2] src/pages/Download.tsx:462-466 — `createEffect(() => { const id = setTimeout(...300ms); return () => clearTimeout(id); })` — **Solid 的 createEffect 忽略返回值**（React useEffect 惯用法），清理函数永不执行 → 300ms"防抖"完全失效：每敲一个字符各发一次 `mod_search`（searchSeq 只保证结果不错乱，不阻止请求风暴）；改用 `onCleanup(() => clearTimeout(id))`。
- [P2] src/stores/downloads.ts:114-119,145-150 与 src/stores/ui.ts:95-98 — 信号 updater 内嵌副作用（`setRecent`+`outcomeId++` 在 `setBatches` updater 内）— Solid 反模式，updater 可能被多次调用导致 recent 列表重复；副作用移出 updater。
- [P2] src/components/TaskCenter.tsx:245-252 — 确认卡在 `createMemo` 内 `setInterval`（125s 才自清），卡片被 resolve 卸载后 interval 空转，memo 重跑会叠加 — 改 `onMount`+`onCleanup`。
- [P2] src/pages/Home.tsx:88-95 — `createMemo` 内执行副作用（默认选中最近实例 `setSelectedId`）— memo 不保证单次执行；改 `createEffect`。
- [P2] src/components/LogDrawer.tsx:258-310 — AI 展开窗用非 keyed Show 函数子节点，条件每条 delta 都变化 → 整个 ai-window DOM **每条流式 delta 全量重建** + `renderMarkdown(state.text)` 对全文重解析（O(n²)）— 长分析时卡顿。
- [P3] src/pages/Home.tsx:488 — 版本比较用字符串比较 `clientVersion() < serverVersion`（"9.9" < "26.2" 会误判）— 演示数据但逻辑即错（后端也无版本号比较体系，与 HMCL 差距联动）。
- [P3] src/pages/Download.tsx:359 — Step3 进入条件双判断，validateName 已失败仍可能放行。
- [P3] src/components/LogDrawer.tsx:66,262 — `aiWindowEl` ref 赋值后从未读取，死引用。

**全库扫描结论**：未发现任何 props 解构丢响应性问题；`For/Show keyed` 使用规范（Plugins.tsx:203-205 有意识地用 keyed 修复了 stale-read 崩溃并留注释）；onMount/onCleanup 配对除 TitleBar/TaskCenter/LogDrawer 拖拽三处外均正确。

## 第四节 · IPC 层契约

- **覆盖核对**：后端注册 100 条命令（含 modloader 2 条），前端 api.ts 封装 99 条，`tsc` 通过。命令↔wrapper 一一对应无漂移；`safeInvoke` 永不 reject 纪律保持。（与 02 文档命令清单口径一致：99 注册 + mod_install_local 双入口。）
- [P3] src/ipc/api.ts:119-121 — `safeInvoke` 把任意 reject 直接 `as IpcError`，Tauri 层字符串错误时 `.message` 为 undefined — 建议归一化 `{code:"unknown", message:String(e)}`。
- [P3] src/stores/auth.ts:126 — `refreshSession` 伪造 `{code:"not_found"} as unknown as IpcError`。
- [P3] src/ipc/types.ts:901 — `McpRecentCallView.durationMs: bigint`，JSON 运行时是 number（同文件其他 u64 均有 `#[ts(type="number")]`，此处漏标）。
- **事件订阅**：全部集中在 `init.ts`（各 store attachEvents + 跨域归因），无重复订阅、无泄漏点。`DOWNLOAD_PAUSED/DOWNLOAD_RESUMED/AUTH_STATE_CHANGED`（events.ts:10-15）前端从未 listen 且后端也无 emit 点——双侧对称预留（与 02 文档 P2 死常量条目联动）。
- [P3] src/stores/init.ts:58-63 — `LAUNCH_EXITED` 只判 `exit_code !== 0` 即 `openLogDrawer`；事件已带 `exit_type`，`launch_kill` → SIGKILL(137) 被当崩溃弹抽屉 — 消费 `exit_type` 或在 kill 时标记用户中止。

## 第五节 · 状态管理

- [P2] src/stores/cache.ts:49-57 — `refreshIcon` 的 inFlight 清理条件永假（新时间戳永不等于旧值）→ Map 随不同 URL 无界增长。
- [P2] src/stores/profiles.ts:22-24 — `refresh(force)` 的 force 被 `if (inflight) return inflight` 吞掉，`DOWNLOAD_COMPLETE → refresh(true)` 可能拿到旧请求结果。
- [P2] src/stores/mcp.ts:48-52 — `resolve` 无条件移除确认卡，忽略 `resolveConfirmation` 后端裁决结果（服务端已超时裁决时 UI 与真态漂移）。
- [P2] src/stores/downloads.ts:95-107,158 — 并发下载时 `rekeyPending("", p.batch_id)` 把无 profile 标识的 pending 挂到**最先到达的任意 batch**，label/version 串号风险。
- [P2] src/components/AccountAvatar.tsx:19-30 — `textureCache` 按 uuid 缓存 Promise 永不失效：换皮肤后 `warmAvatar` 只更新 localStorage + 图标缓存，texture() 仍返回旧 URL → **当次会话头像停留旧皮肤**（重启后恢复）。
- [P2] src/stores/settings.ts:34 — 模块导入期 `applyMotion()` 写 DOM（无 document 环境导入即崩，也阻碍未来测试）— 移入 initTheme/App onMount。
- [P3] src/stores/launch.ts:110-115 — 45s 兜底 setTimeout 不随 `dismissFlow` 取消。
- [P3] src/stores/ui.ts:40 — 注释 "auto-dismiss 4s" 与实现 5000ms 不符。
- **跨 store 协同**：launch 成功 → `recordAccountActivity`（auth）→ `warmAvatar`（cache）→ 头像 localStorage 链路设计良好；`kill` 现带乐观更新 + `launch.status()` 对账（本次改动新增，好）；下载暂停/恢复仍是**纯乐观无对账**（downloads.ts:228-238 注释自认 backend events pending）。

## 第六节 · 冗余与死代码

- [P2] src/styles/app.css:275-331 — `.rail/.rail-item/.rail-badge/.rail-footer/.is-collapsed` 家族死 CSS（注意 `.rail-task-*` 在用勿删）。
- [P2] src/styles/app.css:881-919+ — `.dash-*` 家族死 CSS（仅死页 HomeDashboard 使用）。
- [P2] src/components/rm/controls.tsx:116,222 — `RmTextarea`/`RmBadge` 死导出；src/components/PageHeader.tsx:46 `BackLink` 死导出；src/stores/mcp.ts:20,91-92 `connectedToastsDone` 双导出无消费者。
- [P2] src/ipc/api.ts:313 — `plugin_rollback` wrapper 无任何 UI 入口（后端能力浪费）。
- [P3] i18n 死 key ×6：`home.restoreHiddenNote`、`instances.menu.stop`、`logs.abnormalExit`、`nav.collapse`、`settings.general.mode.dark`、`settings.general.mode.light`。
- [P3] src/theme/index.ts:171-173 — `onThemeModeChange` 单槽监听且全库未用。
- [P3] src/i18n/locales/zh-CN.ts:214 — 一行挤了两个 key，格式异常易漏检。
- **icons.tsx**：44 个图标全部有消费者，无死图标。

## 第七节 · 性能

- [P1] src/pages/Download.tsx:237,734 — 版本列表 `slice(0, 100)`、Mod 版本 `slice(0, 30)` 硬截断，无加载更多/虚拟化/总数提示 — 选 old/all 类型时靠后的版本永远看不到。
- [P2] src/pages/Download.tsx:462-466 — 防抖失效导致每键入一次即发一次搜索请求（见第三节）。
- [P2] src/components/LogDrawer.tsx:184-191 — 日志行 `<For each={filtered()}>` 每行一个 div，1000 行上限 + 每 output 事件触发 `appendLines` 整 Map 重建（launch.ts:63-71 `new Map(map)` 每行一次）— 高频日志时 O(n) 拷贝/行；建议行级 buffer 直接 mutate 或虚拟列表。
- [P2] src/stores/mcp.ts:54-60 — 全局 2s 轮询永不停（enabled 门槛内 `refreshStatus`/`refreshConfirmations`），即便用户从不打开设置页 — 建议挂到设置页/确认卡可见时。
- [P3] src/components/SkinViewer.tsx:49 — `blobUrls` 模块级记忆化永不 revoke（有意设计防协议风暴，量级=不同皮肤数，可接受；注释已说明）。
- **资源策略整体优秀**：Mod 图标三级回退 + `loading="lazy"`；皮肤/头像走 `rmcccicon://` 磁盘直出 + stale-while-revalidate，首帧 localStorage 直读；skinview3d/three.js 动态 import 独立分包，不进首屏。

## 第八节 · 错误处理与 UX 三态

- [P1] src/pages/Settings.tsx:80-85,422-427 — `createEffect` 用后端回填覆盖用户正在编辑的输入（保存任意其他设置都会重置 rootDir/autoMemory/baseUrl/model 草稿）— 仅在控件未 dirty 时回填。
- [P1] src/pages/Settings.tsx:138,146,156,158,170,178,405 — 数字/文本输入 onInput 即写盘，每字符一次 `settings_set` IPC，无防抖无确认；与 InstanceDetail 显式保存模式双心智并存。
- [P1] src/pages/InstanceDetail.tsx:888-890 vs 797 — 分辨率"高"输入被 `overridden("width")` 一并控制 disabled，保存只遍历 `draftOverrides()`：只锁 width 时 height 可编辑但不写入；只锁 height 时无法编辑 — height 独立控制 + 保存并集。
- [P2] src/pages/Settings.tsx — 配置未加载（config null）时控件显示默认值，改动会以默认值+改动合并保存（竞态窗口）；全页无 loading/error 态。
- [P2] 停止运行中游戏 5 处（Home.tsx:681、Instances.tsx:326,367、InstanceDetail.tsx:89,118、TaskCenter.tsx:143）均无确认；TitleBar.tsx:50 关窗不检查运行中实例；Home.tsx:225 登出无确认 — 抽 `confirmStopGame()` 复用。
- [P2] src/components/rm/overlays.tsx:126,145-157 — `RmPromptHost` 的 `initial` 只落 placeholder 不回填输入框 → 所有重命名对话框打开为空，直接回车=取消。
- [P2] 吞错点（`r.ok &&` 无 else/无 toast）：Home.tsx:85,597、InstanceDetail.tsx:231-232,762-763、Download.tsx:511-517,820-822、Plugins.tsx:374-380,512-515、Settings.tsx:87。
- [P2] src/components/SkinViewer.tsx:329-331 + 93,116 — 初始化失败 UI 永远"加载中"（无错误态）；且 `stageInit ??=` 缓存 rejected Promise **永不重试** — 一次瞬态失败后 3D 预览本次会话永久损坏 — catch 中 `stageInit = null` 复位。
- [P2] src/stores/launch.ts:106 + src/stores/init.ts — 启动失败气泡 5s 自动消失且不自动打开日志抽屉，错误信息易丢。
- [P2] src/pages/Download.tsx:260 — Step1 错误态 `RmErrorState` 未传 onRetry（ModStore 的已传 :637）；版本加载无 skeleton。
- [P2] src/pages/Download.tsx:800-861 — 缓存管理无 loading/error 态。
- [P2] src/pages/Accounts.tsx:118 — "刷新登录"对离线账号也显示且必然失败；:191-194 设备码无法主动取消后端轮询；:95-180 无账号时欢迎卡与添加卡双入口重复。
- [P2] src/pages/InstanceDetail.tsx:329-331,513-516 — Mods 搜索只过滤启用列表；扫描 conflicts/unparsed 只有计数无明细。
- [P2] src/pages/Download.tsx:573,580-585 — 商店游戏版本下拉选项未去重；loader 筛选仍只有 fabric/forge/quilt（向导 8 种）。
- [P2] src/pages/Settings.tsx:630,658 — MCP 断开会话、工具策略改 deny/allow 均无确认。
- [P3] src/pages/Plugins.tsx:382-391 — 配置保存成功后 error 残留不清除。
- [P3] src/components/rm/overlays.tsx:52、surfaces.tsx:236-237 — aria-label/title 硬编码英文。
- [P3] src/components/LogDrawer.tsx:68-87 — AI 面板拖宽监听仅 pointerup 摘除，拖拽中卸载泄漏。

**错误边界缺口**：plugin 卡 error boundary 之外，整个 `app-content`（所有 7 页）无错误边界（见第二节 P1）。

## 第九节 · plugin-ui 桥与 iframe 安全

- `bridge.ts:131-137` — postMessage 来源校验到位：`event.source !== frame.contentWindow` 强匹配 + origin 白名单（`"null"` 为 WebKit 沙箱 frame 的合法值）——安全模型正确。
- `bridge.ts:300-306` — 宿主→frame 用 `targetOrigin:"*"`，注释正当（数据均为 frame 应得的自身 id/theme/limits）。
- `PluginFrame.tsx:104-115` — `sandbox="allow-scripts allow-same-origin"`（与宿主不同源，无逃逸面）+ 8s ready 超时 + 错误态分级 — 实现质量好。
- [P1] src/pages/Settings.tsx:716-722 — 见第二节：裸 iframe 绕过桥，既是功能缺陷也是一致性缺陷。
- `plugin://push` 中继（init.ts:40-42 → bridge.ts:49-53）：多 frame 广播 + dispose 注销干净，无泄漏。
- [P3] Settings 的裸 iframe 与 PluginFrame 的 sandbox 属性不一致（`allow-scripts` vs `allow-scripts allow-same-origin`），统一后插件页行为才能一致。

## 第十节 · i18n

- **对齐机制**：`Messages = Record<TranslationKey, string>`（messages.ts:16）编译期强制，实测 en=zh-CN=558 key，双向 diff 为空；`t()` 回退链 en→key 永不空串；语言切换响应式 — 体系健康。
- [P3] 硬编码清单（全部复核仍存）：Home 假数据中文文案（Home.tsx:430-433,537-541）与默认地址（:426）、卡片副标题 "RMCCC"（:171）；优先级枚举（InstanceDetail.tsx:907-911）；`${local.missing_count} missing`（Download.tsx:275）；`" · modded"`（Instances.tsx:262 等）；`${current} (saved)`（Settings.tsx:440）；outcome `ok/denied/failed` 原文（Settings.tsx:685）；Accounts 步骤序号（Accounts.tsx:202,206）；Plugins 列表副标题 state 原文（Plugins.tsx:183）。
- [P3] src/components/SkinViewer.tsx:302 — "更换皮肤"跳转硬编码 `zh-hans` 版官网，英文用户不适配。

## 第十一节 · 已知降级形态核实（当前代码现状）

| 降级项 | 现状 | 证据 |
|---|---|---|
| open_path 未实现 | **确认未实现**：后端无命令、前端无封装；实例路径仅有"复制" | InstanceDetail.tsx:160-162 |
| modloader 移除未开放 | **确认**：无后端命令、LoadersTab 无卸载入口（仅安装+互斥禁用） | InstanceDetail.tsx:540-638 |
| 下载暂停乐观 UI | **确认**：TaskCenter 按钮 = 本地 Set 标记 + IPC；后端无 DOWNLOAD_PAUSED/RESUMED emit，无事件对账 | TaskCenter.tsx:204-217、downloads.ts:228-238 |
| 更新检查灰置 | **确认**：按钮 disabled + "即将推出"；updater endpoint 仍为 `<owner>/<repo>` 占位（见 02 文档） | Settings.tsx:328-330、tauri.conf.json |
| 整合包占位 | **确认**：modpack tab 纯 `RmEmptyState` 占位 | Download.tsx:59-65 |
| CurseForge 源 | 确认禁用态 + toast 提示（后端 rmccc-mods 无 CurseForge 实现，见 01/04 文档） | Download.tsx:552-559 |
| **P0-2 假数据卡（仍存在）** | 服务器卡 mock（Home.tsx:425-480，注释自认"演示数据，后端未接入"）、比对卡服务端硬编码 "26.2"（:487）、公告卡 3 条硬编码（:537-541）——UI 上**无任何"演示"标识**，用户会把假 MOTD/假公告当真实功能 | Home.tsx:423-541 |
| **P0-1 hash 查询参数（仍存在）** | 消费端 `useSearchParams`（Download.tsx:78,411）读不到 hash 内 query；跳转源 6 处仍写 `#mods?instance=`/`#install?version=`（Instances.tsx:155、InstanceDetail.tsx:178,281,415、Home.tsx:617,627）——"实例→Mod 上下文"与"版本预选"两条链路**依然全断** | 同左 |

## 第十二节 · 与 docs/audits/FRONTEND_AUDIT_2026-10-25 的对比

### 已修复（含部分修复）

| 旧编号 | 说明 |
|---|---|
| P3-1（半） | toast 4000→5000ms 值已改，Toast.tsx 新增 5s 进度条；但 ui.ts:40 注释仍写 4s |
| RmSelect 丢选中 | controls.tsx:140-144 新增 effect 修复 options 重建丢值 |
| Plugins stale-read 崩溃 | Plugins.tsx:205 改 keyed Show（注释自述曾致错误风暴） |
| kill 乐观更新 | launch.ts:130-149 乐观移除 + `launch.status()` 对账（新增） |
| LaunchExit.exitType | launch.ts:18-22,186 已接收字段（但 init.ts 未消费 → 半修复） |
| P2-9（部分） | 头像新增 localStorage 首帧直读 + 启动时 warmAvatar 重验（cache.ts:63-77）；但 textureCache Promise 仍不失效 |
| P2-15（部分） | 全局默认 min/max 内存、wrapper、instances.rename/duplicate/remove、默认窗口尺寸已补控件；实例级 min_memory/metaspace/pre_launch/post_exit/java_agents/env_vars 仍无控件 |
| Download Step3 onRetry | ModStore 错误态已带 onRetry；Step1 仍缺 |
| 新功能补齐 | Mod 商店分页、默认下载量排序、多游戏目录过滤/管理、动效开关、主题卡片网格含"跟随系统"、AI 模型 (saved) 兜底、设备码自动复制、SkinViewer 常驻舞台层彻底重写（WebGL 上下文保活 + 顶层 onCleanup 正确） |
| 旧路由删除 | 旧 routes/ 目录已删（commit 92d8743），路由残留清理完成 |

### 仍存在（逐项复核确认）

P0-1、P0-2、P1-1（Settings 覆盖编辑，且 AiSection 新增同款 :422-427）、P1-2、P1-3、P1-4、P1-5、P1-6、P1-7（半修复）、P1-8（dropBuffer 仍只在应用退出时执行；keepBuffers 仍只写不读）、P1-10、P1-11、P1-12、P1-13、P1-14、P1-15、P2-1~P2-8（P2-8 blob 已改为有意记忆化，降级为可接受）、P2-10~P2-22、P3-2~P3-12（P3-4 双侧确认为预留）、合规漂移 3 项（Home hero 未补 ADR、最小窗口冲突、死 CSS 未清）。

### 本次新增发现（旧审计没有）

| 级别 | 问题 |
|---|---|
| P1 | Settings 插件宿主裸 iframe 无桥（Settings.tsx:716-722） |
| P1 | 全应用零 ErrorBoundary（App.tsx） |
| P2 | Download 搜索防抖完全失效（createEffect 返回清理函数，Download.tsx:462-466） |
| P2 | Settings.tsx:710 `window.location.hash="#/plugins"` 不触发路由（按钮无效） |
| P2 | SkinViewer `stageInit` 缓存 rejected Promise 永不重试（SkinViewer.tsx:93,116） |
| P2 | LogDrawer AI 展开窗每 delta 全量重建 DOM + 全文重解析 markdown（LogDrawer.tsx:258-310） |
| P2 | Accounts `replayPendingLaunch` 绕过 launchStore.start（无气泡/无 check-repair，Accounts.tsx:64-67） |
| P2 | launchStore 日志缓冲每行 `new Map` 全量拷贝（launch.ts:63-71） |
| P3 | zh-CN.ts:214 双 key 挤行；SkinViewer zh-hans 硬编码；LogDrawer aiWindowEl 死 ref；Plugins 配置保存成功不清 error |

## 第十三节 · 前端健康度小结

| 模块 | 行数 | 问题数（P0/P1/P2/P3） | 一句话评价 |
|---|---|---|---|
| main.tsx / App.tsx | 128 | 0/1/0/0 | 路由 lazy 与揭窗握手规范，缺全局错误边界 |
| pages/Home.tsx | 716 | 1/2/4/3 | hero 重写后仍是"演示数据重灾区"，假卡无标识是最大诚信问题 |
| pages/HomeDashboard.tsx | 645 | 0/0/1/0 | 整页死代码，但含 5 项待回捞的优质实现 |
| pages/Instances.tsx | 446 | 0/1/2/1 | 全项目三态最规范的一页；新增多目录管理质量好 |
| pages/InstanceDetail.tsx | 1072 | 0/3/2/4 | 继承锁设计优秀，但跨 tab 跳转/height 覆盖/草稿丢弃三处 P1 均未修 |
| pages/Download.tsx | 861 | 1/1/4/2 | hash 查询链路断裂的最大受害页；新增分页/排序是进步，防抖失效是新伤 |
| pages/Accounts.tsx | 256 | 0/0/2/2 | 结构清晰；离线刷新入口与设备码取消缺失 |
| pages/Plugins.tsx | 581 | 0/1/3/2 | keyed Show 修复后健康度上升；拖拽安装疑似整体失效待真机验证 |
| pages/Settings.tsx | 883 | 0/3/4/4 | 覆盖用户编辑 + 每键写盘 + 裸 iframe，是 P1 密度最高的一页 |
| stores/（11 文件） | 1449 | 0/0/6/4 | 分层纪律优秀；updater 副作用与三个"永假/吞掉"守卫是主要技术债 |
| components/（9 文件） | 1565 | 0/2/5/5 | SkinViewer 新实现是亮点；TitleBar/TaskCenter/LogDrawer 生命周期债集中地 |
| components/rm/（3 文件） | 744 | 0/0/0/2 | 组件库抽象被良好遵守，无绕过；仅死导出与英文 aria |
| ipc/ | 1532 | 0/0/0/4 | 99 命令覆盖、safeInvoke 纪律满分；类型桥仅 1 处 bigint 漂移 |
| plugin-ui/ | 435 | 0/1/0/1 | 桥的安全模型是全项目最佳实践；被 Settings 裸 iframe 破功 |
| i18n/ | 1330 | 0/0/0/3 | 双语 558=558 完全对齐，仅剩页面层硬编码收尾与 6 个死 key |
| theme/ + lib/ | 488 | 0/0/1/1 | token 体系干净；onThemeModeChange 死 API、settings.ts 导入期写 DOM |
| styles/app.css | 2023 | 0/0/2/0 | token 化彻底，但 ~135 行死 CSS（6.7%）未清 |
| **合计** | **15253** | **2/14/37/33** | 分层与 IPC 纪律是同类项目标杆；两大 P0（hash 参数、假数据卡）自上轮审计原样存续，且新增 2 个 P1（裸 iframe、零错误边界）；"UI 实验残骸清理"与"两套保存心智统一"是最应优先启动的两件事 |
