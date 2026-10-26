# 03 · 修复方案（Fix Plan）

> 覆盖范围：`01_RECHECK_51.md` 的 40 项 STILL + 9 项 PARTIAL 残余 + `02_DELTA_AUDIT.md` 的 19 项新问题（N-1~N-19）+ P1-11 恶化。
> 组织方式：按依赖与优先级分 6 个批次（B0→B5），每项给出**修什么 / 怎么修（具体到文件与做法）/ 怎么验证**。
> 所有方案基于当前 HEAD `3b863da` 实证（含后端签名核实：`profile_store.delete(id, cleanup_game_dir)` 参数已存在、`generate_types()` 可独立调用、后端无 server ping 命令）。

> **执行状态（2026-10-26，B0–B3 已落地）**
> - **P0-1 ✅**：5 个跳转源 query 移到 path；ModDetail 手写 hash 解析（死代码）删除；ModStore ⚓"退出上下文"响应式修复。
> - **P0-2 ⏭️ 按产品决策跳过**：假数据三卡保留为占位符（D3 取"保留"），待后端 server ping 接入时一并真实化。
> - **N-3 ✅**：`profile_delete` 加 `cleanup_game_dir` 参数（MCP 保守传 false）；删除对话框加目录清理开关（默认开）；先删档案再删版本目录（修 N-18）。
> - **N-1 ✅**：versionCache `gameVersionsError()` 错误通道 + 向导错误态带重试 + "绑定本地版本"恢复（fd95fee 回捞）。
> - **N-2 ✅**：LoaderRow 折叠状态提升到 TaskCenterPanel 层。
> - **B2-1 ✅**：新增 `src/lib/useHashTab.ts`，Download/InstanceDetail/Settings 三页 tab 全部 hash 派生（P1-2/P1-3 连根修）；InstanceDetail 模块级 setTab 删除，Overview 快捷按钮改走 onGoTab。
> - **B2-2 ✅**：prompt 回填 initial（P1-5）；Settings 回填加 touched/dirty 守卫（P1-1）；内存/窗口/JVM/游戏参数/并发全部显式保存（P1-12 残余 + P2-14 竞态窗口消除，settingsStore 加 loadError）；分辨率复合锁成对保存（P1-4）；dirty 切 tab 拦截确认（P1-13）。
> - **B2-3 ✅**：launch store 新增 `stop()`（统一确认）+ `userKilled` 标记 + LAUNCH_STARTED 清标记（P1-7/P1-11）；7 处停止入口全部走 stop()；TitleBar 关窗运行检查 + onCleanup 修复（P1-6）。**D2 偏离**：confirm host 无 checkbox，"不再询问"未做（仅确认本身），后续单独补。
> - **B2-4 ✅**：bindings 漂移测试改只读比较（写盘由 main.rs 启动承担）。
> - **B2-5 ✅**：ModDetail 版本列表"显示全部"开关（P1-14 残余）；版本下拉去重；loader 筛选对齐 LOADER_CARDS 6 种（P2-21；cleanroom/legacy_fabric 已按产品决定移出 UI）。
> - **B2-6 ✅（D1 按默认建议）→ 2026-10-26 修正**：tauri.conf 窗口 1280×760 / min 1280×720（对齐 ui-design/00 §145）。**原随批添加的 1150px 兜底断点已回退删除**——真机验证中该断点在用户窗口宽度下触发，且其 static 兜底在 flex 行容器内产生错位（hero-corner 被挤成右上窄条、accounts 皮肤栏叠到下方），实际破坏了设计布局；hero 双栏与 accounts 分栏恢复为**无条件固定布局**（hero-stack 300-460px + corner 340px、accounts 1fr/360px），1280 最小窗宽下始终容纳。教训：媒体查询兜底必须先在真机窗口宽度下验证，而非只看最小配置。
> - **B3-1 ✅**：HomeDashboard 五项回捞（插件卡 ErrorBoundary / 下载行取消 / 无实例引导 CTA / 正版换肤入口 / 粘贴版本号）后删除文件；死 CSS ~150 行（rail 家族保留 task-indicator、dash-*、quick-launch、account-avatar-img、rm-badge、死断点）；ui.ts rail 状态、api.ts 三条布局方法、RmTextarea/RmBadge/BackLink/connectedToastsDone 死导出全删；**后端三条 dashboard 布局命令一并下线**（ipc registry 守卫测试强制，命令数 100→97，ARCHITECTURE 表同步）；ui-design/02 补 hero 取代网格的决策记录。附带：Home 版本卡接 versionCache（N-19）。
> - **B3-2 ✅**：cache inFlight 比较值修正（P2-5）+ avatarRev 换肤失效链（P2-9）；downloads updater 副作用外移（P2-6）+ rekeyPending 无歧义匹配（P2-12）；profiles force 链式刷新（P2-11）；mcp resolve 失败改 resync（P2-10）；TaskCenter 确认卡 interval onCleanup（P2-7）；SkinViewer blob LRU+revoke 上限 8（P2-8）；versionCache loader 条目 LRU 32 + 形状迁移（N-7）；VirtualList rAF 内读实时 scrollTop（N-13）。
> - **B3-3 ✅**：Home/InstanceDetail(×3)/Plugins(×2) 吞错点补 toast；SkinViewer init 失败显示错误+重试（P2-13 组）；LaunchBubble failed 不再 5s 自动消失（P2-19 前半；启动命令失败无日志缓冲，不开日志抽屉）；theme 模块 effectiveMode signal（N-11）；specs 死字段删除（N-12）；Toast 分 tone 图标（N-15）；死 i18n 键 10 条清理（N-16 + updates.soon/checkNow 随按钮摘除）。
> - **B3-4 ✅**：星辰 top 取模归一（N-4）；ThemeAmbient 加 Switch 兜底（N-5）。
> - **B3-5 ✅**：实例设置补 min_memory/metaspace/前置/退出后命令（带锁）+ 环境变量 textarea（P2-15）；Plugins 移除对话框加"同时删除数据"开关 + 回滚菜单 + previousVersion 展示（P2-15）；MCP 策略修改与断开会话确认（P2-16）；Accounts 离线隐藏刷新 + 设备码陈旧流抑制（P2-22，后端取消接口仍为待办）；Mods 搜索覆盖禁用列表 + 冲突/未解析明细列表（P2-20）。
> - **B3-6/7 ✅**：VirtualList 容器键盘滚动（方向键/翻页/Home/End，N-6）+ 删除 44px 死默认行高（N-14）；"立即检查更新"按钮摘除（P2-17 前半；updater endpoint 仍为发布前必办）。
> - **B4 ✅（2026-10-26）**：P3-1 注释 4s→5s；P3-2 auth 伪造错误改合法 IpcError 字面量；P3-3 safeInvoke 归一化非 IpcError reject（code:"unknown"）；P3-6 onThemeModeChange 改多播 Set；P3-7 duration_ms 加 `#[ts(type="number")]` + types.ts 同步；P3-8 硬编码清单清零（优先级枚举/plugins state 原文/(saved)/outcome/modded×2/锁 aria-label/设备码步骤序号）；P3-9 LogDrawer 拖拽监听 onCleanup；P3-10 45s 兜底句柄随 dismissFlow 取消；P3-11 settings 模块 DOM 写入加环境守卫；N-8 Download 死 signal/死导入删除；N-9 loader 卡双 IPC 合一（onClick 直调删除，仅留 effect 路径）；N-10 openVersions 保留用户已选版本；N-17 三个未引用 mc-icons 移 docs/design/mc-icons-src。
> - **B5 ✅（2026-10-26）**：vitest 5.0.5（`pnpm test` = `vitest run`，vitest.config.ts 与应用构建链隔离）；5 个测试文件 32 用例——markdown（escape-first 安全模型 6 例 + 结构 6 例 + 流式 3 例，安全件优先）、format、exitCodes、loaders、versionCache 的 `isCacheFresh`（新抽的纯 TTL 谓词，gameVersionsWithRefresh 改用它）。
> - 门禁（B4/B5 后）：`pnpm typecheck` / `pnpm test`（32/32）/ `pnpm build` / `cargo clippy --workspace --all-targets -D warnings` / `cargo test --workspace`（654 通过 0 失败）全绿。
> - **并发编辑提示**：crates/ 下存在 9 文件 +342/-141 的未提交后端改动（download 重构、pipeline 异步化、agent/mods），**非本审计会话所做**（疑为并行会话）；其编译、clippy、测试均通过，未被本会话触碰，保留原样。

## 0. 总则

- **顺序**：B0（P0 止血）→ B1（新 P1）→ B2（旧 P1 集群）→ B3（P2 清理）→ B4（P3 收尾）；B5（测试与守卫）与 B0 同步启动，是防止"修一处坏一处"的前提——本轮 N-1（删功能连带删报错路径）正是零测试下的典型回归。
- **每批验收门**：`pnpm typecheck` + `pnpm build` + `cargo test --workspace` 全绿；批次内每个修复项附带一条手动验证路径（下文逐项给出）。
- **文档同步义务**（AGENTS.md 核心约束 1）：B0 涉及首页形态与 B2 涉及窗口尺寸的两处设计决策（D1/D2），落地时须同步更新 `docs/ui-design/`（02 主页、00 总览），先文档后代码。

---

## B0 · P0 止血（预估 0.5 天）

### P0-1 hash 查询参数断链（核心链路修复）

**方案（推荐 A，标准化语义）**：
1. 5 个跳转源统一把 query 从 hash 内挪到 path 上：
   - `Instances.tsx:151`、`InstanceDetail.tsx:183/270/404`：`/download#mods?instance=x` → `/download?instance=x#mods`
   - `Home.tsx:915`：`/download#install?version=x` → `/download?version=x#install`
2. 消费点不变（`Download.tsx:486` `useSearchParams().instance`、`:85-94` version/loader 本就读 path search，改完即通）。
3. 删除 `Download.tsx:774-778` ModDetail 的手写 hash 解析（统一后成死代码）。
4. 若确需保留 hash 内参数的兼容（外部链接），在 main.tsx 加一次性重写：进入 `/download` 时检测 `location.hash` 含 `?`，`replace` 成标准格式。

**验证**：实例列表"浏览 Mod"→ 商店顶部出现 ⚓ 上下文条且版本/loader 被锁定；首页版本卡 → 向导 Step1 预选对应版本；`grep -rn "#mods?" src/` 为零。

> 注意耦合：同页跳转（Step5"去 Mod 商店"）要等 B2-1 的 tab 派生修复才完全生效，本批先修跨页链路。

### P0-2 首页假数据三卡（决策 D3）

**短期（本批，纯前端）**：
- 三卡**移出默认布局**（`Home.tsx:50-53` 默认卡片列表只留账号卡/启动卡/实例卡），保留在"添加卡片"对话框中并加 `hero.demo` 徽标（i18n 双语）。
- 服务器卡保留输入框与四态框架，"连接"按钮改为禁用 + title `hero.server.pending`（"后端 ping 未接入"），删掉 800ms 假延时逻辑（:737-740）与 mock 渲染分支（:727-732 改为空态文案）。
- 公告卡（MOCK_ANNOUNCEMENTS :835-839）整个从默认与添加列表中移除，代码保留待后端。

**中期（需后端配套，另立任务）**：新增 `server_ping` 命令（status/motd/players/latency），前端恢复三卡。已在后端验证：当前无任何 ping/motd 命令，属新开发而非接线。

**验证**：全新用户首启默认布局无任何 mock 文案；添加卡片对话框可见"演示"徽标。

---

## B1 · 新引入 P1（预估 1 天）

### N-3 删除对话框契约失真（破坏性操作信任问题，最先修）

**方案（补参数透传，两方向取"能力补齐"而非"改文案缩水"）**：
1. 后端：`profile_delete` 命令加参数 `cleanup_game_dir: bool`（`src-tauri/src/commands/profile.rs:224`，store 签名 `delete(&id, cleanup_game_dir)` 已支持，含共享根/特征目录双守卫）。
2. 前端：`api.ts:151` `delete(id, cleanupGameDir)`；`DeleteInstanceDialog.tsx:61` 传入。
3. 对话框 UI：加复选框"同时删除独立游戏目录"（**默认勾选**，与现有文案 `deleteAll.body` 一致）；共享版本时隐藏该复选框（走 `deleteAll.shared` 分支本就不删目录）。
4. 顺带修 N-18（部分失败中间态）：调整顺序为 **先 `profiles.delete`（含目录）成功后再 `instances.remove`**；第二步失败时提示"版本已删除，目录记录移除失败，重试仅清理记录"，并确认 `instances.remove` 对不存在条目幂等（后端确认点）。

**验证**：删除隔离实例（勾选）后 mods/saves 目录消失；不勾选时目录保留且文案如实；共享版本实例删除时复选框不出现。

### N-1 向导报错路径 + 绑定本地版本

**方案**：
1. `versionCache.ts`：`refreshGameVersions` 失败时暴露错误——新增模块级 `const [gameError, setGameError] = createSignal<string | null>(null)` 并导出；`.catch` 分支 `setGameError(msg)`，成功路径清空。
2. `Download.tsx` InstallWizard：onMount 取缓存快照后，若 `list.length === 0 && gameError()` → 渲染 `RmErrorState`（带重试按钮，onRetry 重新 refresh）；Step1 的 `setError`（:82）接上或删除（用 store 的 error 代替死 signal）。
3. 从 git 历史（`fd95fee` 的 Download.tsx）恢复"绑定本地版本"对话框与入口按钮，作为离线兜底路径。
4. 手动刷新按钮（:287）失败同样走 error 态而非静默。

**验证**：断网 + 清缓存（删 localStorage `rmccc.versions.game`）冷启动 → Step1 显示错误与重试；恢复网络重试成功；绑定本地版本可走通。

### N-2 TaskCenter 细节面板被进度事件折回

**方案（状态提升，不改 store 语义）**：
- `TaskCenter.tsx`：把 `showDetails` 从 LoaderRow 局部 signal（:182）提升为面板级 `createStore<Record<string, boolean>>`（或 signal Map），行组件按 `expanded[key]()` 读取、`toggle(key)` 切换——行随 `<For>` 重建不再丢状态。
- （可选加固）`modloaders.ts:64-91`：同一 key 的进度更新复用旧对象仅改变化字段（`Object.assign` 到现有 entry 仍产生新引用，故此项不解决根因，仅减少无效渲染；真正的修复是上面的状态提升）。

**验证**：安装 Fabric 过程中展开细节，进度持续上报时面板保持展开且日志滚动。

---

## B2 · 旧 P1 集群（预估 2~2.5 天）

### B2-1 路由/tab 派生化（修 P1-2 + P1-3 + P0-1 残留）

**方案**：新增 `src/lib/useHashTab.ts`：

```ts
export function useHashTab(prefix = "") {
  const read = () => window.location.hash.replace(/^#\/?/, "").split("?")[0] || "";
  const [tab, setTab_] = createSignal(read());
  const onChange = () => setTab_(read());
  window.addEventListener("hashchange", onChange);
  onCleanup(() => window.removeEventListener("hashchange", onChange));
  const setTab = (t: string) => {
    const [path] = window.location.pathname.split("#");
    history.pushState(null, "", `${path}#${prefix}${t}`);  // path 已含 query（P0-1 修复后）
    onChange();
  };
  return [tab, setTab];
}
```

- Download（:44-46）、InstanceDetail（:60，模块级 `setTab` :285-288 删除改用 hook 的 setTab）、Settings（:40-44）三处替换为该 hook。
- Overview 快捷按钮（`InstanceDetail.tsx:273/276`）改调 hook setTab。

**验证**：Overview"缺失扫描"点击切到 mods tab；向导 Step5"去 Mod 商店"切到商店 tab；三个页面直接改 URL hash 均响应。

### B2-2 表单状态组（P1-1 / P1-4 / P1-5 / P1-12 残余 / P1-13 / P2-14）

| 项 | 修法 |
|---|---|
| P1-1 覆盖编辑 | `Settings.tsx:84-89`：effect 回填前判 dirty——`if (rootDirInput() === lastSavedRoot()) setRootDir(...)`（字段当前值 ≠ 已保存值即跳过回填）；autoMemory 开关同法 |
| P1-4 分辨率 | `InstanceDetail.tsx:888-896`：宽高合并为**一个复合锁**（"分辨率"是一行一个语义）：lock 图标同时切换 width+height 的 override；保存循环遇 `width` 或 `height` 任一时**同时写入** `opts.width/opts.height` 并把两者都加入 override_properties。删掉"只锁其一"的可能 |
| P1-5 prompt 回填 | `overlays.tsx`：`createEffect(() => { const r = uiStore.promptRequest(); if (r) setValue(r.initial ?? ""); })`——每次新请求重置为 initial；4 个调用方无需改动 |
| P1-12 残余 | 内存区间（:158-167）、窗口宽高（:177-179）、JVM/游戏参数（:190-199）、并发数（:453-454）全部改为与 rootDir 相同的**显式保存**模式（dirty 才可用 + 保存按钮），消除逐字符 IPC |
| P1-13 草稿守卫 | `InstanceDetail.tsx:118` `RmSideTabs onSelect` 包一层：`dirty() ? confirmLeave() : setTab(t)`（confirmLeave 弹"保存/放弃/取消"三选）；另加 `window.onbeforeunload`（dirty 时拦截关窗） |
| P2-14 | 显式保存落地后自然消除（控件不再在 config null 期写盘）；Settings 顶部加全局 loading skeleton（`settingsStore.loaded` 消费） |

**验证**：改根目录不保存→改语言→根目录输入框不丢；只锁分辨率锁→宽高都可编辑且都保存；重命名框打开预填当前名；JVM 参数输入 100 字符 → Network/IPC 面板只有一次 `settings_set`；dirty 时切 tab 弹确认。

### B2-3 游戏生命周期组（P1-7 残余 / P1-11 恶化）

| 项 | 修法 |
|---|---|
| P1-7 手动停止误报崩溃 | 前端标记法（不依赖后端）：`launch.ts` 的 `stopGame/killGame` 在调用 `launch_kill` 前把 profile_id 加入 `userKilled: Set<string>`；`init.ts:59-64` 判定改为 `exit_code !== 0 && !userKilled.delete(profile_id)` 才 openLogDrawer；LogDrawer 横幅对 kill 路径显示"已手动停止"而非退出码。（后端可选加固：ExitType 加 `Killed` 变体，非本批必须） |
| P1-11 停止确认（7 处） | 抽 `confirmStopGame(profileName)`（RmConfirmHost 包装，文案含实例名）；应用到 `Home.tsx:647-655/651/979`、`Instances.tsx:328/369`、`InstanceDetail.tsx:92/121`、`TaskCenter.tsx:131`；确认框带"不再询问"勾选（prefs 持久化，避免高频打扰）。TitleBar close（:50）前检查 `launchStore.running()` 非空 → 弹"仍有游戏运行，确认退出？" |

**验证**：任一停止入口弹确认；勾选"不再询问"后不再弹；启动游戏后点窗口关闭钮弹运行提示；手动停止后日志抽屉不自动弹出。

### B2-4 工程守卫（P1-9 / P2-18 起步）

- **P1-9**：`bindings.rs` 测试删除 `write_types_file()` 调用，改为 `generate_types()` 的结果与磁盘文件**只读比较**（两个函数已分离存在，纯删一行 + 断言不变）。测试名注释同步更新。
- **B5 详述测试**（见 B5），本项先让守卫真实生效。

### B2-5 列表与筛选（P1-14 残余 / P2-21）

- ModDetail 版本列表（`Download.tsx:807`）：`slice(0,30)` 改为"显示全部"增量按钮或复用 VirtualList（含分页参数则显示页码）。
- `Download.tsx:646` 版本下拉 `[...new Set(...)]` 去重；`:653-658` loader 筛选改用 `lib/loaders.ts` 全量 8 项（向导与商店同源）。

### B2-6 布局与断点（P1-15，决策 D1）

**决策 D1**：按 AGENTS.md"设计文档优先"，`tauri.conf.json` minWidth/minHeight 从 800×500 收紧为 **1280×720**（对齐 `docs/ui-design/00_OVERVIEW.md:145`）。若产品认为 1280 过大，则**先改文档**（00_OVERVIEW 记录新决策 + 理由）再保留较小尺寸——二选一，不允许继续两头矛盾。
**断点（第二道防线）**：app.css 增加两条 `@media (max-width: 1180px)`：hero 单栏（`.hero-corner` 转静态流式）；`accounts-split` 改单列（皮肤预览折叠为可展开）。`.instance-grid`/`master-detail` 已有弹性无需处理。

---

## B3 · P2 清理（预估 2 天）

### B3-1 死代码专项（P2-1/2/3/4，先回捞后删除）

1. **回捞清单**（从 HomeDashboard.tsx 移植到 Home.tsx 后再删）：插件卡 ErrorBoundary 包裹（损坏卡可重试/隐藏）；下载卡行内取消按钮；无实例引导空态（CTA 跳下载）；正版"更换皮肤"入口（账号卡菜单）；版本卡"粘贴版本号"输入框。
2. 删除 `src/pages/HomeDashboard.tsx`（645 行）。
3. 删除死 CSS（保留 `.rail-task-indicator` 家族）：`.rail` 家族 app.css:275-355、`.dash-*` :882-920、`.quick-launch-*` :922-946、`.account-avatar-img` :950；连带 :1607-1613 中只服务死代码的断点。
4. 删除 `ui.ts` railCollapsed/toggleRail/rail pref（:36,94-99,125-126）；`api.ts:335-340` dashboard 布局三方法下线（后端命令暂留，前端无消费者即可，后端清理另行批次）。
5. 死导出清理：mcp.ts connectedToastsDone、controls.tsx RmTextarea/RmBadge、PageHeader BackLink（若 B3 回捞未用到）。
6. **文档同步**：Home hero 形态与 ui-design/02 的偏离在此了断——补一节"hero 方案取代 dashboard 网格"的决策记录（或 ADR），列明回捞/放弃项。

### B3-2 stores 资源与竞态（P2-5/6/7/8/9/10/11/12 + N-7/N-13）

| 项 | 修法 |
|---|---|
| P2-5 cache.ts inFlight | `const now = Date.now(); inFlight.set(url, now); … if (inFlight.get(url) === now) inFlight.delete(url)`（比较 set 后的值） |
| P2-6 updater 副作用 | `downloads.ts:114-119/145-150`：把 setRecent/outcomeId++ 移到 `setBatches(...)` 调用之后顺序执行；ui.ts toggleRail 的 writePref 移出 updater |
| P2-7 TaskCenter interval | ConfirmationCard 改 `onMount(() => { const id = setInterval(...); onCleanup(() => clearInterval(id)); })`，删除 125s setTimeout 兜底 |
| P2-8 SkinViewer blob | 模块 Map 增加替换逻辑：set 新 blob 前 `URL.revokeObjectURL(old)` |
| P2-9 头像缓存失效 | textureCache 键改为 `uuid@rev`，rev 来自 `warmAvatar` 更新时写入的 localStorage 时间戳；skin 事件到达时 bump rev |
| P2-10 mcp resolve | `mcp.resolveConfirmation` 失败时保留卡片并标记超时态（后端可能已裁决，展示"已过期，刷新状态"），成功才移除 |
| P2-11 profiles force | `if (inflight) return force ? inflight.then(() => doRefresh()) : inflight` |
| P2-12 rekeyPending | 向导创建 pending 时记录 label/gameVersion，rekey 仅在能匹配（batch 的 label/version 相等或 pending 无归属且 batch 唯一）时挂载 |
| N-7 versionCache loader | loader 条目加 10min TTL + LRU 上限 32；`fetchedAt` 用于 TTL 判定（消除死字段）；writeJson 配额失败 `console.warn` 一次 |
| N-13 VirtualList onScroll | 回调内读 `el.scrollTop` 而非捕获事件参数位置 |

### B3-3 吞错收口（P2-13 残余 + P2-19 + N-11/N-12/N-15/N-16）

- 逐点补错误 UI：Home onMount（:380,894-897）、InstanceDetail（:150-154,219-222,973-975）、Plugins config/contributions（:374-379,512-514）、settingsStore.refresh（Settings 顶部 error banner + 重试）、SkinViewer init 失败（:246-247 → 错误态 + 重试，替换永远"加载中"）。
- P2-19：LaunchBubble failed 态**不自动消失**（手动关闭）；启动失败事件到达时自动 openLogDrawer。
- N-11：`useAmbientKey` 改为响应式信号（theme store 暴露 `themeMode()` signal，`Home.tsx:143` 改读 signal）。
- N-12：删除 `Home.tsx:100-105` specs() 死字段。
- N-15：恢复 Toast TONE_ICON 三色图标（success/error/info）。
- N-16：删除 4 个死 i18n key（en+zh 同步）。

### B3-4 氛围层回归（N-4 / N-5）

- N-4：`Home.tsx:309` 星星 `top` 改 `${Math.round((Math.abs(p.delay) * 89) % 92)}%`（对齐 PaperAmbient :357 的取模归一化）。
- N-5：`ThemeAmbient` 的 `<Switch>` 加 fallback `<Match when={true}>`（暗色底光）；`ambientThemeClass` 对未知/插件 key 返回的 `.t-dark` 确认 CSS 规则真实存在（当前是死兜底）。

### B3-5 UI 缺控件与确认缺失（P2-15 残余 / P2-16 / P2-22 / P2-20）

- InstanceDetail Settings 高级区补三行输入：前置命令/退出后命令/环境变量（draft 已有字段 :751-752，接上输入框即可）；内存卡补 min_memory_mb/metaspace_mb（进 OVERRIDABLE_FIELDS 的字段全部有控件，消除"覆盖了看不见"）。
- Plugins：移除对话框加"同时删除数据"复选框（`remove(id, deleteData)`）；详情"更多"菜单加"回滚到上一版本"（`plugin_rollback` api.ts:315）+ previousVersion 展示。
- P2-16：工具策略改 allow/deny 弹确认（显示工具名+风险级）；断开会话弹确认。
- P2-20：Mods 搜索同时过滤 disabled 列表；扫描对话框 conflicts/unparsed 改为可展开明细列表。
- P2-22：离线账号隐藏"刷新登录"（`Accounts.tsx:118` 按 provider 条件渲染）；设备码轮询加前端代际 token（新登录开始时旧轮询结果作废——`authStore` 记 pollGen，事件到达时校验），后端可取消接口列为后端待办。

### B3-6 VirtualList 可访问性（N-6 / N-14）

- 容器加 `tabindex="0"` `role="listbox"` `aria-rowcount={total}`；实现 ArrowUp/Down/Home/End 滚动 + 焦点跟随（roving tabindex：仅视口内首行可聚焦，方向键滚动后 focus 对应行按钮）。
- N-14：删除 `.vlist .rm-row { height:44px }` 死默认值，`itemHeight` 作为唯一事实来源（消费点显式传）。

### B3-7 发布项（P2-17）

- updater endpoints 落真实地址（release 流程配套）；落地前把"立即检查更新"按钮从 UI 摘除（比禁用+soon 诚实）。此项 release-blocked，进入发版检查清单。

---

## B4 · P3 收尾（预估 0.5 天，机械化清单）

| 项 | 修法 |
|---|---|
| P3-1 | ui.ts:40 注释 4s→5s |
| P3-2/3 | auth.ts:126 改真实错误分支；api.ts safeInvoke 归一化：`e instanceof Object && "message" in e ? e : { code:"unknown", message:String(e) }` |
| P3-5 | 死导出清理（并入 B3-1） |
| P3-6 | theme onThemeModeChange 删除或改多播 Set |
| P3-7 | mcp.rs:75 duration_ms 加 `#[ts(type="number")]` + 重新生成 types.ts（B2-4 守卫修好后此操作有测试兜底） |
| P3-8 | i18n 硬编码清单逐项 t() 化（Home mock 文案随 P0-2 处理；优先级枚举 InstanceDetail:911-915；Plugins:183 state；Settings:552/798；modded×2；aria-label×3；Accounts 序号） |
| P3-9 | LogDrawer pointermove/pointerup 摘除移入 onCleanup |
| P3-10 | 45s 兜底 timer 存句柄，dismissFlow 时 clearTimeout |
| P3-11 | settings.ts:59 applyMotion 移入 initStores 流程（模块导入期零 DOM 写入） |
| P3-12 | API_Key.txt 移出仓库目录；删 3 个死 i18n key |
| N-8/9/10/17/19 | Download 死 signal/死导入删除；loader 卡 onClick 只留 effect 一条触发路径（删 onClick 直调）；InstanceDetail openVersions 保留用户已选（对齐 Download applyLoaderVersions）；未引用 mc-icons 资产移 docs/design；Home"全部版本"卡接 versionCache |

---

## B5 · 测试与守卫（与 B0 并行启动，预估 1 天）

1. **引入 vitest**（devDependency）+ `package.json scripts.test`；不引入组件渲染测试框架（保持零重依赖原则）。
2. **第一批用例（纯函数层）**：
   - `lib/markdown.ts`：escape-first 安全性（注入样本）、GFM 表格、流式未闭合 fence/表格——自研安全件最需要测试兜底；
   - `lib/format.ts` / `lib/exitCodes.ts` / `lib/loaders.ts` / `lib/mcIcons.ts`；
   - B0 产出的路由参数工具（若有兼容重写则测重写函数）；B2-1 的 `useHashTab.read`（抽纯函数）；versionCache 的 TTL/LRU 判定（抽纯函数 `isFresh(fetchedAt, now)`）。
3. **守卫**：B2-4 的 bindings 只读比较测试；`pnpm test` 加入既有门禁链（与 typecheck/build 并列）。
4. CI/QA 脚本化：把 ui-design 12_SUMMARY 的六条核心旅程写成手动走查清单文件（`docs/qa/JOURNEYS.md`），每批次过一遍相关条目。

---

## 排期与工作量汇总

| 批次 | 内容 | 预估 | 依赖 |
|---|---|---|---|
| B0 | P0×2（hash 参数、假数据短期） | 0.5d | — |
| B1 | 新 P1×3（删除契约、向导报错、细节面板） | 1d | — |
| B2 | 旧 P1 集群（路由 hook、表单组、生命周期、守卫、列表、断点+D1） | 2.5d | B0（路由格式先定） |
| B3 | P2 清理（死代码回捞+删除、stores、吞错、氛围回归、缺控件、a11y、updater） | 2d | B1（删除对话框参数先行） |
| B4 | P3 机械化收尾 | 0.5d | B3 |
| B5 | vitest + 用例 + 门禁接入 | 1d | 与 B0 并行 |
| **合计** | | **≈ 7.5 人日** | |

## 需要产品/负责人拍板的决策点

- **D1**：窗口最小尺寸——收紧为设计文档的 1280×720（默认建议），或保留 800×500 但先修订 00_OVERVIEW 文档。
- **D2**：停止游戏确认的"不再询问"偏好——默认提供（建议），或一律强制确认。
- **D3**：假数据三卡——默认隐藏+可添加（建议），或仅加演示徽标保留默认展示。
- **D4**：HomeDashboard 回捞清单 5 项是否全部采纳（建议全采，成本已含 B3-1）。

## 完成定义（全部批次后）

1. `01_RECHECK_51.md` 与 `02_DELTA_AUDIT.md` 中除"后端配套"标注项（server ping、设备码取消接口、ExitType::Killed）外全部关闭；
2. 两个 P0 链路（实例→商店上下文、版本预选）手动走查通过；
3. 门禁链新增 `pnpm test` 全绿；bindings 只读守卫真实生效；
4. 死代码清零：HomeDashboard/死 CSS/Rail 残留/死导出/死 i18n key 全部删除，`app.css` 回到无死类状态。
