# ADR-011: 主可执行体积门禁（度量对象与阈值）

- **状态**：accepted（2026-10-24 阈值修订：10 MiB → **20 MiB**）
- **日期**：2026-10-20（首次 accepted）；2026-10-24（阈值修订）
- **阶段**：Phase 0
- **关联**：`AGENTS.md` 体积红线；指南 §16.4；审计 §1.1、§15.5

## 背景

`AGENTS.md` 硬约束："最终二进制 < 10MB"。但文档从未说明**度量对象**，而候选对象差异巨大：

| 度量对象 | 当前实测（macOS ARM64，2026-10-20） |
|----------|-----------------------------------|
| GUI 主可执行 `rmccc`（未剥离） | **10,498,088 B = 10.01 MiB / 10.50 MB** |
| GUI 主可执行 `rmccc`（系统 strip 后） | **7,170,752 B = 6.84 MiB / 7.17 MB** |
| CLI 主可执行 `rmcli`（strip 后） | 4,485,120 B = 4.28 MiB |
| `.app` 包 / DMG / NSIS 安装包 | 未实测（需打包） |

结论已分叉：**未剥离 10.01 MiB 恰在红线附近（甚至在十进制下超 10.0 MB），stripped 6.84 MiB 明确达标。**
不先定度量对象，体积门禁无法判定，也无法在 CI 里写阈值。

补充事实：Cargo `strip = true` 在本机**失效**（`rust-objcopy` SIGABRT，缺 `libLLVM.dylib`/llvm-tools），
所以"未剥离"是当前构建的默认产物；必须能 strip 才能得到达标数字。

## 目标

- 定一个**唯一、可自动化、跨平台语义一致**的体积度量对象与阈值。
- 明确 strip 失败时的门禁行为（不能"看起来达标"）。

**不解决**：是否值得为体积牺牲功能；安装包体积优化手段。

## 选项

**A. 度量 stripped 的主可执行文件（GUI `rmccc`），阈值 < 10 MiB。**
- 优点：符合"Cargo release + strip"的标准产物；macOS/Linux 上"二进制"语义自然；6.84 MiB 达标，留有余量。
- 缺点：Windows 上主可执行是 `.exe`（stripped 后语义类似），但 `.app`/安装包不含在内；用户下载的实际是安装包。

**B. 度量未剥离可执行，阈值 < 10 MiB。**
- 优点：构建默认产物，无需额外 strip 步骤。
- 缺点：10.01 MiB 已超标；未剥离体积随调试信息膨胀，不能代表发布质量；与"release 必须 strip"的行业惯例冲突。

**C. 度量整个安装包（`.app`/DMG/NSIS/MSI），阈值 < 10 MiB。**
- 优点：最贴近用户实际下载体积。
- 缺点：平台打包差异大（macOS `.app` 含 Frameworks、Windows 含 WebView2 bootstrapper）；当前未实测；10 MiB 对安装包偏紧，可能迫使削减功能。

**D. 双指标**：主可执行 stripped < 10 MiB（硬红线）+ 安装包单列记录（软指标，不作红线）。
- 优点：既守住"二进制"硬约束，又保留安装包可见性；避免为打包差异扭曲红线。
- 缺点：需要两套记录；安装包超标时无强制门禁。

## 决策（已接受，2026-10-20）

采用 **A + D 的组合**：

1. **硬红线**：`cargo build --release` 后经 strip 的**主可执行文件** < **10 MiB**（二进制兆字节，1 MiB = 1,048,576 B）。
   - macOS：`target/release/rmccc`，strip 后。
   - Windows：`rmccc.exe`，strip 后（或等价 `llvm-strip`）。
   - Linux：`rmccc`，strip 后。
   - CLI `rmcli` 同口径（当前 4.28 MiB）。
2. **软指标**：`.app` / DMG / NSIS / MSI 安装包体积**单列记录**（每 release 报告），不作硬红线；但当安装包显著增长时需在 PR 中说明。
3. **strip 必须可自动化**：release 流水线必须能执行 strip。若 strip 失效（如当前缺 llvm-tools），
   体积门禁判为 **release-blocked**，不得用"未剥离"数字宣称通过。
4. 每次新增依赖/运行时，记录 stripped 主可执行的增量（对照本 ADR 基线）。

## 后果

- 正面：门禁可自动化、跨平台语义一致；6.84 MiB 达标，后续有 ~3 MiB 余量。
- 负面：必须修 strip 工具链（装 `llvm-tools` 或用 `llvm-strip` 后处理），否则门禁永远 release-blocked。
- 对 CI：增加"构建 → strip → 断言 < 10 MiB"步骤；strip 失败即 fail。
- 对依赖决策：任何新增运行时（如 WASM 引擎）必须先测 stripped 增量，超余量则拒绝。

## 迁移影响

- 无数据迁移；仅 CI 与发布脚本。
- `AGENTS.md` 体积红线已更新为"strip 后的主可执行 < 10 MiB"。
- 需在 `docs/guides/OPERATIONS.md` 记录 strip 方法与失败处理（后续补）。

## 跨平台验证方式

- 本机 macOS ARM64：已实测（见上表）；strip 需手动（系统 `/usr/bin/strip`），CI 需补 llvm-tools。
- Windows/Linux：待原生 CI runner，当前 **deferred**（服从指南 §1.4）。
- 判定标准：三平台 stripped 主可执行均 < 10 MiB 才能称"体积达标"；缺平台数据不得填 pass。

## 状态变更记录

- 2026-10-20：proposed（Phase 0 基线，含首次实测体积）。
- 2026-10-20：**accepted**（用户拍板：stripped 主可执行 < 10 MiB 为硬红线）。
- 2026-10-24：**阈值修订**（用户拍板）：硬红线由 < 10 MiB 放宽为 **< 20 MiB**。
  - 动机：为后续功能（外置登录 authlib-injector、更完整的下载/任务运行时等）预留依赖体积空间；
    当前 stripped 主可执行约 7.9–9.1 MiB，10 MiB 红线余量过小，评估依赖时频繁触线。
  - 度量对象与门禁机制**不变**：仍是"strip 后的主可执行文件"硬红线 + 安装包软指标；
    strip 失效仍判 release-blocked。
  - 基线参考：2026-10-24 本机 stripped 主可执行 9.12 MiB，新阈值下余量 ~10.9 MiB。
  - 依赖评估纪律不变：新增依赖仍须 `cargo bloat` 评估并记录增量。
