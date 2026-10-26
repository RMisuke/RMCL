# 02 · 增量代码审计（fd95fee → 3b863da）

> 范围：上次审计基线 `fd95fee` 以来的前端变更（`git diff fd95fee..HEAD -- src/`，约 +1973/-376 行、42 文件）
> 主体：`bf70295`（设置重组/主题氛围/删除流程/AI 面板修复）、`0dff688`（Steven 皮肤）、`3b863da`（SNGL 改名/关于页）；后端 7 个审计批次的少量前端连带
> 只收录**新引入**的问题（旧台账 51 项见 01_RECHECK_51.md，不重复；恶化项除外）
> i18n 经脚本比对：en/zh-CN key 集合完全一致（583=583），无失衡
> P1-A/P1-C/P2-A 等关键结论经主审人工复核证实。

---

## 新问题：P1（3 项）

### N-1（P1）安装向导报错路径被整体删除：冷启动离线 = 空列表零反馈，且"绑定本地版本"入口同批消失

- `src/pages/Download.tsx:82,115-121,287-318`；`src/stores/versionCache.ts:67-88`
- `InstallWizard` 的 `setError`（:82）声明后**从未被调用**（同文件 :525/:531 的调用属于 ModStore 自己的 error），Step1 的错误 UI 成为死代码；`versionCache.refreshGameVersions()` 内部 `.catch(() => …null)` 吞错，onMount 与手动刷新对失败无任何提示。
- 旧代码有 `else setError(res.error.message)`；本次重构把报错路径丢了。首次运行且离线（无缓存）时版本列表空白——无错误、无重试、无 loading。
- 同时"绑定本地版本"对话框（旧 Step1 的离线安装路径）被删除——离线用户完全无路可走。
- 建议：向导消费 versionCache 时保留 error 通道；离线兜底要么恢复绑定本地版本，要么在空态文案中给出指引。

### N-2（P1）TaskCenter 新增的"安装细节"面板会被任何进度事件立即折叠——新功能在核心场景不可用

- `src/components/TaskCenter.tsx:181-212`；`src/stores/modloaders.ts:64-91`
- modloaders store 每收到一次 `modloader://progress` 事件就用对象字面量 `next.set(key, {…, at: Date.now()})` 生成**新引用**；`<For each={loaders()}>`（:115）按引用复用行——引用变化即重建 LoaderRow，行内局部 `showDetails` signal（:182）随行销毁而丢失。
- 结果：安装进行中（正是要看细节的时候）每次进度上报都把已展开的细节日志折回去。
- 建议：store 更新时对未变化字段保持引用稳定（浅比较/复用旧对象），或把 showDetails 状态提升到 Map<key, boolean> 存 store/组件外。

### N-3（P1）DeleteInstanceDialog 承诺"独立游戏目录一并删除"，但后端写死 `cleanup_game_dir=false`——破坏性操作的契约文本与实际行为不符

- `src/components/DeleteInstanceDialog.tsx:47-71,108-109`；`src/ipc/api.ts:151`；`src-tauri/src/commands/profile.rs:226`；`crates/rmccc-profile/src/lib.rs:453`
- 对话框文案（`instanceDetail.deleteAll.body`）与头注释都声明"删除全部游戏资源，独立游戏目录一并删除"，且整个流程是三重防误触 + "此操作不可恢复"的强确认；但前端 `profiles.delete(id)` 单参，后端 `core.profile_store.delete(&id, false)` 硬编码不清理 game_dir——而库函数本就有 `cleanup_game_dir` 参数（带共享根/特征目录双守卫，能力完整）。
- 结果：隔离实例的 mods/saves 残留磁盘，用户却被告知已全删。**对破坏性操作而言，文案夸大于实际行为是必须立即对齐的缺陷**（两个方向任选：补前端选项透传参数，或改文案）。

## 新问题：P2（4 项）

| 编号 | 问题 | 位置 |
|---|---|---|
| N-4 | **靛夜主题"星辰"氛围层 14 颗星只有约 1 颗落在视口内**：`top: Math.round(abs(delay)*38)%`，delay∈(0,24) → top 值实测 25~868%，容器 overflow hidden 全裁掉；对照 PaperAmbient 的 `(abs*89)%92` 取模写法，此处漏了归一化。默认暗色主题的星空几乎完全不可见 | `Home.tsx:309`（对照 :357） |
| N-5 | **插件/自定义主题下 hero 氛围层整体消失（回归）**：`ThemeAmbient` 的 `<Switch>` 无 fallback，`useAmbientKey` 对插件主题 key 原样透传（:141-143），任何 Match 不命中 → 光斑/粒子全不渲染；`ambientThemeClass` 注释称未知 key 沿用 `t-dark` 兜底，但 CSS 中不存在 `.t-dark` 作用域的光斑规则——兜底是死的。旧版无条件渲染三团 `hero-flow-blob`，本次改版让插件主题用户失去全部背景动效 | `Home.tsx:123-164` |
| N-6 | **VirtualList 键盘不可达**：容器无 `tabindex`/`role`/`aria-rowcount`（`VirtualList.tsx:74-95`）；行是可聚焦 button，但 900+ 条版本列表只渲染视口+6 行 overscan，键盘 Tab 只能到达前 ~10 行，其余行永远无法聚焦——对"键盘用户"这是从"全量 DOM 可达"退化为"不可达" | `VirtualList.tsx:74-95`、消费点 `Download.tsx:290-316,379-397` |
| N-7 | **versionCache 的 loader 条目无上限、永不清除**：按 `loader|gameVersion` 组合无界累积，无 TTL、无淘汰；配额超限时 `writeJson` 静默吞掉（缓存悄悄退化为仅内存）；`fetchedAt`（:109）写而不读（游戏版本侧有 FRESH_MS 判定，loader 侧没有）——死字段 | `src/stores/versionCache.ts:99-114` |

## 新问题：P3（12 项）

| 编号 | 问题 | 位置 |
|---|---|---|
| N-8 | Download 死代码：`loaderDialog`/`setLoaderDialog` signal、未用导入 RmSwitch/confirmDanger/formatBytes | `Download.tsx:98,10,29,32` |
| N-9 | 点 loader 卡片触发两次 IPC：onClick 直接 `pickLoader` 后 `setLoaderType` 又触发 createEffect 二次调用（loaderGen 丢弃第一份结果但两次网络请求都发出） | `Download.tsx:339,363,139-143` |
| N-10 | InstanceDetail 版本对话框刷新完成后无条件把 selectedVersion 重置为首个稳定版，覆盖用户在缓存列表上已手点的选择（与 Download `applyLoaderVersions` :159-163 保留现选的逻辑不一致） | `InstanceDetail.tsx:553-557` |
| N-11 | `useAmbientKey` 读 `dataset.themeMode` 非响应式：存量 pref="system" 用户在 OS 明暗切换后氛围层不更新（该入口本次已从设置移除，仅存量用户受影响） | `Home.tsx:143`、`theme/index.ts:109`、`Settings.tsx:333` |
| N-12 | `specs()` 死字段（size/duration/opacity 从未消费，opacity 0 经 `\|\| undefined` 归无） | `Home.tsx:100-105` |
| N-13 | VirtualList onScroll 在 rAF 挂起期间丢弃后续滚动位置（应在回调内读 scrollTop）；快速甩动时窗口落后约一帧，靠 overscan 掩盖 | `VirtualList.tsx:44-52` |
| N-14 | `.vlist .rm-row { height:44px }` 默认值与组件 itemHeight 契约脱节：在用消费点全被 `.version-list {48px}` 覆盖，44 默认无人用，未来漏加 class 即错位 | `app.css`（`.vlist` 段） |
| N-15 | Toast 删除了 TONE_ICON，success/error/info 共用同一铃铛图标，仅靠文字颜色区分——错误辨识度下降 | `Toast.tsx:22` |
| N-16 | 4 个 i18n 死键（en/zh 各一份）：`download.tab.cache`、`download.wizard.localComplete`、`download.wizard.loaderVersion`（本次删除的 UI 残留）、`settings.general.mode.system`（移除跟随系统卡片的残留） | `locales/*` |
| N-17 | 未引用资产：mc-icons 的 book-128/crafting_table-128/oak_hanging_sign-128 无任何 import（不影响 bundle，疑似设计源误放 src/assets） | `src/assets/mc-icons/` |
| N-18 | DeleteInstanceDialog 部分失败中间态：`instances.remove` 成功而 `profiles.delete` 失败时版本目录已删、实例仍在列表；重试走同一流程，若后端对不存在版本报错则该实例删除永久卡死 | `DeleteInstanceDialog.tsx:53-67` |
| N-19 | Home"全部版本"卡仍直连 `versions.list()`，未接本次新建的 versionCache（离线/加载中显示空列表），同调用在死页 HomeDashboard 还有一份 | `Home.tsx:895` |

## 旧问题恶化（1 项）

- **P1-11 恶化**：无确认的停止/强杀入口从 5 处增至 **7 处**（新增 `Home.tsx:647-655` hero"强制关闭"、`:979` GamesCard 停止）。详见 01_RECHECK。

---

## 本次变更的正向修复/改进清单（已核实，14 项）

1. **虚拟滚动**：零依赖 `VirtualList`（rAF 节流、ResizeObserver、"items 变化回顶"、卸载双清理）替换版本列表 `slice(0,100)` 与 loader 列表全量渲染（缓解旧 P1-14）。
2. **版本缓存**：`versionCache`（localStorage+内存镜像）读路径秒开、后台静默替换、启动预刷新（`init.ts:76-77`）、游戏 manifest 10min TTL、并发刷新共享。
3. **竞态修复**：Download `loaderGen` 代际保护（快速连点 loader 不再旧覆新）；InstanceDetail `openVersions` 同思路守卫。
4. **删除流程统一**：`DeleteInstanceDialog` 三重防误触（左置删除键、5s 冷静期、二次点击武装态），共享版本自动保留（对齐 ADR-002），三处删除入口全部收口（但见 N-3 契约失真）。
5. **AI 面板修复**：LogDrawer 分析窗 `Show` 回调改 accessor——修复流式输出"点开窗后文字冻结"，补 running/error 状态标题。
6. **Loader 安装可观测**：modloaders store 新增 `details`（最近 8 行安装器 stdout）与 `recent`（上限 5），TaskCenter 渲染细节折叠（但见 N-2 折叠 bug）。
7. **markdown GFM 表格**：表头+分隔行成表、流式未闭合表格处理、flush 路径补齐 `flushTable`、配套表格 CSS；escape-first 安全模型未破坏。
8. **MC 图标体系**：`mcIcons.ts` 版本类型→草方块/命令方块/圆石、loader→官方彩图（OptiFine 兜底、未知返回 null 由调用方回退）。
9. **Steven 皮肤回退链**：离线账号默认上传版 Steven 皮肤 + 头像 face 裁剪，AccountAvatar/SkinViewer 回退完整。
10. **Settings 重组**：Appearance 独立分区、Debug 模式卡（localStorage 持久化）、镜像"自动选择"选项、缓存管理迁入设置页；补上后端 `default_isolation` 的 UI（缓解旧 P2-15 一项）。
11. **About 双版本**：AppInfoView 契约同步，展示"启动器（SNGL）/内核（RMCCC）"双版本；`sizeValue` 从 "<10 MiB" 修正为 "<20 MiB（ADR-011）"。
12. **契约清理**：events.ts 删除的死事件常量前后端零残留（收掉旧 P3-4）；types.ts/api.ts 变更均被消费。
13. **SNGL 改名一致性**：TitleBar 品牌位、欢迎语、MCP 文案、About 等全部更新；RMCCC 仅保留于内核语义处，无不当用户可见残留。
14. **动效可访问性**：ambient 粒子/光斑全为 transform/opacity 动画，`data-motion="off"` 整层隐藏 + `prefers-reduced-motion` 双保险（app.css:63-65,1729-1730,1859-1860,2069-2074）。
