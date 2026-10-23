# RMCCC 总体执行路线（细化版）

> **基准日期**：2026-10-20，Phase 5 完成后回填至 2026-10-22。**当前状态来源**：[`AUDIT_2026-10-20.md`](../AUDIT_2026-10-20.md) + 本机实测（`cargo metadata` / `cargo test --workspace` / `IPC_COMMANDS` / strip 后产物）。
> **目标架构来源**：[`RMCCC_PLUGIN_PLATFORM_ARCHITECTURE_GUIDE.md`](../RMCCC_PLUGIN_PLATFORM_ARCHITECTURE_GUIDE.md) Phase 0–7。
> **执行约束**：本机 macOS Apple Silicon 单环境开发；不在本机跨平台编译或 Windows/Linux 实测（指南 §1.4）。

---

## 1. 当前起点一句话

引擎层（安装→索引→启动）已贯通并实机验证；**617 项 Rust 测试**（含插件 106 项 + Dashboard 17 项 +
逻辑运行时 8 项单测与 9 项 helper spawn 集成测试 + Agent 63 项 + 6 项嵌入资源校验）
+ `pnpm sdk:test` **14 项**（含 5 项跨工件契约：schema ↔ Rust ↔ SDK），CI 三门禁本地全绿。
**Phase 0–5 全部完成**：结构债清偿（0/0.5/1）、声明式插件系统（2）、Dashboard 布局与主题（3）、
隔离 UI + SDK（4）、逻辑运行时 helper 进程 + `logic.invoke` 只读扩展（5）。
**Phase 6 进行中**：ADR-009/010 落定，6.1 LLM adapter、6.2 凭据 vault、6.3 会话事件流/任务快照、
6.6 只读工具、6.5 第一批（执行循环 + ApprovalBroker + IPC `agent_*` 9 条 + `/agent` 页面）完成（M7 ✅，2026-10-24）；
6.4 注册表持久化随 Phase 7，6.7 隐私上下文范围 UI 随下一批。
**Phase 7（插件目录/签名/生产分发）未开始**，需先落 ADR-008。

## 2. 阶段总览与依赖

```text
Phase 0  基线重建（环境/工具链/门禁复跑/体积基线/差距清单）
   │  必须先完成：后续一切重构的可信起点
   ▼
Phase 0.5  既有产品缺口补齐（Mod 浏览器 / 多账号 UI / 日志页 / 暂停恢复 / 本地 mod 安装）
   │  与 Phase 1 可并行；优先做，投入产出比最高
   ▼
Phase 1  Core 收敛 + 平台适配层（pipeline 统一 / loader 单体系 / context.rs 拆分 / zip-slip /
   │      IPC 契约测试 / api.ts wrapper / profile-instance 语义 / 适配层 trait 落地）
   │  插件平台依赖的稳定底座
   ▼
Phase 2  插件协议与声明式插件管理器（Manifest/权限/生命周期/.rmcpkg 安装事务）
   │  只做数据型 + 声明式，不执行第三方代码
   ▼
Phase 3  Dashboard 卡片与主题引擎（Widget registry / 布局 / 主题令牌）
   │  依赖 Phase 2 的注册表
   ▼
Phase 4  隔离 UI 扩展 + SDK（iframe/postMessage 桥 / TS SDK / 打包验证工具）
   │  依赖 Phase 2 协议 + Phase 3 卡片宿主
   ▼
Phase 5  逻辑插件运行时（WASM vs 进程 PoC → ADR-007）
   │  依赖 Phase 1 适配层 + Phase 2 权限模型
   ▼
Phase 6  Agent 平台（LLM adapter / Credential / Tool registry / 审批审计）
   │  依赖 Phase 1 Core Service + Phase 2/5 权限系统
   ▼
Phase 7  插件目录 / 签名 / 生产分发（registry / 签名 / updater / 三平台验收）
        依赖 Phase 2–6 全部稳定 + 原生 CI runner 触达
```

> **关键原则**（指南 §2.3）：每阶段都形成可运行增量；禁止跨多层无验收的大重构；任何阶段都保持 Minecraft 安装/启动主链路可用。

## 3. 与 DEVELOPMENT_PLAN.md Phase 1–5 的关系

`DEVELOPMENT_PLAN.md` 的 Phase 1–4 已完成（后端）或大部分完成（前端缺 Mod 浏览器）；Phase 5 配置齐备但端到端未验证。本路线**不重复**这些已完成的 Phase 1–4，而是：

- **Phase 0 / 0.5 / 1** 对应 DEVELOPMENT_PLAN Phase 5 中"未完成"项 + 审计 Sprint A/B 的细化。
- **Phase 2–7** 是 DEVELOPMENT_PLAN 之外的**插件平台 + Agent + 发布**方向（指南 Phase 2–7）。
- DEVELOPMENT_PLAN Phase 5 的"E2E / 体积 / 公证"等并入本路线 Phase 0（基线）与 Phase 7（分发）。

## 4. 当前状态锚点（审计快照）

| 维度 | 当前事实 | 来源 |
|------|---------|------|
| Workspace | **18 成员**（15 crates + `src-tauri` + `src-cli` + `src-logic-host`），新增 `rmccc-agent` | `Cargo.toml` / `cargo metadata --no-deps` |
| 测试 | **617 passed / 0 failed**（2026-10-22 本机全量复跑）；插件 106 项 + Dashboard 17 项 + 逻辑运行时 8 单测 + 9 项 helper spawn 集成 + Agent 63 项 + bundle 6 项；另 `pnpm sdk:test` **14 项** | `cargo test --workspace` + `pnpm sdk:test` |
| IPC | **77 定义 = 77 注册**；`IPC_COMMANDS` 与 `generate_handler!` 由 `ipc_tests` 交叉校验；前端 `api.ts` 全覆盖（除显式白名单） | `handlers.rs` / `ipc_tests` |
| 结构债 | ✅ pipeline 收口（P1-3）；✅ 平台适配层 + 领域零 `cfg`（1.1）；✅ loader 单体系（P1-4）；✅ context.rs 93 行（P2-1）；✅ 修 P2-12 | 审计 §5.3、§15.1/§15.2 |
| 插件系统 | ✅ Phase 2：Manifest v1 / `.rmcpkg` 事务 / 状态机 / 权限 / 配置 / CLI / UI | ADR-003、ADR-004 |
| Dashboard | ✅ Phase 3.1–3.5：布局 Schema/归一化/网格/拖放/主题令牌 | ADR-005、`rmccc-dashboard` |
| 隔离 UI | ✅ Phase 4.1–4.5：`rmcccplugin://` 独立源 iframe + 版本化 Host Bridge + TS SDK + 外部样例 | ADR-006、`plugin-sdk/` |
| 逻辑运行时 | ✅ Phase 5.1–5.5：PoC 实测 → 独立 helper 进程 + wasmi、fuel/内存/超时 kill、`logic.invoke` 只读扩展；单版本 5.6 打包分发并入 Phase 7 | ADR-007、`rmccc-logic` / `src-logic-host` |
| 安全缺口 | ✅ natives zip-slip（P1.5）；✅ 插件包 zip-slip/zip bomb/路径/类型限制（2.2） | 审计 §15.4 |
| 平台执行散落 | ✅ 领域 crate 无平台 `#[cfg]`；插件原子替换经 `PlatformFs` | PLATFORM_ADAPTATION.md |
| 环境回归 | Homebrew rustup 路径失效、`~/.cargo` 缺失、**`strip = true` 仍失效（缺 libLLVM）** | 审计 §15.5、P2-7 |
| Agent | 🚧 Phase 6.1/6.2/6.3/6.6 完成（`crates/rmccc-agent`，60 测试）；6.5（loop/审批）与 6.7（隐私）未做，6.4 仅缺注册表持久化；ADR-009/010 accepted | `docs/roadmap/08_phase6_agent.md` |
| 发布 | release.yml 齐备但缺证书/Secret，updater `endpoints` 仍是 `<owner>/<repo>` 占位；无 `cargo bloat` 审计；E2E 未实跑；插件目录（ADR-008）未定 | 审计 §13.7、`09_phase7_distribution.md` |

## 5. 并行与串行

- **可并行**：Phase 0.5（前端缺口，纯既有后端接线）与 Phase 1 前半（pipeline 收敛）。建议先做 Phase 0.5 见效快，Phase 1 重构同步推进。
- **进度**：Phase 0 ✅、Phase 0.5 ✅、Phase 1.1–1.8 ✅、Phase 2.1–2.6 ✅（+2.7 IPC/UI）、
  Phase 3.1–3.5 ✅、Phase 4.1–4.5 ✅、**Phase 5.1–5.5 ✅**（ADR-007 选型为独立 helper 进程 +
  内嵌 wasmi；fuel/内存/超时 kill；`logic.invoke` 只读扩展；5.6 打包分发并入 Phase 7）。
  **Phase 0–5 全部完成**；**Phase 6 进行中**：6.1 LLM adapter、6.2 凭据 store（ADR-009）、
  6.3 会话事件流/任务快照、6.6 只读工具与 dry-run 安装计划完成；6.5（loop/审批）与 Phase 7 待做。
- **强串行**：Phase 2 依赖 Phase 1 的 Core Service 窄门面；Phase 3 依赖 Phase 2 注册表；Phase 4 依赖 Phase 2+3；Phase 5 依赖 Phase 1 适配层；Phase 6 依赖 Phase 1 Core + Phase 2/5 权限；Phase 7 依赖全链路 + 原生 CI。
- **平台验证串行**：所有 Windows/Linux 原生验证推迟到原生 CI runner 触达后统一在 Phase 7 验收；在此之前各阶段只做 macOS 本机验收 + 适配层 trait 占位。

## 6. ADR 触发时机（指南 §20）

| ADR | 决策 | 本路线触发阶段 |
|-----|------|---------------|
| ADR-001 | OS 最低版本/架构 | Phase 0 |
| ADR-002 | Core Service API + Profile/Instance 语义 | Phase 1 |
| ADR-003 | 插件包归档/目录/签名/信任 | Phase 2 |
| ADR-004 | Manifest Schema / ID / 版本兼容 | Phase 2 |
| ADR-005 | Dashboard Layout Schema / 冲突算法 | Phase 3 |
| ADR-006 | 隔离 UI 容器 + 消息桥 | **已 accepted（Phase 4）** |
| ADR-007 | 逻辑运行时 WASM vs 进程（需 PoC） | **✅ accepted（Phase 5，独立 helper 进程 + wasmi）** |
| ADR-008 | 插件目录 / 签名发布 | Phase 7 前 |
| ADR-009 | 凭据存储：文件 vault（AES-256-GCM）为首选，Keychain 可替换 | **✅ accepted（Phase 6）** |
| ADR-010 | Agent 会话存储 / 工具审计 / 隐私 | **✅ accepted（Phase 6）** |
| ADR-011 | <10MB 度量对象与门禁 | Phase 0 / 运行时 PoC 前 |
| ADR-012 | macOS Intel runner/打包/最低系统 | Phase 0 |

## 7. 全程不变的硬约束

- 体积红线 < 10MB（AGENTS.md）；新增依赖须评估体积（ADR-011 定度量对象）。
- 设计文档优先：冲突先提 ADR，不静默改协议。
- 生产路径禁 `unwrap/expect/panic`（CI 已门禁）。
- 平台差异进适配层，领域核心只依赖 trait（指南 §5.7）。
- 插件最小权限；不把插件脚本载入可信主窗口；第三方代码不持 `CoreContext`/Token。
- macOS 本机不宣称 Windows 可用（指南 §1.4）。

## 8. 里程碑（建议节奏，非承诺日期）

| 里程碑 | 完成标志 | 依赖阶段 |
|--------|---------|---------|
| M0 可信基线 ✅ | 5 门禁本机全绿复跑 + 体积基线表 + 差距清单 | Phase 0 |
| M1 主线闭环补齐 ✅ | UI 可直达 ≥ 40/46 命令；Mod 浏览器可用 | Phase 0.5 |
| M2 单一编排 ✅ | Tauri/CLI 同操作走同一 Core；context.rs < 600 行；适配层 trait 落地 | Phase 1 |
| M3 声明式插件 ✅ | 损坏插件不影响主应用；声明式插件可装/禁/卸 | Phase 2 |
| M4 可定制 Dashboard ✅ | 卡片注册 + 布局持久化 + 主题令牌 | Phase 3 |
| M5 第三方隔离 UI ✅ | 外部样例可独立构建/打包/安装；无法越权（独立源 + CSP + 逐条授权） | Phase 4 |
| M6 逻辑运行时 ✅ | 运行时 PoC + ADR-007；插件崩溃不崩主程序 | Phase 5 |
| M7 Agent 只读 ✅ | 只读工具 + 计划预览 + 审批可审计（6.5 第一批：执行循环 + 审批链 + IPC/UI，2026-10-24） | Phase 6（前半） |
| M8 生产分发 | 三平台原生产物 + 签名/公证 + updater 真实 endpoint | Phase 7 |
