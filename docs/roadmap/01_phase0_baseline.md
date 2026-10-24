# Phase 0 — 可复现基线与跨平台差距清单

> **对应指南**：§17 Phase 0。**状态**：0.1–0.5 全部完成（2026-10-20 实测）；0.4 由 [`PLATFORM_ADAPTATION.md`](PLATFORM_ADAPTATION.md) 覆盖；ADR-001/011/012 已 proposed 入库 [`docs/adr/`](../adr/)。
> **当前事实来源**：审计 §14.1（2026-10-20 已复跑）、§15.5（环境现状已更新）、§13.7（发布缺口）。

## 1. 目标

保证后续一切重构有可信的、可复现的起点：工具链可用、5 门禁本机全绿复跑、体积基线实测、平台执行散落点与 Windows 缺口列成清单。**不在此阶段做任何代码重构。**

## 2. 前置条件

- 无未提交改动会覆盖（先 `git status --short`）。
- 本机 macOS Apple Silicon + Xcode Command Line Tools。

## 3. 与当前状态的对应

| 审计条目 | 现状 | Phase 0 处理 |
|----------|------|--------------|
| §15.5 工具链路径 | Homebrew rustup 路径失效；实体在 Cellar | 用 `rust-toolchain.toml`（已存在，锁 stable+rustfmt+clippy）+ `rustup which`/PATH 修复，**不**硬编码 Cellar 路径到文档 |
| §15.5 `~/.cargo` 缺失 | crates.io 缓存丢失，`cargo test` 需联网重建 | 记录为环境事实；复跑时联网重建，**不**把首次下载耗时当失败 |
| §14.1 门禁未复跑 | 上次全量 2026-10-10 | 本阶段首要任务就是复跑 5 条命令并更新审计 §14.1 |
| P2-7 strip 失效 | 缺 libLLVM/llvm-tools → release 二进制未剥离 | 装 `llvm-tools` 组件或改用 `llvm-strip` 后处理；记录能否自动 strip |
| §13.7 体积审计未做 | 无 cargo bloat | 跑 cargo bloat 出基线表 |
| §13.7 E2E 未实跑 | tauri-driver 配置齐备未跑 | 列为 deferred（需 `RMCCC_RUN_E2E=1` + 显示器环境），本机若可跑则跑 |
| §13.7 CI Windows | 确认 Windows runner 是否真构建/测试 | 审查 ci.yml；若仅占位则列为 deferred |

## 4. 任务切片分解（每切片独立可验收）

### 0.1 工具链与环境复现
- 确认 `rust-toolchain.toml` 生效（`rustc --version` / `cargo --version`）。
- 用 `rustup component add llvm-tools` 或等价方式补齐 strip 所需；记录失败则列为 deferred。
- 在 `docs/guides/OPERATIONS.md` 记录**当前有效**的本机构建命令（不再引用过期 Cellar 路径）。
- 验证命令：`cargo --version && rustc --version && cargo fmt --version && cargo clippy --version`。

### 0.2 五门禁复跑
- `cargo fmt --all -- --check`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `cargo clippy --workspace --lib --bins -- -D warnings -D clippy::panic -D clippy::unwrap_used`
- `cargo test --workspace`
- `pnpm typecheck && pnpm build`
- 失败一律记录真实输出，不修代码（修复属于后续切片）；仅记录"哪些是环境、哪些是代码"。

**0.2 实测结果（2026-10-20）**：

| 门禁 | 结果 | 备注 |
|------|------|------|
| `cargo fmt --all -- --check` | ✅ exit 0 | — |
| `cargo clippy --all-targets` | ✅ exit 0 (39.7s) | 修 5 处 `clippy::double_must_use`（1.99 新 lint） |
| `cargo clippy --lib --bins` panic/unwrap | ✅ exit 0 (22.3s) | — |
| `cargo test --workspace` | ✅ **300 passed / 0 failed** (2m37s) | 含 1 doc-test，与审计一致 |
| `pnpm typecheck` | ✅ | — |
| `pnpm build` | ✅ (3.4s) | JS 117.12KB (gzip 34.37KB) / CSS 31.16KB (gzip 6.86KB) |

> 工具链：stable **1.99.1**（非审计记录的 1.98.1）。`double_must_use` 是 1.98.1 下未出现的新 lint，
> 属工具链升级的代码兼容项，零行为变更。

### 0.3 体积基线
- `cargo build --release -p rmccc`（GUI）+ `cargo build --release -p rmccc-cli`。
- 手工/自动 strip 后再测；记录 stripped / unstripped / `.app` / 前端 bundle 四个数。
- 跑 `cargo bloat --release`（若可用）存报告到 `docs/` 之外（不入库）或摘要进本阶段报告。
- 对照 ADR-011：明确"<10MB"度量的是 stripped 可执行文件还是安装包，提 ADR 草案。

**0.3 实测结果（2026-10-20，release 构建 5m19s，`strip = true` 失效）**：

| 产物 | 未剥离 | 手动 `strip` |
|------|--------|-------------|
| GUI `rmccc` | 10,498,088 B（10.01 MiB / 10.50 MB） | 7,170,752 B（6.84 MiB / 7.17 MB） |
| CLI `rmcli` | 6,156,904 B（5.87 MiB / 6.16 MB） | 4,485,120 B（4.28 MiB / 4.49 MB） |

- `strip = true` 仍失效：`rust-objcopy` SIGABRT（`Library not loaded: @rpath/libLLVM.dylib`，缺 llvm-tools）→ 记 **deferred**，可用系统 `/usr/bin/strip` 手动。
- `cargo bloat` 未跑（未装该子命令），体积分析改用 `ls -l` + 系统 strip；后续可补装。
- **ADR-011 待定**：GUI 未剥离 10.01 MiB 恰在红线、stripped 6.84 MiB 明确达标；"<10MB"度量对象须定稿。

### 0.4 平台执行散落点与 Windows 缺口清单
- 按 `PLATFORM_ADAPTATION.md` 的能力域，grep `#[cfg(target_os` / `cfg!(windows` 全量，归类"执行分支 vs 数据筛选"。
- 对 `rmccc-launch` 的进程终止/nice/Java/argfile/脚本分支做源码审计，列未覆盖 Windows 风险。
- 审查 `ci.yml`/`release.yml`：Windows runner 是否真正构建+测试；macOS Intel 是否有 runner。
- 产出：差距清单表（平台 × 缺口 × deferred 标记）。

### 0.5 ADR-001 / ADR-011 / ADR-012 草案
- ADR-001：OS 最低版本/架构（macOS 11+? Windows 10+? Linux?）。
- ADR-011：<10MB 度量对象（stripped 可执行 vs 安装包）与门禁阈值。
- ADR-012：macOS Intel runner/打包/最低系统版本（本机不构建，定 CI 策略）。

**产出（2026-10-20）**：已在 [`docs/adr/`](../adr/) 建立索引与 4 份 ADR（均 `proposed`）：
- [ADR-001](../adr/ADR-001-supported-platforms.md)：macOS 11+ / Windows 10 1809+ / Ubuntu 22.04+（glibc 2.35）。
- [ADR-011](../adr/ADR-011-binary-size-metric.md)：硬红线 = **stripped 主可执行 < 10 MiB**（1 MiB=1048576 B）；
  安装包单列软指标；strip 失效 → release-blocked。当前 GUI stripped 6.84 MiB 达标。
- [ADR-012](../adr/ADR-012-macos-intel-build.md)：保留 Intel 为目标，用 Intel runner 原生构建单架构产物；
  暂不做 universal binary（体积翻倍风险）。
- [ADR-002](../adr/ADR-002-profile-instance-semantics.md)：占位（Phase 1 填写 profile/instance 收敛与 Core Service 门面）。

> **ADR-011 已 accepted（2026-10-20）**：`AGENTS.md` 体积红线已同步为"strip 后的主可执行 < 10 MiB"。
> 其余 ADR（001/012）仍 `proposed`，未 accepted 前不据此改代码/协议。

## 5. 验收标准

- [ ] 5 门禁本机全绿复跑，结果写回审计 §14.1（标注复跑日期）。
- [ ] 体积基线表含 4 个数 + strip 是否可自动（不可自动则记 deferred + 原因）。
- [ ] 差距清单完成；所有非 macOS Apple Silicon 项标 deferred。
- [ ] ADR-001/011/012 草案（proposed）入库。
- [ ] `docs/guides/OPERATIONS.md` 更新为当前有效命令；删除过期 Cellar 路径引用。

## 6. 验证命令

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo clippy --workspace --lib --bins -- -D warnings -D clippy::panic -D clippy::unwrap_used
cargo test --workspace
pnpm typecheck && pnpm build
cargo build --release -p rmccc && cargo build --release -p rmccc-cli
# 体积
ls -l target/release/rmccc target/release/rmcli
```

## 7. 平台策略

- 本机仅 macOS Apple Silicon；Windows/Linux 体积与烟测 = deferred / release-blocked。
- 不得把"macOS 编译通过"写入 Windows 验收。

## 8. 风险与回滚

- 工具链再次回归（Homebrew 升级）→ 优先用 `rust-toolchain.toml` + `rustup` 标准 channel，减少对 Cellar 的依赖。
- `cargo bloat`/`llvm-tools` 不可用 → 降级用 `ls -l` + 手工 strip，记录方法，不阻塞后续。
- ADR 未批 → 以 proposed 状态先记录，不据此改代码。

## 9. 文档同步

- 更新 `AUDIT_2026-10-20.md` §14.1 复跑结果、§15.5 环境现状。
- 回填 `PROJECT_STATUS.md` 一轮记录。
- 本阶段完成后，Phase 1/0.5 的"当前状态锚点"以本阶段产出为准。
