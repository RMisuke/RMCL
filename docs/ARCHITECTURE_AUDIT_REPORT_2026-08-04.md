# RMCCC 后端架构审计报告（2026-08-04）

> **审计日期**：2026-08-04
> **审计范围**：Rust 后端全量（10 个 crates + `src-tauri` 应用层 + `src-cli` 命令行层）＋前端 IPC 契约对照
> **审计方法**：逐文件源码审查 + 接口定义对照 + 实际构建/测试验证（`cargo test --workspace` 实测）+ 与 2026-07-31 旧审计报告交叉比对
> **审计人**：Zoo（Debug Agent）

---

## 第一部分：审计摘要

### 1.1 总体结论

RMCCC 后端已从旧审计（2026-07-31）的"Phase 2–3 交界"推进到 **Phase 2–4 交界**，且**核心引擎（CLI 路径）已被反复端到端验证可用**：

- **CLI 实测验证**：离线登录 → 创建实例 → 下载资源（5057 个资源文件/455MB、131 个库、74 个 native）→ 启动原版 Minecraft 26.2 成功进游戏；Fabric 0.19.3 + Fabric API 安装后成功启动；NeoForge 1.20.1 安装成功、ModLauncher 正常启动（窗口 NPE 系 NeoForge 自身兼容性问题，非启动器缺陷）。证据见 [`docs/test_log.md`](docs/test_log.md)。
- **构建与测试**：`cargo test --workspace` 实测 **122 个测试全部通过**，编译零错误。
- **旧审计中的多项问题已被修复**：`CoreContext` 已成真 DI 容器（P04）；`launch_start` 已拆分为 7 个 Phase 函数（P01）；Profile JSON 原子写入（P19）；配置 Schema 版本字段（P18）；`ModLoaderConfig.loader_type` 类型化（P14）；CI 三平台流水线（P08/P09）；mods/modlog/download 等 IPC 命令补齐。

**但"可完整对接前端、支撑业务稳定上线"仍未达成。** 存在 4 个 P0 级功能阻断点与 1 个前端侧阻断点（前端 `main.tsx` 渲染的是占位组件，见 [`src/App.tsx`](src/App.tsx:4)）。即便前端完成接线，以下后端流程依然无法跑通：**Microsoft 登录设备码无法触达用户、Mod 安装/移除接口缺失、旧版加载器选择即 panic、下载进度/取消机制失效**。

### 1.2 后端健康度评级

| 维度 | 评级 | 得分 | 核心依据 |
|------|------|------|----------|
| 代码结构与分层 | 🟢 B+ | 75/100 | DI 容器成型、launch 已拆分；CLI context 仍为大类，命令层绕过 DI |
| 模块划分与职责 | 🟡 B | 70/100 | crate 边界清晰；两套 modloader 系统并存；旧加载器生产代码 panic |
| API 设计 | 🟡 B | 72/100 | 24/24 前端契约同步、错误结构统一；但 Mod 安装缺失、设备码事件缺失 |
| 数据模型与存储 | 🟡 B- | 65/100 | 原子写已做；`resolve_version` 继承链合并有正确性 bug；无配置迁移逻辑 |
| 认证授权 | 🟡 C+ | 60/100 | 5 步 OAuth2 完整；但设备码无法触达用户、单账户、刷新三处重复 |
| 依赖与配置管理 | 🟢 A- | 85/100 | workspace 集中管理、Cargo.lock 提交；缺 rust-toolchain/审计/环境区分 |
| 日志与异常处理 | 🟡 B | 72/100 | tracing + 统一 IpcError；无请求追踪 ID、部分模块缺日志 |
| 安全防护 | 🟡 B | 72/100 | AES-GCM + response file + safe_join；**response file 未清理（token 落盘）**、Windows ACL 缺失 |
| 性能与可扩展性 | 🟡 C+ | 58/100 | 连接池/并发下载良好；大文件整块读内存、无限流熔断、镜像未接入 GUI 安装 |
| 测试覆盖与代码质量 | 🟡 C+ | 60/100 | 122 测试全绿；但 mods/profile/core/命令层 0 测试，集成测试未覆盖 provider |
| 部署与运维 | 🟡 D+ | 40/100 | CI 有 lint+test 矩阵；无 release 构建/签名/公证/监控/更新 |
| **综合健康度** | **🟡 B-** | **66/100** | **核心可用，但存在 P0 功能阻断点，暂不可完整对接前端** |

### 1.3 "是否可完整对接前端"判断

**当前不具备完整对接前端、支撑业务稳定上线的条件。** 结论依据分三层：

1. **后端命令契约层（已就绪）**：`src-tauri` 注册 28 个 IPC 命令（[`src-tauri/src/main.rs:45`](src-tauri/src/main.rs:45)），与前端 [`src/ipc/api.ts`](src/ipc/api.ts) 定义的 24 个 wrapper **逐一对应、参数经 Tauri camelCase→snake_case 自动转换后匹配**，错误结构 `IpcError{code,message,retryable}` 统一。
2. **后端功能可用性层（存在 4 个 P0 阻断点）**：
   - **P0-A**：Microsoft 登录设备码无法触达用户——`auth_microsoft_login` 传 `authenticate(None)`（[`src-tauri/src/commands/auth.rs:57`](src-tauri/src/commands/auth.rs:57)），`AuthProgress::DeviceCode` 回调从未触发，`AUTH_DEVICE_CODE` 事件从未发射，浏览器不自动打开，**GUI 与 CLI 都无法完成 MS 登录**。
   - **P0-B**：Mod 安装/移除功能缺失——后端仅有 `mod_search / mod_get_versions / mod_get_download_url / mod_list / mod_toggle`（[`src-tauri/src/commands/mods.rs`](src-tauri/src/commands/mods.rs)），**没有 `mod_install` / `mod_remove`**，`ModSource` trait（[`crates/rmccc-mods/src/lib.rs:70`](crates/rmccc-mods/src/lib.rs:70)）也无 install 能力，前端拿到下载地址后无从落地。
   - **P0-C**：Tauri `modloader_list / modloader_install` 对 `quilt/liteloader/optifine/cleanroom/legacyfabric` 触发**生产代码 `panic!`**（[`crates/rmccc-modloader/src/lib.rs:138`](crates/rmccc-modloader/src/lib.rs:138)），GUI 直接崩溃。
   - **P0-D**：下载进度事件 `batch_id` 为空字符串（[`src-tauri/src/commands/version.rs:122`](src-tauri/src/commands/version.rs:122)）且命令同步阻塞直至下载完成、`DownloadHandle` 被丢弃——前端无法把进度关联到批次，`download_cancel/pause/resume` 对安装流程无效。
3. **前端接入层（未就绪）**：`src/main.tsx` 渲染的是占位组件（[`src/App.tsx:19`](src/App.tsx:19)："GUI is under construction. Please use the CLI"），路由/`Dashboard.tsx`/`Login.tsx` 均未接线。这属于前端侧工作，但直接决定"完整对接"的最终交付。

**结论**：后端引擎（下载/启动/Modloader 安装）质量与完成度高、经 CLI 验证充分，可视为"引擎就绪"；但**对接层的 4 个 P0 功能阻断点必须先修复**，之后前端才具备完整对接的前提。

---

## 第二部分：各项发现与状态评估

> 严重程度标注：🔴 阻断 / 🟠 高 / 🟡 中 / 🟢 低。所有引用基于 2026-08-04 源码实况。

### 2.1 代码结构与分层

**现状**：Cargo workspace 10 个业务 crate + `src-tauri` + `src-cli`，依赖拓扑为严格 DAG（`rmccc-error` 为叶子），符合架构设计。`CoreContext` 已成为**真正的 DI 容器**，持有 [`version_provider`](crates/rmccc-core/src/lib.rs:48)、[`download_provider`](crates/rmccc-core/src/lib.rs:52)、[`auth_provider`](crates/rmccc-core/src/lib.rs:56)、[`modloader_providers`](crates/rmccc-core/src/lib.rs:62)、[`mod_sources`](crates/rmccc-core/src/lib.rs:66)、[`download_manager`](crates/rmccc-core/src/lib.rs:41)、[`profile_store`](crates/rmccc-core/src/lib.rs:44)，并提供 [`modloader_provider()`](crates/rmccc-core/src/lib.rs:163) 访问器——旧审计 P04 基本解决。`launch_start` 已拆分为 `resolve_launch_profile → validate/ensure → detect_java → build_classpath → resolve_auth → build_launch_args → spawn_game_monitor` 7 个 Phase（[`src-tauri/src/commands/launch.rs:57`](src-tauri/src/commands/launch.rs:57)），不再是 357 行单体——旧审计 P01 基本解决。

| # | 问题 | 位置 | 严重程度 |
|---|------|------|----------|
| 1 | **命令层绕过 DI**：`version_list/version_install` 直接 `MojangVersionProvider::new(...)`，`modloader_list/install` 直接 `create_provider(...)`，未走 `core.version_provider` / `core.modloader_provider()` | [`src-tauri/src/commands/version.rs:12`](src-tauri/src/commands/version.rs:12)、[`src-tauri/src/commands/version.rs:61`](src-tauri/src/commands/version.rs:61)、[`src-tauri/src/commands/modloader.rs:20`](src-tauri/src/commands/modloader.rs:20) | 🟡 中 |
| 2 | **CLI context 仍是大类（God Class）**：`src-cli/src/context.rs` 760+ 行，`resource_download`/`repair_resources`/`launch_run`/`resolve_game_version` 均为 100–200 行大方法 | [`src-cli/src/context.rs`](src-cli/src/context.rs) | 🟠 高 |
| 3 | **两套 modloader 系统并存**：旧 `ModLoaderProvider` trait（Tauri 命令使用）+ 新 `DependencyManager/VersionList/InstallTask`（CLI 部分使用），两套 `install` 语义不同（返回 `String` vs 返回 `GameVersion`），维护成本与不一致风险高 | [`crates/rmccc-modloader/src/lib.rs:127`](crates/rmccc-modloader/src/lib.rs:127)、[`crates/rmccc-modloader/src/lib.rs:155`](crates/rmccc-modloader/src/lib.rs:155)、[`src-cli/src/commands/modloader.rs:25`](src-cli/src/commands/modloader.rs:25) | 🟠 高 |
| 4 | **`core.auth_provider` / `core.download_provider` 字段在 Tauri 命令层基本未使用**（download_provider 仅在 CLI 生效，见 2.9） | [`crates/rmccc-core/src/lib.rs:52-56`](crates/rmccc-core/src/lib.rs:52) | 🟡 中 |

### 2.2 模块划分与职责边界

**现状**：crate 边界整体清晰，无循环依赖、无典型"上帝类"（`launch_start` 已拆分）、`ModLoaderConfig.loader_type` 已类型化为 `ModLoaderType`（旧 P14 解决）、`AuthSession` 实现自定义 `Debug` 脱敏（[`crates/rmccc-auth/src/types.rs:17`](crates/rmccc-auth/src/types.rs:17)，旧 P26 部分解决）。

| # | 问题 | 位置 | 严重程度 |
|---|------|------|----------|
| 1 | **`create_provider()` 对 5 种旧版加载器直接 `panic!`**（生产代码可达 panic，Tauri 命令传入任意已通过 `from_str` 的类型即触发） | [`crates/rmccc-modloader/src/lib.rs:133-139`](crates/rmccc-modloader/src/lib.rs:133) | 🔴 阻断 |
| 2 | **Auth 刷新逻辑三处重复**：Tauri `try_refresh_if_needed`、CLI `auth_refresh`、`MicrosoftAuthProvider::refresh`；vault 初始化与 auth.json 存取逻辑亦在 Tauri/CLI 重复 | [`src-tauri/src/commands/auth.rs:8`](src-tauri/src/commands/auth.rs:8)、[`src-cli/src/context.rs:59`](src-cli/src/context.rs:59)、[`crates/rmccc-auth/src/microsoft.rs:405`](crates/rmccc-auth/src/microsoft.rs:405) | 🟡 中 |
| 3 | **`ManagedProcess` 已实现但 Tauri 未使用**：`spawn_managed/recent_lines/drain_lines` 为可用封装，但 Tauri `launch_start` 自建 inline 4KB 缓冲监视（两套进程监控逻辑） | [`crates/rmccc-launch/src/managed.rs:155`](crates/rmccc-launch/src/managed.rs:155)、[`src-tauri/src/commands/launch.rs:236`](src-tauri/src/commands/launch.rs:236) | 🟡 中 |
| 4 | **`DownloadManager` trait 已定义但未真正实现**：`ConcurrentDownloadManager` 仅有同名 inherent 方法，未 `impl DownloadManager`（无法 `Box<dyn DownloadManager>`，DI 注入受限） | [`crates/rmccc-download/src/lib.rs:203-224`](crates/rmccc-download/src/lib.rs:203) | 🟡 中 |

### 2.3 API 设计（IPC 契约）

**现状**：命令命名 `{domain}_{action}` 统一；28 个命令已在 [`src-tauri/src/main.rs:45-75`](src-tauri/src/main.rs:45) 注册；统一错误结构 `IpcError{code,message,retryable}` + `ErrorCode`（已扩展 `AlreadyRunning/ValidationError/NotFound/InternalError`，[`crates/rmccc-error/src/lib.rs:94`](crates/rmccc-error/src/lib.rs:94)）；前端 [`api.ts`](src/ipc/api.ts) 24 个 wrapper 与后端**全部一一对应**；事件常量前后端一致（[`src/ipc/events.ts`](src/ipc/events.ts) ↔ [`src-tauri/src/events.rs`](src-tauri/src/events.rs)）。

#### 2.3.1 契约对照（前端 wrapper ↔ 后端命令）

| 前端域 | 方法数 | 后端匹配 | 说明 |
|--------|--------|----------|------|
| `auth` | 4 | ✅ 4/4 | `auth_refresh_token` 后端存在但前端无 wrapper |
| `profiles` | 4 | ✅ 4/4 | — |
| `versions` | 2 | ✅ 2/2 | — |
| `launch` | 3 | ✅ 3/3 | — |
| `mods` | 5 | ✅ 5/5 | 但**无 install/remove 对应后端**（见下） |
| `downloads` | 3 | ✅ 3/3 | `download_cancel_batch` 已从两端移除，口径一致 |
| `settings` | 3 | ✅ 3/3 | — |
| 合计 | 24 | ✅ 24/24 | 另有 `log_get/log_clear/auth_refresh_token` 3 个后端命令无前端 wrapper |

#### 2.3.2 缺失或不可用接口（重点）

| # | 接口 | 状态 | 严重程度 |
|---|------|------|----------|
| 1 | **`mod_install` / `mod_remove`**：后端无下载 jar 至 `mods/` 的能力，`ModSource` trait 缺 `install`；前端只能取 `download_url` 无法落地 | 完全缺失 | 🔴 阻断（Phase 4） |
| 2 | **`AUTH_DEVICE_CODE` 事件**：`authenticate(None)` 使设备码流程对外不可见 | 事件未发射 | 🔴 阻断（MS 登录） |
| 3 | **下载进度批次关联**：`version_install` 发射的 `batch_id` 恒为空串，且命令同步阻塞、`_handle` 被丢弃 | 无效 | 🟠 高 |
| 4 | **`MODLOADER_INSTALL_PROGRESS`**：常量已定义但 modloader 安装无进度事件 | 未实现 | 🟡 中 |
| 5 | **`launch_start` 过期 token 不自动刷新**：`resolve_auth_session` 直接读 vault，未走 `try_refresh_if_needed` | 行为缺陷 | 🟡 中 |
| 6 | **`profile_import/export`、`log_get/log_clear` 前端 wrapper** | 接口存在/前端未接线 | 🟢 低 |

#### 2.3.3 非 REST 说明

IPC 为 Tauri 进程内调用而非 HTTP，因此"RESTful 资源命名/HTTP 状态码/版本管理"不适用；对应语义由 `{domain}_{action}` 命名 + `ErrorCode` 枚举承担，属于符合 Tauri 生态的合理设计，不视为缺陷。但**分页/过滤约定不统一**：`mod_search` 有 `page/limit`，`version_list`/`profile_list` 无分页（列表规模小，可接受）。

### 2.4 数据模型与存储

**现状**：JSON 文件存储（config/auth/profiles/cache），符合架构决策。`ProfileStore::save` 已原子写（tmp+rename，[`crates/rmccc-profile/src/lib.rs:81`](crates/rmccc-profile/src/lib.rs:81)，旧 P19 解决）；`AppConfig` 已含 `version` 字段（[`crates/rmccc-config/src/lib.rs:13`](crates/rmccc-config/src/lib.rs:13)，旧 P18 部分解决）；`GameVersion` serde rename 已修复（Bug #11）；损坏 profile 文件跳过并 warn（[`crates/rmccc-profile/src/lib.rs:54`](crates/rmccc-profile/src/lib.rs:54)）。

| # | 问题 | 位置 | 严重程度 |
|---|------|------|----------|
| 1 | **`resolve_version()` 继承链合并正确性 bug**：`current` 上计算出的父级 `assetIndex`/`javaVersion` 继承被丢弃——函数末尾重新 `fetch_version(version_id)` 生成 `resolved`，仅 `libraries` 合并生效；主类继承代码为空实现（注释存在、无逻辑）。直接影响 Forge/NeoForge 等 `inheritsFrom` 版本 | [`crates/rmccc-version/src/repository.rs:30-76`](crates/rmccc-version/src/repository.rs:30) | 🟠 高 |
| 2 | **配置无实际迁移逻辑**：`version` 字段仅记录，`AppConfig::load` 直接反序列化，未来字段变更仍可能反序列化失败 | [`crates/rmccc-config/src/lib.rs:73`](crates/rmccc-config/src/lib.rs:73) | 🟡 中 |
| 3 | **版本列表缓存无条件请求**：`VersionCache` 仅 TTL 文件缓存，无 ETag/`If-None-Match`，每次 `version_list` 都完整下载 manifest 并后台写盘 | [`crates/rmccc-version/src/cache.rs`](crates/rmccc-version/src/cache.rs) | 🟡 中 |
| 4 | **auth.json 写入非原子**（`vault.save_session` 直接 `fs::write`，虽然加密，断电仍可能损坏） | [`crates/rmccc-auth/src/vault.rs:108`](crates/rmccc-auth/src/vault.rs:108) | 🟢 低 |
| 5 | **profile `game_dir` 与 `game_root_dir` 无关联校验**（旧 P38 未解决） | [`crates/rmccc-profile/src/lib.rs:18`](crates/rmccc-profile/src/lib.rs:18) | 🟢 低 |

### 2.5 认证授权

**现状**：Microsoft OAuth2 device code 5 步流程完整（device code → MS token → XBL → XSTS → MC token/profile，[`crates/rmccc-auth/src/microsoft.rs`](crates/rmccc-auth/src/microsoft.rs)）；离线 UUIDv5 确定性生成（[`crates/rmccc-auth/src/offline.rs:22`](crates/rmccc-auth/src/offline.rs:22)）；AES-256-GCM 加密 + key `0o600`（[`crates/rmccc-auth/src/vault.rs:32`](crates/rmccc-auth/src/vault.rs:32)）；`OfflineAuthProvider::authenticate` 已正确返回离线会话（旧 P41 解决）；`auth_get_session` 自动刷新过期 token。

| # | 问题 | 位置 | 严重程度 |
|---|------|------|----------|
| 1 | **设备码无法触达用户（阻断 MS 登录）**：Tauri 与 CLI 均以 `authenticate(None)` 调用，`AuthProgress::DeviceCode` 回调从未触发，`AUTH_DEVICE_CODE` 事件未发射，浏览器不自动打开，用户全程看不到 `user_code` | [`src-tauri/src/commands/auth.rs:57`](src-tauri/src/commands/auth.rs:57)、[`src-cli/src/context.rs:37`](src-cli/src/context.rs:37)、[`crates/rmccc-auth/src/microsoft.rs:335`](crates/rmccc-auth/src/microsoft.rs:335) | 🔴 阻断 |
| 2 | **单账户模式**：`auth.json` 单文件，多账户无法切换（旧 P20 未解决） | [`crates/rmccc-auth/src/vault.rs:116`](crates/rmccc-auth/src/vault.rs:116) | 🟡 中 |
| 3 | **刷新逻辑三处重复**（见 2.2#2） | 多处 | 🟡 中 |
| 4 | **`.key` Windows 无 ACL**：仅 Unix `0o600`（旧 P27 未解决） | [`crates/rmccc-auth/src/vault.rs:43`](crates/rmccc-auth/src/vault.rs:43) | 🟡 中 |
| 5 | **device code 过期无自动重试**（旧 P39 未解决） | [`crates/rmccc-auth/src/microsoft.rs:99`](crates/rmccc-auth/src/microsoft.rs:99) | 🟢 低 |
| 6 | 无 Authlib-Injector 外置登录（远期需求） | — | 🟢 低 |

### 2.6 依赖与配置管理

**现状**：workspace 依赖集中管理（[`Cargo.toml:18`](Cargo.toml:18)）、`Cargo.lock` 已提交、release profile（lto/strip/panic=abort）已配；`reqwest::Client` 仅 CoreContext 单点创建（旧 P22 已解决，全库唯一 `Client::builder()`）；`tauri-plugin-updater` 等依赖已在 workspace 声明。

| # | 问题 | 位置 | 严重程度 |
|---|------|------|----------|
| 1 | **无 `rust-toolchain.toml`**：CI 用 `dtolnay/rust-toolchain@stable`，开发者本地版本可能与 CI 不一致（旧 P21 未解决） | 根目录 | 🟡 中 |
| 2 | **无 `cargo-audit`/`cargo-deny`**：依赖漏洞与许可证无持续扫描（旧 P23 未解决） | CI 配置 | 🟡 中 |
| 3 | **无环境区分**：dev/prod 配置混用单一 `config.json`，无 profile 隔离 | — | 🟡 中 |
| 4 | **`macOSPrivateApi: true`** 影响未来 MAS 上架（旧 P42 未解决） | [`src-tauri/tauri.conf.json:30`](src-tauri/tauri.conf.json:30) | 🟢 低 |
| 5 | **`ts-rs` 依赖已声明但全库 0 使用**（`derive(TS)` 检索 0 结果），类型桥接未落地（旧 P33 未解决） | [`crates/rmccc-error/src/lib.rs`](crates/rmccc-error/src/lib.rs)、[`src/ipc/api.ts:10`](src/ipc/api.ts:10) | 🟡 中 |
| 6 | **前端 pnpm 版本未锁定**：`package.json` 无 `packageManager` 字段（旧 P51 未解决） | [`package.json`](package.json) | 🟢 低 |

### 2.7 日志记录与异常处理

**现状**：`tracing` + `tracing-appender`（日滚动、最多 10 文件、JSON 结构化，[`crates/rmccc-config/src/logging.rs:15`](crates/rmccc-config/src/logging.rs:15)）；全部 28 个 IPC 命令带 `#[tracing::instrument(skip(state))]`；统一 `From<RmcccError> for IpcError` 异常桥接（[`crates/rmccc-error/src/lib.rs:126`](crates/rmccc-error/src/lib.rs:126)）；`AuthSession` 自定义 Debug 已脱敏。

| # | 问题 | 位置 | 严重程度 |
|---|------|------|----------|
| 1 | **无请求追踪 ID**：跨模块（profile→version→auth→launch）无法用单一 ID 串联日志（旧 P24 未解决） | 全局 | 🟡 中 |
| 2 | **`rmccc-mods` / `rmccc-profile` 缺少业务日志**（仅 `tracing::warn` 跳过损坏文件） | 各 crate | 🟢 低 |
| 3 | **CLI 与 Tauri 日志体系不一致**：CLI 用 `tracing_subscriber::fmt`（非 JSON、无滚动） | [`src-cli/src/main.rs:187`](src-cli/src/main.rs:187) | 🟢 低 |
| 4 | **device code/XBL/XSTS 失败时记录完整响应 `body`**：虽通常不含 token，但属敏感面，建议脱敏或限长 | [`crates/rmccc-auth/src/microsoft.rs:81`](crates/rmccc-auth/src/microsoft.rs:81) 等多处 | 🟢 低 |

### 2.8 安全防护

**现状**：AES-256-GCM token 加密；response file 防 `ps aux` 泄露（[`crates/rmccc-launch/src/process.rs:21`](crates/rmccc-launch/src/process.rs:21)）；`safe_join` 路径遍历防护（[`crates/rmccc-config/src/paths.rs:48`](crates/rmccc-config/src/paths.rs:48)）；Tauri capability 最小权限（无 `fs`/`shell` 通用权限，[`src-tauri/capabilities/default.json`](src-tauri/capabilities/default.json)）；输入 clamp（memory、username≤32、name≤64）；磁盘预检；Log4j RCE 修复参数注入。

| # | 问题 | 位置 | 严重程度 |
|---|------|------|----------|
| 1 | **response file 未清理（token 落盘）**：Tauri `launch_start` 中仅有"Clean up response file immediately"注释，**无实际 `remove_file` 调用**，含 `--accessToken` 的文件残留于游戏目录 | [`src-tauri/src/commands/launch.rs:411-412`](src-tauri/src/commands/launch.rs:411) | 🟠 高 |
| 2 | **`.key` Windows ACL 缺失**（同 2.5#4） | [`crates/rmccc-auth/src/vault.rs:43`](crates/rmccc-auth/src/vault.rs:43) | 🟡 中 |
| 3 | **`launch_kill` 语义不一致**：Unix `libc::kill(SIGKILL)` 仅杀 Java 主进程（不杀进程树），Windows `taskkill /F` 杀进程树；且对已退出进程无兜底清理 | [`src-tauri/src/commands/launch.rs:433`](src-tauri/src/commands/launch.rs:433) | 🟡 中 |
| 4 | **无依赖漏洞扫描**（同 2.6#2） | CI | 🟡 中 |
| 5 | **镜像/第三方 URL 无完整性约束**：mod 下载 URL 来自 Modrinth 响应，本地无额外校验（SHA1 来自远端声明） | [`crates/rmccc-mods/src/modrinth.rs`](crates/rmccc-mods/src/modrinth.rs) | 🟢 低 |

### 2.9 性能与可扩展性

**现状**：连接池 `pool_max_idle_per_host(32)`（[`crates/rmccc-core/src/lib.rs:81`](crates/rmccc-core/src/lib.rs:81)）；并发下载 worker 默认 16；SHA1 缓存仓库 `CacheRepository` 复用；`ThrottledEmitter` 100ms 限频已接入 `version_install`（旧 P25 解决）；并发启动上限 3（旧 P48 解决，[`src-tauri/src/commands/launch.rs:350`](src-tauri/src/commands/launch.rs:350)）。

| # | 问题 | 位置 | 严重程度 |
|---|------|------|----------|
| 1 | **镜像（download_provider）未接入 Tauri 安装流**：CLI 的 `resource_download` 使用 `core.download_provider.inject_url_with_candidates`（[`src-cli/src/context.rs:220`](src-cli/src/context.rs:220)），但 Tauri `version_install` 直接使用 Mojang 原始 URL 且 `resolve_asset_items` 硬编码 `resources.download.minecraft.net`（[`crates/rmccc-version/src/assets.rs:75`](crates/rmccc-version/src/assets.rs:75)）——`mirror_url` 配置对 GUI 安装完全无效 | [`src-tauri/src/commands/version.rs:61-98`](src-tauri/src/commands/version.rs:61) | 🟠 高 |
| 2 | **大文件整块读入内存**：`download_single` 用 `response.bytes().await`（assets 可达数百 MB）——旧 P29 未解决 | [`crates/rmccc-download/src/lib.rs:617`](crates/rmccc-download/src/lib.rs:617) | 🟡 中 |
| 3 | **`MojangVersionProvider::fetch_version` 每次先全量 `fetch_versions`**，额外一次 manifest 请求 | [`crates/rmccc-version/src/manifest.rs:93`](crates/rmccc-version/src/manifest.rs:93) | 🟡 中 |
| 4 | **HTTP client 无 connect/timeout 显式配置**（架构文档称 10s 但代码未设置） | [`crates/rmccc-core/src/lib.rs:81`](crates/rmccc-core/src/lib.rs:81) | 🟡 中 |
| 5 | **无流控/熔断**：仅下载模块有指数退避重试，其余外部调用无节流 | 全局 | 🟢 低 |
| 6 | **无请求级缓存**（版本列表每次全量获取，同 2.4#3） | — | 🟡 中 |

### 2.10 测试覆盖与代码质量

**现状**：实测 `cargo test --workspace` **122 个测试全部通过**（auth 3 / config 2 / download 18 / launch 28 / modloader 36 / version 32 / 集成 2 / doc 1）。`docs/test_log.md` 记录了 CLI 端到端验证（下载、启动、Fabric/NeoForge 安装）与 19 个 bug 的修复证据。CI 已配置 fmt + clippy(`-D warnings`) + typecheck + 三平台测试矩阵（[`.github/workflows/ci.yml`](.github/workflows/ci.yml)，旧 P08/P09 解决）。

| # | 问题 | 位置 | 严重程度 |
|---|------|------|----------|
| 1 | **关键模块 0 测试**：`rmccc-mods`（Modrinth 解析）、`rmccc-profile`（存储）、`rmccc-core`（编排）、Tauri 命令层（28 个命令）、CLI 层均为 0 测试 | 各处 | 🟠 高 |
| 2 | **集成测试未真正覆盖 provider**：`tests/version_integration_test.rs` 两个用例只验证 wiremock 本身，未构造 `MojangVersionProvider` 指向 mock server 验证 `fetch_versions/fetch_version` | [`crates/rmccc-version/tests/version_integration_test.rs:34`](crates/rmccc-version/tests/version_integration_test.rs:34) | 🟡 中 |
| 3 | **无 E2E 测试（tauri-driver）**（旧 P07 未解决） | 全局 | 🟡 中 |
| 4 | **`#![allow(dead_code, unused_imports)]` 仍在**：掩盖死代码与未用导入（`ts-rs`、`events.rs` 多个常量、`ManagedProcess` 等）；`events.rs` 亦有 `#![allow(dead_code)]` | [`src-tauri/src/main.rs:1`](src-tauri/src/main.rs:1)、[`src-tauri/src/events.rs:5`](src-tauri/src/events.rs:5) | 🟡 中 |
| 5 | **生产代码 panic 未被 CI 拦截**：`create_provider` 的 `panic!` 通过 clippy（默认不告警 panic!）——需 `clippy::panic` 或重构 | [`crates/rmccc-modloader/src/lib.rs:138`](crates/rmccc-modloader/src/lib.rs:138) | 🟠 高 |
| 6 | **前端类型手动维护**，`ts-rs` 未启用（旧 P33，同 2.6#5） | [`src/ipc/api.ts`](src/ipc/api.ts) | 🟡 中 |

### 2.11 部署与运维

**现状**：CI 已具备 lint + 三平台 test 矩阵；Tauri `tauri.conf.json` 配置了 beforeBuild/beforeDev；版本 0.1.0。

| # | 问题 | 位置 | 严重程度 |
|---|------|------|----------|
| 1 | **无 release/打包 job**：CI 无 `pnpm tauri build`、无产物上传、无 GitHub Release 发布 | `.github/workflows/ci.yml` | 🟠 高 |
| 2 | **无 macOS 代码签名/公证**：直接分发将被 Gatekeeper 拦截 | — | 🟠 高 |
| 3 | **无自动更新**：`tauri-plugin-updater` 在 workspace 声明但 `src-tauri` 未启用、无 endpoints/pubkey（旧 P34 未解决） | [`src-tauri/Cargo.toml`](src-tauri/Cargo.toml) | 🟡 中 |
| 4 | **无监控/告警/日志聚合/crash 上报**（旧 P50 未解决） | — | 🟢 低 |
| 5 | **无运维文档**（升级/备份/故障排查手册缺失） | — | 🟢 低 |

---

## 第三部分：按优先级排序的改进措施清单

### P0（阻断性——必须立即修复，否则无法完整对接前端/支撑上线）

| # | 改进项 | 目标 | 具体行动方案 | 预期效果 | 工作量 |
|---|--------|------|--------------|----------|--------|
| P0-1 | **打通 Microsoft 设备码到用户** | 用户能看到验证码、浏览器自动打开 | ① `auth_microsoft_login` 传入 progress 回调，通过 `state.app_handle.emit(AUTH_DEVICE_CODE, ...)` 发射 `{user_code, verification_uri}`；② 前端 Login 页监听并展示；③ 可选用 `tauri-plugin-opener` 自动打开浏览器；④ CLI 同步打印验证码 | MS 正版登录在 GUI/CLI 均可完成 | 小（0.5–1 人日） |
| P0-2 | **补齐 Mod 安装/移除能力** | 前端可从 `download_url` 完成下载→落 `mods/` | ① `ModSource` 增加 `install(profile_id, version_id) -> Result`（下载 jar + SHA1 校验 + 写入 `{game_dir}/mods/`）；② 新增 `mod_install`/`mod_remove` IPC 命令并注册；③ `api.ts` 补 wrapper；④ 依赖解析（`ModDependency`）后续接入 | Phase 4 Mod 管理可闭环 | 中（2–3 人日） |
| P0-3 | **消除旧加载器 panic** | GUI 选择任意加载器不崩溃 | ① 移除 `create_provider` 的 `panic!`，改为返回 `Result`；② Tauri 命令统一走 `core.modloader_provider(&lt)`（已内置错误返回）；③ 为 5 种旧加载器接入 `DependencyManager` 路径或返回明确"暂不支持"错误 | 选择 Quilt/LiteLoader/OptiFine 等不再崩溃 | 小（1 人日） |
| P0-4 | **修复下载进度/取消链路** | 前端能实时看进度并取消/暂停 | ① `version_install` 使用真实 `handle.id` 作为 `batch_id` 发射进度；② 改为异步任务注册到 `AppState`（`batch_id → task`），命令立即返回；③ `download_cancel/pause/resume` 据此生效；④ `modloader_install` 同样接入进度与 `MODLOADER_INSTALL_PROGRESS` | 进度条可用、取消/暂停可用 | 中（2–3 人日） |

### P1（高风险——本阶段内必须修复）

| # | 改进项 | 目标 | 具体行动方案 | 预期效果 | 工作量 |
|---|--------|------|--------------|----------|--------|
| P1-1 | **清理 response file（安全）** | 启动后立即删除含 token 文件 | `spawn_game` 返回后立即 `std::fs::remove_file(&response_file)`（进程已读取）；或 `tempfile` keep(false) | 消除 token 落盘风险 | 小 |
| P1-2 | **修复 `resolve_version` 继承合并** | `inheritsFrom` 版本元数据完整合并 | 重写 `repository.rs`：直接在 `current` 上完成主类/assetIndex/javaVersion 继承后返回，去掉末尾重复 fetch；补主类继承逻辑；添加 Forge/NeoForge 继承链单测 | Forge/NeoForge 启动元数据正确 | 小–中 |
| P1-3 | **镜像接入 Tauri 安装流** | `mirror_url` 对 GUI 生效 | `version_install` 使用 `core.download_provider.inject_url_with_candidates` 注入 candidates；`assets.rs` 增加 provider 参数 | 国内用户安装提速 | 中 |
| P1-4 | **`launch_start` 自动刷新过期 token** | 启动用有效 token | 启动前复用统一刷新逻辑（`core` 层收敛后的 `refresh_session_if_needed`） | 过期 MS 会话启动不再失败 | 小 |
| P1-5 | **统一 Auth 刷新逻辑** | 消除三处重复 | `CoreContext` 增加 `refresh_session_if_needed(data_dir) -> Result<Option<AuthSession>>`，Tauri/CLI 均调用；删除 `try_refresh_if_needed` 与 CLI 重复代码 | 刷新行为一致、减少 bug | 小 |
| P1-6 | **补关键模块测试** | mods/profile/core/命令层有基础覆盖 | ① Modrinth 解析（wiremock：search/versions/download_url）；② `ProfileStore` CRUD + 原子写；③ `mod_install`/`mod_toggle` 等命令层测试；④ 修正集成测试真正指向 provider | 回归安全网 | 中 |
| P1-7 | **`create_provider`/Tauri 命令接入 DI** | 消除绕过 DI 与两套系统割裂 | Tauri `version_*`/`modloader_*` 走 `core.version_provider`/`core.modloader_provider`；评估以 `DependencyManager` 统一 modloader 路径 | 可测试、可替换 | 中 |

### P2（中风险——下个阶段修复）

| # | 改进项 | 目标 | 具体行动方案 | 预期效果 | 工作量 |
|---|--------|------|--------------|----------|--------|
| P2-1 | **接入 `ts-rs` 自动类型生成** | 前后端类型零维护 | 为 IPC 相关 struct 加 `derive(TS)`，`build.rs` 导出 `src/ipc/types.ts`，`api.ts` 改导入；CI 校验生成文件最新 | 消除类型漂移 | 小 |
| P2-2 | **去除 `#![allow(dead_code,...)]` + clippy::panic** | 死代码暴露、panic 被 CI 拦截 | 删除 allow 属性；启用 `clippy::panic`/`clippy::unwrap_used` 门禁；清理 `ManagedProcess`/`events` 死代码 | 代码质量约束 | 小 |
| P2-3 | **`rust-toolchain.toml` + `cargo-audit`** | 工具链一致、漏洞可追踪 | 创建 `rust-toolchain.toml`(stable)；CI 加 `cargo audit`/`cargo deny` | 版本一致、安全可见 | 小 |
| P2-4 | **大文件流式写入** | 降低内存峰值 | `download_single` 改 `response.chunk()` 边下边写 + 流式 SHA1 | 大资源下载内存受控 | 中 |
| P2-5 | **请求追踪 ID** | 跨模块日志串联 | IPC 入口生成 `request_id`，span field 传播，JSON 日志输出 | 可观测性提升 | 中 |
| P2-6 | **Windows `.key` ACL + `launch_kill` 进程树统一** | 跨平台安全一致 | Windows 用 `icacls`/`OpenOptionsExt`；kill 统一进程树终止 | 安全一致 | 小 |
| P2-7 | **多账户 + 配置迁移 + device code 重试** | 提升可用性 | `accounts/{uuid}.json` 目录化 + active 指针；`ConfigMigrator`；设备码过期重试 | 用户体验 | 中 |
| P2-8 | **release CI + 签名公证 + 自动更新** | 可发布交付 | CI 加 `pnpm tauri build` 三平台打包 + Release；macOS 签名/公证；启用 updater | 用户可自动升级 | 大 |

---

## 第四部分：总结性审计意见

### 4.1 对后端整体架构的中长期建议

1. **确立"引擎-编排-适配"三层收敛策略**。当前 `src-cli` 与 `src-tauri` 各自实现了资源下载/启动/Modloader 编排，且 modloader 存在新旧两套系统。中长期应：
   - 将编排逻辑统一收敛到 `rmccc-core`（如 `pipeline.rs`），CLI 与 Tauri 只做"参数适配 + 输出呈现"；
   - 以 `DependencyManager` + `VersionList`/`InstallTask` 为 modloader 唯一实现路径，废弃旧 `ModLoaderProvider` 的重复分支；
   - `CoreContext` 的 `download_provider`/`auth_provider` 字段必须被 Tauri 命令真正消费，消除"DI 已建但绕行"的现象。

2. **引入统一的异步任务模型（Task 框架）**。当前下载/安装均直接 `tokio::spawn`，导致进度传播、取消链接、错误聚合困难（P0-4 根因）。建议在 `rmccc-core` 引入轻量 `Task<T>`/`TaskExecutor`（参考 HMCL），统一管理下载、Modloader 安装、版本安装等长时操作，并为前端提供统一的 `task://` 进度/取消语义。

3. **坚持 JSON 存储但补上版本迁移与原子性**。Profile/配置 JSON 方案适合当前规模；应补 `ConfigMigrator`、`auth.json` 原子写。若未来接入大型 Modpack（100+ mods），再评估 SQLite + 连接池，届时启用 `sqlx` 而非手写。

4. **测试投资与安全补丁不得再拖延**。`rmccc-mods`/`rmccc-profile`/命令层的 0 测试与"集成测试不测 provider"是当前最大质量隐患；同时 response file 残留与设备码不可达属于**上线即事故**级问题，应列为首批修复。

5. **发布工程（release/签名/更新）是上线的硬门槛**。后端功能已基本就绪，但 macOS 无签名公证、无 release CI、无自动更新，无法进入"业务稳定上线"状态，应纳入 P1 规划。

### 4.2 关键技术债务

| 债务 | 估算偿还成本 | 搁置风险 |
|------|-------------|----------|
| 两套 modloader 系统并存 + `create_provider` panic | 中 | GUI 崩溃、维护割裂（P0-C） |
| MS 设备码不可达 | 小 | 正版登录完全不可用（P0-A） |
| Mod 安装/移除缺失 | 大 | Phase 4 无法交付（P0-B） |
| 下载任务无统一模型、进度/取消失效 | 中 | 前端 UX 无法实现（P0-D） |
| `resolve_version` 继承合并 bug | 小 | Forge/NeoForge 元数据错误 |
| response file 残留（安全） | 小 | token 落盘 |
| 镜像未接入 GUI 安装 | 中 | 国内用户下载慢 |
| 命令层绕过 DI、Auth 逻辑三处重复 | 中 | 一致性风险 |
| `rmccc-mods`/命令层 0 测试 | 中 | 回归风险高 |
| `ts-rs` 未启用、前端类型手动维护 | 小 | 类型漂移 |
| 无 release/签名/更新/监控 | 大 | 无法正式上线 |

### 4.3 下一步应立即启动的推进事项

按顺序（依赖关系排列）：

1. **（本周·立即）P0 修复**：P0-1（设备码触达）→ P0-3（panic）→ P0-4（下载进度/取消）→ P0-2（Mod 安装/移除）。这 4 项是"可完整对接前端"的硬前置。
2. **（本周·随 P0 附带）P1-1 response file 清理 + P1-2 `resolve_version` 继承修复 + P1-4 启动 token 刷新**——均为 1 人日以内、直接消除事故级风险。
3. **（下周）P1-5 统一 Auth 刷新 + P1-7 命令层接入 DI + P1-3 镜像接入**——架构一致性收口。
4. **（下周）P1-6 补关键测试（mods/profile/命令层）+ 修正集成测试**——为后续重构建立安全网。
5. **（两周内）前端接线**：`main.tsx` 启用路由，接入 `Dashboard`/`Login`/`Settings` 的 24 个 IPC wrapper 与 `launch://`、`download://` 事件监听（前端侧，但依赖 P0-4 的 batch_id 语义）。
6. **（两周内）P2 序列**：`ts-rs`、`rust-toolchain.toml`、`cargo-audit`、clippy::panic 门禁，随后 release CI + 签名公证。

---

> **审计结论**：RMCCC 后端引擎成熟度高、CLI 路径经端到端验证可用，架构骨架（DI/分层/错误/日志/安全基座）扎实。当前**不具备完整对接前端、支撑业务稳定上线的条件**，卡点集中在对接层的 4 个 P0 功能阻断点（MS 设备码不可达、Mod 安装/移除缺失、旧加载器 panic、下载进度/取消失效）与前端占位未接线。建议按 P0→P1→P2 顺序推进，优先在一周内完成 4 个 P0，随后补测试与发布工程，可在 2–4 周内达到可完整对接前端并进入上线准备的状态。
