# ADR-012: macOS Intel 的 runner、打包和最低系统版本

- **状态**：proposed
- **日期**：2026-10-20
- **阶段**：Phase 0
- **关联**：指南 §5.1、§16.3；ADR-001；ADR-011

## 背景

指南 §5.1 把 macOS Intel（`x86_64-apple-darwin`）列为"必须纳入目标"，但标注"实际可用性需 CI runner/构建环境确认"。
当前本机是 Apple Silicon，**不在本机构建 Intel 产物**（指南 §1.4），因此 Intel 的可交付性完全取决于 CI 策略。

现状事实：

- 本机只能产出 `aarch64-apple-darwin` 产物。
- GitHub Actions 的 macOS runner 有 Apple Silicon（`macos-14`/`macos-15` arm64）与 Intel（`macos-13`）两类；
  Intel runner 可用性随 GitHub 政策变化，需持续跟踪。
- 打包形态有单架构 `.app`/DMG 与 universal binary（`lipo` 合并）两种。

## 目标

- 决定 macOS Intel 是否继续"必须"、如何构建、如何打包、最低系统版本。
- 保证不把"Apple Silicon 上交叉编译成功"误当 Intel 验收。

**不解决**：Windows/Linux 的 Intel/ARM 问题（另议）。

## 选项

**A. 保留 Intel 为目标，用 Intel runner 原生构建单架构产物。**
- 优点：每个架构单独验证、体积不翻倍（符合 ADR-011）；Intel 用户可下载对应 DMG。
- 缺点：需维护两类 runner 与两个发布产物；Intel runner 可能被 GitHub 下线。

**B. universal binary（`lipo` 合并 arm64 + x86_64）。**
- 优点：用户无需选择架构，一个 `.app` 通吃。
- 缺点：**体积近乎翻倍**（与 <10 MiB 红线冲突，ADR-011 余量仅 ~3 MiB）；构建复杂度高；Intel 部分仍需在 Intel 机器/runner 上实测。

**C. 放弃 Intel，仅支持 Apple Silicon。**
- 优点：最小维护与体积。
- 缺点：违背指南 §5.1"必须纳入目标"；Intel Mac 用户（仍占相当比例）无法使用。

**D. 保留 Intel 但降级为"best-effort"**：仅在 Intel runner 可用时构建，不承诺。
- 优点：灵活。
- 缺点：发布承诺模糊，用户不知道能否用。

## 决策（提案）

采用 **A**，并明确：

1. **继续把 macOS Intel 列为必须目标**（不降级为 best-effort），但构建与实测**只在 CI Intel runner** 进行。
2. **打包形态：单架构**（Apple Silicon 一个 DMG、Intel 一个 DMG），**暂不做 universal binary**；
   待 ADR-011 的体积余量评估后，若余量充足再单独立 ADR 讨论 universal。
3. **最低系统版本 macOS 11+**（与 ADR-001 一致）；Intel 产物同样不得使用 macOS 12+ 独有 API。
4. Intel runner 策略：优先 GitHub `macos-13`；若其下线，改用自托管 Intel 机器或停止 Intel 支持（届时修订本 ADR）。
5. 发布完成度表（指南 §16.5）必须分别列出 macOS ARM64 与 macOS Intel 两行，缺一不得称"macOS 达标"。

## 后果

- 正面：体积不翻倍；两架构各自原生验证，符合"不交叉编译冒充"的原则。
- 负面：维护两个 DMG；Intel runner 可用性是不可控外部依赖；用户需按架构下载。
- 对 CI：`release.yml` 需 `macos-13`(x86_64) + `macos-14/15`(aarch64) 两 job。

## 迁移影响

- 无数据迁移；release 产物命名需带架构后缀（如 `RMCCC-0.1.0-macos-intel.dmg`）。
- `tauri.conf.json` 的 updater target 需覆盖 `darwin-x86_64` 与 `darwin-aarch64`。

## 跨平台验证方式

- 本机（Apple Silicon）：**无法**构建/运行 Intel 产物 → deferred。
- Intel runner：原生 `cargo build/test` + `.app` 启动烟测 + DMG 安装烟测。
- 当前阶段（无 Intel runner）：Intel 相关一律 deferred / release-blocked，不得填 pass。

## 状态变更记录

- 2026-10-20：proposed（Phase 0 基线）。
