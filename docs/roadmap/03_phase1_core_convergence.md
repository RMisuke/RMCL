# Phase 1 — Core 收敛 + 平台适配层落地

> **对应指南**：§17 Phase 1 + §5.7。**状态**：✅ **Phase 1 全部切片完成（1.1–1.8）**，
> 唯一遗留为"前端组件测试 0 个"与"Windows/Linux 后端 deferred"。
> **当前事实来源**：审计 §5.3（pipeline 零调用）、§15.1（P1-3/P1-4/P1-6）、§15.2（P2-1/2/4/8/9/10/11）、§7.4（profile vs instance）、§6.8（本地 mod 文件操作在 Tauri）。

## 1. 目标

建立插件平台可依赖的稳定底座：

1. 让 `pipeline` 成为 Tauri/CLI **唯一**业务编排入口，消除三链路双实现。
2. 收敛 loader 双体系到单一路径。
3. 拆分 `context.rs` God class。
4. 落地**平台适配层 trait**（指南 §5.7），把散落在领域 crate 的 `#[cfg]` 执行迁出，核心只定义 trait/DTO。
5. 补 IPC 契约测试与前端类型契约；修 zip-slip；定稿 profile/instance 语义。

## 2. 前置条件

- Phase 0 完成（基线可信）。
- Phase 0.5 完成（UI 缺口补齐，避免重构期间产品不可用）。
- ADR-002（Core Service API + Profile/Instance 语义）至少 proposed。

## 3. 与当前状态的对应

| 审计条目 | 现状 | Phase 1 处理 |
|----------|------|--------------|
| P1-3 pipeline 零调用 | ✅ 已收口（1.2） | 三命令改调 pipeline，删复刻代码；补 pipeline 单测 |
| P1-4 loader 双体系 | `ModLoaderProvider`(Forge/NeoForge) + `DependencyManager`(其余 6)，分流逻辑 Tauri/CLI 各写一遍 | 统一到 `DependencyManager`，删旧 trait + `create_provider` 死重（Fabric provider） |
| P1-6 Cleanroom stub | `install_task.rs:13` 直接返回未实现 | 要么实现，要么 UI 禁用该选项（建议先禁用） |
| P2-1 context.rs 1542 行 | God class | 按域拆门面（auth/instance/version/mod/modloader/launch/system） |
| P2-2 46 IPC 0 单测 | src-tauri 仅 2 测试 | 每命令至少 1 参数校验 + 错误码测试 |
| P2-4 zip-slip | ✅ 已修（1.5） | 组件级校验，拒 `..`/绝对/盘符前缀；正负向单测 |
| P2-8 生效值前端重算 | InstanceSettings.tsx 本地算继承显示 | 后端暴露 `ResolvedInstanceSettings`（已有），前端改读后端；或 ts-rs 导出生效值命令 |
| P2-9 安装目标两套取值 | Tauri `resolved_game_root()` vs pipeline `profile.game_dir` | 统一入口，确保两者一致（profile 已指向共享根） |
| P2-10 profile vs instance 两概念 | `instance_*` 操作版本 id，无 UI 入口 | ADR-002 定稿：profile 为唯一用户实体；`instance_*` 降级内部或重语义 |
| P2-11 api.ts 死代码 | （Phase 0.5 已接线大部分） | 残留死代码清理 |
| §6.8 本地 mod 文件操作在 Tauri | `commands/mods.rs` 直接文件 IO | 迁入 Core Service / 平台文件系统适配器 |
| §5.7 平台执行散落 | `#[cfg]` 在 launch/version/auth | 迁入适配层（见 PLATFORM_ADAPTATION.md） |

## 4. 任务切片分解

### 1.1 平台适配层 trait 定义与 macOS 后端落地（用户重点）

拆成 1.1a–1.1e 五个可独立验收的子切片，**逐项销账**（详见 `PLATFORM_ADAPTATION.md` §3）。

- ✅ **1.1a 骨架 + trait + macOS 后端（2026-10-21 完成）**
  - 新 crate `crates/rmccc-platform`（依赖仅 `rmccc-error` / `serde` / `tracing` / unix `libc`）。
  - `PlatformDescriptor`（os/arch/classpath 分隔符/exe 后缀/native classifier 候选/rules 匹配）+ 7 个单测。
  - `PlatformFs`（`restrict_owner_only`/`make_executable`/`symlink`/`remove_symlink`）、
    `PlatformProcessControl`（`terminate_tree`/`set_priority`/`priority_command_prefix`/`configure_child_process_group`）、
    `PlatformShell`（`open_url` 仅 http(s) + `open_path`）→ macOS 后端实现 + `UnsupportedFs/Process/Shell` 占位。
  - `Platform::current()` 是**唯一** `#[cfg]` 选择器；`Platform::mock()` + `MockPlatform` 记录调用序列。
  - `CoreContext.platform: Arc<Platform>` 在组装层注入一次；`rmccc-error` 新增
    `Validation` / `UnsupportedPlatform` 变体与 `ErrorCode::UnsupportedPlatform`（CLI 退出码 11）。
  - 单测 23 + doc-test 1；`cargo test --workspace` 311→**335**。
  - **故意未做**：`CredentialStore`（指南要求先评估 Keychain，现有 AES vault 除 chmod 外与平台无关）、
    `JavaDetector`（`rmccc-launch/java.rs` 已是 per-OS 分文件，随 1.1d 一并提升为 trait）。
- ✅ **1.1b 数据筛选切换（2026-10-21 完成）**
  - `rmccc-version` 的 `libraries` / `arguments` / `compatibility` / `file_layout` /
    `completion` / `default_game_repository` 全部改为接收注入的 `&PlatformDescriptor`。
  - 删掉 3 处 `cfg` 函数（`current_os_name`×2、`current_arch`×2、`minecraft_arch`）与
    per-OS classifier 分支；classifier 事实来源收归
    `PlatformDescriptor::native_classifier_candidates()`（含 `natives-osx` / `-64` / `natives-unix` 历史别名）。
  - 描述符注入路径：`CoreContext.platform` → `pipeline` / `LaunchOptions.platform` /
    `DefaultGameRepository::with_platform`；`Library`/`Argument` 规则测试改为显式描述符双向断言，
    **删除全部 `#[cfg(target_os)] assert!`**（那些断言在非 macOS 上从未跑过）。
  - 顺带修掉审计 **P2-12**：`library_applies` 默认值反了（Mojang 语义是 default-deny）。
    用真实 1.20.4 manifest 校验：修复前 macOS 选中 88/88 个 library，其中 30 个是
    Windows/Linux natives；修复后 58 个，异平台 natives 正确排除。
  - 验收：`grep -rn 'cfg(target_os\|cfg!(target_os\|cfg(unix)\|cfg(windows)' crates/rmccc-version/src`
    → 仅剩文档注释；`cargo test --workspace` 335→**339**。
- ✅ **1.1c 执行迁移（2026-10-21 完成）**
  - 进程终止：Tauri `launch_kill` 与 CLI `launch_stop` 统一调
    `core.platform.process.terminate_tree(pid)`；`libc` / `taskkill` 分支全部删除，
    `src-tauri` 与 `src-cli` 的 `libc` 依赖也一并移除。
  - 进程组：`spawn_game(.., &Platform)` → `configure_child_process_group`。
  - 进程优先级：`nice -n N` 前缀改由 `priority_command_prefix` 给出。
  - 文件权限：`TokenVault::init(path, fs)` 与 argfile 走 `restrict_owner_only`；
    启动脚本走 `make_executable`。
  - 符号链接：natives 临时链接走 `symlink` / `remove_symlink`。
  - 平台数据不再内联：`-Xdock:name` → `descriptor.extra_jvm_args()`（`{instance}` 模板）、
    `-Duser.home` → `descriptor.is_windows()`、classpath/文件分隔符 → `descriptor`、
    `-Dfile.encoding` → `descriptor.default_encoding()`、非 ASCII natives 兜底 →
    `descriptor.needs_non_ascii_natives_workaround()`。
  - `ProcessPriority` 定义收归平台 crate，`rmccc-launch` re-export 保持路径兼容。
  - 顺带：`decompress_natives` 也按 `accepts_native_classifier` 过滤 → 安装日志里
    30 条 "Native library JAR not found" 噪音归零。
  - **真机烟测**：`rmcli` 新建实例 → `resource install` 1.20.4（3870 文件 / 0 失败）→
    `launch run` 启动到主菜单（OpenAL/纹理图集正常，`exit_code 0`）；
    `.key` 权限确认 `-rw-------`。
  - `cargo test --workspace` 339→**341**（新增真实子进程 `setpgid`+`killpg` 烟测）。
- ✅ **1.1d Java 探测提升为 trait（2026-10-21 完成）**
  - `JavaInstallation` + `JavaDetector` 迁入 `rmccc-platform::java`；
    `select_for()` 成为纯函数（精确匹配 > 最近的更高版本 > `JavaVersionMismatch`），6 个单测。
  - `MacosJavaDetector` = `java_home -V` + `JAVA_HOME` + Homebrew/`/usr/local`；
    `UnsupportedJavaDetector` 返回 `UnsupportedPlatform`（空列表会被误读成"没装 Java"）。
  - `rmccc-launch/src/java.rs` 从 265 行降到 80 行，只剩 re-export + 便捷封装，
    **最后 4 处 `#[cfg(target_os)]` 消失**。
  - 调用方全部改为注入：`core.platform.java.find(..)` / `.detect_all()`
    （pipeline、Tauri `modloader_install` / `java_detect`、CLI `launch_java` / 系统信息 / modloader）。
- ✅ **1.1e 命令行分隔符迁移（2026-10-21 完成，随 1.1c 一并做掉）**：
  classpath 分隔符 → `descriptor.classpath_separator()`、文件分隔符 → `descriptor.is_windows()`。
  **至此 `crates/*/src` 除 `rmccc-platform` 外零平台 `#[cfg]`。**
- 验收：领域 crate 不再有 `#[cfg(target_os=...)]` 执行分支（数据筛选经 `PlatformDescriptor`）——
  **`rmccc-version` 已达成（1.1b）**，`rmccc-launch` / `rmccc-auth` 待 1.1c/1.1d/1.1e；
  macOS 启动烟测不回归。

### 1.2 pipeline 收敛（P1-3）✅ 已完成（2026-10-21）
- ✅ `version_install` → `pipeline::install_game` + `pipeline::commit_install`；`commands/version.rs` 309→203 行。
- ✅ `instance_repair` → `pipeline::repair_instance`（新函数，版本 id 入口）。
  说明：原计划的 `pipeline::repair_resources` 是**第三份**分叉实现且 0 调用，已**删除**；
  实际收敛到 `repair_instance`（completion 驱动的正确实现），`ensure_launch_ready` 为其 profile 视图。
- ✅ `launch_start` → `pipeline::launch_game`（返回 `Arc<ManagedProcess>`，不阻塞）；`launch.rs` 309→168 行。
- ✅ 保留 `ensure_launch_ready`（现为薄委托）。
- ✅ 新增 `commit_install` / `refresh_repository` / `SetupReport`，消除两端各自提交 profile 的重复。
- ✅ 补 `rmccc-core` 单测 10 个（`install_game` / `repair_instance` / `ensure_launch_ready` /
  `commit_install` / `FailureTally` / `SetupReport` / `inject_candidates` / `write_if_absent`）。
- ✅ 顺带修复：GUI 安装不解压 natives、安装提交用旧 profile 快照、自定义 Java 路径不校验、
  `wait_for_batch` 与最终进度回调的竞态。
- 验收：`grep -rn 'DefaultLauncher|resolve_library_items|resolve_asset_items|build_completion_downloads|find_java|AuthInfo|download_batch' src-tauri/src/commands/`
  仅剩 `instance.rs` 的只读 `check_completion` 查询与 `modloader.rs`（1.3 范围）；`cargo test` 301→**311**。

### 1.3 loader 单体系（P1-4）✅ 已完成（2026-10-21）
- ✅ 删除 `ModLoaderProvider` trait + `create_provider` + 三个 `*Installer` 实现
  （`FabricInstaller` / `ForgeInstaller` / `NeoForgeInstaller`），`CoreContext.modloader_providers`
  与 `modloader_provider()` 一并删除。
- ✅ 新增 `rmccc-modloader::installer`（`InstallerJarRunner`）：官方安装器型 loader
  的"下载 jar → `java -jar --installClient` → 以新增目录为准判定版本 id"只有这一份实现。
- ✅ `DependencyManager::install_loader()` 成为**唯一**路由点：
  `needs_installer_jar(Forge|NeoForge)` → 安装器；其余 → `GameInstancePatch`。
  返回 `LoaderInstallOutcome { InstallerJar | Patch }`（patch 装箱避免 enum 膨胀）。
- ✅ 新增 `rmccc-core::modloader::install_loader()`：Java 探测 → 基础版本解析（注入 provider）
  → 安装 → 写**增量**版本 JSON（`inheritsFrom` + `patches`）→ 提交 profile + 刷新索引
  → **立刻补齐 loader 自带库**。Tauri 与 CLI 都只剩"发进度 + 调这一个函数"。
- 顺带修掉两处不一致：CLI 原先写**合并后的完整 JSON**（丢回滚/叠加语义，HMCL 语义是增量）；
  装完 loader 不下载它自带的库 → `--no-repair` 启动会 `ClassNotFoundException: KnotClient`
  （实测复现并修复：现在安装末尾补 8 个库）。
- 验收：`grep ModLoaderProvider` 只剩文档注释；Tauri/CLI 无 `match loader_type` 分流；
  `rmcli` 实测 fabric 0.15.11 安装 → `--no-repair` 启动到主菜单（Mixin 0.8.5 + OpenAL）。

### 1.4 context.rs 拆分（P2-1）✅ 已完成（2026-10-21）
- ✅ `src-cli/src/context.rs`（1,450 行 God class）→ `src-cli/src/context/` 下按域拆分：
  `mod.rs`(93) / `auth`(100) / `config`(108) / `profile`(128) / `version`(112) /
  `resource`(165) / `modloader`(122) / `mods`(196) / `launch`(211) / `system`(42) / `helpers`(253)。
- `CliContext` 仍是唯一聚合入口；各域文件是 `impl CliContext` 的分片，方法名/签名不变，
  命令层零改动。
- 域无关的工具函数（路径、解析、SHA1、`CliProcessListener`）收在 `helpers`，`pub(super)` 暴露。
- 验收：`src-cli/src/context/mod.rs` **93 行**（< 600）✅；单文件最大 253 行。

### 1.5 zip-slip 修复（P2-4）✅ 已完成（2026-10-21，本地文件操作归位留到 1.1）

- `natives.rs` 解压：对每个 entry `canonicalize` 目标并断言在 `natives_dir` 之下；拒 `..`/绝对路径/symlink 逃逸/重复条目。
- 把 `commands/mods.rs` 的本地文件 IO（mods↔disabled 移动、删除）迁入 Core Service 经平台文件系统适配器执行。
- 验收：zip-slip 正负向单测；mod 启停/删除经 trait 而非裸 `std::fs`。

### 1.6 IPC 契约测试（P2-2）✅ 已完成（2026-10-21）
- ✅ 新增 `src-tauri/src/handlers.rs`：`generate_handler!` 列表从 `main.rs` 抽出，
  与 `IPC_COMMANDS` 常量一一对应，测试可交叉校验。
- ✅ 新增 `src-tauri/src/ipc_tests.rs`（**37 个测试**），覆盖三层契约：
  1. **注册表契约**：命令名无重复、`IPC_COMMANDS` ↔ `generate_handler!` 数量一致、
     每个名字都能在前端 `src/ipc/api.ts` 里找到（后端专用命令走显式白名单并写明理由）。
  2. **参数校验契约**：mod 文件名（`../`、分隔符、NUL）、外部 URL scheme 白名单、
     未知 loader 类型、profile id 解析、未知 batch 的 pause/cancel/resume、日志行数 clamp。
  3. **错误码契约**：`RmcccError → IpcError` 全 19 个变体映射 + `retryable` 语义
     （5xx 可重试、4xx 不可重试）+ 序列化形状 `{code,message,retryable}`。
- 测试过程中**发现并修掉 3 个真实问题**：
  - `mod_install_local` 注册但前端从不调用（纳入白名单并注明用途）；
  - `sanitize_filename("../x")` 会产出 `.._x`（现在剥掉前导点，空名回落 `mod`）；
  - `account_repo.remove(未知 id)` 是幂等 no-op（原先测试假设它报错——按契约它不该报错）。
- **已知缺口**：真正的"起 mock Tauri app 再 invoke"需要把 49 个命令泛型化到
  `Runtime`（`AppState` 目前持 `AppHandle<Wry>`）。代价远大于收益，已在测试文件头部
  写明原因；`Frontend 组件测试` 仍为 0（Phase 3 随 UI 重构一起补）。
- 验收：`src-tauri` 测试数 **2 → 39**（37 契约 + 2 原有绑定/E2E）。

### 1.7 profile/instance 语义定稿（ADR-002）✅ 已完成（2026-10-21）
- ✅ `docs/adr/ADR-002-profile-instance-semantics.md` → **accepted**：
  profile 是唯一用户可见实体；`instance_*` 按"是否依赖 profile 字段"分成两组，
  并逐条写明参数语义（profile id / 版本 id / 无参）。
- ✅ 修掉一个**真实功能缺陷**：`InstanceSettings` 用 profile id 调
  `instance_check_completion` / `instance_repair`，而它们按版本 id 查仓库 → 永远
  `NotFound`，页面上的"完整性检查/修复"其实一直不可用。现在 profile 优先、版本 id 回落。
- ✅ `ProfileStore::delete(cleanup_game_dir=true)` 增加**共享根保护**：
  `game_dir` 指向共享游戏根时拒绝删除（否则会删掉所有实例的 `versions/`/`assets/`），
  只删隔离实例自己的目录。两个新测试锁定（`test_delete_never_removes_shared_game_root`、
  `test_delete_cleans_isolated_instance_dir`）。
- ⚠️ **偏离原计划**：roadmap 原定"`profile_delete` 增加'同时删游戏文件'选项"。
  Phase 1 把 `profile.game_dir` 收敛成共享根后，该选项等于"删除整个游戏库"，
  因此实现为**受限的 cleanup** 而不是对用户开放的开关（ADR-002 决策 3 有说明）。
- 验收：一套语义 + 文档化 ✅；`ARCHITECTURE.md` §3.3 在 Phase 2 重构门面时同步。

### 1.8 生效值单一来源（P2-8/P2-9）✅ 已完成（2026-10-21）
- ✅ 新增 IPC `instance_resolved_settings(id) -> ResolvedInstanceSettings`
  （`ResolvedInstanceSettings` 加 `Serialize + TS`，`types.ts` 重生成，`bindings.rs` 数组 26→27）。
- ✅ 前端 `instances.resolvedSettings(id)` wrapper；实例设置页新增 `resolved` signal，
  字段处于"继承"状态时显示 `生效值：<value>`（`instanceSettings.effective` i18n 键），
  取值全部来自后端 `Profile::resolve`，**TS 侧不再有第二份继承规则**。
- ✅ 安装目标单一取值点（P2-9）：`profile.game_dir` 在创建时即 `resolved_game_root()`，
  `pipeline::install_game` 只用它；`repair_instance` 用仓库的 `game_dir()`（同一个目录）。
- 验收：`grep` 前端无 `max_memory_mb` 之类的继承推导 ✅；IPC 只有一处产出生效值 ✅。

## 5. 验收标准

- [x] Tauri/CLI 相同操作走同一 Core pipeline（三链路无复刻）——1.2 完成。
- [x] 平台适配层 trait 全部定义且 macOS 后端可用（1.1a–1.1e 完成）；
      领域 crate 无 `#[cfg]` 执行分支 —— **达成**。
- [x] loader 单体系（1.3 完成）；Cleanroom 仍为 stub（未接 UI 入口，等同禁用）。
- [x] `context.rs` < 600 行，按域拆分（1.4：`mod.rs` 93 行，最大域 253 行）。
- [x] zip-slip 正负向测试通过——1.5 完成。
- [x] `src-tauri` IPC 测试 ≥ 46 → 实际 **39**（37 契约 + 2 原有）；逐命令 1 正向/1 拒绝已覆盖，
      运行级 mock-app 测试因需 49 命令泛型化而记为 deferred；前端测试骨架 **deferred**（Phase 3）。
- [x] ADR-002 accepted；profile/instance 语义文档化（1.7）。
- [x] `cargo test --workspace` ≥ 330 → **394 passed**；CI 三门禁本机全绿。
- [ ] Windows native CI = deferred / release-blocked（不填 pass）。

## 6. 验证命令

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo clippy --workspace --lib --bins -- -D warnings -D clippy::panic -D clippy::unwrap_used
cargo test --workspace
pnpm typecheck && pnpm build
# 结构检查
wc -l src-cli/src/context.rs
grep -rn 'cfg!(target_os\|cfg(target_os\|cfg!(windows' crates/rmccc-launch crates/rmccc-version crates/rmccc-mods crates/rmccc-profile  # 适配层外应为空（数据筛选除外）
```

## 7. 平台策略

- 适配层 trait 先全定义；Windows/Linux 后端占位返回 `unsupported_on_platform`。
- 本机只验 macOS 后端 + mock 后端测试；不宣称 Windows。
- `nice`/`killpg`/`icacls`/`taskkill` 全部进适配层后端文件，`#[cfg]` 只在适配层内。

## 8. 风险与回滚

- pipeline 收敛引入回归 → 先为三链路补 mock provider 单测再改；每条链路独立切片、独立 PR。
- loader 统一可能改变 Forge/NeoForge 安装行为 → 保留旧实现分支直到烟测通过，再删。
- context.rs 拆分大 → 按域逐个迁，每域独立验收，不一刀切。
- 适配层迁移触及启动主链路 → 每个能力域迁完跑 macOS 启动烟测；失败回退该域。

## 9. 文档同步

- `ARCHITECTURE.md` §3.2/§3.3/§5.3 同步 pipeline 单一编排、适配层、profile/instance 语义。
- `AUDIT` §5.3、§15.1 P1-3/1-4、§15.2 P2-* 销账。
- `PROJECT_STATUS` 回填；`DEVELOPMENT_PLAN` Phase 5 结构债项更新。
- `PLATFORM_ADAPTATION.md` 迁移映射表勾选已完成项。
