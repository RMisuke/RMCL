# RMCCC 总体执行路线（细化版）

> **基准日期**：2026-10-20。**当前状态来源**：[`AUDIT_2026-10-20.md`](../AUDIT_2026-10-20.md)。
> **目标架构来源**：[`RMCCC_PLUGIN_PLATFORM_ARCHITECTURE_GUIDE.md`](../RMCCC_PLUGIN_PLATFORM_ARCHITECTURE_GUIDE.md) Phase 0–7。
> **执行约束**：本机 macOS Apple Silicon 单环境开发；不在本机跨平台编译或 Windows/Linux 实测（指南 §1.4）。

---

## 1. 当前起点一句话

引擎层（安装→索引→启动）已贯通并实机验证；**485 项测试**（含 74 项插件测试 + 17 项 Dashboard 布局测试）+
CI 三门禁本地全绿。Phase 0/0.5/1 的结构债已清偿，Phase 2 的声明式插件系统
（Manifest/权限/生命周期/`.rmcpkg` 安装事务/CLI/UI）已落地。
**还没做的是 Dashboard 布局引擎、隔离 UI、逻辑运行时、Agent 与生产分发**——
它们按 Phase 3–7 引入，不与现有启动主链路混在一起。

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
| Workspace | 14 成员（12 crates + `src-tauri` + `src-cli`），新增 `rmccc-plugin` | Cargo.toml |
| 测试 | 485 passed（2026-10-21 全量复跑）；插件 74 项 + Dashboard 17 项 | `cargo test --workspace` |
| IPC | 64 定义 = 64 注册；前端 `api.ts` 全覆盖（除显式白名单） | `handlers.rs` / `ipc_tests` |
| 结构债 | ✅ pipeline 收口（P1-3）；✅ 平台适配层 + 领域零 `cfg`（1.1）；✅ loader 单体系（P1-4）；✅ context.rs 93 行（P2-1）；✅ 修 P2-12 | 审计 §5.3、§15.1/§15.2 |
| 插件系统 | ✅ Phase 2：Manifest v1 / `.rmcpkg` 事务 / 状态机 / 权限 / 配置 / CLI / UI | ADR-003、ADR-004 |
| Dashboard | ✅ Phase 3.1–3.5：布局 Schema/归一化/网格/拖放/主题令牌 | ADR-005、`rmccc-dashboard` |
| 安全缺口 | ✅ natives zip-slip（P1.5）；✅ 插件包 zip-slip/zip bomb/路径/类型限制（2.2） | 审计 §15.4 |
| 平台执行散落 | ✅ 领域 crate 无平台 `#[cfg]`；插件原子替换经 `PlatformFs` | PLATFORM_ADAPTATION.md |
| 环境回归 | Homebrew rustup 路径失效、`~/.cargo` 缺失、strip 失效（缺 libLLVM） | 审计 §15.5 |
| 发布 | release.yml 齐备缺证书/Secret；无体积审计；E2E 未实跑 | 审计 §13.7 |

## 5. 并行与串行

- **可并行**：Phase 0.5（前端缺口，纯既有后端接线）与 Phase 1 前半（pipeline 收敛）。建议先做 Phase 0.5 见效快，Phase 1 重构同步推进。
- **进度**：Phase 0 ✅、Phase 0.5 ✅、Phase 1.1–1.8 ✅、Phase 2.1–2.6 ✅（+2.7 IPC/UI）、
  **Phase 3.1–3.5 ✅**（ADR-005、布局归一化、Widget Registry、CSS Grid + 拖放、主题令牌 +
  浅/深/跟随系统、插件卡片占位）。**Phase 1/2/3 主体完成**；下一步 Phase 4（隔离 UI + SDK，
  需先建 ADR-006）。
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
| ADR-006 | 隔离 UI 容器 + 消息桥 | Phase 4 前 |
| ADR-007 | 逻辑运行时 WASM vs 进程（需 PoC） | Phase 5 前 |
| ADR-008 | 插件目录 / 签名发布 | Phase 7 前 |
| ADR-009 | 凭据存储 Keychain/Credential Manager + 回退 | Phase 6 前 |
| ADR-010 | Agent 会话存储 / 工具审计 / 隐私 | Phase 6 |
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
| M0 可信基线 | 5 门禁本机全绿复跑 + 体积基线表 + 差距清单 | Phase 0 |
| M1 主线闭环补齐 | UI 可直达 ≥ 40/46 命令；Mod 浏览器可用 | Phase 0.5 |
| M2 单一编排 | Tauri/CLI 同操作走同一 Core；context.rs < 600 行；适配层 trait 落地 | Phase 1 |
| M3 声明式插件 ✅ | 损坏插件不影响主应用；声明式插件可装/禁/卸 | Phase 2 |
| M4 可定制 Dashboard ✅ | 卡片注册 + 布局持久化 + 主题令牌 | Phase 3 |
| M5 第三方隔离 UI | 外部样例可独立构建/打包/安装；无法越权 | Phase 4 |
| M6 逻辑运行时 | 运行时 PoC + ADR-007；插件崩溃不崩主程序 | Phase 5 |
| M7 Agent 只读 | 只读工具 + 计划预览 + 审批可审计 | Phase 6（前半） |
| M8 生产分发 | 三平台原生产物 + 签名/公证 + updater 真实 endpoint | Phase 7 |
