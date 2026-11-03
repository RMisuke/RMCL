# 05 · 审计意见与优化路线

> 基于 01-04 文档的整改建议。原则：先止血（用户可感知缺陷）→ 再清理（实验残骸）→ 后补齐（HMCL 对齐）→ 长效（守卫与测试）。所有编号引用 `04_ISSUES.md`。

---

## 1. 修复路线（建议顺序）

### S0 · 立即（半天内，2 项 P0）

| 项 | 动作 |
|---|---|
| P0-1 | 统一 hash 参数方案。推荐：跳转统一改 `/download?instance=x#mods`（query 挂 path，`useSearchParams` 直接可用，删除 ModDetail 的手写解析）；涉及 5 个跳转源 + 2 个消费点 |
| P0-2 | 首页假数据三卡加"演示"徽标或从默认布局移除（默认只留账号卡+启动卡+实例卡）；在 issue tracker 立真实 server ping 后端需求 |

### S1 · 本迭代（1-2 天，P1 核心 10 项）

1. **路由/tab 状态派生化**（P1-2/P1-3 一并修）：三个 hash-tab 页（Download/InstanceDetail/Settings）的 tab signal 改为从 hash 派生 + hashchange 监听，Overview 快捷按钮与 Step5 跳转即恢复。
2. **输入类三连**：P1-5 prompt 回填 initial（一行修复，影响全部重命名流）、P1-4 分辨率 height 独立锁、P1-1 Settings createEffect 不覆盖 dirty 输入。
3. **游戏生命周期**：P1-7 消费 exit_type 区分用户中止与崩溃；P1-11 抽 `confirmStopGame()` + 关窗运行提示（这是 HMCL 用户认知中的底线行为）。
4. P1-6 TitleBar onCleanup、P1-13 草稿丢失拦截、P1-9 漂移守卫改只读比较。

### S2 · 清理专项（半天，先做决策再删）

1. **决策 Home 形态**：在用 hero 版 vs 死代码 dashboard 版。建议：保留 hero 形态但**回捞 5 项实现**（插件卡 ErrorBoundary、下载行取消、无实例引导空态、正版换肤入口、粘贴版本号输入框），随后删除 `HomeDashboard.tsx` 并把决策补进 `docs/ui-design/02` 或新 ADR（消除合规漂移）。
2. 删死 CSS ~135 行（P2-2，保留 `.rail-task-indicator` 家族）、死状态（P2-3）、死导出（P3-5）、死 i18n key。
3. 决策 dashboard 布局后端协议去留（P2-4）：若 hero 不回头，从 api.ts 与后端命令中下线该协议面。

### S3 · 一致性与资源（与 S2 并行，2-3 天）

1. **保存模式统一**（P1-12）：建议全站统一"显式保存 + dirty 指示"，Settings 分区各加保存条；这与实例设置的继承锁心智一致，也顺带消除 P2-14 竞态。
2. 资源生命周期批处理：P1-8 日志缓冲策略、P2-5 inFlight、P2-7 interval、P2-8 blob revoke、P2-9 头像缓存失效（换肤事件驱动 invalidate）。
3. 吞错点收尾（P2-13）：统一"失败必有 UI 反馈"——错误态组件或 toast 二选一；SkinViewer init 失败给出重试。
4. 列表体验（P1-14/P2-20/P2-21）：版本列表与 Mod 版本列表接分页/虚拟化；商店 loader 筛选补齐 neo_forge/optifine；版本下拉去重。
5. 布局断点（P1-15）：决策最小窗宽（建议按设计文档 1280×720，或修订文档），补 accounts-split 折叠、hero 单栏、sidetabs 自动收窄三处断点。

### S4 · 功能补齐路线（对齐 03 文档，按用户价值排序）

| 批次 | 内容 | 依据 |
|---|---|---|
| 第一批（主流程断点） | 服务器真实 ping/MOTD、打开目录（Finder/资源管理器）、停止确认已含 S1、版本列表完整浏览已含 S3、CurseForge 源解禁 | 03 A 级 |
| 第二批（安装方式） | 整合包：mrpack/CF 导入 → 双源搜索 → 更新向导；导出整合包（Modrinth 格式优先） | 03 A/B 级 |
| 第三批（内容管理） | 世界管理（列表/备份/导出 zip 起步）、资源包/光影 tab | 03 A 级 |
| 第四批（账户生态，国内用户优先） | authlib-injector 服务器管理+登录、离线自定义 UUID、正版皮肤直传 | 03 B 级 |
| 第五批（体验增强） | 启动建议体系（Java 版本检查起步）、崩溃确定性规则兜底（AI 之外）、日志导出文件、Quick Play、启动器可见性、代理设置、Java 下载 | 03 B 级 |
| 显式不做 | Terracotta、NBT 编辑器、主题包、便携模式、结构图、ChunkBase——建议各记一条"不做"决策，防止无限追赶 | 03 C 级 |

### S5 · 长效工程

1. **恢复守卫**：P1-9 漂移测试改只读比较后，把它当作 types.ts 的唯一事实来源。
2. **前端测试起点**（P2-18）：优先给纯函数与高杠杆逻辑补测——`lib/format`、`lib/exitCodes`、`lib/markdown`（自研安全件最需要测试）、hash 参数解析工具（S0 产出）、继承锁 draft→opts 映射（P1-4 的回归防线）。
3. **updater**（P2-17）：endpoint 落真实地址前，把"立即检查"从 UI 摘掉比留禁用按钮更诚实。
4. i18n 硬编码收尾按 P3-8 清单逐项处理（约 1 小时工作量）。

---

## 2. 布局形态意见（对应 02 §9）

1. Dock 导航形态成立，保留；Rail 残骸清理后不可回潮。
2. 最小窗宽必须先决策再适配——当前 800×500 与三处固定宽度（hero-corner 340 / accounts-split 360 / sidetabs 200）的组合在最小窗宽必然破损。推荐路径：收紧到 1280×720（对齐设计文档），固定栏改"可折叠/窄窗自动隐藏"。
3. 溢出保护应组件库化：RmField/RmListRow 内建 ellipsis + title；`<img>` 在 AccountAvatar 三级回退模式上推广（实例图标、mod 图标、插件图标统一 onError 兜底）。

## 3. 选项分布意见（对应 02 §10）

1. 保存模式统一为显式保存（S3-1）是选项分布层面最重要的一项——它同时消除 P1-1/P1-12/P2-14 三个问题。
2. MCP 卡拆出子页（设置分区变 8 个），AI 卡留原分区；权限矩阵单独一屏。
3. `instances.rename/duplicate` 移入实例上下文菜单（Instances 行菜单 + InstanceDetail ManageMenu），"设置-高级-版本仓库"仅保留仓库级操作（重扫描/删除）。
4. 无账号时 Accounts 只保留欢迎卡一份登录入口。
5. 后端字段补控件顺序（性价比排序）：pre/post 命令与 env_vars（高级折叠区加三行）> min_memory/metaspace（内存卡加两字段）> plugin deleteData/rollback（移除对话框加勾选）。

## 4. 保持项（不要动）

- IPC 三件套（safeInvoke / init.ts 集中监听 / ts-rs 桥）与揭窗握手——项目范式，勿在页面内直接 listen/invoke。
- markdown.ts 自研路线与 skinview3d 动态分包——体积红线下的正确取舍。
- i18n 编译期同 key 集机制——已保证双语零漂移。
- 非模态任务中心、MCP 破坏性确认卡、插件权限审批流——差异化能力，继续投入。
