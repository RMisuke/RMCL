# 01 · 旧台账 51 项逐项复核（2026-10-26）

> 复核基线：`docs/audits/FRONTEND_AUDIT_2026-10-25/04_ISSUES.md` 的 51 项问题
> 复核对象：main @ `3b863da`（2026-10-26 00:19）
> 判定含义：**FIXED** 已修复 · **PARTIAL** 部分修复 · **STILL** 原样存在（行号已按当前代码重定位）· **OBSOLETE** 代码已删不再适用
> 每条判定均有当前代码证据；关键项另经主审人工复核（P0×2、P1-C 契约、星星定位等）。

## 统计总览

| 判定 | 数量 | 占比 |
|---|---|---|
| FIXED | 2 | 4% |
| PARTIAL | 9 | 18% |
| STILL | 40 | 78% |
| OBSOLETE | 0 | — |

按严重度看修复率（FIXED+PARTIAL 视为有进展）：

| 级别 | 有进展 | 完全未动 |
|---|---|---|
| P0（2） | 1（PARTIAL：P0-2） | 1（P0-1） |
| P1（15） | 4（FIXED 1 + PARTIAL 3） | 11 |
| P2（22） | 2（均 PARTIAL） | 20 |
| P3（12） | 4（FIXED 1 + PARTIAL 3） | 8 |

**结论：上一轮审计的整改建议（05_RECOMMENDATIONS S0/S1）基本未被执行——S0 两项 P0 均未落地；12 个提交中 7 个是后端审计批次，前端修复面窄（约 11/51 项有实质进展）。**

---

## P0（2 项）

| 编号 | 判定 | 当前证据 |
|---|---|---|
| P0-1 | **STILL** | 消费端 `src/pages/Download.tsx:486`（`useSearchParams().instance`）与 `:85-88`（`searchParams.version`）仍读 path search；跳转源仍全是 hash 内查询：`Instances.tsx:151`、`InstanceDetail.tsx:183/270/404`、`Home.tsx:915` → 两条链路参数仍读不到；ModDetail 仍手写 hash 解析只修一处（`Download.tsx:774-778`） |
| P0-2 | **PARTIAL** | 假数据本体全在：ServerCard mock（`Home.tsx:727-740`）、CompareCard 硬编码 "26.2"（:785）、MOCK_ANNOUNCEMENTS 三条硬编码（:835-839），三卡仍默认展示（:50-53）；仅比对卡操作反馈加了"演示"文案，服务器卡假在线状态/公告卡仍无任何标识 |

## P1（15 项）

| 编号 | 判定 | 当前证据 |
|---|---|---|
| P1-1 | STILL | `Settings.tsx:84-89` createEffect 仍无条件用 config() 回填 rootDir/autoMemory，无 dirty 检查 |
| P1-2 | STILL | `InstanceDetail.tsx:273,276` 快捷按钮调模块级 setTab（:285-288 只 replaceState+dispatch）；组件 tab signal（:60）不监听 hashchange，全仓无任何 hashchange 监听者 |
| P1-3 | STILL | `Download.tsx:44-46` tab 只在挂载时读一次 hash，Step5 `navigate("/download#mods")`（:470）同页无响应；`Settings.tsx:40-44` 同款 |
| P1-4 | STILL | `InstanceDetail.tsx:894` height 输入仍由 `overridden("width")` 控制禁用与取值；save 仍只遍历 `draftOverrides()`（:801） |
| P1-5 | STILL | `overlays.tsx:156` 仍 `placeholder={req.placeholder ?? req.initial}`，从不 `setValue(req.initial)` |
| P1-6 | STILL | `TitleBar.tsx:37-42` onCleanup 仍在 async onMount 的 await 之后注册（noop）；`SkinViewer.tsx:268-277` 的正确写法未推广 |
| P1-7 | PARTIAL | 前端已解析 exit_type（`launch.ts:177-186`）；但 `init.ts:59-64` 崩溃判定仍只看 `exit_code !== 0`（手动 kill→137 仍弹崩溃抽屉）；后端 ExitType 仍仅 Normal/Abnormal（`process_listener.rs:14-19`），无"用户中止"语义 |
| P1-8 | **FIXED** | `LogDrawer.tsx:97-100` onCleanup 时无崩溃即 dropBuffer；`launch.ts:191-204` keepBuffers 恢复读写——"关闭抽屉即释放、崩溃保留"的设计意图已落地 |
| P1-9 | STILL | `src-tauri/src/bindings.rs:164-181` 测试仍先 `write_types_file()` 重写再 assert_eq——恒真且改写工作区 |
| P1-10 | STILL | `Plugins.tsx:141-143` 拖拽仍读 `(file as {path?}).path`（WKWebView 无此属性） |
| P1-11 | STILL（**恶化**） | 无确认停止点从 5 处增至 **7 处**：新增 hero 运行卡"强制关闭"（`Home.tsx:647-655`，注释自述无二次确认）与 GamesCard 停止钮（:979）；原 5 处（`Home.tsx:651`、`Instances.tsx:328,369`、`InstanceDetail.tsx:92,121`、`TaskCenter.tsx:131`）不动；TitleBar close 仍无运行检查 |
| P1-12 | PARTIAL | rootDir（`Settings.tsx:103-106`）与缓存上限（:496-508）已改显式保存；内存区间（:158-167）、窗口宽高（:177-179）、JVM/游戏参数（:190-199）、并发数（:453-454）仍 onInput 逐字符写盘无防抖 |
| P1-13 | STILL | dirty 存在（`InstanceDetail.tsx:738`）但 `RmSideTabs onSelect={setTab}`（:118）直接切换无拦截 |
| P1-14 | PARTIAL | 版本列表已改 VirtualList（`Download.tsx:290-316`，loader 列表 :379 亦虚拟化）；ModDetail 版本仍 `slice(0, 30)`（:807）无提示 |
| P1-15 | STILL | tauri.conf 仍 minWidth 800（productName 改 SNGL 未动尺寸）；app.css 仅有的宽度断点（:1607-1613）仍只服务死代码 `.dashboard-grid`/`.rail`，+592 行新 CSS 未新增任何断点 |

## P2（22 项）

| 编号 | 判定 | 当前证据 |
|---|---|---|
| P2-1 | STILL | `HomeDashboard.tsx` 仍存在（645 行，零引用） |
| P2-2 | STILL | 死 CSS 全在：`.rail` 家族（app.css:275-355）、`.dash-*`（:882-920）、`.quick-launch-*`（:922-946）、`.account-avatar-img`（:950） |
| P2-3 | STILL | `ui.ts:36,94-99,125-126` railCollapsed/toggleRail 无消费者 |
| P2-4 | STILL | `api.ts:335-340` dashboard 布局协议唯一消费者是死页 HomeDashboard（在用 Home 走 localStorage :54-75） |
| P2-5 | STILL | `cache.ts:49-57` inFlight 清理条件永假，Map 无界增长 |
| P2-6 | STILL | updater 内副作用：`downloads.ts:114-119,145-150`、`ui.ts:95-98` |
| P2-7 | STILL | `TaskCenter.tsx:279-287` createMemo 内 setInterval + 125s 兜底 |
| P2-8 | STILL | `SkinViewer.tsx:49,57,67` blobUrls Map 永不 revoke（记忆化使泄漏有界但未释放） |
| P2-9 | STILL | `AccountAvatar.tsx:19-30` textureCache 永不失效；新增 warmAvatar 仅启动游戏时重验且被 resource 旧值覆盖 |
| P2-10 | STILL | `mcp.ts:48-52` resolve 无条件本地移除确认卡 |
| P2-11 | STILL | `profiles.ts:23` force 仍被 inflight 吞 |
| P2-12 | STILL | `downloads.ts:158` rekeyPending 仍挂到任意最早 pending |
| P2-13 | PARTIAL | 已修：Download 搜索错误态+重试（:530-531,710）、fabricApi 失败 toast（:233,242）；仍在：Home onMount 吞错（:380,894-897）、InstanceDetail paths/resolved 静默（:150-154,219-222,973-975）、Plugins config/contributions 静默（:374-379,512-514）、settingsStore.refresh 无错误通道、SkinViewer init 失败永远"加载中"（:246-247,329-331） |
| P2-14 | STILL | Settings 无 loading/error 态；config null 时控件显示默认值且逐字符保存（与 P1-12 联动） |
| P2-15 | PARTIAL | 已修：default_isolation 补了 UI（Settings）、rename/duplicate 有入口（:919-925）；仍在：min_memory_mb/metaspace_mb/pre_launch/post_exit 声明于 OVERRIDABLE_FIELDS（`InstanceDetail.tsx:646-667`）无控件、env_vars 无控件、plugin remove 固定 deleteData=false、rollback/previousVersion 无 UI |
| P2-16 | STILL | `Settings.tsx:767-780` 策略改 allow/deny 无确认；:738-747 断开会话无确认 |
| P2-17 | STILL | "立即检查"仍 disabled（:313-315）；updater endpoints 仍 `<owner>/<repo>` 占位（pubkey 已填但 endpoint 未改） |
| P2-18 | STILL | src/ 仍无任何测试文件，package.json 无 test 脚本 |
| P2-19 | STILL | `launch.ts:105-107` 失败 5s 自动清除；不自动开日志抽屉 |
| P2-20 | STILL | `InstanceDetail.tsx:318-320` 搜索只过滤 enabled；:442-464 disabled 不参与；:502-505 conflicts/unparsed 仅计数 |
| P2-21 | STILL | `Download.tsx:646` 版本下拉未去重；:653-658 loader 筛选仍 3 种（向导 8 种） |
| P2-22 | STILL | `Accounts.tsx:118` 刷新按钮不分 provider；:191-194 后端轮询不可取消 |

## P3（12 项）

| 编号 | 判定 | 当前证据 |
|---|---|---|
| P3-1 | PARTIAL | `Toast.tsx:5` 注释已改 5s；`ui.ts:40` 注释仍写 4s（实际 5000ms） |
| P3-2 | STILL | `auth.ts:126` as unknown as IpcError 伪造 |
| P3-3 | STILL | `api.ts:119-121` 非 IpcError reject 直接 as |
| P3-4 | **FIXED** | events.ts 已删 DOWNLOAD_PAUSED/RESUMED、AUTH_STATE_CHANGED 常量，前后端零残留 |
| P3-5 | STILL | 死导出全在：connectedToastsDone（mcp.ts:20,91-92）、RmTextarea/RmBadge（controls.tsx:119-129,226-237）、BackLink（PageHeader.tsx:46） |
| P3-6 | STILL | `theme/index.ts:171` onThemeModeChange 无调用 |
| P3-7 | STILL | `mcp.rs:75` duration_ms 无 `#[ts(type="number")]` → types.ts:909 bigint 与运行时不符 |
| P3-8 | PARTIAL | 已修：hero 卡文案、Download "missing"、plugins.state 标签；仍在：mock MOTD/公告硬编码中文（Home.tsx:728-731,835-839）、优先级枚举原文（InstanceDetail.tsx:911-915）、plugins state 原文 subtitle（Plugins.tsx:183）、`(saved)`（Settings.tsx:552）、outcome 原文（:798）、`modded`（Settings.tsx:913、Instances.tsx:264）、aria-label 英文（overlays.tsx:52、surfaces.tsx:236）、步骤序号（Accounts.tsx:202,206） |
| P3-9 | STILL | `LogDrawer.tsx:68-87` 拖拽监听仅 pointerup 摘除 |
| P3-10 | STILL | `launch.ts:110-115` 45s 兜底不随 dismiss 取消（有 stage 幂等守卫，风险低） |
| P3-11 | STILL | `settings.ts:59` 模块导入期写 DOM |
| P3-12 | PARTIAL | 死 i18n key 6→3（settings.general.mode.dark/light/system）；API_Key.txt 仍在仓库目录（未跟踪、已忽略，未移出） |
