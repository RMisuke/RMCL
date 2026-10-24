# RMCCC 执行提示词集（Agent 启动用）

> 本文件是一套**可复制**的提示词，用于在新的 Agent session 中快速定向、判断优先级、按切片执行。
> 它服从 [`AGENTS.md`](../../AGENTS.md)、[`RMCCC_PLUGIN_PLATFORM_ARCHITECTURE_GUIDE.md`](../RMCCC_PLUGIN_PLATFORM_ARCHITECTURE_GUIDE.md) §1.4/§5.7，并锚定 [`AUDIT_2026-10-20.md`](../audits/AUDIT_2026-10-20.md) 的条目。
> 维护：随阶段推进更新"当前优先级"块；切片协议与 deferred 规则长期不变。

---

## Prompt 0 — 会话启动与定向（每个 session 第一步）

```text
你正在维护 RMCCC。开始任何工作前，按序完成定向，不要凭记忆假设当前状态：

1. `git status --short` + `git log --oneline -5`，确认工作区干净、不覆盖未提交改动。
2. 读 `AGENTS.md`（顶层约束 + Session Startup）。
3. 读 `docs/roadmap/README.md` 与 `docs/roadmap/00_ROADMAP_OVERVIEW.md`，确认当前细化阶段与里程碑。
4. 读 `docs/audits/AUDIT_2026-10-20.md` §1（一页纸结论）、§14（测试现状）、§15（技术债台账）、§15.5（环境回归），确认当前事实。
5. 若任务涉及插件/Agent/平台适配，读 `docs/RMCCC_PLUGIN_PLATFORM_ARCHITECTURE_GUIDE.md`，特别注意：
   - §1.4：当前本机仅 macOS Apple Silicon；不跨平台编译、不做 Windows/Linux 实测；非 macOS 验收一律 deferred。
   - §5.7：平台特定能力（文件系统/进程/凭据/shell/Java 等）归档到平台适配层，启动器核心只定义 trait/DTO，不内联 #[cfg] 执行。
6. 用源码与实测命令确认问题，不得把目标架构当成已实现代码。

完成后输出：当前阶段、本轮目标、受影响 crate/文件、跨平台风险、测试计划。未经我确认不开始修改。
```

---

## Prompt 1 — 当前优先级队列（决定"先做什么"）

```text
按以下优先级选择本轮工作。优先级来自 docs/roadmap/00_ROADMAP_OVERVIEW.md 的依赖图与审计 Sprint A/B：

【必须先做 / now】
- Phase 0（docs/roadmap/01_phase0_baseline.md）：重建可信基线。
  首切片 0.1 工具链 + 0.2 五门禁复跑（cargo fmt/clippy×2/test、pnpm typecheck/build）。
  不做任何代码重构；失败只记录真实输出，区分"环境 vs 代码"。

【高 ROI / 并行】（Phase 0 完成后）
- Phase 0.5（docs/roadmap/02_phase0.5_product_gaps.md）：补既有后端的 UI 缺口。
  顺序：0.5.1 前端契约统一（api.ts wrapper、删裸 safeInvoke、补 INSTANCES_REFRESHED）
        → 0.5.2 Mod 浏览器（P1-1）→ 0.5.3 多账号 UI（P1-2）→ 0.5.4 本地 mod 安装（P1-5）
        → 0.5.5 日志页+暂停恢复 → 0.5.6 完整性检查入口。
  目标：UI 可直达 ≥ 40/46 命令。
- Phase 1（docs/roadmap/03_phase1_core_convergence.md）：
  1.2 pipeline 收敛 ✅ 完成（2026-10-21）；1.5 zip-slip ✅ 完成。
  下一步 1.1 平台适配层 trait 定义 + macOS 后端（用户重点）；随后 1.3/1.4/1.6/1.7/1.8。
  每个切片独立 PR。

【延后 / deferred】（本轮不做，仅记录）
- Phase 2–7（插件协议/Dashboard 隔离 UI/逻辑运行时/Agent/分发）：依赖 Phase 1 稳定底座。
- 所有 Windows/Linux 原生构建与实测：推迟到原生 CI runner（Phase 7）。
- WASM vs 进程运行时 PoC（ADR-007）、Agent 工具管线：Phase 5/6。
- CurseForge 源、依赖解析、Cleanroom loader 实现：可延后。

本轮我选：________（填切片编号）。只做这一个切片，不跨阶段。
```

---

## Prompt 2 — 单切片执行协议（每轮工作通用）

```text
执行所选切片时严格遵守（服从插件平台指南 §18）：

Step 1 调查：找到实际入口、依赖、测试；先读相关文件，不凭文件名猜。识别属于
  Core / Tauri adapter / CLI adapter / Frontend / Platform adapter / Plugin 哪一层。
  写清当前行为 vs 目标行为 vs 平台差异 vs 风险。

Step 2 最小方案：先给出本轮目标、明确不做的事、受影响文件、API/契约是否变化、
  是否涉及 Windows/macOS 差异、测试与验收命令、是否需要 ADR。
  （插件 API/权限/Manifest/持久化 Schema/运行时/跨平台发布策略变更需先提 ADR。）

Step 3 修改：增量扩展 + 兼容适配；不无关重命名/格式化/目录重排。
  核心业务逻辑不直接依赖 Tauri；平台差异进适配层（docs/roadmap/PLATFORM_ADAPTATION.md）；
  新增依赖记录功能/替代/体积/平台要求。

Step 4 验证（按改动范围跑）：
  cargo fmt --all -- --check
  cargo clippy --workspace --all-targets -- -D warnings
  cargo clippy --workspace --lib --bins -- -D warnings -D clippy::panic -D clippy::unwrap_used
  cargo test --workspace
  pnpm typecheck && pnpm build
  工具链/缓存阻止时记录真实失败输出，不得声称完整通过。
  平台特有代码：本机只验 macOS，Windows/Linux 显式标 deferred。

Step 5 交付报告：修改内容与理由 / 架构契约影响 / 实际验证命令与结果 /
  未运行项与原因 / 剩余风险与迁移注意 / 文档是否同步。
  完成后停下，不自动进入下一阶段。
```

---

## Prompt 3 — 延后清单与 deferred 标记规则（防止越权提前做）

```text
本轮严禁提前触碰以下内容（服从 §1.4 与路线依赖）：

1. 跨平台编译/实测：不在 macOS 上为 windows/linux 目标构建或跑测；不宣称 Windows 可用。
2. 插件运行时、Agent、Dashboard 卡片化、插件目录/签名：属 Phase 2–7，依赖 Phase 1 底座。
3. 把"目标架构"（GameService/InstanceService/Host Broker/Manifest 等）当成已实现。
4. 为"插件化"立即把现有功能改成外部插件；把第三方脚本载入可信主窗口。
5. 用 cfg!(windows) 在领域逻辑里临时实现平台分支——新增平台能力一律进适配层。
6. 未经 ADR 改插件 API/权限/Manifest/持久化 Schema/运行时选择。

deferred 标记规则：
- 任何非 macOS Apple Silicon 的"必须验证"项，在交付报告与验收清单中记 deferred / release-blocked，附原因。
- 不得在本机环境填 pass；不得因环境限制提高体积红线或放宽安全约束。
- 适配层 Windows/Linux 后端可留 trait + 返回 unsupported_on_platform 占位，待原生 CI 补实现。
```

---

## Prompt 4 — 三个可直接启动的任务提示词（按优先级）

### 4A. Phase 0.1+0.2 基线复跑（最高优先，无代码改动）

```text
任务：执行 docs/roadmap/01_phase0_baseline.md 的切片 0.1 与 0.2。

- 确认 rust-toolchain.toml 生效（stable + rustfmt + clippy）。
- 尝试补 llvm-tools 以修复 strip（失败则记 deferred + 原因，不阻塞）。
- 复跑五门禁：cargo fmt --check / clippy×2 / cargo test --workspace / pnpm typecheck+build。
- 失败一律记录真实输出，本轮不修代码（修复属后续切片）。
- 产出：复跑结果表（命令 / 结果 / 日期），区分环境问题与代码问题。
- 更新 docs/audits/AUDIT_2026-10-20.md §14.1 复跑日期与结果；§15.5 环境现状。
- 平台：仅 macOS；Windows/Linux 体积与烟测 = deferred。
- 完成后停下，报告结果，不自动进 0.3。
```

### 4B. Phase 0.5.1 前端契约统一（Phase 0 完成后，高 ROI）

```text
任务：执行 docs/roadmap/02_phase0.5_product_gaps.md 切片 0.5.1（审计 P2-11、P2-5）。

- 让 src/routes/* 改用 src/ipc/api.ts 分域 wrapper，不再裸 safeInvoke("命令名")。
- 删 Dashboard 残留未使用 import（auth/profiles/versions/launch，审计 §10.2）。
- src/ipc/events.ts 补 INSTANCES_REFRESHED = "instances://refreshed"（后端 12 个常量对齐）。
- 验收：grep -rn 'safeInvoke(' src/routes 为空（wrapper 内部除外）；pnpm typecheck+build 通过。
- 不改后端行为；不改命令签名。
- 完成后报告，列出仍裸调用的残留点（若有）。
```

### 4C. Phase 1.1 平台适配层 trait 定义（用户重点，可与 4B 并行）

```text
任务：执行 docs/roadmap/03_phase1_core_convergence.md 切片 1.1 + 参考 docs/roadmap/PLATFORM_ADAPTATION.md。

- 新建 crates/rmccc-platform/（或先在 rmccc-core 内 platform/ 模块），定义 §5.7.2 全部 trait：
  PlatformPaths / PlatformFs / ArchiveExtractor / FilePermissions /
  PlatformProcessControl / ProcessLauncher / JavaDetector /
  CredentialStore / PlatformShell / PlatformDescriptor。
- macOS 后端：迁入现有 nice/killpg/chmod 600/dirs 路径/symlink/java_home（见 PLATFORM_ADAPTATION.md §3 表）。
- Windows/Linux 后端：留 unimplemented!() 或返回 unsupported_on_platform，标 deferred。
- CoreContext 初始化注入 macOS 后端；提供 mock 后端供测试。
- 硬约束：领域 crate 不再有 #[cfg(target_os)] 执行分支；数据筛选类（libraries/arguments 的 rules[].os）改用注入的 PlatformDescriptor。
- 验收：grep 确认适配层外无新增 #[cfg] 执行分支；macOS 启动烟测不回归；cargo test 通过。
- 涉及行为变化（如 Windows terminate_tree 改 Job Object）先提 ADR。
- 平台：仅 macOS 后端实测；Windows/Linux = deferred。
- 完成后更新 PLATFORM_ADAPTATION.md §3 状态列勾选。
```

---

## 使用约定

- 每轮只选 Prompt 4 中的一个切片（或 Prompt 1 中一个切片），独立 PR。
- Prompt 0 每次必做；Prompt 2 每轮通用；Prompt 3 每轮自查。
- 切片完成后：回填 `docs/PROJECT_STATUS.md` 一轮记录；在 `docs/audits/AUDIT_2026-10-20.md` 对应 P1-x/P2-x 销账；更新对应 `docs/roadmap/0x_*.md` 的状态标记。
- 阶段切换时更新本文件 Prompt 1 的"当前优先级"块。
