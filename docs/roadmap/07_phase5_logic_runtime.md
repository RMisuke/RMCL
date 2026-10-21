# Phase 5 — 逻辑插件运行时

> **对应指南**：§6.2 阶段 C/D、§6.3，§17 Phase 5。**状态**：5.1–5.3 完成（2026-10-21），5.4–5.5 进行中。
> **前置**：Phase 1 平台适配层 + Phase 2 权限模型。**ADR-007**（WASM vs 独立进程）须 PoC 后 accepted。

## 1. 目标

通过 PoC 决定逻辑插件运行时（嵌入式 WASM Component Model 或独立宿主进程），定义调用契约与资源限制，先开放一个低风险 Mod Source / 只读兼容性扩展。插件崩溃/超时不崩主程序；宿主能强制权限与资源限制。

## 2. 与当前状态的对应

- 当前 `ModSource` trait 已有（Modrinth 实现，审计 §6.8），但无第三方运行时；`ModDependency` 已定义无解析器（P3-2）。

## 3. 任务切片分解

### 5.1 运行时 PoC（ADR-007 前置）
- 候选 A：嵌入式 WASM Component Model（WIT/Wasmtime）。
- 候选 B：独立宿主进程 + JSON-RPC。
- 测量指标（两平台，Windows = deferred）：冷启动延迟/空插件体积/内存/调用延迟/权限隔离/跨平台构建/更新方式。
- 体积增量必须对照 ADR-011 < 10MB 门禁记录。

### 5.2 ADR-007 决策
- 记录背景/目标/选项/选择理由/后果/迁移影响/跨平台验证方式/状态。

### 5.3 Host Import allowlist + 资源限制
- 宿主定义显式导入；超时/内存(fuel)/并发限制/任务取消。
- 网络与文件经 Host Broker，不直接给插件。
- 崩溃恢复测试。

### 5.4 版本兼容与回滚
- 插件 API 版本独立；未知 major 拒绝启用。
- 升级失败恢复前一可运行版本。

### 5.5 第一个低风险扩展
- 开放一个只读 Mod Source 或兼容性扩展（如额外 mod 来源元数据），经权限系统授权。

## 4. 验收标准

- [ ] ADR-007 accepted（基于 PoC 实测，非"能编译"）。
- [ ] 插件崩溃/超时不崩主程序；资源限制可强制。
- [ ] Host Import allowlist + 撤销测试通过。
- [ ] macOS ARM64 有实际构建验证；Windows x64/Intel = deferred。
- [ ] 运行时增量体积已记录对照 ADR-011。

## 5. 平台策略

- PoC 两平台对比中，Windows 数据在原生 CI 触达前标 deferred；不据 macOS 单平台结论定 ADR。
- 运行时后端若依赖平台原生库，进平台适配层。

## 6. 风险与回滚

- 运行时增量超体积红线 → 不接受该候选，回退另一候选或推迟。
- WIT/组件工具链成熟度风险 → PoC 阶段暴露，不进生产。

## 7. 文档同步

- `ARCHITECTURE.md` 增"逻辑运行时"章节；ADR-007 入库。
- `AUDIT` §1.3/§6.7 更新；`PROJECT_STATUS` 回填。

## 8. 进度（2026-10-21）

| 切片 | 状态 | 证据 |
|------|------|------|
| 5.1 PoC | ✅ | [`poc/logic-runtime/README.md`](../../poc/logic-runtime/README.md)：strip 后 baseline 0.29 MiB / wasmi +1.96 / wasmtime +5.89 MiB；wasmi 调用 0.047 µs、RSS 2.9 MiB；独立进程 spawn 0.3–0.9 ms、往返 8.5 µs |
| 5.2 ADR-007 | ✅ | [ADR-007](../adr/ADR-007-logic-runtime.md) accepted：嵌入式 wasmtime 违反 ADR-011（13.7 MiB），嵌入式 wasmi 只剩 0.2 MiB 且无崩溃隔离 → **独立 helper 进程 + wasmi core module** |
| 5.3 运行时限流 | ✅ | `crates/rmccc-logic` + `src-logic-host`：导入白名单、`StoreLimits` 内存、fuel、超时 kill、重启、载荷边界检查；8 项协议测试 + 9 项真实 spawn 集成测试 |
| 5.4 版本兼容/回滚 | 待做 | 逻辑 API 版本协商已在握手实现（未知 `v` 拒绝），插件包层面的 `entry.logic` 校验与回滚待做 |
| 5.5 第一个低风险扩展 | 待做 | 计划：`logic.invoke` 能力（medium，需显式授予）+ 只读 `compat.check` / `mods.source.search` |

### 验收对照

- [x] ADR-007 accepted（基于 PoC 实测，非"能编译"）。
- [x] 插件崩溃/超时不崩主程序；资源限制可强制（fuel / memory / wall clock）。
- [x] Host Import allowlist + 撤销测试通过（`imports_outside_the_allowlist_are_rejected_before_running`）。
- [x] macOS ARM64 有实际构建验证；Windows x64/Intel = deferred。
- [x] 运行时增量体积已记录对照 ADR-011（主可执行文件不变）。
