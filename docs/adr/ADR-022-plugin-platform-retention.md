# ADR-022: 插件平台保留（否证 2026-10-28 的收缩提案）

- **状态**：accepted（2026-11-03）
- **日期**：2026-11-03
- **阶段**：Phase 2（插件协议）/ Phase 4（隔离 UI SDK）/ Phase 5（逻辑运行时）
- **关联**：取代 [`../archive/PLUGIN_SYSTEM_REMOVAL_ANALYSIS_2026-10-28.md`](../archive/PLUGIN_SYSTEM_REMOVAL_ANALYSIS_2026-10-28.md)（方案 B）与
  [`../archive/TRIM_TO_THEMES_AND_CARDS_ANALYSIS_2026-10-28.md`](../archive/TRIM_TO_THEMES_AND_CARDS_ANALYSIS_2026-10-28.md)（收缩建议）；
  ADR-003/004/005/006/007/014/021；指南 §6/§9/§14；`../ui-design/14_CARD_SYSTEM.md`

## 背景

2026-10-27 的插件安全审计之后，10-28 连续产出两份分析，主张把插件系统收缩为「只剩主题 + 主页卡片」，
删掉逻辑运行时（wasmi）、隔离 UI 容器与第三方 SDK。此后**没有任何 ADR 或活文档记录这个决定是否执行**，
而代码与后续设计却按相反方向演进：

| 事实 | 证据（2026-11-03 本机实测） |
|---|---|
| 逻辑运行时与插件 crate 仍在 workspace | `Cargo.toml:16,18,23`（`crates/rmccc-plugin`、`crates/rmccc-logic`、`src-logic-host`） |
| 插件 IPC 全量注册并被契约测试锁死 | `src-tauri/src/handlers.rs:110-124`（15 条 `plugin_*`）；总数 132 由 `src-tauri/src/ipc_tests.rs:168` 断言 |
| 隔离 UI scheme 仍在应用入口生效 | `src-tauri/src/main.rs` 的 `UI_SCHEME` 注册 |
| 第三方 SDK 与样例插件在增长 | `plugin-sdk/packages/{plugin-api,plugin-ui}`；`examples/plugins/` 实有 **4** 个（`server-dashboard`、`theme-midnight`、`theme-aurora`、`theme-grass`） |
| 插件卡片契约在收缩提案**之后**被设计并实施 | [`../ui-design/14_CARD_SYSTEM.md`](../ui-design/14_CARD_SYSTEM.md)（2026-10-31），manifest `maxSize` + Host Bridge `tasks.list` / `launch.status` / `server.status` |

即：收缩提案在事实上已被 10-31 的卡片系统 v3 否决，但 `docs/README.md` 仍把两份分析与活的架构文档并列为
「活文档」，`RMCCC_PLUGIN_PLATFORM_ARCHITECTURE_GUIDE.md` 与 `roadmap/04–07` 继续按全量插件平台推进。
这个口径空洞会让后续 Agent 误以为「随时可以删插件系统」。

## 目标

- 明确插件平台的**保留范围**：主题、主页卡片、隔离 UI、逻辑运行时、SDK 与安装事务全部继续。
- 让两份 10-28 分析降级为「问题溯源 + 代价测算」，不再作为可执行的收缩依据。

**不解决**：ADR-008（目录与 Ed25519 包签名）是否实施——仍 `proposed`，是发布前置项；
Windows 侧隔离 UI 与 helper 原生化验收——仍 `deferred / release-blocked`；
`examples/plugins/` 的数量口径修正（属文档同步，随本 ADR 一并落）。

## 选项

**A. 全量保留（即现状）。**
- 优点：与 `ARCHITECTURE.md`、插件平台指南、`roadmap/04–07` 的 ✅ 结论一致；不销毁已实施的卡片契约与 SDK；
  逻辑插件继续走独立 helper 进程（ADR-007），主可执行不被 wasmi 污染。
- 缺点：保留 helper 的打包链路（`bundle.externalBin` + `pnpm prep:host`）与攻击面；Windows 需要
  `rmccc-logic-host.exe` 原生验收；插件 IPC 占 132 条中的 15 条，维护面大。

**B. 收缩到「主题 + iframe 卡片」（10-28 方案 B）。**
- 优点：删掉 wasmi/ABI/supervisor 与 SDK 协议面。
- 缺点：`TRIM_TO_THEMES_AND_CARDS_ANALYSIS_2026-10-28.md` 自己的实测就是反证——删插件能省的
  `__TEXT,__text` 只有 **0.6%（保留 iframe 卡片）到 3%（改声明式卡片）**，启动与内存收益在测量噪声内（该文 §6/结论）；
  同时需要删除 `crates/rmccc-plugin`（静态计数 121 个 `#[test]`）与 `plugin-sdk/`，废掉 10-31 刚落地的卡片契约，
  回滚成本远高于收益。

**C. 分级保留：主题 + 卡片 + 隔离 UI，逻辑运行时暂缓。**
- 优点：砍掉最重的 wasmi 依赖面。
- 缺点：`src-logic-host` 是唯一链接 wasmi 的二进制（ADR-007），不进主可执行，因此 C 省不下体积红线压力，
  只是把已完成的 Phase 5 成果闲置。

## 决策

采用 **A**。理由按权重：

1. **收益不成立**：两份分析的核心卖点是性能/体积，而实测收缩只能省下 0.6%–3% 的
   `__TEXT,__text`，启动与内存收益在测量噪声内；ADR-011 的 20 MiB 红线当前余量充足
   （strip 后主可执行 **10,734,704 B ≈ 10.24 MiB**，2026-11-03 实测）。
2. **既成事实**：10-31 卡片系统 v3 已经把插件卡尺寸档位、实例 id（`plugin:<pluginId>/<cardId>`）与
   Host Bridge 三个新方法做实并写进 schema/SDK；收缩等于自我推翻。
3. **一致性**：保留 ADR-003/004/005/006/007/014 全部 accepted 状态，避免协议级 ADR 出现「accepted 但代码不存在」
   或「代码存在但无 ADR 背书」的双向漂移。

**约束（新增）**：

- 任何再次收缩插件平台的提案必须以**新 ADR 取代本 ADR** 为前提，不得直接以两份 10-28 分析为依据改代码或删 crate。
- 发布前必须补 ADR-008：当前 `crates/` 与 `src-tauri/src/` 中无 Ed25519 验签实现，静态签名索引仍属空白。
- 15 条 `plugin_*` IPC 继续受 `ipc_tests.rs` 的总数断言保护，增删需同步
  `ARCHITECTURE.md` §11、`ui-design/11_IPC_COVERAGE.md` 与 `src/ipc/api.ts`。

## 后果

- 正面：文档口径统一——`docs/audits/` 只留在跟踪的台账，收缩分析进 `docs/archive/` 并加否决横幅；
  后续 Agent 读到的插件平台是「在做」而非「待删」。
- 负面：继续承担逻辑运行时的安全与验收债；Windows helper、隔离 UI 实机验证仍是 release-blocked 清单成员。
- 对 CI/打包：`src-logic-host` 与 `bundle.externalBin` 链路继续保留，不得因「瘦身」诉求移除。

## 迁移影响

无代码变更。仅文档：两份分析 `git mv` 至 `docs/archive/` 并加横幅；`docs/README.md`、`AGENTS.md`、
`docs/adr/README.md` 的索引与数字同步；指向旧 `docs/audits/<归档名>` 的引用（含
`crates/rmccc-modloader/src/lib.rs:347` 的注释路径）已改指 `docs/archive/`。回滚 = 反向 `git mv` 并撤销索引改动。

## 跨平台验证方式

- macOS（本机 aarch64）：本 ADR 只做口径收口，不动代码，无需新实测；引用的体积数字来自
  `target/release/bundle/macos/SNGL.app/Contents/MacOS/rmccc` 的 `stat`。
- Windows：`rmccc-logic-host.exe` 解析、iframe/隔离 UI、`CREATE_NO_WINDOW` 等仍按
  `../audits/AUDIT_2026-11-01_WINDOWS.md` §5 记 deferred，不得因本 ADR 宣称验收。
- Linux：未在本机实测，保持 deferred。

## 状态变更记录

- 2026-11-03：accepted（追溯确认 2026-10-31 卡片系统 v3 的实施决定，否证 10-28 收缩提案）。
