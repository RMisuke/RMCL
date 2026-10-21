# ADR-007 PoC — 逻辑插件运行时（Phase 5.1）

> **不是 workspace 成员**（`Cargo.toml` 里 `[workspace]` 自成一体）。它依赖 wasmtime，
> 绝不能被主二进制链接。测量完的源码留在这里作为 ADR-007 的证据。

测量环境：macOS 14 / Apple Silicon（arm64），`--release`、`lto = true`、`codegen-units = 1`、
`/usr/bin/strip` 后；时间用 `std::time::Instant`，取三次运行的中位数；RSS 取
`/usr/bin/time -l` 的 `maximum resident set size`。

## 复现

```bash
cd poc/logic-runtime
export CARGO_HOME=/tmp/rmccc-cargo    # 任何可写的 cargo home，避免污染全局缓存
cp ~/.cargo/config.toml "$CARGO_HOME" # 沿用 rsproxy 镜像

cargo build --release --bin baseline_poc --bin echo-host --bin spawn_poc
cargo build --release --features wasmi-engine    --bin wasmi_poc
cargo build --release --features wasmtime-engine --bin wasmtime_poc
/usr/bin/strip target/release/{baseline_poc,wasmi_poc,wasmtime_poc,echo-host,spawn_poc}

./target/release/baseline_poc
./target/release/wasmi_poc
./target/release/wasmtime_poc
./target/release/spawn_poc
```

## 体积（strip 后）

| 产物 | 字节 | MiB | 相对 baseline |
|------|------|-----|---------------|
| `baseline_poc`（仅 std） | 302,656 | 0.29 | — |
| `echo-host`（stdio 子进程） | 302,712 | 0.29 | +0.00 |
| `wasmi_poc`（解释器 + `wat`） | 2,356,712 | **2.25** | **+1.96** |
| `wasmtime_poc`（Cranelift JIT） | 6,481,664 | **6.18** | **+5.89** |

`wasmi_poc` 还额外链接了 `wat`（PoC 里现场解析 WAT）；生产构建直接读 `.wasm`，
实际增量会更小一些。

## 时间与内存

| 引擎 | 引擎初始化 | 编译模块 | 实例化 | 单次调用（1000 次平均） | 进程 RSS |
|------|-----------|---------|--------|------------------------|----------|
| wasmi | 1–12 µs | 51–441 µs | 7 µs | 0.047 µs | 2.9 MiB |
| wasmtime | 55–327 µs | 0.82–3.0 ms | 25–127 µs | 0.055 µs | 6.4 MiB |
| 独立进程 + stdio（`spawn_poc`） | spawn 0.28–0.88 ms | — | 握手 2.0–3.5 ms（冷启动首轮 388 ms） | 8.5 µs / 次往返 | — |

## 与门禁的关系

ADR-011 的红线是**strip 后主可执行文件 < 10 MiB**，当前主程序 7.85 MiB，余量 **2.15 MiB**：

- 嵌入式 wasmtime（+5.89 MiB）→ 13.7 MiB，**直接违规**；
- 嵌入式 wasmi（+1.96 MiB）→ 9.81 MiB，勉强过线但余量只剩 0.2 MiB，且**没有崩溃隔离**；
- 独立进程 + wasmi：主程序不变，多一个 2.25 MiB 的 helper（可按需/按平台分发），
  换来真正的进程级隔离与可 kill 的超时。

## 未测（deferred）

Windows x64 与 Linux x64：没有原生 runner，按指南 §1.4 标记 **deferred / release-blocked**。
ADR-007 的结论只基于 macOS ARM64 实测 + 结构性理由（体积门禁、崩溃隔离），
不据此宣称 Windows 上的性能数字。
