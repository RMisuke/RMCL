# RMCCC 后端问题结构化清单

> ⚠️ **已被取代（2026-10-10）**：本文档是 2026-07-31 审计的问题索引，**不含任何状态列**，因此无法用它判断
> 进度；其中多条已被后续工作修复（含被 2026-08-04 审计推翻的结论，例如 response file 清理）。
> **当前状态请以 [`PROJECT_STATUS.md`](PROJECT_STATUS.md) 为准**；本文档仅保留为问题溯源。

> **来源**：[`ARCHITECTURE_AUDIT_REPORT.md`](ARCHITECTURE_AUDIT_REPORT.md)（2026-07-31 审计）  
> **交叉验证**：[`ARCHITECTURE_IMPROVEMENT_PLAN.md`](ARCHITECTURE_IMPROVEMENT_PLAN.md) 第 2.5–2.6 节  
> **提取方法**：逐章节扫描审计报告，去重合并（重复问题标注"同 Pxx"），按严重程度降序排列  
> **标记说明**：`待确认` = 报告中信息不足以精准判定，需人工复核

---

## 一、汇总表

| 编号 | 严重程度 | 所属模块 | 一句话问题描述 | 建议批次 |
|------|----------|----------|----------------|----------|
| P01 | 🔴 Critical | `src-tauri` / launch 命令 | `launch_start` 为 357 行 God Function，不可复用不可测试 | Batch 1 |
| P02 | 🔴 Critical | `src-cli` / CLI context | `CliContext::resource_download` 200+ 行，未拆分 | Batch 1 |
| P03 | 🔴 Critical | `src-cli` / CLI context | `CliContext::launch_run` 200+ 行，未拆分 | Batch 1 |
| P04 | 🔴 Critical | `rmccc-core` / DI 容器 | `CoreContext` 是假 DI 容器，不持有 trait object | Batch 1 |
| P05 | 🔴 Critical | `rmccc-mods` / Mod 管理 | `rmccc-mods` 仅有数据类型定义，API 实现完全缺失 | Batch 1 |
| P06 | 🔴 Critical | 全局 / 测试 | 无集成测试（wiremock），Mojang/Modrinth API 未模拟 | Batch 1 |
| P07 | 🔴 Critical | 全局 / 测试 | 无 E2E 测试（tauri-driver 未配置） | Batch 1 |
| P08 | 🔴 Critical | 全局 / CI/CD | 无 GitHub Actions CI/CD，无法自动化构建测试发布 | Batch 1 |
| P09 | 🔴 Critical | 全局 / CI/CD | 无跨平台构建验证，仅开发者本地 macOS 测试 | Batch 1 |
| P10 | 🟠 High | `src-tauri` / IPC | 5 个 Mod 管理 IPC 命令完全缺失（mod_search 等） | Batch 2 |
| P11 | 🟠 High | `src-tauri` / IPC | 4 个下载控制 IPC 包装缺失（download_cancel 等） | Batch 2 |
| P12 | 🟡 Medium | `src-tauri` / launch 命令 | Tauri 命令绕过 DI 直接创建 `MojangVersionProvider` | Batch 2 |
| P13 | 🟡 Medium | `rmccc-core` / DI 容器 | `CoreContext` 不持有 `AuthProvider`，auth 逻辑分散 | Batch 2 |
| P14 | 🟡 Medium | `rmccc-profile` / 类型 | `ModLoaderConfig.loader_type` 为 `String` 而非 `ModLoaderType` | Batch 2 |
| P15 | 🟡 Medium | 多处 / Auth | Auth 刷新逻辑在 3 处重复实现 | Batch 2 |
| P16 | 🟡 Medium | `rmccc-download` / trait | `DownloadManager` trait 在设计文档中定义但代码未实现 | Batch 2 |
| P17 | 🟡 Medium | `src/ipc` / 前后端契约 | `api.ts` 中 `modloaders.list` 调用签名与后端不一致 | Batch 2 |
| P18 | 🟡 Medium | `rmccc-config` / 配置 | 配置文件无 Schema 版本号，格式变更无法自动迁移 | Batch 2 |
| P19 | 🟡 Medium | `rmccc-profile` / 存储 | JSON 文件写入无原子性保证，断电可能损坏 | Batch 2 |
| P20 | 🟡 Medium | 全局 / Auth | 单账户模式，无法切换账户 | Batch 2 |
| P21 | 🟡 Medium | 根目录 / 工具链 | 无 `rust-toolchain.toml`，CI 和开发者版本可能不一致 | Batch 2 |
| P22 | 🟡 Medium | `rmccc-modloader` / forge | `forge/install_task.rs` 每次创建新 `reqwest::Client`，不复用 | Batch 2 |
| P23 | 🟡 Medium | 全局 / 安全扫描 | 无 `cargo-deny` / `cargo-audit` 依赖漏洞扫描 | Batch 2 |
| P24 | 🟡 Medium | 全局 / 日志 | 无请求追踪 ID，跨模块调用无法关联日志 | Batch 2 |
| P25 | 🟡 Medium | `src-tauri` / 事件 | `ThrottledEmitter` 已定义但 `version_install` 中未使用 | Batch 2 |
| P26 | 🟡 Medium | 多处 / 日志安全 | 敏感字段 `access_token` 可能在日志中出现 | Batch 2 |
| P27 | 🟡 Medium | `rmccc-auth` / 安全 | `.key` 文件在 Windows 上未设置 ACL 限制 | Batch 2 |
| P28 | 🟡 Medium | `rmccc-version` / 缓存 | 无请求级缓存策略，版本列表每次完整重新获取 | Batch 2 |
| P29 | 🟡 Medium | `rmccc-download` / 下载 | 下载大文件时整个加载到内存（`bytes()`），无流式写入 | Batch 2 |
| P30 | 🟡 Medium | `rmccc-download` / 测试 | 无网络层测试：重试、断点续传、SHA1 校验未验证 | Batch 2 |
| P31 | 🟡 Medium | `rmccc-launch` / 测试 | `managed.rs` 仅有一个构造测试，无进程生命周期测试 | Batch 2 |
| P32 | 🟡 Medium | 全局 / 代码质量 | 无 `cargo clippy` CI 门禁，`#![allow(dead_code, unused_imports)]` | Batch 2 |
| P33 | 🟡 Medium | `src/ipc` / 类型桥接 | 前端类型手动维护 TypeScript interface，未使用 `ts-rs` 自动生成 | Batch 2 |
| P34 | 🟡 Medium | `src-tauri` / 更新 | `tauri-plugin-updater` 已引入但未配置自动更新端点 | Batch 3 |
| P35 | 🟢 Low | `src-tauri` / IPC | `log_get` / `log_clear` 2 个日志 IPC 命令缺失 | Batch 3 |
| P36 | 🟢 Low | `src-tauri` / IPC | `profile_import` / `profile_export` 接口缺失 | Batch 3 |
| P37 | 🟢 Low | `rmccc-config` / 配置 | 无配置热重载，修改 `config.json` 需重启应用 | Batch 3 |
| P38 | 🟢 Low | `rmccc-profile` / 校验 | Profile 的 `game_dir` 与 `config.game_root_dir` 无关联校验 | Batch 3 |
| P39 | 🟢 Low | `rmccc-auth` / 重试 | Device code flow 无超时后自动重试机制 | Batch 3 |
| P40 | 🟢 Low | `rmccc-auth` / 功能 | 无 Authlib-Injector（外置登录）支持 | Batch 3 |
| P41 | 🟢 Low | `rmccc-auth` / 设计 | `OfflineAuthProvider::authenticate()` 返回错误而非实现 | Batch 3 |
| P42 | 🟢 Low | `src-tauri` / 配置 | `tauri.conf.json` 中 `macOSPrivateApi: true` 可能影响商店审核 | Batch 3 |
| P43 | 🟢 Low | 多处 / 代码规范 | 部分 `unwrap()` 在生产代码中（如 `microsoft.rs:81`） | Batch 3 |
| P44 | 🟢 Low | `rmccc-error` / 错误码 | `ErrorCode` 缺少 `VersionNotInstalled` 等语义码 | Batch 3 |
| P45 | 🟢 Low | `src-tauri` / launch | `launch_start` 输出读取使用 `from_utf8_lossy`，可能乱码 | Batch 3 |
| P46 | 🟢 Low | 全局 / 输入校验 | 无输入长度限制，username 可任意长 | Batch 3 |
| P47 | 🟢 Low | 全局 / 性能 | 无数据库连接池（当前 JSON 文件，未来若迁移 SQLite 需要） | Batch 3 |
| P48 | 🟢 Low | `src-tauri` / 并发 | 无并发启动限制，理论可同时起 N 个 MC 实例耗尽内存 | Batch 3 |
| P49 | 🟢 Low | `src-tauri` / launch | 响应文件在进程退出后才清理，若 crash 可能残留 | Batch 3 |
| P50 | 🟢 Low | 全局 / 运维 | 无日志聚合方案，日志仅写本地文件 | Batch 3 |
| P51 | 🟢 Low | 根目录 / 构建 | `pnpm-lock.yaml` 存在但 pnpm 版本管理不严格 | Batch 3 |

> **共计 51 个问题**：Critical 9 个，High 2 个，Medium 23 个，Low 17 个

---

## 二、详细问题清单

### 🔴 Critical（严重 — 9 个）

---

#### P01 — `launch_start` God Function（357 行单体函数）

| 字段 | 内容 |
|------|------|
| **严重程度** | 🔴 Critical |
| **所属模块** | `src-tauri` / launch 命令 |
| **问题描述** | `launch_start` 在 [`src-tauri/src/commands/launch.rs:45-320`](src-tauri/src/commands/launch.rs:45) 中实现了 profile 加载 → 版本元数据获取 → Java 检测 → classpath 构建 → auth session 加载 → 参数构建 → response file 创建 → 进程启动 → stdout/stderr 读取 → 进程监控 → 清理 等全部逻辑，严重违反单一职责原则。审计报告称"这些应该拆分为 `LaunchPipeline` 编排函数"。 |
| **涉及文件** | [`src-tauri/src/commands/launch.rs:45-320`](src-tauri/src/commands/launch.rs:45) |
| **影响范围** | 游戏启动核心流程不可复用、不可单元测试；任何启动逻辑变更都需修改此巨型函数；合并冲突风险高；Tauri 命令层与 CLI 层各自实现启动逻辑导致不一致 |
| **修复建议** | 提取 `LaunchPipeline` 编排函数到 `rmccc-core/src/pipeline.rs`，拆分为：`resolve_profile` → `resolve_version` → `detect_java` → `build_classpath` → `prepare_auth` → `build_launch_args` → `spawn_managed_process` → `monitor_process`。Tauri 命令处理器仅负责参数提取和 IPC 响应 |
| **前置依赖** | P04（CoreContext DI 容器完善后 pipeline 才有注入点） |

---

#### P02 — `CliContext::resource_download` 200+ 行方法

| 字段 | 内容 |
|------|------|
| **严重程度** | 🔴 Critical |
| **所属模块** | `src-cli` / CLI context |
| **问题描述** | [`src-cli/src/context.rs:113-258`](src-cli/src/context.rs:113) 中 `resource_download` 方法承载了版本解析、资源下载、库文件下载、进度回调等全部逻辑，约 145 行，与 CLI 层耦合过深。审计报告标记为严重。 |
| **涉及文件** | [`src-cli/src/context.rs:113-258`](src-cli/src/context.rs:113) |
| **影响范围** | CLI 资源下载逻辑无法复用到 Tauri；进度回调硬编码为 CLI 格式 |
| **修复建议** | 将核心下载编排逻辑提取到 `rmccc-core/src/pipeline.rs` 中的 `install_game` 函数，进度通过 channel/trait 回调而非 `println!`。CLI 和 Tauri 各自提供进度回调实现 |
| **前置依赖** | P04（DI 容器） |

---

#### P03 — `CliContext::launch_run` 200+ 行方法

| 字段 | 内容 |
|------|------|
| **严重程度** | 🔴 Critical |
| **所属模块** | `src-cli` / CLI context |
| **问题描述** | [`src-cli/src/context.rs:409-609`](src-cli/src/context.rs:409) 中 `launch_run` 方法约 200 行，实现了与 P01 几乎相同的启动逻辑（但未使用 `ManagedProcess`），导致 CLI 和 Tauri 两套启动实现需要分别维护。 |
| **涉及文件** | [`src-cli/src/context.rs:409-609`](src-cli/src/context.rs:409) |
| **影响范围** | 修复一个平台的启动 bug 需同时修改两处；CLI 端启动无 `ManagedProcess` 封装，stdout/stderr 读取方式简陋 |
| **修复建议** | 与 P01 同步重构，统一使用 `LaunchPipeline` 编排函数 + `ManagedProcess`。删除 CLI 中重复的启动逻辑 |
| **前置依赖** | P01、P04 |

---

#### P04 — `CoreContext` 是假 DI 容器

| 字段 | 内容 |
|------|------|
| **严重程度** | 🔴 Critical |
| **所属模块** | `rmccc-core` / DI 容器 |
| **问题描述** | [`crates/rmccc-core/src/lib.rs:26-52`](crates/rmccc-core/src/lib.rs:26) 中 `CoreContext` 不持有 `AuthProvider`、`ModLoaderProvider`、`ModSource` 等 trait object，所有 provider 在调用点临时创建。审计报告与 [`ARCHITECTURE_IMPROVEMENT_PLAN.md` 第 243 行](docs/ARCHITECTURE_IMPROVEMENT_PLAN.md:243) 均标记为 P0 阻塞性问题。这直接导致：无法 mock 测试、无法替换 Provider 实现、Tauri 命令绕过 DI 直接创建对象。 |
| **涉及文件** | [`crates/rmccc-core/src/lib.rs:26-52`](crates/rmccc-core/src/lib.rs:26) |
| **影响范围** | **全局架构基石问题**。影响可测试性（无法注入 mock）、可扩展性（新 Provider 需改调用方代码）、一致性（Tauri/CLI 各自创建 provider） |
| **修复建议** | 1. 在 `CoreContext` 中添加 `auth_provider: Option<Box<dyn AuthProvider>>` 字段；2. 添加 `modloader_providers: HashMap<ModLoaderType, Box<dyn ModLoaderProvider>>`；3. 添加 `mod_sources: Vec<Box<dyn ModSource>>`；4. 所有 Tauri/CLI 命令通过 `app_state.core.xxx()` 获取 provider；5. 工厂方法支持注入（用于测试） |
| **前置依赖** | 无（这是最底层基础设施问题） |

---

#### P05 — `rmccc-mods` API 实现完全缺失

| 字段 | 内容 |
|------|------|
| **严重程度** | 🔴 Critical |
| **所属模块** | `rmccc-mods` / Mod 管理 |
| **问题描述** | [`crates/rmccc-mods/src/lib.rs`](crates/rmccc-mods/src/lib.rs) 仅定义了 `ModSearchQuery`、`ModInfo`、`ModVersion`、`ModDependency`、`ModSource` trait 等数据类型，**Modrinth API 和 CurseForge API 实现完全为空**。审计报告明确指出"Phase 4（Mod 管理）仅有数据类型定义，API 实现完全缺失"。 |
| **涉及文件** | [`crates/rmccc-mods/src/lib.rs`](crates/rmccc-mods/src/lib.rs) |
| **影响范围** | Phase 4 无法启动，前端 Mod 管理页面无法对接，P10 的 5 个 Mod IPC 命令无法实现 |
| **修复建议** | 1. 实现 `ModrinthSource` struct 及其 `ModSource` trait；2. 实现 `search_mods`、`get_mod_info`、`get_mod_versions`、`get_version_download_url`；3. 后续实现 `CurseForgeSource` |
| **前置依赖** | P04（需要 DI 容器注入 ModSource） |

---

#### P06 — 无集成测试（wiremock）

| 字段 | 内容 |
|------|------|
| **严重程度** | 🔴 Critical |
| **所属模块** | 全局 / 测试 |
| **问题描述** | 审计报告明确指出"Mojang/Modrinth/CurseForge API 未模拟"。当前所有测试均为纯单元测试，无任何外部 API 的集成测试。这意味着任何对 Mojang API 解析逻辑的修改都没有自动化安全网。 |
| **涉及文件** | 全局（需新建 `tests/` 目录及 wiremock fixtures） |
| **影响范围** | 重构风险极高；版本解析、modloader 版本列表等依赖外部 API 的逻辑修改后无法自动验证 |
| **修复建议** | 1. 添加 `wiremock` 开发依赖；2. 创建 mock fixtures：Mojang version manifest、version JSON、Forge/Fabric/NeoForge API 响应；3. 编写关键路径集成测试：`fetch_versions → fetch_version → resolve_version` 继承链；4. 测试 Forge/Fabric/NeoForge 版本列表解析 |
| **前置依赖** | 无 |

---

#### P07 — 无 E2E 测试（tauri-driver）

| 字段 | 内容 |
|------|------|
| **严重程度** | 🔴 Critical |
| **所属模块** | 全局 / 测试 |
| **问题描述** | 审计报告明确指出"`tauri-driver` 未配置"。无端到端测试意味着无法自动验证 Tauri IPC 命令从前端调用到后端执行的完整链路。 |
| **涉及文件** | 全局 |
| **影响范围** | 前后端集成回归只能手工验证；P10–P11 新增的 IPC 命令无自动化验证手段 |
| **修复建议** | 1. 配置 `tauri-driver`；2. 编写关键用户旅程 E2E：登录 → 创建实例 → 下载版本 → 安装 modloader → 启动游戏；3. 集成到 CI |
| **前置依赖** | P08（需要 CI/CD 跑 E2E） |

---

#### P08 — 无 GitHub Actions CI/CD

| 字段 | 内容 |
|------|------|
| **严重程度** | 🔴 Critical |
| **所属模块** | 全局 / CI/CD |
| **问题描述** | 审计报告指出"`.github/` 目录仅含一个 `skills/` 目录，**无 CI/CD workflow**"。无自动化构建、测试、格式检查流水线。 |
| **涉及文件** | `.github/`（需新建 `workflows/ci.yml`） |
| **影响范围** | 无法保证每次 PR 的代码质量；无法自动化多平台构建；手动测试负担重 |
| **修复建议** | 创建 `.github/workflows/ci.yml`，至少包含：1. `cargo test --workspace`；2. `cargo clippy -- -D warnings`；3. `cargo fmt --check`；4. `pnpm typecheck`；5. 三平台矩阵构建（ubuntu/macos/windows） |
| **前置依赖** | 无（可立即执行） |

---

#### P09 — 无跨平台构建验证

| 字段 | 内容 |
|------|------|
| **严重程度** | 🔴 Critical |
| **所属模块** | 全局 / CI/CD |
| **问题描述** | 审计报告明确指出"仅开发者在本地 macOS 测试"，从未在 Windows/Linux 上验证构建和运行。考虑到 `rmccc-launch` 中有大量平台特定的 Java 检测逻辑（macOS `java_home`、Windows 注册表、Linux `/usr/lib/jvm`），跨平台验证缺失是高风险的。 |
| **涉及文件** | [`crates/rmccc-launch/src/java.rs`](crates/rmccc-launch/src/java.rs)（平台特定代码）、[`rmccc-auth/src/vault.rs`](crates/rmccc-auth/src/vault.rs)（Unix 权限） |
| **影响范围** | Windows/Linux 用户可能遇到构建失败或运行时错误；`.key` 文件权限在 Windows 上不安全（见 P27） |
| **修复建议** | 在 CI 中添加 `matrix.os: [ubuntu-latest, macos-latest, windows-latest]` 构建矩阵 |
| **前置依赖** | P08（CI 配置建立后才能加矩阵） |

---

### 🟠 High（高 — 2 个）

---

#### P10 — 5 个 Mod 管理 IPC 命令完全缺失

| 字段 | 内容 |
|------|------|
| **严重程度** | 🟠 High |
| **所属模块** | `src-tauri` / IPC |
| **问题描述** | 审计报告 2.3.2 节前端契约匹配表标记 `mods.*` 全部 ❌。前端 [`src/ipc/api.ts`](src/ipc/api.ts) 已定义了 5 个 Mod 相关调用方法，但后端 19 个注册命令中无对应实现。缺失命令：`mod_search`、`mod_install`、`mod_remove`、`mod_list`、`mod_toggle`。 |
| **涉及文件** | [`src/ipc/api.ts`](src/ipc/api.ts)（前端已定义）、[`src-tauri/src/main.rs:45-65`](src-tauri/src/main.rs:45)（缺少注册） |
| **影响范围** | 前端 Mod 管理页面完全无法工作；阻塞 Phase 4 验收 |
| **修复建议** | 1. 在 `src-tauri/src/commands/` 新建 `mods.rs`；2. 实现 5 个命令处理器；3. 在 [`src-tauri/src/main.rs`](src-tauri/src/main.rs) 注册；4. 每个命令委托 `CoreContext` 的 `ModSource` trait |
| **前置依赖** | P05（rmccc-mods API 实现）、P04（DI 容器注入 ModSource） |

---

#### P11 — 4 个下载控制 IPC 包装缺失

| 字段 | 内容 |
|------|------|
| **严重程度** | 🟠 High |
| **所属模块** | `src-tauri` / IPC |
| **问题描述** | 审计报告 2.3.3 节指出 `download_cancel`、`download_cancel_batch`、`download_pause`、`download_resume` 四个前端定义的下载控制方法在后端无 IPC 包装，尽管 [`ConcurrentDownloadManager`](crates/rmccc-download/src/lib.rs:203) 已实现 `cancel_batch`、`pause`、`resume` 方法。 |
| **涉及文件** | [`src/ipc/api.ts`](src/ipc/api.ts)（前端已定义）、[`crates/rmccc-download/src/lib.rs:398-417`](crates/rmccc-download/src/lib.rs:398)（后端能力已具备） |
| **影响范围** | 前端无法取消/暂停/恢复正在进行的下载任务；用户体验受损 |
| **修复建议** | 1. 在 `src-tauri/src/commands/` 新建 `download.rs`；2. 实现 4 个 IPC 包装，接收 `batch_id` 参数；3. 通过 `AppState` 访问 `ConcurrentDownloadManager`；4. 注册到 [`main.rs`](src-tauri/src/main.rs) |
| **前置依赖** | P04（需要 AppState 持有 download_manager 引用） |

---

### 🟡 Medium（中等 — 23 个）

---

#### P12 — Tauri 命令绕过 DI 直接创建 `MojangVersionProvider`

| 字段 | 内容 |
|------|------|
| **严重程度** | 🟡 Medium |
| **所属模块** | `src-tauri` / launch 命令 |
| **问题描述** | [`src-tauri/src/commands/launch.rs:71`](src-tauri/src/commands/launch.rs:71) 中 `launch_start` 直接 `MojangVersionProvider::new(http_client)` 创建 provider，而非通过 `CoreContext` 获取。审计报告标记为中等。 |
| **涉及文件** | [`src-tauri/src/commands/launch.rs:71`](src-tauri/src/commands/launch.rs:71) |
| **影响范围** | 无法替换 VersionProvider 实现（如测试 mock、离线 stub）；与 DI 架构设计违背 |
| **修复建议** | 通过 `app_state.core.version_provider()` 获取，而非直接构造 |
| **前置依赖** | P04（DI 容器持有 VersionProvider） |

---

#### P13 — `CoreContext` 不持有 `AuthProvider`

| 字段 | 内容 |
|------|------|
| **严重程度** | 🟡 Medium |
| **所属模块** | `rmccc-core` / DI 容器 |
| **问题描述** | [`crates/rmccc-core/src/lib.rs:26`](crates/rmccc-core/src/lib.rs:26) 中 `CoreContext` 字段列表不含 `auth_provider`，auth 逻辑被迫分散在 Tauri 命令层和 CLI 层各自处理。审计报告标记此问题。 |
| **涉及文件** | [`crates/rmccc-core/src/lib.rs:26-52`](crates/rmccc-core/src/lib.rs:26) |
| **影响范围** | auth 逻辑无法通过 DI 统一管理；导致 P15（auth 刷新重复） |
| **修复建议** | 在 `CoreContext` 中添加 `auth_provider: Box<dyn AuthProvider>` 字段和 `token_vault: Arc<TokenVault>` 字段 |
| **前置依赖** | P04（是 P04 的子任务） |

---

#### P14 — `ModLoaderConfig.loader_type` 类型不安全

| 字段 | 内容 |
|------|------|
| **严重程度** | 🟡 Medium |
| **所属模块** | `rmccc-profile` / 类型 |
| **问题描述** | [`crates/rmccc-profile/src/lib.rs:31`](crates/rmccc-profile/src/lib.rs:31) 中 `ModLoaderConfig` 的 `loader_type` 字段类型为 `String`，而 `rmccc-modloader` 已定义了 [`ModLoaderType` 枚举](crates/rmccc-modloader/src/lib.rs:53)。使用 String 失去了编译期类型校验，可能写入无效值。审计报告两处提及（2.2.2 #2 和 2.4.2 #4）。 |
| **涉及文件** | [`crates/rmccc-profile/src/lib.rs:31`](crates/rmccc-profile/src/lib.rs:31)、[`crates/rmccc-modloader/src/lib.rs:53`](crates/rmccc-modloader/src/lib.rs:53) |
| **影响范围** | 运行时可能遇到无法解析的 loader_type 字符串；序列化/反序列化不一致风险 |
| **修复建议** | 将 `loader_type` 改为 `ModLoaderType`，为 `ModLoaderType` 添加 `Serialize`/`Deserialize` derive，确保 JSON 序列化兼容 |
| **前置依赖** | 无 |

---

#### P15 — Auth 刷新逻辑在 3 处重复实现

| 字段 | 内容 |
|------|------|
| **严重程度** | 🟡 Medium |
| **所属模块** | 多处 / Auth |
| **问题描述** | Token 刷新逻辑在三处独立实现：1. [`src-tauri/src/commands/auth.rs:8-49`](src-tauri/src/commands/auth.rs:8) 的 `try_refresh_if_needed()`；2. [`src-cli/src/context.rs:59-78`](src-cli/src/context.rs:59) 的 `auth_refresh()`；3. [`crates/rmccc-auth/src/microsoft.rs:405-470`](crates/rmccc-auth/src/microsoft.rs:405) 的 `MicrosoftAuthProvider::refresh()`。审计报告 2.2.2 #3 和 2.5.2 #3 均标记。 |
| **涉及文件** | [`src-tauri/src/commands/auth.rs:8-49`](src-tauri/src/commands/auth.rs:8)、[`src-cli/src/context.rs:59-78`](src-cli/src/context.rs:59)、[`crates/rmccc-auth/src/microsoft.rs:405-470`](crates/rmccc-auth/src/microsoft.rs:405) |
| **影响范围** | 修改刷新策略需同步三处；不一致风险（如一处加了重试另外两处没加） |
| **修复建议** | 在 `CoreContext` 中添加 `refresh_session_if_needed()` 统一方法，Tauri 和 CLI 都调用此方法。删除 `try_refresh_if_needed` 和 CLI 中重复逻辑 |
| **前置依赖** | P04、P13 |

---

#### P16 — `DownloadManager` trait 在设计文档中定义但代码未实现

| 字段 | 内容 |
|------|------|
| **严重程度** | 🟡 Medium |
| **所属模块** | `rmccc-download` / trait |
| **问题描述** | [`ARCHITECTURE.md`](docs/ARCHITECTURE.md) 中定义了 `DownloadManager` trait，但 [`ConcurrentDownloadManager`](crates/rmccc-download/src/lib.rs:203) 未实现任何 trait。审计报告 2.2.2 #5 和 [`ARCHITECTURE_IMPROVEMENT_PLAN.md` 第 277 行](docs/ARCHITECTURE_IMPROVEMENT_PLAN.md:277) 均标记。 |
| **涉及文件** | [`crates/rmccc-download/src/lib.rs:203`](crates/rmccc-download/src/lib.rs:203)、[`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md)（trait 定义） |
| **影响范围** | 无法替换下载引擎实现；无法 mock 测试下载逻辑；与架构设计文档不一致 |
| **修复建议** | 1. 在 `rmccc-download` 中定义 `DownloadManager` trait（含 `download_batch`、`cancel_batch`、`pause`、`resume`、`wait_for_batch`）；2. 为 `ConcurrentDownloadManager` 实现该 trait；3. `CoreContext` 通过 `Box<dyn DownloadManager>` 持有 |
| **前置依赖** | P04 |

---

#### P17 — `api.ts` 中 `modloaders.list` 调用签名与后端不一致

| 字段 | 内容 |
|------|------|
| **严重程度** | 🟡 Medium |
| **所属模块** | `src/ipc` / 前后端契约 |
| **问题描述** | 审计报告 2.3.3 修正说明：`modloader_list` 和 `modloader_install` 已在 [`main.rs:63-64`](src-tauri/src/main.rs:63) 注册，但"前端 `api.ts` 中 `modloaders.list` 调用方式与后端签名不完全匹配（后端需要 `loader_type: String, game_version: String`）"。 |
| **涉及文件** | [`src/ipc/api.ts`](src/ipc/api.ts)、[`src-tauri/src/commands/modloader.rs:9-23`](src-tauri/src/commands/modloader.rs:9) |
| **影响范围** | 前端调用 `modloaders.list` 可能因参数名/类型不匹配而失败 |
| **修复建议** | 核对 `api.ts` 中 `modloaders.list(loaderType, gameVersion)` 的参数名与后端 `modloader_list` 的 Tauri 自动转换规则是否兼容，如有不一致则修正前端调用签名 |
| **前置依赖** | 无（可独立修复） |

---

#### P18 — 配置文件无 Schema 版本号

| 字段 | 内容 |
|------|------|
| **严重程度** | 🟡 Medium |
| **所属模块** | `rmccc-config` / 配置 |
| **问题描述** | [`crates/rmccc-config/src/lib.rs:14`](crates/rmccc-config/src/lib.rs:14) 中 `AppConfig` 无 `version` 字段。当未来配置格式变更时，无法自动迁移旧格式，用户可能遇到反序列化错误。审计报告 2.4.2 #1 标记。 |
| **涉及文件** | [`crates/rmccc-config/src/lib.rs:14-41`](crates/rmccc-config/src/lib.rs:14) |
| **影响范围** | 未来配置格式升级时用户需手动删除 `config.json`；升级体验差 |
| **修复建议** | 1. 在 `AppConfig` 添加 `version: u32` 字段，当前值设为 `1`；2. 实现 `ConfigMigrator`，在 `load()` 时检测版本并逐级迁移；3. `save()` 时写入最新版本号 |
| **前置依赖** | 无 |

---

#### P19 — JSON 文件写入无原子性保证

| 字段 | 内容 |
|------|------|
| **严重程度** | 🟡 Medium |
| **所属模块** | `rmccc-profile` / 存储 |
| **问题描述** | [`crates/rmccc-profile/src/lib.rs:89`](crates/rmccc-profile/src/lib.rs:89) 中 `ProfileStore::save()` 直接使用 `fs::write` 写入，若写入过程中断电或崩溃，文件可能损坏（半写）。审计报告 2.4.2 #2 标记。 |
| **涉及文件** | [`crates/rmccc-profile/src/lib.rs:89`](crates/rmccc-profile/src/lib.rs:89)、[`crates/rmccc-config/src/lib.rs:78`](crates/rmccc-config/src/lib.rs:78)（`AppConfig::save` 同样问题） |
| **影响范围** | `config.json` 和 `profiles/{id}.json` 损坏导致配置丢失或启动失败 |
| **修复建议** | 使用"写临时文件 → `fs::rename`"模式（同一文件系统内 rename 是原子操作）：1. `serde_json::to_vec` 序列化到内存；2. `fs::write(temp_path, data)`；3. `fs::rename(temp_path, target_path)` |
| **前置依赖** | 无 |

---

#### P20 — 单账户模式

| 字段 | 内容 |
|------|------|
| **严重程度** | 🟡 Medium |
| **所属模块** | 全局 / Auth |
| **问题描述** | 审计报告 2.5.2 #1：当前仅支持一个活跃 session（`auth.json` 单文件存储），无法切换账户。对于需要多个 Minecraft 账户的用户（如主账号和小号），这是一个功能缺失。 |
| **涉及文件** | [`crates/rmccc-auth/src/vault.rs`](crates/rmccc-auth/src/vault.rs)（单文件存储）、[`src-tauri/src/commands/auth.rs`](src-tauri/src/commands/auth.rs)（单 session 管理） |
| **影响范围** | 多账户用户无法切换；`auth_microsoft_login` 会覆盖已有 session |
| **修复建议** | 1. 将 `auth.json` 改为 `accounts/{uuid}.json` 目录结构；2. 维护 `active_account` 字段指向当前使用的账户 UUID；3. 添加 `auth_switch_account` IPC 命令 |
| **前置依赖** | 无 |

---

#### P21 — 无 `rust-toolchain.toml`

| 字段 | 内容 |
|------|------|
| **严重程度** | 🟡 Medium |
| **所属模块** | 根目录 / 工具链 |
| **问题描述** | 审计报告 2.6.2 #1：根目录无 `rust-toolchain.toml`，CI 和开发者可能使用不同 Rust 版本，导致编译结果不一致。 |
| **涉及文件** | 根目录（需新建 `rust-toolchain.toml`） |
| **影响范围** | CI 构建失败难以复现；新贡献者可能使用不兼容的 Rust 版本 |
| **修复建议** | 创建 `rust-toolchain.toml`，指定 `channel = "stable"` 并锁定最小支持的 Rust 版本；CI 中通过 `rustup show` 验证 |
| **前置依赖** | 无 |

---

#### P22 — `forge/install_task.rs` 每次创建新 `reqwest::Client`

| 字段 | 内容 |
|------|------|
| **严重程度** | 🟡 Medium |
| **所属模块** | `rmccc-modloader` / forge |
| **问题描述** | 审计报告 2.6.2 #3：`forge/install_task.rs:112`（实际位于 [`crates/rmccc-modloader/src/forge/install_task.rs:112`](crates/rmccc-modloader/src/forge/install_task.rs:112)）的 `download_installer()` 函数内每次调用都创建新的 `reqwest::Client`，而非复用已有的连接池。 |
| **涉及文件** | [`crates/rmccc-modloader/src/forge/install_task.rs:112`](crates/rmccc-modloader/src/forge/install_task.rs:112)、[`crates/rmccc-modloader/src/neoforge/install_task.rs:92`](crates/rmccc-modloader/src/neoforge/install_task.rs:92)（同样问题） |
| **影响范围** | 重复 TLS 握手，下载性能下降；资源泄漏风险 |
| **修复建议** | 将 `reqwest::Client` 通过参数传入 `download_installer()`（或从 `CoreContext` 获取），复用全局连接池 |
| **前置依赖** | 无 |

---

#### P23 — 无 `cargo-deny` / `cargo-audit` 依赖漏洞扫描

| 字段 | 内容 |
|------|------|
| **严重程度** | 🟡 Medium |
| **所属模块** | 全局 / 安全扫描 |
| **问题描述** | 审计报告 2.6.2 #4：项目无依赖漏洞扫描工具集成。依赖项（如 `reqwest`、`tokio`、`zip`）可能存在已知 CVE，需要持续监控。 |
| **涉及文件** | CI 配置（需添加 `cargo-audit` step） |
| **影响范围** | 使用存在已知漏洞的依赖版本可能导致安全风险 |
| **修复建议** | 1. 在 CI 中添加 `cargo install cargo-audit && cargo audit`；2. 也可添加 `cargo deny check` 检查许可证合规 |
| **前置依赖** | P08（需 CI 配置） |

---

#### P24 — 无请求追踪 ID

| 字段 | 内容 |
|------|------|
| **严重程度** | 🟡 Medium |
| **所属模块** | 全局 / 日志 |
| **问题描述** | 审计报告 2.7.2 #1：当前 tracing span 未携带请求级追踪 ID（如 `x-request-id`），当一次用户操作跨越多个模块调用时（如"启动游戏"涉及 profile→version→auth→launch），无法通过一个 ID 串联所有日志。 |
| **涉及文件** | 所有 IPC 命令处理器、`CoreContext` |
| **影响范围** | 生产环境问题排查困难；无法按用户操作维度聚合日志 |
| **修复建议** | 1. 在 IPC 命令入口生成 `request_id: Uuid`；2. 通过 `Span::current()` 的 field 传播；3. 在所有 `#[tracing::instrument]` 中包含 `request_id` 字段；4. JSON 日志中输出 `request_id` |
| **前置依赖** | 无 |

---

#### P25 — `ThrottledEmitter` 已定义但未在 `version_install` 中使用

| 字段 | 内容 |
|------|------|
| **严重程度** | 🟡 Medium |
| **所属模块** | `src-tauri` / 事件 |
| **问题描述** | 审计报告 2.7.2 #2：`ThrottledEmitter` 在 [`src-tauri/src/events.rs:47`](src-tauri/src/events.rs:47) 已完整实现（100ms 限频），但 [`version_install`](src-tauri/src/commands/version.rs:116) 中使用空回调 `|_| {}`，未实际发射下载进度事件到前端。 |
| **涉及文件** | [`src-tauri/src/events.rs:47-80`](src-tauri/src/events.rs:47)（已实现）、[`src-tauri/src/commands/version.rs:116-118`](src-tauri/src/commands/version.rs:116)（空回调） |
| **影响范围** | 前端看不到下载进度条；用户体验受损 |
| **修复建议** | 1. 在 `version_install` 和 `modloader_install` 中创建 `ThrottledEmitter` 实例；2. 传入 `progress_callback` 闭包调用 `emitter.emit_progress()`；3. 确保前端监听 `download://progress` 事件 |
| **前置依赖** | 无 |

---

#### P26 — 敏感字段 `access_token` 可能在日志中出现

| 字段 | 内容 |
|------|------|
| **严重程度** | 🟡 Medium |
| **所属模块** | 多处 / 日志安全 |
| **问题描述** | 审计报告 2.7.2 #5：`#[tracing::instrument]` 宏会默认记录所有函数参数。若某函数参数包含 `AuthSession`（含 `access_token` 字段），则该 token 会出现在结构化日志中。审计报告指出应使用 `skip` 标记 token 字段。 |
| **涉及文件** | [`src-tauri/src/commands/auth.rs`](src-tauri/src/commands/auth.rs)、[`src-cli/src/context.rs`](src-cli/src/context.rs) 中所有接受 `AuthSession` 的函数 |
| **影响范围** | 日志文件泄露 access_token，安全风险 |
| **修复建议** | 1. 对所有接受 `AuthSession` 的 `#[tracing::instrument]` 添加 `skip(session)`；2. 为 `AuthSession` 实现自定义 `Debug`（屏蔽 `access_token`）；3. 日志审计脚本检查 JSON 日志中是否出现 `access_token` |
| **前置依赖** | 无 |

---

#### P27 — `.key` 文件在 Windows 上未设置 ACL 限制

| 字段 | 内容 |
|------|------|
| **严重程度** | 🟡 Medium |
| **所属模块** | `rmccc-auth` / 安全 |
| **问题描述** | 审计报告 2.8.2 #1：[`vault.rs:43-47`](crates/rmccc-auth/src/vault.rs:43) 中 `TokenVault::init()` 仅在 Unix 平台设置 `0o600` 权限（通过 `std::fs::set_permissions`），Windows 上无等效的 ACL 限制。任何同机用户可读取 key 文件。 |
| **涉及文件** | [`crates/rmccc-auth/src/vault.rs:43-47`](crates/rmccc-auth/src/vault.rs:43) |
| **影响范围** | Windows 平台上密钥文件可能被其他用户进程读取；跨平台安全不一致 |
| **修复建议** | 在 Windows 上使用 `std::os::windows::fs::OpenOptionsExt` 或调用 `icacls` 设置文件权限为当前用户独占 |
| **前置依赖** | 无 |

---

#### P28 — 无请求级缓存策略

| 字段 | 内容 |
|------|------|
| **严重程度** | 🟡 Medium |
| **所属模块** | `rmccc-version` / 缓存 |
| **问题描述** | 审计报告 2.9.2 #2：[`VersionCache`](crates/rmccc-version/src/cache.rs) 仅有基于文件时间戳的 TTL 缓存，无 HTTP 条件请求（`If-None-Match` / `If-Modified-Since`）支持。每次获取版本列表完整下载 manifest JSON，即使内容未变化。 |
| **涉及文件** | [`crates/rmccc-version/src/cache.rs`](crates/rmccc-version/src/cache.rs)、[`crates/rmccc-version/src/manifest.rs:29-83`](crates/rmccc-version/src/manifest.rs:29) |
| **影响范围** | 网络带宽浪费；启动时版本列表加载延迟 |
| **修复建议** | 1. 在 `VersionCache` 中存储 `ETag` 或 `Last-Modified`；2. `fetch_versions()` 发送条件请求；3. 304 响应时使用缓存 |
| **前置依赖** | 无 |

---

#### P29 — 下载大文件时整个加载到内存

| 字段 | 内容 |
|------|------|
| **严重程度** | 🟡 Medium |
| **所属模块** | `rmccc-download` / 下载 |
| **问题描述** | 审计报告 2.9.2 #3：[`download_single()`](crates/rmccc-download/src/lib.rs:594) 使用 `response.bytes().await?` 将整个响应体加载到内存后再写入文件。对于大型 Minecraft 资源文件（assets 可达数百 MB），这会导致内存峰值过高。 |
| **涉及文件** | [`crates/rmccc-download/src/lib.rs:594`](crates/rmccc-download/src/lib.rs:594) |
| **影响范围** | 内存峰值过高（下载 200MB 文件时内存 +200MB）；无法边下载边写盘 |
| **修复建议** | 使用 `response.chunk()` 流式读取 + `tokio::io::AsyncWriteExt::write_all` 分块写入，在写入过程中同时计算 SHA1 |
| **前置依赖** | 无 |

---

#### P30 — `rmccc-download` 无网络层测试

| 字段 | 内容 |
|------|------|
| **严重程度** | 🟡 Medium |
| **所属模块** | `rmccc-download` / 测试 |
| **问题描述** | 审计报告 2.10.2 #3：下载模块的核心功能——重试机制、断点续传（HTTP Range）、SHA1 校验——均未在自动化测试中验证。现有测试仅覆盖了 `RetryConfig` 和 `all_urls()` 辅助函数。 |
| **涉及文件** | [`crates/rmccc-download/src/lib.rs:625-655`](crates/rmccc-download/src/lib.rs:625)（现有测试仅 3 个） |
| **影响范围** | 下载重试/续传逻辑修改后无自动化验证；回归风险 |
| **修复建议** | 使用 `wiremock` 模拟：1. 服务器返回 500 → 重试 → 200；2. 服务器支持 Range → 续传验证；3. 服务器返回错误 SHA1 → 重下载验证 |
| **前置依赖** | P06（需 wiremock 集成测试框架） |

---

#### P31 — `managed.rs` 测试极简

| 字段 | 内容 |
|------|------|
| **严重程度** | 🟡 Medium |
| **所属模块** | `rmccc-launch` / 测试 |
| **问题描述** | 审计报告 2.10.2 #4：[`managed.rs:168`](crates/rmccc-launch/src/managed.rs:168) 仅有一个 `test_managed_process_creation` 测试（验证构造不 panic），无进程启动、stdout/stderr 捕获、kill、recent_lines 等功能测试。 |
| **涉及文件** | [`crates/rmccc-launch/src/managed.rs:168-187`](crates/rmccc-launch/src/managed.rs:168) |
| **影响范围** | `ManagedProcess` 的竞态条件（如已修复的 Bug #5）可能回归 |
| **修复建议** | 添加集成测试：启动一个 `echo` 子进程 → 验证 stdout 捕获 → `recent_lines()` → `kill()` → 验证退出码 |
| **前置依赖** | 无 |

---

#### P32 — 无 `cargo clippy` CI 门禁

| 字段 | 内容 |
|------|------|
| **严重程度** | 🟡 Medium |
| **所属模块** | 全局 / 代码质量 |
| **问题描述** | 审计报告 2.10.2 #5：无 clippy 检查集成到 CI，且 [`src-tauri/src/main.rs:1`](src-tauri/src/main.rs:1) 存在 `#![allow(dead_code, unused_imports)]` —— 这个属性掩盖了潜在的死代码和未使用导入。 |
| **涉及文件** | [`src-tauri/src/main.rs:1`](src-tauri/src/main.rs:1)、CI 配置 |
| **影响范围** | 死代码累积；代码质量无自动化约束；潜在 bug 被 allow 属性掩盖 |
| **修复建议** | 1. 移除 `#![allow(dead_code, unused_imports)]`；2. 修复所有 clippy warning；3. 在 CI 中添加 `cargo clippy -- -D warnings`；4. 逐步启用 `clippy::pedantic` 规则 |
| **前置依赖** | P08（需 CI 配置） |

---

#### P33 — 前端类型手动维护，未使用 `ts-rs` 自动生成

| 字段 | 内容 |
|------|------|
| **严重程度** | 🟡 Medium |
| **所属模块** | `src/ipc` / 类型桥接 |
| **问题描述** | 审计报告 2.10.2 #6：尽管 `ts-rs` 已在项目依赖中，但 [`src/ipc/api.ts:10-87`](src/ipc/api.ts:10) 中的 TypeScript interface（`AuthSession`、`Profile`、`VersionInfo` 等）完全手动维护，与 Rust struct 不同步。`ARCHITECTURE.md` 第 9.3 节设计的自动类型生成流程未实现。 |
| **涉及文件** | [`src/ipc/api.ts:10-87`](src/ipc/api.ts:10)（手动类型）、所有 IPC 相关的 Rust struct |
| **影响范围** | Rust struct 增减字段时前端类型不同步，导致运行时反序列化错误；维护成本高 |
| **修复建议** | 1. 为所有 IPC 相关 struct 添加 `#[derive(TS)]`；2. 在 `src-tauri/build.rs` 中配置 `ts-rs` 导出到 `src/ipc/types.ts`；3. `api.ts` 从 `types.ts` 导入；4. 添加 CI 检查（`cargo test` 后 diff 检查生成文件是否最新） |
| **前置依赖** | 无 |

---

#### P34 — `tauri-plugin-updater` 已引入但未配置

| 字段 | 内容 |
|------|------|
| **严重程度** | 🟡 Medium |
| **所属模块** | `src-tauri` / 更新 |
| **问题描述** | 审计报告 2.11.2 #3：[`src-tauri/Cargo.toml:35`](src-tauri/Cargo.toml:35) 已引入 `tauri-plugin-updater` 依赖，但未配置更新端点（`endpoints`）、签名公钥等，自动更新功能不可用。 |
| **涉及文件** | [`src-tauri/Cargo.toml:35`](src-tauri/Cargo.toml:35)、[`src-tauri/tauri.conf.json`](src-tauri/tauri.conf.json) |
| **影响范围** | 用户无法自动更新到新版本；需手动下载安装 |
| **修复建议** | 1. 在 `tauri.conf.json` 的 `plugins.updater` 中配置 `endpoints` 指向 GitHub Releases API；2. 配置 `pubkey` 签名验证；3. 前端添加更新提示 UI |
| **前置依赖** | P08（CI 自动发布 Release） |

---

### 🟢 Low（低 — 17 个）

---

#### P35 — `log_get` / `log_clear` 2 个日志 IPC 命令缺失

| 字段 | 内容 |
|------|------|
| **严重程度** | 🟢 Low |
| **所属模块** | `src-tauri` / IPC |
| **问题描述** | 审计报告 2.3.3 节：前端 `api.ts` 定义了 `logs.get()` 和 `logs.clear()`，后端无 IPC 包装。日志文件已存在于磁盘但无法通过前端读取。 |
| **涉及文件** | [`src/ipc/api.ts`](src/ipc/api.ts)（前端已定义）、[`src-tauri/src/main.rs`](src-tauri/src/main.rs)（缺少注册） |
| **影响范围** | 前端无法展示运行日志；用户需手动打开日志文件 |
| **修复建议** | 在 `src-tauri/src/commands/` 新建 `logs.rs`，实现 `log_get`（读取最近 N 行）和 `log_clear`（清空日志文件），注册到 `main.rs` |
| **前置依赖** | 无 |

---

#### P36 — `profile_import` / `profile_export` 接口缺失

| 字段 | 内容 |
|------|------|
| **严重程度** | 🟢 Low |
| **所属模块** | `src-tauri` / IPC |
| **问题描述** | 审计报告 2.3.3 节：`ARCHITECTURE.md` / `DEVELOPMENT_PLAN.md` 中计划的实例导入导出功能未实现。 |
| **涉及文件** | 需新建命令处理器 |
| **影响范围** | 用户无法迁移实例到其他设备 |
| **修复建议** | 实现 `profile_export(id) → JSON 文件路径` 和 `profile_import(json_path) → Profile` |
| **前置依赖** | 无 |

---

#### P37 — 无配置热重载

| 字段 | 内容 |
|------|------|
| **严重程度** | 🟢 Low |
| **所属模块** | `rmccc-config` / 配置 |
| **问题描述** | 审计报告 2.4.2 #3：修改 `config.json` 后需重启应用才能生效。 |
| **涉及文件** | [`crates/rmccc-config/src/lib.rs`](crates/rmccc-config/src/lib.rs) |
| **影响范围** | 用户体验差（修改设置需重启） |
| **修复建议** | 通过 `notify` crate 监听 `config.json` 变更事件，自动重新加载并发射 `config://changed` 事件通知前端 |
| **前置依赖** | 无 |

---

#### P38 — Profile 的 `game_dir` 与 `config.game_root_dir` 无关联校验

| 字段 | 内容 |
|------|------|
| **严重程度** | 🟢 Low |
| **所属模块** | `rmccc-profile` / 校验 |
| **问题描述** | 审计报告 2.4.2 #5：创建 Profile 时若 `game_dir` 不在 `game_root_dir` 下，无任何校验或警告。 |
| **涉及文件** | [`crates/rmccc-profile/src/lib.rs`](crates/rmccc-profile/src/lib.rs) |
| **影响范围** | 用户可能将实例创建到意外位置；磁盘空间管理混乱 |
| **修复建议** | 在 `save()` 或创建时检查 `game_dir` 是否以 `game_root_dir` 为前缀，不匹配时返回错误或警告 |
| **前置依赖** | 无 |

---

#### P39 — Device code flow 无超时后自动重试

| 字段 | 内容 |
|------|------|
| **严重程度** | 🟢 Low |
| **所属模块** | `rmccc-auth` / 重试 |
| **问题描述** | 审计报告 2.5.2 #2：[`microsoft.rs:99-167`](crates/rmccc-auth/src/microsoft.rs:99) 的 `poll_for_token()` 在设备码过期（`authorization_pending` 超时）后返回错误，不会自动重新请求新的 device code。 |
| **涉及文件** | [`crates/rmccc-auth/src/microsoft.rs:99-167`](crates/rmccc-auth/src/microsoft.rs:99) |
| **影响范围** | 用户设备码过期后需手动重新触发登录 |
| **修复建议** | 在 `authenticate()` 外层添加重试循环：`authorization_pending` 超时后自动 `request_device_code()` 并重新开始轮询（最多 N 次） |
| **前置依赖** | 无 |

---

#### P40 — 无 Authlib-Injector（外置登录）支持

| 字段 | 内容 |
|------|------|
| **严重程度** | 🟢 Low |
| **所属模块** | `rmccc-auth` / 功能 |
| **问题描述** | 审计报告 2.5.2 #4：不支持 Authlib-Injector 外置登录（国内部分服务器使用）。审计标注为"远期需求"。 |
| **涉及文件** | `rmccc-auth`（需新增 `authlib.rs` 或类似模块） |
| **影响范围** | 使用外置登录的用户无法通过本启动器登录 |
| **修复建议** | 实现 `AuthlibInjectorAuthProvider`，支持自定义 `auth_server` URL 和 `Yggdrasil` 协议 |
| **前置依赖** | 无 |

---

#### P41 — `OfflineAuthProvider::authenticate()` 返回错误而非实现

| 字段 | 内容 |
|------|------|
| **严重程度** | 🟢 Low |
| **所属模块** | `rmccc-auth` / 设计 |
| **问题描述** | 审计报告 2.5.2 #5：[`offline.rs:40-49`](crates/rmccc-auth/src/offline.rs:40) 中 `OfflineAuthProvider::authenticate()` 返回 `Err(RmcccError::Auth("Offline mode requires a username".into()))`，而不接受 `username` 参数。这迫使调用方使用 `create_offline_session()` 函数绕过 `AuthProvider` trait。设计不合理。 |
| **涉及文件** | [`crates/rmccc-auth/src/offline.rs:40-49`](crates/rmccc-auth/src/offline.rs:40) |
| **影响范围** | `AuthProvider` trait 的 `authenticate()` 对离线模式形同虚设；调用方需判断 provider 类型分支处理 |
| **修复建议** | 1. 考虑修改 `AuthProvider::authenticate()` trait 方法签名，接受可选的 `username: Option<&str>`；2. 或在 `OfflineAuthProvider` 中通过构造参数传入 username |
| **前置依赖** | 无 |

---

#### P42 — `macOSPrivateApi: true` 可能影响 Mac App Store 审核

| 字段 | 内容 |
|------|------|
| **严重程度** | 🟢 Low |
| **所属模块** | `src-tauri` / 配置 |
| **问题描述** | 审计报告 2.6.2 #2：[`tauri.conf.json:30`](src-tauri/tauri.conf.json:30) 中 `macOSPrivateApi: true` 启用了私有 API 访问。这在直接分发时无问题，但若未来计划上架 Mac App Store，可能导致审核被拒。 |
| **涉及文件** | [`src-tauri/tauri.conf.json:30`](src-tauri/tauri.conf.json:30) |
| **影响范围** | Mac App Store 上架（远期计划）可能受阻 |
| **修复建议** | 评估是否实际需要 `macOSPrivateApi`（如 `NSWindow` 透明标题栏）。如果不需要则设为 `false`；如果需要则保留并备注"仅直接分发，不上架 MAS" |
| **前置依赖** | 无 |

---

#### P43 — 部分 `unwrap()` 在生产代码中

| 字段 | 内容 |
|------|------|
| **严重程度** | 🟢 Low |
| **所属模块** | 多处 / 代码规范 |
| **问题描述** | 审计报告 2.7.2 #3：多处使用了 `unwrap_or_default()` 等可能掩盖错误的方法。审计报告以 [`microsoft.rs:81`](crates/rmccc-auth/src/microsoft.rs:81) 为例，但指出"实际安全但不够规范"。 |
| **涉及文件** | [`crates/rmccc-auth/src/microsoft.rs:81`](crates/rmccc-auth/src/microsoft.rs:81) 等 |
| **影响范围** | 代码可读性和维护性降低；不易区分"确信安全的 unwrap"和"潜在的 bug" |
| **修复建议** | 逐步替换 `unwrap()` / `unwrap_or_default()` 为 `.ok_or_else(|| RmcccError::internal("..."))?` 或添加 `// SAFETY: ...` 注释 |
| **前置依赖** | 无 |

---

#### P44 — `ErrorCode` 缺少语义化错误码

| 字段 | 内容 |
|------|------|
| **严重程度** | 🟢 Low |
| **所属模块** | `rmccc-error` / 错误码 |
| **问题描述** | 审计报告 2.7.2 #4：[`ErrorCode`](crates/rmccc-error/src/lib.rs:94) 枚举缺少 `VersionNotInstalled`、`JavaNotFound`、`DiskSpaceInsufficient` 等语义化变体，前端难以根据错误码做用户友好的提示。 |
| **涉及文件** | [`crates/rmccc-error/src/lib.rs:94-110`](crates/rmccc-error/src/lib.rs:94) |
| **影响范围** | 前端错误提示不够精确 |
| **修复建议** | 添加 `VersionNotInstalled`、`JavaNotFound`、`DiskSpaceInsufficient`、`ModLoaderInstallFailed`、`NetworkTimeout` 等语义码 |
| **前置依赖** | 无 |

---

#### P45 — `launch_start` 输出读取使用 `from_utf8_lossy`

| 字段 | 内容 |
|------|------|
| **严重程度** | 🟢 Low |
| **所属模块** | `src-tauri` / launch |
| **问题描述** | 审计报告 2.8.2 #2：[`launch.rs:248`](src-tauri/src/commands/launch.rs:248) 使用 `String::from_utf8_lossy()` 读取游戏进程 stdout/stderr，可能产生乱码但不影响功能。 |
| **涉及文件** | [`src-tauri/src/commands/launch.rs:248`](src-tauri/src/commands/launch.rs:248) |
| **影响范围** | 游戏日志中非 UTF-8 字符显示为 � |
| **修复建议** | 使用 `std::str::from_utf8()` 匹配，对无效 UTF-8 进行 base64 编码或替换为 `\u{FFFD}` |
| **前置依赖** | P01（重构 `launch_start` 时一并修复） |

---

#### P46 — 无输入长度限制

| 字段 | 内容 |
|------|------|
| **严重程度** | 🟢 Low |
| **所属模块** | 全局 / 输入校验 |
| **问题描述** | 审计报告 2.8.2 #3：username、instance name 等用户输入无长度限制。虽不直接影响安全，但极端输入可能影响 UI 布局或存储。 |
| **涉及文件** | `profile_create`、`auth_offline_login` 等命令处理器 |
| **影响范围** | 极长用户名导致 UI 溢出或文件系统路径过长 |
| **修复建议** | 在 IPC 命令入口添加输入校验：username ≤ 32 字符，instance name ≤ 64 字符等 |
| **前置依赖** | 无 |

---

#### P47 — 无数据库连接池

| 字段 | 内容 |
|------|------|
| **严重程度** | 🟢 Low |
| **所属模块** | 全局 / 性能 |
| **问题描述** | 审计报告 2.9.2 #1：当前使用 JSON 文件存储，无此需求。但审计指出"未来若迁移 SQLite 需 pool"。 |
| **涉及文件** | N/A（远期规划） |
| **影响范围** | 暂无实际影响 |
| **修复建议** | 待迁移 SQLite 时使用 `r2d2-sqlite` 或 `sqlx` 的连接池。当前可忽略 |
| **前置依赖** | 无 |

---

#### P48 — 无并发启动限制

| 字段 | 内容 |
|------|------|
| **严重程度** | 🟢 Low |
| **所属模块** | `src-tauri` / 并发 |
| **问题描述** | 审计报告 2.9.2 #4：[`state.rs:25`](src-tauri/src/state.rs:25) 的 `running_games` DashMap 仅记录运行状态，不限制并发数。理论上可同时启动多个 MC 实例耗尽内存。 |
| **涉及文件** | [`src-tauri/src/state.rs:25`](src-tauri/src/state.rs:25) |
| **影响范围** | 用户同时启动多个实例可能导致系统 OOM |
| **修复建议** | 在 `launch_start` 入口检查 `running_games.len() >= MAX_CONCURRENT_GAMES`（如 3），超出则返回错误 |
| **前置依赖** | 无 |

---

#### P49 — 响应文件清理时机不当

| 字段 | 内容 |
|------|------|
| **严重程度** | 🟢 Low |
| **所属模块** | `src-tauri` / launch |
| **问题描述** | 审计报告 2.9.2 #5：[`launch.rs:298`](src-tauri/src/commands/launch.rs:298) 中响应文件在进程退出后才清理（`wait()` 之后）。若进程 crash 或用户强制 kill，响应文件残留（含 access_token）。 |
| **涉及文件** | [`src-tauri/src/commands/launch.rs:298`](src-tauri/src/commands/launch.rs:298) |
| **影响范围** | 磁盘残留含 token 的文件 |
| **修复建议** | 1. 响应文件在 `spawn_game()` 返回后立即删除（进程已读取文件内容）；2. 或使用 `tempfile` crate 并设置 `tempfile::Builder::new().keep(false)` |
| **前置依赖** | P01（重构 `launch_start` 时一并修复） |

---

#### P50 — 无日志聚合方案

| 字段 | 内容 |
|------|------|
| **严重程度** | 🟢 Low |
| **所属模块** | 全局 / 运维 |
| **问题描述** | 审计报告 2.11.2 #4：日志仅写本地文件，无集中式日志收集方案（如 Sentry、OpenTelemetry）。对生产环境 cras h 排查不利。 |
| **涉及文件** | [`crates/rmccc-config/src/logging.rs`](crates/rmccc-config/src/logging.rs) |
| **影响范围** | 用户报 bug 时需手动提供日志文件；无 crash 自动上报 |
| **修复建议** | 1. 集成 `sentry` crate 做 crash 上报；2. 或添加 `tracing-opentelemetry` 支持 Jaeger 导出（开发者自建） |
| **前置依赖** | 无 |

---

#### P51 — `pnpm` 版本管理不严格

| 字段 | 内容 |
|------|------|
| **严重程度** | 🟢 Low |
| **所属模块** | 根目录 / 构建 |
| **问题描述** | 审计报告 2.11.2 #5：`package.json` 无 `packageManager` 字段指定 pnpm 版本，`pnpm-lock.yaml` 无版本约束，不同开发者可能使用不同 pnpm 版本导致 lockfile 冲突。 |
| **涉及文件** | [`package.json`](package.json) |
| **影响范围** | 团队协作时 lockfile 冲突 |
| **修复建议** | 1. 在 `package.json` 添加 `"packageManager": "pnpm@9.x.x"`；2. 或使用 Corepack：`corepack enable && corepack prepare pnpm@9.x.x --activate` |
| **前置依赖** | 无 |

---

## 三、建议修复顺序

### 总体排序（仅编号序列）

```
Batch 1 (立即修复): P08 → P21 → P32 → P23 → P09 → P04 → P13 → P15 → P12 → P02 → P03 → P01
Batch 2 (核心流程): P05 → P10 → P11 → P14 → P16 → P17 → P25 → P33 → P06 → P30 → P31 → P07 → P22 → P24 → P26 → P27 → P28 → P29 → P18 → P19 → P20 → P34
Batch 3 (质量改进): P35 → P36 → P37 → P38 → P39 → P40 → P41 → P42 → P43 → P44 → P45 → P46 → P47 → P48 → P49 → P50 → P51
```

### 排序理由

**Batch 1（必须立即修复 — 安全/崩溃/架构基石）**：
1. **P08（CI/CD）** 排在最前 —— 没有 CI 一切改动都无法自动化验证
2. **P21（rust-toolchain.toml）** 紧随其后 —— 确保 CI 和本地版本一致
3. **P32（clippy CI）、P23（cargo-audit）** —— CI 质量门禁
4. **P09（跨平台构建）** —— 依赖 P08 完成后加入矩阵
5. **P04（DI 容器）** —— **最关键的架构重构**，后续几乎所有问题都依赖它
6. **P13+P15+P12** —— DI 容器完善后立即消除 auth 重复和 DI 绕过
7. **P02+P03+P01** —— God Function 拆分（依赖 DI 容器提供注入点）

**Batch 2（核心流程正确性）**：
- **P05+P10+P11** —— Mod 管理和下载控制 API 实现（阻塞前端对接）
- **P14+P16+P17** —— 类型安全和 trait 契约修复
- **P25+P33** —— 前端进度事件和类型同步（阻塞前端 UX）
- **P06+P30+P31+P07** —— 测试体系建立（从集成到 E2E）
- 其余为分散的模块级问题（安全、性能、缓存等）

**Batch 3（架构优化与质量改进）**：
- 全部 Low 级别 + 非核心 Medium 问题
- 可在后续 Phase 中逐步消化，不阻塞当前功能交付

---

> **附注**：
> - `待确认` 标记：本清单中 P42（`macOSPrivateApi`）是否需要保留取决于分发策略；P47（DB 连接池）当前无实际影响，待 SQLite 迁移时再评估
> - 与 [`ARCHITECTURE_IMPROVEMENT_PLAN.md`](docs/ARCHITECTURE_IMPROVEMENT_PLAN.md) 的关系：该文档的 P0-1（God Object: `AppState`）、P0-2（双 GUI 并存）涉及前端/egui，不在本次后端审计范围内。其余问题均已覆盖整合
