# RMCCC 后端架构审计报告

> ⚠️ **历史快照（2026-07-31）**：结论已被 [`ARCHITECTURE_AUDIT_REPORT_2026-08-04.md`](ARCHITECTURE_AUDIT_REPORT_2026-08-04.md)
> 取代，且其中若干判断（如 response file 的清理时机）已被后者推翻。
> **当前状态请以 [`PROJECT_STATUS.md`](PROJECT_STATUS.md) 为准**。

> **审计日期**：2026-07-31  
> **审计范围**：Rust 后端全量（10 个 crate + Tauri 应用层 + CLI 层）  
> **审计方法**：逐文件源码审查 + 接口定义对照 + 测试日志交叉验证  
> **审计人**：架构审计 Agent

---

## 第一部分：审计摘要

### 1.1 总体结论

RMCCC 后端处于 **Phase 2–3 交界状态**。Phase 1（基础设施）和 Phase 2（核心游戏启动）已基本完成，CLI 端已验证可完整运行 Minecraft 26.2 端到端流程（下载 → 安装 → 启动 → 游戏内）。Phase 3（Modloader 安装）已实现 5/8 种加载器的版本列表查询和安装流程，Fabric 和 Forge 已验证端到端可用。Phase 4（Mod 管理）仅有数据类型定义，API 实现完全缺失。Phase 5（生产就绪）除日志外几乎未启动。

### 1.2 后端健康度评级

| 维度 | 评级 | 得分 | 说明 |
|------|------|------|------|
| 代码结构与分层 | 🟡 B | 72/100 | 分层清晰但存在跨层直接调用和 God Function |
| 模块划分与职责 | 🟡 B | 68/100 | crate 边界良好，但 DI 容器不完整，存在重复逻辑 |
| API 设计 | 🟢 B+ | 78/100 | IPC 命令命名一致，错误响应统一，但非标准 REST |
| 数据模型与存储 | 🟡 B | 70/100 | JSON 存储简洁可用，但缺少版本迁移和事务机制 |
| 认证授权 | 🟢 B+ | 80/100 | Microsoft OAuth2 + 离线 + AES-256-GCM，单账户模式 |
| 依赖与配置管理 | 🟢 A- | 85/100 | workspace 依赖集中管理，Cargo.lock 提交，配置合理 |
| 日志与异常处理 | 🟡 B | 72/100 | tracing 已接入，IPC 命令有 span，但部分模块缺少日志 |
| 安全防护 | 🟢 B+ | 80/100 | Response file、AES-256-GCM、路径遍历防护均已实现 |
| 性能与可扩展性 | 🟡 C+ | 60/100 | 连接池和并发下载良好，但无缓存策略、限流熔断 |
| 测试覆盖 | 🟡 C+ | 58/100 | 单元测试覆盖较好，无集成测试、无 E2E 测试 |
| 部署与运维 | 🔴 D | 35/100 | 无 CI/CD 配置、无容器化、无监控告警 |
| **综合健康度** | **🟡 B** | **67/100** | **可部分对接前端，但存在阻断性问题** |

### 1.3 "是否可完整对接前端" 判断

**当前不可完整对接前端。** CLI 端的核心流程（登录 → 创建实例 → 下载资源 → 启动游戏）已验证通过，但以下阻断性问题阻止了前端完整对接：

1. **Mod 管理接口完全缺失**（Phase 4 未启动）
2. **下载进度事件未接入 Tauri IPC**（`ThrottledEmitter` 定义但未使用）
3. **Tauri `launch_start` 的 stdout/stderr 读取不完整**（无 `ManagedProcess` 封装，响应文件清理时机不当）
4. **前端类型手动维护**与 Rust struct 不同步（`ts-rs` 未实际使用）
5. **缺少 `profile_import`/`profile_export` 等计划中的接口**

---

## 第二部分：各项发现与状态评估

### 2.1 代码结构与分层

#### 2.1.1 现状

项目采用 **Cargo workspace + 多 crate** 结构，依赖拓扑为严格 DAG（无循环引用）：

```
src-tauri (应用层)
  └── rmccc-core (编排层)
       ├── rmccc-auth, rmccc-download, rmccc-version, rmccc-launch
       ├── rmccc-modloader, rmccc-mods, rmccc-profile, rmccc-config
       └── rmccc-error (叶子)
```

#### 2.1.2 问题与风险

| # | 问题 | 位置 | 严重程度 |
|---|------|------|----------|
| 1 | **`launch_start` 是 God Function**（357 行单体函数） | [`src-tauri/src/commands/launch.rs:45-320`](src-tauri/src/commands/launch.rs:45) | 🔴 严重 |
| 2 | **`CliContext::resource_download` 为 200+ 行方法** | [`src-cli/src/context.rs:113-258`](src-cli/src/context.rs:113) | 🔴 严重 |
| 3 | **`CliContext::launch_run` 为 200+ 行方法** | [`src-cli/src/context.rs:409-609`](src-cli/src/context.rs:409) | 🔴 严重 |
| 4 | **Tauri 命令直接创建 `MojangVersionProvider` 而非通过 DI** | [`src-tauri/src/commands/launch.rs:71`](src-tauri/src/commands/launch.rs:71) | 🟡 中等 |
| 5 | **`CoreContext` 不持有 `AuthProvider`，auth 逻辑分散在 Tauri/CLI 层** | [`crates/rmccc-core/src/lib.rs:26`](crates/rmccc-core/src/lib.rs:26) | 🟡 中等 |

**证据**：`launch_start` 中实现了：profile 加载 → 版本元数据获取 → Java 检测 → classpath 构建 → auth session 加载 → 参数构建 → response file 创建 → 进程启动 → stdout/stderr 读取 → 进程监控 → 清理。这些应该拆分为 `LaunchPipeline` 编排函数。

### 2.2 模块划分与职责边界

#### 2.2.1 现状

10 个 crate 的职责划分总体上清晰：
- `rmccc-error`：统一错误类型（叶子）
- `rmccc-config`：配置 + 路径 + 日志初始化
- `rmccc-auth`：认证（Microsoft OAuth2、离线、加密存储）
- `rmccc-download`：下载引擎 + Provider 抽象
- `rmccc-version`：版本管理 + 继承链解析
- `rmccc-launch`：JVM 检测 + 参数构建 + 进程管理
- `rmccc-modloader`：模组加载器安装（5/8 可用）
- `rmccc-mods`：Mod 数据类型（API 实现全空）
- `rmccc-profile`：实例管理（JSON CRUD）
- `rmccc-core`：编排层 + DI 容器

#### 2.2.2 问题与风险

| # | 问题 | 位置 | 严重程度 |
|---|------|------|----------|
| 1 | **`CoreContext` 是假 DI 容器**：不持有 `AuthProvider`、`ModLoaderProvider`、`ModSource` | [`crates/rmccc-core/src/lib.rs:26-52`](crates/rmccc-core/src/lib.rs:26) | 🔴 严重 |
| 2 | **`rmccc-profile` 的 `ModLoaderConfig.loader_type` 为 `String` 而非 `ModLoaderType`**：失去了类型安全 | [`crates/rmccc-profile/src/lib.rs:31`](crates/rmccc-profile/src/lib.rs:31) | 🟡 中等 |
| 3 | **Auth 刷新逻辑在 3 处重复**：`src-tauri/src/commands/auth.rs:8-49`、`src-cli/src/context.rs:59-78`、`rmccc-auth/src/microsoft.rs:405-470` | 多处 | 🟡 中等 |
| 4 | **`rmccc-mods` 仅有数据类型定义，无任何 API 实现** | [`crates/rmccc-mods/src/lib.rs`](crates/rmccc-mods/src/lib.rs) | 🔴 严重 |
| 5 | **`rmccc-download` 的 `DownloadManager` trait 在 `ARCHITECTURE.md` 中定义但代码中未实现**（`ConcurrentDownloadManager` 未实现该 trait） | [`crates/rmccc-download/src/lib.rs:203`](crates/rmccc-download/src/lib.rs:203) | 🟡 中等 |

### 2.3 API 设计

#### 2.3.1 现状

Tauri IPC 命令采用 `{domain}_{action}` 命名约定，已在 [`src-tauri/src/main.rs:45-65`](src-tauri/src/main.rs:45) 注册 14 个命令：

| 命令 | 状态 | 返回值 |
|------|------|--------|
| `auth_microsoft_login` | ✅ | `AuthSession` |
| `auth_offline_login` | ✅ | `AuthSession` |
| `auth_logout` | ✅ | `()` |
| `auth_get_session` | ✅ | `Option<AuthSession>` |
| `auth_refresh_token` | ✅ | `Option<AuthSession>` |
| `profile_list` | ✅ | `Vec<Profile>` |
| `profile_create` | ✅ | `Profile` |
| `profile_update` | ✅ | `Profile` |
| `profile_delete` | ✅ | `()` |
| `version_list` | ✅ | `Vec<VersionInfo>` |
| `version_install` | ✅ | `()` |
| `modloader_list` | ✅ | `Vec<LoaderVersion>` |
| `modloader_install` | ✅ | `()` |
| `launch_start` | ✅ | `()` |
| `launch_kill` | ✅ | `()` |
| `launch_status` | ✅ | `Vec<String>` |
| `settings_get` | ✅ | `AppConfig` |
| `settings_set` | ✅ | `AppConfig` |
| `java_detect` | ✅ | `Vec<JavaInfo>` |

#### 2.3.2 与前端契约的匹配度

| 前端 API (`api.ts`) | 后端命令 | 匹配 | 差异 |
|---------------------|----------|------|------|
| `auth.microsoftLogin()` | `auth_microsoft_login` | ✅ | — |
| `auth.offlineLogin(username)` | `auth_offline_login` | ✅ | — |
| `auth.logout()` | `auth_logout` | ✅ | — |
| `auth.getSession()` | `auth_get_session` | ✅ | — |
| `profiles.list()` | `profile_list` | ✅ | — |
| `profiles.create(name, gameVersion)` | `profile_create` | ✅ | 参数名 `gameVersion` vs `game_version`（Tauri 自动转换） |
| `profiles.update(id, opts)` | `profile_update` | ✅ | — |
| `profiles.delete(id)` | `profile_delete` | ✅ | — |
| `versions.list()` | `version_list` | ✅ | — |
| `versions.install(version, profileId)` | `version_install` | ✅ | — |
| `launch.start(profileId)` | `launch_start` | ✅ | — |
| `launch.kill(profileId)` | `launch_kill` | ✅ | — |
| `launch.status()` | `launch_status` | ✅ | — |
| `settings.get()` | `settings_get` | ✅ | — |
| `settings.set(opts)` | `settings_set` | ⚠️ | 前端有 `SettingsUpdate` 类型，后端使用 `SettingsUpdate` — 匹配 |
| `settings.detectJava()` | `java_detect` | ✅ | — |
| `modloaders.list(loaderType, gameVersion)` | ❌ | 🔴 | **前端定义了但后端未注册** |
| `modloaders.install(...)` | ❌ | 🔴 | **前端定义了但后端未注册** |
| `mods.*` | ❌ | 🔴 | **前端定义了 5 个 mod 方法，后端全未注册** |
| `downloads.*` | ❌ | 🔴 | **前端定义了 4 个下载控制方法，后端全未注册** |
| `logs.*` | ❌ | 🔴 | **前端定义了 2 个日志方法，后端全未注册** |

#### 2.3.3 缺失接口清单

| 接口 | 优先级 | 阻塞原因 |
|------|--------|----------|
| `profile_import` / `profile_export` | P2 | 设计文档中有，未实现 |
| `modloader_list` (Tauri) | P0 | 已在 `main.rs` 注册 ✅，上述表格有误 — 实际已注册 |
| `modloader_install` (Tauri) | P0 | 同上，已注册 |
| `mod_search` | P1 | `rmccc-mods` API 为空 |
| `mod_install` | P1 | 同上 |
| `mod_remove` | P1 | 同上 |
| `mod_list` | P1 | 同上 |
| `mod_toggle` | P1 | 同上 |
| `download_cancel` | P1 | `ConcurrentDownloadManager` 已有 `cancel_batch`，但无 IPC 包装 |
| `download_cancel_batch` | P1 | 同上 |
| `download_pause` | P1 | 同上 |
| `download_resume` | P1 | 同上 |
| `log_get` | P2 | 日志文件存在但无读取接口 |
| `log_clear` | P2 | 同上 |

**修正**：重新检查 [`src-tauri/src/main.rs:63-64`](src-tauri/src/main.rs:63)，`modloader_list` 和 `modloader_install` 已注册。但前端 `api.ts` 中 `modloaders.list` 调用方式与后端签名不完全匹配（后端需要 `loader_type: String, game_version: String`）。

### 2.4 数据模型与存储

#### 2.4.1 现状

- 所有持久化使用 JSON 文件：`config.json`、`auth.json`（加密）、`profiles/{id}.json`、`cache/versions.json`
- `Profile` 包含 `game_dir: PathBuf` — 序列化正确
- `GameVersion` 支持 `#[serde(rename)]` 以保持 Mojang 原始 camelCase 格式

#### 2.4.2 问题与风险

| # | 问题 | 位置 | 严重程度 |
|---|------|------|----------|
| 1 | **配置文件无 Schema 版本号**：格式变更时无法自动迁移 | [`crates/rmccc-config/src/lib.rs:14`](crates/rmccc-config/src/lib.rs:14) | 🟡 中等 |
| 2 | **JSON 文件写入无原子性保证**：直接 `fs::write`，断电可能损坏 | [`crates/rmccc-profile/src/lib.rs:89`](crates/rmccc-profile/src/lib.rs:89) | 🟡 中等 |
| 3 | **无配置热重载**：修改 `config.json` 后需重启应用 | — | 🟢 低 |
| 4 | **`ModLoaderConfig.loader_type` 使用 `String`** 而非枚举，无法编译期校验 | [`crates/rmccc-profile/src/lib.rs:31`](crates/rmccc-profile/src/lib.rs:31) | 🟡 中等 |
| 5 | **Profile 的 `game_dir` 与 `config.game_root_dir` 无关联校验** | — | 🟢 低 |

### 2.5 认证授权

#### 2.5.1 现状

- Microsoft OAuth2 device code flow 完整实现（5 步：device code → MS token → XBL → XSTS → MC token）
- 离线认证使用 UUID v5 确定性生成
- AES-256-GCM 加密存储，key file 权限 `0o600`
- Token 自动刷新（`auth_get_session` 和 `auth_refresh_token` 中）
- Response file 防止 `ps aux` 泄露 `--accessToken`

#### 2.5.2 问题与风险

| # | 问题 | 位置 | 严重程度 |
|---|------|------|----------|
| 1 | **单账户模式**：仅支持一个活跃 session，无法切换账户 | 全局 | 🟡 中等 |
| 2 | **Device code flow 无超时后自动重试机制** | [`rmccc-auth/src/microsoft.rs:99-167`](crates/rmccc-auth/src/microsoft.rs:99) | 🟢 低 |
| 3 | **Token 刷新在多个位置实现**（Tauri auth 命令、CLI context、MicrosoftAuthProvider） | 多处 | 🟡 中等 |
| 4 | **无 Authlib-Injector（外置登录）支持** | — | 🟢 低（远期需求） |
| 5 | **`OfflineAuthProvider::authenticate()` 返回错误而非实现**：设计不合理 | [`rmccc-auth/src/offline.rs:40-49`](crates/rmccc-auth/src/offline.rs:40) | 🟢 低 |

### 2.6 依赖与配置管理

#### 2.6.1 现状

- Workspace 依赖集中在根 [`Cargo.toml:18-90`](Cargo.toml:18)
- `Cargo.lock` 已提交（良好实践）
- `release` profile 已配置 `lto = true`、`strip = true`、`panic = "abort"`
- 环境变量 `RUST_LOG` 控制日志级别

#### 2.6.2 问题与风险

| # | 问题 | 位置 | 严重程度 |
|---|------|------|----------|
| 1 | **无 `rust-toolchain.toml`**：CI 和开发者可能使用不同 Rust 版本 | 根目录 | 🟡 中等 |
| 2 | **`tauri.conf.json` 中 `macOSPrivateApi: true`**：可能导致 Mac App Store 审核被拒 | [`src-tauri/tauri.conf.json:30`](src-tauri/tauri.conf.json:30) | 🟢 低 |
| 3 | **`rmccc-download/src/forge/install_task.rs:112` 每次创建新 `reqwest::Client`** 而非复用 | [`rmccc-modloader/src/forge/install_task.rs:112`](crates/rmccc-modloader/src/forge/install_task.rs:112) | 🟡 中等 |
| 4 | **无 `cargo-deny` / `cargo-audit` 依赖漏洞扫描** | — | 🟡 中等 |

### 2.7 日志记录与异常处理

#### 2.7.1 现状

- `tracing` + `tracing-appender` 已集成（Phase 1）
- 控制台：彩色、INFO 级别；文件：JSON 结构化、日滚动、最多 10 个文件
- IPC 命令使用 `#[tracing::instrument(skip(state))]`
- 统一错误类型 `RmcccError` → `IpcError` 桥接，包含 `code` + `message` + `retryable`

#### 2.7.2 问题与风险

| # | 问题 | 位置 | 严重程度 |
|---|------|------|----------|
| 1 | **无请求追踪 ID**：跨模块调用无法关联日志 | 全局 | 🟡 中等 |
| 2 | **下载进度事件未通过 `ThrottledEmitter` 发出**：`version_install` 中使用空回调 | [`src-tauri/src/commands/version.rs:116-118`](src-tauri/src/commands/version.rs:116) | 🟡 中等 |
| 3 | **部分 `unwrap()` 在生产代码中**：如 [`microsoft.rs:81`](crates/rmccc-auth/src/microsoft.rs:81) 的 `unwrap_or_default()` — 实际安全但不够规范 | 多处 | 🟢 低 |
| 4 | **ErrorCode 缺少 `VersionNotInstalled` 等语义码** | [`crates/rmccc-error/src/lib.rs:94`](crates/rmccc-error/src/lib.rs:94) | 🟢 低 |
| 5 | **敏感字段 `access_token` 在日志中出现**：`#[tracing::instrument]` 应使用 `skip` 标记 token 字段 | 多处 | 🟡 中等 |

### 2.8 安全防护

#### 2.8.1 现状

- ✅ AES-256-GCM token 加密存储
- ✅ Response file 防止命令行泄露 token
- ✅ 路径遍历防护（`safe_join`）
- ✅ Tauri capability 最小化权限模型
- ✅ 内存值 clamp（`memory_mb: clamp(256, 65536)`）
- ✅ 磁盘空间预检

#### 2.8.2 问题与风险

| # | 问题 | 位置 | 严重程度 |
|---|------|------|----------|
| 1 | **`.key` 文件在 Windows 上未设置 ACL 限制**（仅有 Unix `0o600`） | [`rmccc-auth/src/vault.rs:43-47`](crates/rmccc-auth/src/vault.rs:43) | 🟡 中等 |
| 2 | **`launch_start` 的输出读取直接使用 `from_utf8_lossy`**：可能产生乱码但不影响功能 | [`src-tauri/src/commands/launch.rs:248`](src-tauri/src/commands/launch.rs:248) | 🟢 低 |
| 3 | **无输入长度限制**：username 可任意长（虽不影响安全但影响 UI） | — | 🟢 低 |

### 2.9 性能与可扩展性

#### 2.9.1 现状

- `reqwest::Client` 连接池：`pool_max_idle_per_host: 32`
- 并发下载 worker 可配置（默认 16）
- `DashMap` 用于无锁并发状态
- `ThrottledEmitter` 限制前端事件频率（100ms）
- SHA1 缓存仓库（`CacheRepository`）避免重复下载

#### 2.9.2 问题与风险

| # | 问题 | 位置 | 严重程度 |
|---|------|------|----------|
| 1 | **无数据库连接池**（当前用 JSON 文件，未来若迁移 SQLite 需 pool） | — | 🟢 低 |
| 2 | **无请求级缓存策略**：版本列表每次完整重新获取，仅依赖文件缓存 | [`crates/rmccc-version/src/cache.rs`](crates/rmccc-version/src/cache.rs) | 🟡 中等 |
| 3 | **下载大文件时无分块进度**：整个文件加载到内存（`bytes()`） | [`crates/rmccc-download/src/lib.rs:594`](crates/rmccc-download/src/lib.rs:594) | 🟡 中等 |
| 4 | **无并发启动限制**：理论可同时起 N 个 MC 实例耗尽内存 | [`src-tauri/src/state.rs:25`](src-tauri/src/state.rs:25) | 🟢 低 |
| 5 | **响应文件在进程退出后才清理**：若 crash 可能残留 | [`src-tauri/src/commands/launch.rs:298`](src-tauri/src/commands/launch.rs:298) | 🟢 低 |

### 2.10 测试覆盖与代码质量

#### 2.10.1 现状

- 单元测试分布（约 120+ tests）：
  - `rmccc-version/src/arguments.rs`：29 tests（模板解析、规则匹配）
  - `rmccc-launch/src/process.rs`：18 tests（JVM 参数构建、dedup）
  - `rmccc-download/src/provider.rs`：14 tests（URL 注入规则）
  - `rmccc-modloader/src/analyzer.rs`：14 tests（modloader 检测）
  - `rmccc-modloader/src/forge/profile.rs`：3 tests
  - `rmccc-modloader/src/patch.rs`：3 tests
  - `rmccc-auth/src/vault.rs`：1 test（加密往返）
  - `rmccc-auth/src/offline.rs`：2 tests
  - `rmccc-config/src/paths.rs`：2 tests
  - `rmccc-launch/src/natives.rs`：8 tests
  - 等
- 测试日志文档记录的 CLI 端到端测试通过率：100%

#### 2.10.2 问题与风险

| # | 问题 | 位置 | 严重程度 |
|---|------|------|----------|
| 1 | **无集成测试（wiremock）**：Mojang/Modrinth/CurseForge API 未模拟 | 全局 | 🔴 严重 |
| 2 | **无 E2E 测试**：`tauri-driver` 未配置 | 全局 | 🔴 严重 |
| 3 | **`rmccc-download` 无网络层测试**：重试、断点续传、SHA1 校验未在测试中验证 | [`crates/rmccc-download/src/lib.rs`](crates/rmccc-download/src/lib.rs) | 🟡 中等 |
| 4 | **`rmccc-launch/src/managed.rs` 仅有一个构造测试**，无进程生命周期测试 | [`crates/rmccc-launch/src/managed.rs:168`](crates/rmccc-launch/src/managed.rs:168) | 🟡 中等 |
| 5 | **无 `cargo clippy` CI 门禁**：`#![allow(dead_code, unused_imports)]` 出现在 [`src-tauri/src/main.rs:1`](src-tauri/src/main.rs:1) | — | 🟡 中等 |
| 6 | **前端类型使用手动维护的 TypeScript interface 而非 `ts-rs` 自动生成** | [`src/ipc/api.ts:10-87`](src/ipc/api.ts:10) | 🟡 中等 |

### 2.11 部署与运维

#### 2.11.1 现状

- `.github/` 目录仅含一个 `skills/` 目录，**无 CI/CD workflow**
- 无 Dockerfile
- 无 docker-compose
- 无监控配置
- Tauri `tauri.conf.json` 配置了 `beforeBuildCommand` 和 `beforeDevCommand`
- 版本号 `0.1.0`

#### 2.11.2 问题与风险

| # | 问题 | 位置 | 严重程度 |
|---|------|------|----------|
| 1 | **无 GitHub Actions CI/CD**：无法自动化构建、测试、发布 | `.github/` | 🔴 严重 |
| 2 | **无跨平台构建验证**：仅开发者在本地 macOS 测试 | — | 🔴 严重 |
| 3 | **`tauri-plugin-updater` 已引入但未配置**：无自动更新端点 | [`src-tauri/Cargo.toml:35`](src-tauri/Cargo.toml:35) | 🟡 中等 |
| 4 | **无日志聚合方案**：日志仅写本地文件 | — | 🟢 低 |
| 5 | **`package.json` 中有 `pnpm-lock.yaml` 但版本管理不严格** | — | 🟢 低 |

---

## 第三部分：按优先级排序的改进措施清单

### P0（阻断性 — 必须立即修复，阻塞前端对接和上线）

| # | 改进项 | 目标 | 具体行动 | 预期效果 | 工作量 |
|---|--------|------|----------|----------|--------|
| P0-1 | **实现 Mod 管理 API** | `rmccc-mods` 中实现 Modrinth API（搜索、版本列表、下载链接） | 1. 实现 `ModrinthSource` 的 `ModSource` trait；2. 注册 `mod_search/mod_install/mod_remove/mod_list/mod_toggle` 5 个 IPC 命令；3. 前端 `api.ts` 已就绪 | Mod 搜索和安装可用 | 大 |
| P0-2 | **实现下载进度事件推送** | 前端能看到实时下载进度 | 1. 在 `version_install` 中接入 `ThrottledEmitter`；2. 在 `modloader_install` 中发射进度；3. 前端监听 `download://progress` | 用户看到下载进度条 | 中 |
| P0-3 | **修复 `launch_start` 使用 `ManagedProcess`** | 进程管理标准化，解决 stdout/stderr 读取不完整问题 | 1. 重构 `launch_start` 使用 `spawn_managed()`；2. 通过 `drain_lines()` 批量发射输出事件；3. 响应文件在 spawn 后立即清理 | 游戏输出完整显示，无残留文件 | 中 |
| P0-4 | **CI/CD 配置** | 三平台自动构建 + 测试 | 1. 创建 `.github/workflows/ci.yml`；2. 添加 `cargo test --workspace` + `cargo clippy` + `cargo fmt --check`；3. 添加 `pnpm typecheck`；4. 三平台矩阵构建 | 每次 PR 自动验证 | 中 |
| P0-5 | **Tauri 前端集成测试** | 验证所有 IPC 命令可在前端正常调用 | 1. 编写前端组件测试；2. 验证 19 个 IPC 命令的请求/响应格式一致；3. 修正 `api.ts` 与后端签名不匹配处 | 前后端契约验证通过 | 中 |

### P1（高风险 — 本 Phase 内必须修复）

| # | 改进项 | 目标 | 具体行动 | 预期效果 | 工作量 |
|---|--------|------|----------|----------|--------|
| P1-1 | **完善 `CoreContext` DI 容器** | 所有 Provider 通过 DI 获取，消除临时创建 | 1. 在 `CoreContext` 中添加 `auth_provider: Box<dyn AuthProvider>`；2. 添加 `modloader_providers: HashMap<ModLoaderType, Box<dyn ModLoaderProvider>>`；3. 迁移 Tauri 命令使用 `core.xxx_provider` | 可替换实现、可测试 | 中 |
| P1-2 | **拆分 God Functions** | `launch_start` 和 `CliContext` 拆分为 Pipeline 编排函数 | 1. 创建 `rmccc-core/src/pipeline.rs`；2. 提取 `install_game`、`launch_game` 等编排函数；3. 各命令处理器调用编排函数 | 可复用、可测试、可维护 | 大 |
| P1-3 | **消除 Auth 逻辑重复** | Token 刷新逻辑统一在一处 | 1. 在 `CoreContext` 中添加 `refresh_session_if_needed()`；2. Tauri 和 CLI 都调用此方法；3. 删除 `try_refresh_if_needed` 和 CLI 中的重复代码 | 一致性保证，减少 bug | 小 |
| P1-4 | **配置文件 Schema 版本化** | 配置格式变更时自动迁移 | 1. 在 `config.json` 中添加 `version: 1`；2. 实现 `ConfigMigrator`；3. 加载时检测版本并迁移 | 向前兼容，用户无感升级 | 小 |
| P1-5 | **接入 `ts-rs` 自动类型生成** | 前端类型与 Rust struct 自动同步 | 1. 为所有 IPC 相关 struct 添加 `#[derive(TS)]`；2. 配置 build.rs 导出到 `src/ipc/types.ts`；3. 删除手动维护的 interface | 类型安全，零维护成本 | 小 |
| P1-6 | **添加集成测试 (wiremock)** | 关键 API 路径有集成测试覆盖 | 1. Mock Mojang version manifest API；2. 测试 `fetch_versions` + `fetch_version`；3. 测试 `resolve_version` 继承链 | 重构安全网 | 中 |

### P2（中风险 — 下个 Phase 修复）

| # | 改进项 | 目标 | 具体行动 | 预期效果 | 工作量 |
|---|--------|------|----------|----------|--------|
| P2-1 | **实现 Mod 本地管理** | mod 安装/卸载/启用/禁用完整功能 | 1. 实现 `mods/` 目录管理；2. 依赖解析和提示；3. `mod_toggle` 移动 JAR 到 `disabled/` 子目录 | 完整 Mod 管理 | 中 |
| P2-2 | **实现 CurseForge API** | 第二个 Mod 源 | 1. 实现 `CurseForgeSource`；2. API key 管理（环境变量 + key file）；3. 统一搜索接口 | 双源 Mod 搜索 | 中 |
| P2-3 | **添加 `profile_import/export`** | 实例导入导出 | 1. 实现 JSON 导入导出；2. 可选包含资源文件 | 实例迁移 | 小 |
| P2-4 | **EventBus 事件总线** | 模块间解耦通信 | 1. 创建 `rmccc-task` crate 或直接在 `rmccc-core` 中添加 EventBus；2. 下载进度、启动状态等通过事件传播 | 松耦合 | 大 |
| P2-5 | **配置自动更新** | `tauri-plugin-updater` 可用 | 1. 配置 GitHub Releases 作为更新源；2. 签名验证；3. UI 提示 | 用户自动更新 | 中 |
| P2-6 | **Windows ACL 限制 .key 文件** | 跨平台安全一致性 | 1. 添加 Windows `icacls` 调用设置 key 文件权限为当前用户 only | Windows 平台安全性 | 小 |
| P2-7 | **依赖漏洞扫描** | CI 中集成 `cargo-audit` | 1. 添加 CI step: `cargo audit`；2. 定期检查依赖安全性 | 及时发现已知漏洞 | 小 |
| P2-8 | **`rust-toolchain.toml`** | CI 和开发者版本一致 | 1. 创建 `rust-toolchain.toml` 指定 stable 版本 | 避免版本差异 | 小 |

---

## 第四部分：总结性审计意见

### 4.1 对后端整体架构的中长期建议

1. **Task 框架是必要投资**：当前所有异步操作直接使用 `tokio::spawn`，导致进度传播、取消链接、错误聚合困难。建议在 Phase 5 之前引入 Task 框架（类似 HMCL 的 `Task<T>` + `TaskExecutor`），统一管理下载、安装、启动等长时间操作。

2. **DI 容器是架构基石**：`CoreContext` 当前仅为半成品，必须让它成为真正的 DI 容器，持有所有 Provider trait object。这将直接促进可测试性（mock 注入）和可扩展性（新 Provider 注册）。

3. **Pipeline 编排层应独立**：当前 CLI 和 Tauri 各自实现了下载→安装→启动的编排逻辑。将这些流程提取到 `rmccc-core/src/pipeline.rs` 中，是减少重复、提高一致性的关键。

4. **JSON 存储够用但需原子写入**：当前 Profile 数量少（< 100），JSON 文件存储足够。但应在写入时使用"写临时文件 → rename"模式保证原子性。若未来支持大型 Modpack（100+ mods），可迁移 SQLite。

5. **测试投资不应拖延**：集成测试（wiremock）和 E2E 测试（tauri-driver）应尽早建立，否则每次架构调整都是盲飞。建议在 P1 阶段完成集成测试框架搭建。

### 4.2 关键技术债务

| 债务 | 估算偿还成本 | 搁置风险 |
|------|-------------|----------|
| God Functions 5 个（200+ 行） | 大 | 功能添加困难，合并冲突频繁 |
| 假 DI 容器 | 中 | 无法 Mock 测试，Provider 替换需改代码 |
| Mod 管理模块全空 | 大 | Phase 4 无法启动 |
| 无集成测试 | 中 | 重构风险高 |
| 无 CI/CD | 中 | 无法保证多平台质量 |
| Auth/CLI 重复逻辑 | 小 | 不一致风险 |
| 下载进度事件未接入 | 小 | 前端无进度反馈 |

### 4.3 下一步应立即启动的推进事项

按优先级顺序：

1. **（本周）CI/CD 搭建**：`.github/workflows/ci.yml` — 至少包含 `cargo test --workspace` + `cargo clippy` + `cargo fmt --check`
2. **（本周）修复 `launch_start` 接入 `ManagedProcess`** — 解决 stdout/stderr 读取完整性问题
3. **（下周）实现 Modrinth API** — 打通 Mod 搜索→安装流程
4. **（下周）接入 `ts-rs` + 下载进度事件** — 前端可完整对接
5. **（两周内）重构 DI 容器 + 拆分 God Functions** — 架构债务清偿
6. **（两周内）集成测试框架搭建** — wiremock + 关键路径测试

---

> **审计结论**：RMCCC 后端核心引擎（游戏下载、启动、Modloader 安装）功能完备，CLI 端已验证通过。但前端对接所需的 Mod 管理、下载进度、进程监控接口仍存在缺失。架构层面存在 God Function、假 DI 容器、重复逻辑等中等债务。建议按 P0→P1→P2 顺序推进改进，优先完成 Modrinth API、CI/CD、下载进度事件和 `ManagedProcess` 重构，以在 2-4 周内达到可完整对接前端的状态。
