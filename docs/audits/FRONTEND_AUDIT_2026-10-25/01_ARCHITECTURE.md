# 01 · 前端架构与横切层审计

> 审计对象：RMCL 前端（Tauri 2.x + SolidJS，分支 `ui/home-hero-dock`）
> 审计范围：`src/App.tsx`、`src/main.tsx`、`src/stores/*`、`src/ipc/*`、`src/i18n/*`、`src/theme/*`、`src/plugin-ui/*`、`src/lib/*`、`src/styles/app.css`、构建配置、`src-tauri/tauri.conf.json`
> 所有结论均经代码实证（`文件:行号`），关键结论另经主审人工复核。

---

## 1. 总体架构

```
main.tsx (路由表: 7 活跃页 + 3 遗留重定向, 全部 lazy)
  └─ App shell (App.tsx)
       ├─ TitleBar（品牌 + 中央 Dock 导航 + 窗口控制 + 任务指示器）
       ├─ app-content（路由出口）
       └─ 全局浮层: TaskCenterPanel / LogDrawer / LaunchBubble /
          RmConfirmHost / RmPromptHost / ToastContainer
stores/  ← 唯一 Tauri listen() 点（init.ts 集中注册、幂等）
ipc/     ← safeInvoke 统一封装 + ts-rs 生成的 types.ts
i18n/    ← zh-CN/en 双 bundle，编译期强制同 key 集
theme/   ← CSS token 体系（--rmccc-*）
plugin-ui/ ← rmcccplugin:// iframe 桥（bridge.ts 协议 + PluginFrame 宿主）
```

**分层纪律是该前端最突出的优点**，属同类 Tauri 项目中的优秀实践：

| 优点 | 证据 |
|---|---|
| `safeInvoke` 统一错误封装，所有 invoke 永不 reject，IPC 错误处理覆盖率实质 100% | `src/ipc/api.ts:112-122` |
| 事件监听集中在 `init.ts`，stores 是唯一 `listen()` 点，幂等注册 | `src/stores/init.ts` |
| ts-rs 生成类型桥（Rust → TypeScript），当前抽查无漂移 | `bindings.rs` ↔ `types.ts`（AuthSession/Profile 32 字段/McpStatusView 等逐项一致） |
| `index.html` 内联 boot guard + App.tsx 揭窗握手 + `ensureReachable` 多显示器可达性检查 | `App.tsx:24-58` |
| skinview3d（内含 three.js）经动态 import 独立分包，不进首屏 chunk | `SkinViewer.tsx:8,149` |
| 依赖极简：运行时仅 6 个直接依赖，符合 <10MiB 体积红线 | `package.json` |
| `markdown.ts` 自研但理由正当：escape-first 安全模型 + 流式输出友好，无 markdown-it/DOMPurify 供应链成本 | `src/lib/markdown.ts:11-18,147` |
| SolidJS 使用整体规范：全库无 props 解构丢响应性问题，多处正确 `onCleanup`，Download 搜索有 `searchSeq` 竞态守卫 | 全库扫描 |

---

## 2. 风险与缺陷（横切层）

### 2.1 IPC 与类型桥

| 编号 | 问题 | 位置 |
|---|---|---|
| ARC-1 | **bindings 漂移守卫测试恒真**：测试先 `write_types_file()` 重写 types.ts 再与重新生成内容 `assert_eq`，永远通过，无法发现提交过期 types.ts，且 `cargo test` 会改写工作区文件。当前文件虽无漂移，但守卫机制失效使一致性无持续保障 | `src-tauri/src/bindings.rs:164-178` |
| ARC-2 | `safeInvoke` 把任意 reject 直接 `as IpcError`——后端命令统一 `Result<T, IpcError>` 时无误，但 Tauri 层字符串错误会使 `.message` 为 undefined | `src/ipc/api.ts:119-121` |
| ARC-3 | `McpRecentCallView.durationMs` 声明为 `bigint`，JSON 运行时是 number（同文件其他 u64 字段有 `#[ts(type="number")]` 标注，此处漏标）；当前 UI 未读该字段 | `src/ipc/types.ts:901` |
| ARC-4 | `DOWNLOAD_PAUSED` / `DOWNLOAD_RESUMED` / `AUTH_STATE_CHANGED` 前端从未监听（Rust 侧对称标注 dead_code 预留） | `src/ipc/events.ts:10-14` |

### 2.2 状态管理（stores）

| 编号 | 问题 | 位置 |
|---|---|---|
| ARC-5 | **cacheStore.refreshIcon 的 inFlight 清理条件永假**：比较的是 `set` 之前的旧值，`delete` 永不执行，Map 随不同 URL 无界增长（去重 60s 窗口本身仍有效） | `src/stores/cache.ts:49-57` |
| ARC-6 | 信号 updater 内嵌副作用（Solid 反模式）：`setRecent`+`outcomeId++` 在 `setBatches` updater 内；`toggleRail` 在 updater 内写 localStorage | `src/stores/downloads.ts:114-119,145-150`、`src/stores/ui.ts:95-98` |
| ARC-7 | `profilesStore.refresh` 的 `force` 参数被 inflight 判定吞掉：`DOWNLOAD_COMPLETE → refresh(true)` 可能拿到刚发起的旧请求结果 | `src/stores/profiles.ts:22-24` |
| ARC-8 | 并发下载时 `rekeyPending("", p.batch_id)` 对每个 progress 事件执行，无 profile 标识的向导 pending 会挂到最先到达的任意 batch 上，label/version 有串号风险 | `src/stores/downloads.ts:158` |
| ARC-9 | `mcpStore.resolve` 忽略后端结果：`resolveConfirmation` 失败（如服务端已超时裁决）也无条件从 UI 移除确认卡 | `src/stores/mcp.ts:48-52` |
| ARC-10 | mcpStore 全局 2s 轮询永不停止（有 enabled 门槛，注释已自述，属已知取舍） | `src/stores/mcp.ts:54-60` |

### 2.3 生命周期与资源泄漏

| 编号 | 问题 | 位置 |
|---|---|---|
| ARC-11 | **TitleBar 的 onCleanup 在 async onMount 的 await 之后调用，实际是 noop**：此时 Solid 无 Owner，`onResized` 监听永不注销（HMR/重挂载叠加监听）。`SkinViewer.tsx:204-205` 注释表明团队已知此坑，TitleBar 是漏网点 | `src/components/TitleBar.tsx:46-56` |
| ARC-12 | **游戏日志缓冲区实际永不释放**：`dropBuffer` 的 onCleanup 注册在常驻组件顶层（仅应用退出时执行）；关闭抽屉仅在崩溃时 keepBuffer，非崩溃路径既不 keep 也不 drop；`keepBuffers` 集合只写不读。每个启动过的 profile 最多 1000 行日志驻留内存至退出，与 launch.ts 头注释的设计意图相悖 | `src/components/LogDrawer.tsx:102-105,148-152`、`src/stores/launch.ts:43,10` |
| ARC-13 | TaskCenter 确认卡在 `createMemo` 内 `setInterval`，卡片卸载后 interval 空转最长 125s，memo 重跑会叠加 | `src/components/TaskCenter.tsx:245-252` |
| ARC-14 | SkinViewer 两次 `URL.createObjectURL` 全库无 `revokeObjectURL`，每次换肤/切账号泄漏一个 blob | `src/components/SkinViewer.tsx:41,47` |
| ARC-15 | AccountAvatar 模块级纹理缓存按 uuid 永不失效：换皮肤后页内头像仍用旧 URL 直至重启 | `src/components/AccountAvatar.tsx:19-30` |
| ARC-16 | LogDrawer 拖拽调宽的 pointermove/pointerup 仅在 pointerup 时摘除，拖拽中卸载则泄漏 | `src/components/LogDrawer.tsx:76-87` |
| ARC-17 | launch.ts 45s 兜底 setTimeout 不随 `dismissFlow` 取消（有 stage 守卫，纯悬挂句柄） | `src/stores/launch.ts:110-115` |

### 2.4 事件语义

| 编号 | 问题 | 位置 |
|---|---|---|
| ARC-18 | **用户手动停止游戏会弹"崩溃"日志抽屉**：init.ts 仅判断 `exit_code !== 0` 即 openLogDrawer；`launch_kill` 走进程组终止（SIGKILL→137），点"停止"反而触发崩溃抽屉。事件已带 `exit_type`（Normal/Abnormal）但前端未消费 | `src/stores/init.ts:58-63`、`src-tauri/src/commands/launch.rs:11,128-140` |

### 2.5 主题与缓存

| 编号 | 问题 | 位置 |
|---|---|---|
| ARC-19 | `onThemeModeChange` 单槽监听器且全库未用，两处订阅会互相覆盖 | `src/theme/index.ts:171-173` |
| ARC-20 | `settings.ts` 模块导入期即写 DOM（`applyMotion`），无 document 的环境（如未来引入测试）导入即崩 | `src/stores/settings.ts:34` |

---

## 3. 冗余与死代码

两次 UI 实验（Rail→Dock、Dashboard→Hero）的残骸未清理，是当前最大的一笔架构债：

| 编号 | 内容 | 位置 | 规模 |
|---|---|---|---|
| ARC-21 | **整页死代码 `HomeDashboard.tsx`**：全仓（含路由表与 lazy import）无任何引用；且死代码版本反而保留了 5 项在用版丢失的实现（详见 `02_PAGES_AND_COMPONENTS.md` §0 对照表） | `src/pages/HomeDashboard.tsx` | 645 行 |
| ARC-22 | 死 CSS：dashboard-grid / `.dash-*` / `.quick-launch-*` 家族 | `src/styles/app.css:879-950,1498,1501` | ~72 行 |
| ARC-23 | Rail 残留：`railCollapsed`/`toggleRail`/`rail.collapsed` pref 无任何组件消费（Rail 已被 Dock 取代） | `src/stores/ui.ts:36,94-99,125-126` | 死状态 |
| ARC-24 | 死 CSS：`.rail/.rail-item/.rail-item-label/.rail-badge/.rail-footer/.is-collapsed`（注意 `.rail-task-indicator` 家族仍在用，勿删） | `src/styles/app.css:275-331,344,1502-1503` | ~57 行 |
| ARC-25 | 后端 dashboard 布局协议失去消费者：`dashboard.getLayout/saveLayout/resetLayout`、`DashboardLayout/WidgetInstance/GridSize` 类型前端已无引用（在用 Home.tsx 改用 localStorage `rmccc.heroLayout.v2`，仅 `listWidgets`/theme 系列仍被用） | `src/ipc/api.ts`、后端 commands | 跨层冗余 |
| ARC-26 | 死导出：`RmTextarea`/`RmBadge` 无消费者、`PageHeader` 的 `BackLink` 未使用、`mcp.ts` 的 `connectedToastsDone`/`setConnectedToastsDone` 全库无使用 | `controls.tsx`、`PageHeader.tsx`、`mcp.ts:20,91-92` | 小 |
| ARC-27 | 死 i18n key 6 个（约 1%）：`home.restoreHiddenNote`、`instances.menu.stop`、`logs.abnormalExit`、`nav.collapse`、`settings.general.mode.dark/light` | `src/i18n/locales/*` | 6 key |

**CSS 死代码合计约 135 行 ≈ 6.7%**（app.css 共 2015 行 / 393 个类选择器）。样式本体质量好：全 token 化、`!important` 仅 2 处（均为 `prefers-reduced-motion` 正当使用）、此前疑似死的类多数实为动态模板构造使用（已逐一验证非死）。

---

## 4. i18n 体系

结论：**质量高，双语完全一致**。

- en.ts 与 zh-CN.ts 各 558 个 key，双向 diff 为空（脚本实测）；
- `Messages = Record<TranslationKey, string>`（`locales/messages.ts:16`）在编译期强制两 bundle 同集；
- 代码中 487 处 `t("…")` 引用的 key 全部存在（missing=0）；动态拼接 key（`plugins.state.${state}` 等）对应 key 也齐全；
- 回退链 en → key 本身（`i18n/index.ts:89-96`），永不渲染空串。

缺口在"绕过 i18n 的硬编码文本"（页面层问题，清单见 `02` 各页⑤与 `04_ISSUES.md` H 类），i18n 体系本身无缺陷。

---

## 5. 布局形态与窗口约束（横切）

| 编号 | 问题 | 位置 |
|---|---|---|
| ARC-28 | **响应式断点只存在于死代码**：`@media` 断点仅覆盖 dashboard-grid（app.css:1497-1503，已死）；所有在用布局（Home hero 双栏、accounts-split 360px 固定、sidetabs 200px 固定、master-detail）全部无断点 | `src/styles/app.css` |
| ARC-29 | **窗口最小尺寸与设计文档冲突**：tauri.conf.json 为 `minWidth: 800 / minHeight: 500`（默认 1100×700），而 `docs/ui-design/00_OVERVIEW.md:145` 规定"最小窗口 1280×720（与 Dashboard 断点 wide 对齐）"。按 AGENTS.md"设计文档优先"，二者必有一错；且在 800px 宽下 Home 的 hero-stack(300px)+hero-corner(340px) 会重叠、Accounts 左栏被 360px 固定皮肤栏挤压 | `src-tauri/tauri.conf.json` ↔ `docs/ui-design/00_OVERVIEW.md:145` |

---

## 6. 构建与依赖

| 项 | 结论 |
|---|---|
| 依赖 | 运行时仅 6 个（router/tailwind/tauri-api/skinview3d/solid-js），无冗余依赖 |
| 大件加载 | skinview3d 动态 import 分包正确；无其他大件风险 |
| markdown | 自研合理（安全模型 + 流式），不建议换 marked/DOMPurify |
| 测试 | **前端零测试**：src 下无任何 `*.test.*`；`lib/format.ts`、`lib/exitCodes.ts`、`lib/markdown.ts` 自述 "unit-testable" 却无配套 |
| updater | `plugins.updater.endpoints` 仍为 `https://github.com/<owner>/<repo>/...` 占位符，且 `$schema` 指向 fork（nicois/tauri）——发布前必须处理（release-blocked 性质） |

---

## 7. 横切层健康度小结

| 维度 | 评价 |
|---|---|
| 分层与 IPC 纪律 | ★★★★★ 优秀，可作为项目范式保持 |
| 类型桥 | ★★★★☆ 当前一致，守卫失效（ARC-1）需修复 |
| SolidJS 使用 | ★★★★☆ 整体规范，2 处真实反模式（ARC-11/13） |
| 资源生命周期 | ★★★☆☆ 日志缓冲策略退化为死代码（ARC-12），若干小泄漏 |
| 实验残留清理 | ★★☆☆☆ 645 行死页 + ~7% 死 CSS + 死状态未清理 |
| 测试与守卫 | ★☆☆☆☆ 零前端测试 + 漂移守卫恒真 |
| i18n | ★★★★★ 双语完全一致，仅剩页面层硬编码收尾 |
