# RMCCC 全量审计（2026-10-25）— 总览与执行摘要

> 审计基线：分支 `ui/home-hero-dock` 工作区（含未提交改动），commit `92d8743` 之后。
> 审计范围：`crates/`（16 crate，约 3.9 万行）、`src-tauri/`（约 9.5 千行）、`src/` 前端（约 1.5 万行）、
> 对照库 `/Volumes/APPLE 1TB/Code/HMCL-main`（HMCL 深度重构版 fork，HMCLCore 425 + HMCL 454 个 Java 文件，约 118 个后端功能点）。
> 审计方法：5 路并行代码审计（crate 层 / Tauri 层 / 前端 / HMCL 盘点 / 功能矩阵）+ 关键矛盾点人工复核 + 门禁命令实测。

---

## 文档索引

| 文档 | 内容 |
|---|---|
| [01_CRATES_AUDIT.md](01_CRATES_AUDIT.md) | `crates/` 全部 16 个 crate 逐个审计 + 跨 crate 架构问题 |
| [02_SRC_TAURI_AUDIT.md](02_SRC_TAURI_AUDIT.md) | `src-tauri/` IPC 层：99 条命令面、锁与阻塞、MCP/协议安全专项 |
| [03_FRONTEND_AUDIT.md](03_FRONTEND_AUDIT.md) | 前端架构、SolidJS 反应式、IPC 契约、性能与 UX 三态 |
| [04_HMCL_COMPARISON.md](04_HMCL_COMPARISON.md) | 与 HMCL 的逐域功能差距比对（12 域全景） |
| [05_AUDIT_OPINIONS.md](05_AUDIT_OPINIONS.md) | 审计意见：分级整改清单、安全专项、架构收敛、HMCL 差距路线图 |

## 门禁实测信号（审计当日）

| 检查 | 结果 |
|---|---|
| `cargo check --workspace` | ✅ 通过（28.2s，零错误零警告输出） |
| `pnpm typecheck`（tsc --noEmit） | ✅ 通过（0 类型错误） |
| 静态统计 `#[test]`/`#[tokio::test]` | ≈ 632 项（10-20 审计时为 300，已翻倍；本次审计未运行全量测试） |
| IPC 契约 | 99 命令注册 ↔ 99 定义 ↔ 前端 99 wrapper 三向钉死（`ipc_tests.rs`），`mod_install_local` 为 MCP 专用双入口 |

## 执行摘要

**总体判断：架构方向正确、工程质量分层明显，但「核心启动链路的正确性」与「加载器安装的完整性」存在成簇缺陷，且与 HMCL 的功能差距集中在整合包 / CurseForge / 外置登录 / Java 自动下载 / 非 macOS 平台五大块。**

### 各层健康度一览

| 层 | 规模 | 问题数（P0/P1/P2/P3） | 一句话结论 |
|---|---|---|---|
| crates/ Rust 核心 | 16 crate / 3.9 万行 | **0 / 14 / 36 / 49** | 分层与依赖方向健康；rmccc-plugin 教科书级，rmccc-version/rmccc-modloader 是缺陷重灾区 |
| src-tauri IPC 层 | 99 命令 / 9.5 千行 | **0 / 2 / 14 / 12** | 命令面与类型桥是标杆；**锁纪律是最大短板**（嵌套读锁死锁 + 长持锁） |
| 前端 SolidJS | 7 页 / 1.5 万行 | **2 / 14 / 37 / 33** | 分层纪律优秀；两个 P0（hash 查询链路断裂、假数据卡无标识）自上轮审计原样存续 |
| **合计** | — | **2 / 30 / 87 / 94** | — |

### 审计中发现并当场复核修正的两个误判

初轮 agent 结论中有两处经人工复核后**修正定级**，详见 01 文档对应条目：

1. `replace_templates`（`crates/rmccc-version/src/arguments.rs:164`）按字节 `as char` 处理——**模板文本**中的非 ASCII 确会乱码，但**替换值**经 `push_str` 完好无损（初判"中文用户名必乱码"过重）。触发面：Profile 自定义 JVM/游戏参数含非 ASCII。定级 P0→**P1**，修复方式不变（按 `char_indices` 迭代）。
2. `ManagedProcess::kill`（`crates/rmccc-launch/src/managed.rs:202`）只杀直接子进程——但 GUI「停止游戏」实际走 IPC `launch_kill` → `platform.terminate_tree`（进程组，正确）；`ManagedProcess::kill` 当前无游戏链路调用方。定级 P1→**P2**（双层 kill 语义不一致，防未来误用）。

### 最优先修复（详见 05 文档）

**第一梯队（核心路径正确性 / 死锁，7 项）**
1. `src-tauri/src/commands/auth.rs:229` 嵌套读锁死锁路径（auth_get_session / refresh）
2. `src-tauri/src/commands/version.rs:147` 整个下载持 core 读锁 → 下载期间 IPC 近乎全卡
3. `crates/rmccc-version/src/file_layout.rs:119` 短 hash 切片 panic（镜像可控输入）
4. `crates/rmccc-download/src/lib.rs:173` `sha1_to_path` 无校验（缓存读写可被定向到任意路径）
5. `crates/rmccc-modloader/src/forge/new_install.rs:198` processor classpath 分隔符在**所有平台**都错（`MAIN_SEPARATOR_STR` ≠ Java classpath 分隔符）
6. OptiFine / LiteLoader 安装产物必然不可用 + Cleanroom 纯 stub 但 UI 可选（应下架或补全）
7. 三套 `inheritsFrom` 合并与三套 rules 求值语义互斥并存（收敛到 `default_game_repository` 一处）

**第二梯队（安全）**：vault key 文件权限窗口、profile/instance/layout id 零校验进路径拼接、installer 下载无 SHA 校验、`capabilities` 的 `opener:default` 过宽、`bindings.rs` release 运行时写源码树、microsoft.rs 日志未脱敏。

**第三梯队（HMCL 差距，按用户价值）**：整合包导入（.mrpack/Curse）→ CurseForge API → authlib-injector 外置登录 → assets/loader 元数据镜像贯通 → Java 自动下载。详见 04/05 文档。

### RMCL 相对 HMCL 的独有优势（应保持）

- **插件平台**：wasm 逻辑插件（独立 helper 进程 + fuel/memory 限额）+ 隔离 UI 源 + 权限/审计/回滚——HMCL 完全没有等价物。
- **AI/MCP 面**：LLM 日志流式分析、31 工具 MCP 服务器（回环 + Bearer + 两阶段确认 + 审计 fail-closed）。
- **体积与栈**：Rust + SolidJS 主 chunk 145 KB（gzip 47 KB），对照 HMCL JavaFX 数百 MB 运行时。
- **字段级继承锁**（20+ 字段 override_properties）与契约测试体系（三向命令钉死）在同类项目中少见。
