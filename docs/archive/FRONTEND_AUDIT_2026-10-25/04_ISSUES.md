# 04 · 问题台账（全量去重）

> 汇总来源：01 架构审计 + 02 页面审计 + 03 HMCL 对比，跨文档重复项已合并。
> 严重度定义：**P0** 核心旅程断裂/用户被误导 · **P1** 功能失效或用户输入丢失，应修 · **P2** 质量与一致性缺陷/资源泄漏/能力缺口，建议修 · **P3** 微小/工程卫生。
> 统计：P0 ×2 · P1 ×15 · P2 ×22 · P3 ×12，共 51 项。

---

## P0（2 项）

| 编号 | 问题 | 位置 | 修复建议 |
|---|---|---|---|
| P0-1 | **hash 内查询参数不被解析，两条核心链路断裂**：所有 `/download#mods?instance=` 与 `/download#install?version=` 跳转的参数 `useSearchParams` 读不到（它只解析 path 的 search）→ "实例→Mod 商店上下文（锁定过滤+推荐版本+直装目标）"与"首页版本卡→向导预选"全部失效。旁证：ModDetail 手写了 hash 解析（作者已知此坑只修了一处） | `Download.tsx:411-413,86-96`；跳转源 `Instances.tsx:155`、`InstanceDetail.tsx:178,281,415`、`Home.tsx:617` | 全局统一：改为 `/download?instance=x#mods`（query 挂 path）或抽 `parseHashQuery()` 工具并在 4 个消费点使用；补一条路由参数单测 |
| P0-2 | **首页三张默认卡片为假数据且无任何标识**：服务器卡（mock MOTD/人数/延迟/版本）、版本比对卡（服务端版本硬编码 "26.2"）、公告卡（3 条硬编码）。用户会把假在线状态/假公告当真实功能 | `Home.tsx:423-480,484-541` | 短期：加"演示数据"徽标或默认隐藏三卡；中期：接真实 ping（后端 server ping 能力）或删除比对/公告卡 |

## P1（15 项）

| 编号 | 问题 | 位置 | 修复建议 |
|---|---|---|---|
| P1-1 | `createEffect` 用后端返回覆盖用户正在编辑、尚未保存的输入（保存任一其他设置都会重置根目录等编辑框） | `Settings.tsx:80-85` | 仅在控件未 dirty 时回填，或改显式保存模式（见 P1-12） |
| P1-2 | InstanceDetail Overview 快捷按钮（缺失扫描/设置）跨 tab 跳转失效：`setTab()` 只改 hash，组件 `tab` signal 不监听 hashchange | `InstanceDetail.tsx:285,287,296-299,57` | tab signal 改为派生自 hash（createMemo + hashchange 监听）或 store 化 |
| P1-3 | 同页 hash 跳转不切 tab：Download Step5"去 Mod 商店"点击无可见变化；Settings 已开时改 hash 同款 | `Download.tsx:42-44,397`、`Settings.tsx:37` | 与 P1-2 同一修法：tab 状态派生化 |
| P1-4 | 分辨率"高"编辑被静默丢弃：宽高输入框都由 `overridden("width")` 控制禁用，保存只遍历 `draftOverrides()` | `InstanceDetail.tsx:888-890` vs `:797` | height 输入用 `overridden("height")` 独立控制；保存遍历宽高并集 |
| P1-5 | `RmPromptHost` 的 `initial` 只落 placeholder，从不回填输入框 → 全部"重命名"对话框打开是空输入，直接回车=取消 | `overlays.tsx:126,153-157`；调用方 `Settings.tsx:752,764`、`Instances.tsx:164`、`InstanceDetail.tsx:192` | 打开时 `setValue(req.initial ?? "")` |
| P1-6 | TitleBar 的 `onCleanup` 在 async onMount 的 await 之后注册 → 无 Owner 时静默丢弃，`onResized` 监听永不注销 | `TitleBar.tsx:46-56` | 监听注册与清理移入同步作用域或用 `createAsync`/显式句柄；参考 `SkinViewer.tsx:204-205` 的写法 |
| P1-7 | 手动停止游戏被当成崩溃：仅判 `exit_code !== 0` 即开日志抽屉（kill→137），`exit_type` 事件字段未消费 | `init.ts:58-63`、`src-tauri/src/commands/launch.rs:11,128-140` | 消费 `exit_type`；kill 路径标记为用户中止，不弹崩溃横幅 |
| P1-8 | 游戏日志缓冲实际永不释放：`keepBuffers` 只写不读、`dropBuffer` 运行时是死代码，每 profile 最多 1000 行驻留至退出 | `LogDrawer.tsx:102-105,148-152`、`launch.ts:43,10` | 明确策略：关闭抽屉即 drop（或 LRU 上限），`keepBuffers` 恢复设计意图 |
| P1-9 | bindings 漂移守卫测试恒真：先重写 types.ts 再 assert_eq，永远通过且 `cargo test` 改写工作区 | `src-tauri/src/bindings.rs:164-178` | 改为对比内存生成内容与磁盘文件（只读比较） |
| P1-10 | 插件拖拽安装依赖 `file.path`，WKWebView 无该属性 → 拖拽安装疑似整体失效（需真机验证） | `Plugins.tsx:142` | 改用 Tauri drag-drop 事件（`getCurrent().onDragDropEvent`，在 Web 环境拿 paths） |
| P1-11 | 停止运行中游戏 5 处均无确认；关闭窗口无"游戏运行中"提示 | `Home.tsx:681`、`Instances.tsx:326,367`、`InstanceDetail.tsx:89,118`、`TaskCenter.tsx:143`、`TitleBar.tsx:50` | 抽 `confirmStopGame()` 复用 RmConfirm；TitleBar 关闭前检查运行中实例 |
| P1-12 | Settings 全页数字/文本输入 onInput 即刻写盘：每字符一次 IPC，无防抖无确认；与 InstanceDetail 显式保存模式并存 | `Settings.tsx:138-170,405` | 统一为显式保存（推荐，与实例设置一致）或至少 500ms 防抖+失焦提交 |
| P1-13 | 实例 SettingsTab 草稿切换 tab/离开页面被静默丢弃（dirty 状态存在但无拦截） | `InstanceDetail.tsx:127-134,734` | dirty 时拦截 tab 切换/路由离开，弹确认 |
| P1-14 | 版本列表 `slice(0,100)`、Mod 版本 `slice(0,30)` 硬截断：无加载更多/虚拟化/提示 | `Download.tsx:237,734` | 虚拟列表（大集合）或增量"加载更多"+总数提示 |
| P1-15 | 在用页面零响应式断点 + 窗口最小尺寸与设计文档冲突（800×500 vs 00_OVERVIEW.md:145 的 1280×720）；800px 宽下 hero 双栏重叠、accounts 360px 固定栏挤压 | `src/styles/app.css`、`src-tauri/tauri.conf.json` | 先决策最小尺寸（建议按设计文档 1280×720 收紧），再补 2-3 个断点或让固定栏可折叠 |

## P2（22 项）

| 编号 | 问题 | 位置 |
|---|---|---|
| P2-1 | 整页死代码 `HomeDashboard.tsx`（645 行，零引用）；其中 5 项实现优于在用版（ErrorBoundary/下载取消/空态引导/换肤入口/粘贴版本号），删除前建议回捞 | `src/pages/HomeDashboard.tsx` |
| P2-2 | 死 CSS ~135 行（6.7%）：`.dash-*`/`.quick-launch-*`（app.css:879-950,1498,1501）、`.rail` 家族（:275-331,344,1502-1503）、`.account-avatar-img`（:954）；注意 `.rail-task-indicator` 仍在用 | `src/styles/app.css` |
| P2-3 | Rail 死状态：`railCollapsed`/`toggleRail`/`rail.collapsed` pref 无消费者 | `src/stores/ui.ts:36,94-99,125-126` |
| P2-4 | dashboard 布局后端协议失去消费者（getLayout/saveLayout/resetLayout + DashboardLayout 类型） | `src/ipc/api.ts`、后端 commands |
| P2-5 | cacheStore inFlight 清理条件永假 → Map 无界增长 | `src/stores/cache.ts:49-57` |
| P2-6 | 信号 updater 内副作用（setRecent/outcomeId++/writePref） | `downloads.ts:114-119,145-150`、`ui.ts:95-98` |
| P2-7 | TaskCenter 确认卡 createMemo 内 setInterval，卸载后空转 ≤125s | `TaskCenter.tsx:245-252` |
| P2-8 | SkinViewer blob URL 从不 revoke | `SkinViewer.tsx:41,47` |
| P2-9 | AccountAvatar 纹理缓存永不失效，换肤后头像停旧图 | `AccountAvatar.tsx:19-30` |
| P2-10 | mcpStore.resolve 无条件移除确认卡，忽略后端裁决结果 | `mcp.ts:48-52` |
| P2-11 | profilesStore.refresh 的 force 被 inflight 吞掉 | `profiles.ts:22-24` |
| P2-12 | 并发下载 `rekeyPending("", batch_id)` 挂错 batch 串号风险 | `downloads.ts:158` |
| P2-13 | 吞错点清单：`Home.tsx:85,598`、`InstanceDetail.tsx:762-763,160-162`、`Download.tsx:511-517,820-822`、`Plugins.tsx:374-380,512-515`、settingsStore.refresh、SkinViewer init 失败永远"加载中" | 各文件 |
| P2-14 | Settings 无 loading/错误态：config null 时控件显示默认值，改动会以默认值合并保存（竞态窗口） | `Settings.tsx` |
| P2-15 | 后端字段 UI 无控件：min_memory_mb/metaspace_mb/env_vars/pre_launch/post_exit、plugin deleteData/previousVersion/rollback、instances.rename/duplicate 藏位 | `types.ts:412-422` 等（03 §4 DISP-1~5） |
| P2-16 | MCP 工具策略改 deny/allow 无确认；断开会话无确认 | `Settings.tsx:629-634,657-660` |
| P2-17 | 更新检查按钮永久禁用 + updater endpoint 为 `<owner>/<repo>` 占位符（release-blocked） | `Settings.tsx`、`tauri.conf.json` |
| P2-18 | 前端零测试（format/exitCodes/markdown 自述 unit-testable 无配套） | `src/` |
| P2-19 | LaunchBubble 失败 5s 自动消失、启动失败不自动开日志抽屉，错误信息易丢 | `LaunchBubble.tsx`、`init.ts` |
| P2-20 | Mods 禁用列表不参与搜索；扫描 conflicts/unparsed 只有计数 | `InstanceDetail.tsx:329-331,513-516` |
| P2-21 | 商店筛选缺口：游戏版本下拉未去重、loader 筛选缺 neo_forge/optifine（向导 8 种 vs 商店 3 种） | `Download.tsx:573,580-585` |
| P2-22 | Accounts 入口语义两处：设备码无法主动取消轮询（authStore 无取消 API）；"刷新登录"对离线账号显示且必然失败 | `Accounts.tsx:118,191-194` |

## P3（12 项）

| 编号 | 问题 | 位置 |
|---|---|---|
| P3-1 | 注释与实现不符：toast 注释 4s 实为 5000ms | `ui.ts:41,55`、`Toast.tsx:2` |
| P3-2 | `as unknown as IpcError` 伪造 `{code:"not_found"}` | `auth.ts:126` |
| P3-3 | safeInvoke 对非 IpcError reject 直接 as，`.message` 可能 undefined | `api.ts:119-121` |
| P3-4 | DOWNLOAD_PAUSED/RESUMED、AUTH_STATE_CHANGED 前端从未监听 | `events.ts:10-14` |
| P3-5 | 死导出：connectedToastsDone/setConnectedToastsDone、RmTextarea、RmBadge、BackLink | `mcp.ts:20,91-92` 等 |
| P3-6 | onThemeModeChange 单槽监听且未用 | `theme/index.ts:171-173` |
| P3-7 | durationMs 声明 bigint，运行时 number（漏 `#[ts(type="number")]`） | `types.ts:901` |
| P3-8 | i18n 硬编码收尾清单：Home 假数据文案（:430-540）、优先级枚举（InstanceDetail:907-911）、`missing`（Download:275）、插件 state 原文（Plugins:183）、`(saved)`/outcome（Settings:440,685）、`modded`（Instances:262、Settings:801）、aria-label 英文（overlays.tsx:52、surfaces.tsx:236-237）、步骤序号（Accounts:202） | 各文件 |
| P3-9 | LogDrawer 拖拽监听仅 pointerup 摘除，拖拽中卸载泄漏 | `LogDrawer.tsx:76-87` |
| P3-10 | launch.ts 45s 兜底 setTimeout 不随 dismiss 取消 | `launch.ts:110-115` |
| P3-11 | settings.ts 模块导入期写 DOM（applyMotion） | `settings.ts:34` |
| P3-12 | 运维备注：仓库根 `API_Key.txt` 为本地未入库文件（.gitignore:5 已忽略，`git ls-files` 确认未跟踪）——保持忽略即可，建议移出仓库目录防误提交；另有 6 个死 i18n key | `.gitignore:5`、`locales/*` |

---

### 与设计文档的漂移（合规项，单列）

按 AGENTS.md 核心约束 1"设计文档优先"：

1. 在用 Home.tsx（hero 布局 + localStorage 持久化 + 假数据卡）已偏离 `docs/ui-design/02_HOME_DASHBOARD.md`（后端布局协议 + 五态 QuickLauncher + ErrorBoundary），且未更新设计文档——**要么补 ADR/更新 ui-design 文档承认 hero 方案，要么回退对齐**，当前状态违反"先改文档再改代码"约束。
2. 窗口最小尺寸冲突（P1-15）同属实现-文档漂移。
3. `12_SUMMARY.md` 阶段 F 要求"旧 CSS 清零、旧路由删除"——旧 CSS 未清（P2-2），路由清理已完成。
