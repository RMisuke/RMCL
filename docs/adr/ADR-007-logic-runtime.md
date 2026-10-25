# ADR-007: 逻辑插件运行时 —— 独立宿主进程 + WASM 解释执行

- **状态**：accepted
- **日期**：2026-10-21
- **阶段**：Phase 5
- **关联**：指南 §6.2（阶段 C/D）、§6.3、§13.4、§17 Phase 5；ADR-011（体积门禁）；ADR-003（包布局）；ADR-006（隔离 UI）

## 背景

Phase 2 的插件是纯数据（Manifest + 主题 + JSON 配置），Phase 4 开放了隔离 UI，但仍然
**没有第三方代码执行通道**：`entry.logic` 在 Manifest 校验阶段就被拒绝。

Phase 5 要回答"逻辑插件跑在哪里"。指南 §6.3 要求先做 PoC，测量两个候选的
体积/冷启动/调用延迟/内存/隔离强度，**不能因为"能编译"就下结论**。

硬约束：

- ADR-011：strip 后的**主可执行文件** < 20 MiB（2026-10-24 修订）。当前主程序 **7.85 MiB**，余量 **12.15 MiB**。
- 指南 §6.2：不得把第三方代码以 ES module 载入主窗口；逻辑插件不得直接拿到文件/网络。
- 指南 §16.1：Windows/Linux 数据要原生 runner 才有意义；本机只有 macOS ARM64。

## 目标

- 插件崩溃、死循环、内存爆炸**不能**影响主程序。
- 宿主能强制超时、内存、并发与权限（导入白名单），并能**取消**在跑的任务。
- 运行时增量对主可执行文件可接受（守住 ADR-011）。
- 首个开放的能力是**只读**扩展（Mod Source / 兼容性检查），不碰写操作。

**不做**：WIT/Component Model 工具链落地、第三方原生动态库（`.dylib`/`.dll`）插件、
远程插件市场（Phase 7）、Windows/Linux 实测（deferred）。

## 选项与实测

PoC 源码与复现步骤：[`poc/logic-runtime/`](../../poc/logic-runtime/README.md)。
macOS ARM64、release、LTO、strip 后实测：

| 产物 | MiB | 相对 baseline |
|------|-----|---------------|
| baseline（仅 std） | 0.29 | — |
| `wasmi` 嵌入式 | **2.25** | +1.96 |
| `wasmtime`（Cranelift JIT）嵌入式 | **6.18** | +5.89 |

| 引擎 | 引擎初始化 | 编译模块 | 实例化 | 单次调用 | RSS |
|------|-----------|---------|--------|----------|-----|
| wasmi（解释器） | 1–12 µs | 51–441 µs | 7 µs | 0.047 µs | 2.9 MiB |
| wasmtime（JIT） | 55–327 µs | 0.82–3.0 ms | 25–127 µs | 0.055 µs | 6.4 MiB |
| 独立进程 + stdio | spawn 0.28–0.88 ms | — | 握手 2–3.5 ms | 8.5 µs/往返 | 0.3 MiB（helper） |

### A. 嵌入式 wasmtime（Component Model）

优点：JIT 最快，Component Model 有正式 WIT 接口。
缺点：**+5.89 MiB → 主程序 13.7 MiB，直接违反 ADR-011**；Component Model 还要更多体积与
工具链依赖；崩溃仍在主进程内（一个 `panic!`/OOM 会带走整个应用）。

### B. 嵌入式 wasmi（解释器，仍同进程）

优点：+1.96 MiB，9.81 MiB 勉强过线；纯 Rust、无平台代码；fuel/内存限制易做。
缺点：**余量只剩 0.2 MiB**（下一个功能就会顶破红线）；**没有崩溃隔离**——
本进程内的 OOM/栈溢出/abort 会杀掉启动器；超时只能靠 fuel，不能强制中断一个被宿主调用阻塞的模块。

### C. 独立宿主进程 + 进程内 WASM（选中）

优点：主可执行文件**不增加**；进程级隔离（崩溃只死 helper，可重启）；超时可以直接 `kill`
子进程，不必依赖协作式 fuel；权限模型简单（子进程只拿到 stdin/stdout 管道，不继承网络/文件
能力，导入表由宿主决定）；helper 可以按平台/按需分发（Phase 7 甚至可以可选安装）。
缺点：多一个二进制（2.25 MiB）与 IPC；spawn + 握手约 2–3.5 ms（足够低频的插件调用，不适合
每帧渲染）；调试链路更长。

## 决策

采用 **C**：

1. **运行时放在独立 helper 进程里**：`rmccc-logic-host`，通过 stdin/stdout 上的
   **JSON-RPC（行分隔）** 与主程序通信。helper 不链接 Tauri、不打开网络、不访问用户数据目录。
2. **插件代码是 core WASM 模块**（`logic/plugin.wasm`），由 helper 用 **`wasmi`** 解释执行。
   *不用* Component Model：它的工具链与本 ADR 的体积预算冲突，而插件 API 只有窄窄一小组导入，
   自定义 ABI（`rmccc_alloc` / `rmccc_invoke` 导出 + `host_call` 导入）足够且可测。
3. **导入白名单**：模块只能导入 `rmccc` 命名空间下的宿主函数；helper 启动时枚举模块导入，
   出现任何非白名单导入就拒绝加载（不是运行时才发现）。首个白名单：
   `rmccc.log(level, ptr, len)`、`rmccc.host_call(req_ptr, req_len) -> ptr`。
4. **资源限制**：单次调用超时（默认 5 s，上限 30 s）、每个实例内存上限（wasmi `StoreLimits`）、
   `fuel` 上限（默认 5000 万）、同一插件并发 1 个调用、任务可取消（宿主 kill helper 并重启）。
5. **权限在宿主侧复核**：helper 只转发 `host_call`；真正的授权（`authorize(plugin_id, capability, host, instance)`）
   在主程序执行——与 Phase 4 的桥同一条规则：**每条调用重新检查**。
6. **首个开放能力**：只读 `mods.source.resolve`（Mod Source 查询）与 `compat.check`。
   写操作（安装/删除/启动）不在 Phase 5 开放。

## 后果

**正面**

- 主可执行文件体积不变（7.85 MiB），红线余量保持 2.15 MiB；helper 作为独立产物计量。
- 崩溃/死循环/内存爆炸被限制在子进程；超时无需模块配合。
- 同一套 JSON-RPC 契约可复用于未来的进程类扩展；运行时升级不影响主程序。

**负面/代价**

- 安装包总量 +2.25 MiB（helper），启动第一个逻辑插件有 2–3.5 ms 握手成本。
- 多一层故障面：helper 版本要与主程序的逻辑 API 版本匹配（握手时协商，未知版本拒绝）。
- wasmi 是解释执行，重计算型插件明显慢于 JIT（本阶段插件都是 I/O 型查询，可接受）。

## 迁移影响

- `entry.logic` 从"拒绝"改为"校验后允许"：必须是 `logic/` 下、`.wasm`、存在；
  仍禁止 `.dylib`/`.dll`/`.so`/脚本（包校验的拒绝清单不变）。
- 已装插件不受影响（都没有 `logic/`）；卸载 helper 时逻辑功能整体不可用，其它功能照常。

## 跨平台验证方式

- macOS ARM64（本机）：PoC 数据 + helper 真实 spawn/超时/kill 测试。
- Windows x64 / Linux x64：**deferred / release-blocked**。理由：`wasmi` 是纯 Rust（无平台代码），
  但进程管理、管道与 kill 语义需要原生 runner 验证；在原生 CI 触达前不宣称可用。
- 单元测试覆盖：导入白名单拒绝、fuel/内存/超时、取消、崩溃重启、未知逻辑 API 版本拒绝。

## 状态变更记录

- 2026-10-21：proposed（PoC 完成，数据见 `poc/logic-runtime/README.md`）→ accepted
