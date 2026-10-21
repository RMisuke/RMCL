# ADR-001: 支持的 OS 最低版本与架构

- **状态**：proposed
- **日期**：2026-10-20
- **阶段**：Phase 0
- **关联**：指南 §5.1 平台矩阵；`ARCHITECTURE.md` §13.1；审计 §1.1

## 背景

RMCCC 目标平台为 macOS Apple Silicon、macOS Intel、Windows x64、Linux x64（指南 §5.1）。
当前只在 macOS Apple Silicon 开发与手测（指南 §1.4），但代码与 CI 必须锁定一个可复现的
"最低支持版本"，否则无法判断某个 API/依赖是否可用，也无法定义 CI runner 的最低镜像。

现状事实：

- `ARCHITECTURE.md` §13.1 写 macOS 11+ / Windows 10 / Ubuntu 22.04+，但未成 ADR，也未在 CI 中强制。
- Tauri 2 使用系统 WebView：macOS 为 WebKit、Windows 为 WebView2、Linux 为 WebKit2GTK。
- 本机 macOS 版本高于最低目标，单机开发无法自然发现"低于最低版本"的兼容问题。

## 目标

- 为四个目标平台各定一个 **最低 OS 版本 + 架构**，作为依赖选择、CI runner、代码 `cfg` 门禁的依据。
- 明确"不支持的系统"行为（明确报 `unsupported_on_platform`，不静默失败）。

**不解决**：是否支持 32 位、ARM Windows、Android/iOS。

## 选项

**A. 保守（与 ARCHITECTURE 现状一致）**：macOS 11+、Windows 10 1809+、Linux Ubuntu 22.04+（glibc 2.35）。
- 优点：与现有文档一致；Tauri 2/WebView2/WebKit2GTK 4.0 均满足；覆盖面足够。
- 缺点：放弃更老的系统（Win7/8、macOS 10.15、Ubuntu 20.04）。

**B. 更宽（macOS 10.15+、Windows 10 全版本、Ubuntu 20.04+）**：
- 优点：覆盖更多老机器。
- 缺点：Tauri 2 在旧 WebKit/WebView2 上的渲染差异与安全补丁缺失；Ubuntu 20.04 已接近 EOL；CI 需维护更老镜像。

**C. 更窄（macOS 12+、Windows 11、Ubuntu 24.04+）**：
- 优点：减少兼容负担。
- 缺点：排除大量仍在用的 macOS 11 / Windows 10 用户，与"桌面启动器"定位不符。

## 决策（提案）

采用 **A**：

| 平台 | 最低版本 | 架构 | 依据 |
|------|---------|------|------|
| macOS | **11 (Big Sur)** | `aarch64-apple-darwin`、`x86_64-apple-darwin` | Tauri 2 / WKWebView |
| Windows | **10 1809 (17763)+** | `x86_64-pc-windows-msvc` | WebView2 最低要求 |
| Linux | **Ubuntu 22.04+（glibc 2.35+）** | `x86_64-unknown-linux-gnu` | WebKit2GTK 4.0+ |

不支持的系统：应用启动时检测并返回明确的 `unsupported_on_platform`，不得静默崩溃。

## 后果

- 正面：与现有文档一致，CI 可用主流 runner；依赖可选择较新的 crate 版本。
- 负面：Win7/8、macOS 10.15、Ubuntu 20.04 用户不在支持范围，需在 README/发布说明中写明。
- 对代码：不得使用高于最低版本的 OS API（如 macOS 12+ 独有 API）；新增平台相关代码必须在适配层
  （指南 §5.7）标注最低版本假设。

## 迁移影响

- 仅文档与 CI：需在 `README.md`、`OPERATIONS.md`、发布说明中写明最低版本。
- 无数据迁移。

## 跨平台验证方式

- 本机（macOS ARM64）：只能验证"≥ 本机版本"，**无法**验证 macOS 11 边界 → 标记 deferred。
- Windows/Linux 最低版本边界：需原生 CI runner 或虚拟机，当前 deferred（服从指南 §1.4）。
- 代码层：CI 增加"不得使用高于最低版本 API"的人工审查项（无法完全自动化）。

## 状态变更记录

- 2026-10-20：proposed（Phase 0 基线）。
