# RMCCC 项目现状与继续开发交接（2026-10-21 更新）

> 本文档是**变更记录与交接入口**，按轮次记录"这轮改了什么"。
> **完整架构说明 + 功能矩阵 + 技术债台账**见 [`AUDIT_2026-10-20.md`](AUDIT_2026-10-20.md)（横切全景，不按轮次组织）。
> 历史审计与整改台账仍然有效作为**问题溯源**（见 §6），但其"当前状态"结论已过期。
> 本文所有数字均由本机实际执行命令得到，未采信文档自述。

---

## 0. 更新记录

**2026-10-20（第 8 轮）**：执行 **Phase 0 基线重建**（`docs/roadmap/01_phase0_baseline.md` 切片 0.1+0.2+0.3），
产出可信基线，未做业务重构。

- **工具链**：Homebrew rustup 1.29.1 下重装 stable **1.99.1** 到 `~/.rustup/toolchains/stable-aarch64-apple-darwin`
  （原 1.98.1 已随环境回归丢失）。`~/.cargo/bin` shim 未生成，调用须显式
  `export PATH="$HOME/.rustup/toolchains/stable-aarch64-apple-darwin/bin:$PATH"`。
- **五门禁复跑全绿**：`cargo fmt` ✅、`clippy --all-targets` ✅、`clippy --lib --bins` panic/unwrap ✅、
  `cargo test --workspace` **300 passed / 0 failed**（含 1 doc-test）、`pnpm typecheck` ✅、`pnpm build` ✅。
- **工具链升级回归**：1.99 新 lint `clippy::double_must_use` 击中 5 处 async_trait trait
  （`DownloadManager`/`VersionProvider`/`AuthProvider`/`ModSource`/`ModLoaderProvider` 等），
  已加 1 处 trait 级 + 4 处 crate 级 `#[allow]`（注释"1.99.1 兼容、非业务逻辑"），零行为变更。
- **体积基线**（本机实测）：GUI `rmccc` 未剥离 10,498,088 B（10.01 MiB）/ 手动 strip 7,170,752 B（6.84 MiB）；
  CLI `rmcli` 未剥离 6,156,904 B（5.87 MiB）/ 手动 strip 4,485,120 B（4.28 MiB）。
  `strip = true` 仍失效（`rust-objcopy` SIGABRT，缺 `libLLVM.dylib`），可系统 `strip` 手动后处理。
  `<10MB` 度量对象待 ADR-011 定稿（GUI 未剥离 10.01 MiB 恰在红线）。
- **Phase 0 状态**：0.1–0.5 全部完成。0.4 平台执行散落清单由 `docs/roadmap/PLATFORM_ADAPTATION.md` 的 grep 映射覆盖；
  0.5 已在 `docs/adr/` 建立索引 + ADR-001（OS 最低版本）、ADR-011（<10MB = stripped 主可执行，**已 accepted**）、
  ADR-012（macOS Intel 策略）、ADR-002 占位。ADR-011 accepted 后 `AGENTS.md` 体积红线已同步为
  "strip 后的主可执行 < 10 MiB"；其余 ADR 仍 `proposed`。
- **Phase 0.5.1 前端契约统一（同日）**：26 处裸 `safeInvoke` 改用 `api.ts` 分域 wrapper；
  `api.ts` wrapper 全部改为 `safeInvoke` 返回 `{ok,error}`（统一错误处理）；`i18n` 3 处适配；
  `events.ts` 补 `INSTANCES_REFRESHED`；`mods` 在实例设置页以 `mods as modsApi` 别名避开局部 signal 遮蔽。
  `pnpm typecheck` ✅、`pnpm build` ✅（JS 118.59KB gzip 34.91KB）。`AUDIT` §10.2 与 P2-11 已回填。
- **Phase 0.5.2 Mod 浏览器（同日）**：新增 `/mods` 页（`src/routes/ModBrowser.tsx`）——实例选择 +
  Modrinth 搜索 + 分页 + 展开版本列表 + 安装；Dashboard 实例面板加入口。复用 `mods.search/getVersions/install`。
  类型桥补 `ModInfo` / `ModSearchResult` 的 ts-rs 导出（`u64` 字段以 `#[ts(type = "number")]` 修正为 number，
  避免生成 `bigint` 与 JSON 实际不符），`types.ts` 重新生成，`bindings.rs` 数组 24→26。
  `pnpm typecheck` ✅、`build` ✅（JS 123.80KB gzip 36.41KB / CSS 32.99KB gzip 7.17KB）；
  `cargo fmt` / `clippy` 通过。UI 可直达命令 **27 → 30**，`AUDIT` §1.1/§1.2/§10.5/P1-1/P2-6 已同步。
- **Phase 0.5.3 / 0.5.5 / 0.5.6（同日）**：Settings 页新增「账户管理」分区（列表 / 切换 / 删除 / 刷新令牌，
  复用 `auth_list/switch/remove/refresh`）与「日志」分区（读取 200 行 / 清空，复用 `log_get/clear`）；
  实例设置页新增「完整性」分区（检查 / 修复，复用 `instance_check_completion/repair`）。
  `pnpm typecheck` / `build` 通过（JS 130.64KB gzip 37.97KB）。UI 可直达 **30 → 38**；
  `AUDIT` §1.1/§1.2/§10.5/P1-2/P2-6 已同步。
- **Phase 0.5.4 本地 mod 安装（同日，方案 A）**：引入 `tauri-plugin-dialog`（`dialog:allow-open` capability），
  新增命令 `mod_pick_and_install`（Rust 侧原生文件选择器 + 复制到 `instance_mods_dir`）与
  `mod_install_local`（按路径复制，备用）；实例设置页「模组」分区加「从本地文件安装…」按钮。
  IPC 命令 **46 → 48**，UI 可直达 **38 → 39**。体积：unstripped 10.30 MiB / **stripped 7.03 MiB**
  （dialog 依赖 +0.19 MiB，仍 < ADR-011 的 10 MiB 红线）。`cargo fmt`/`clippy`、`pnpm typecheck` 通过。
  **Phase 0.5 全部切片（0.5.1–0.5.6）完成。**
- **修复：Mod 下载 `error decoding response body`（同日）**：根因是 `reqwest` 只启用
  `rustls-tls/json/stream`，**缺 `gzip`/`brotli`**，Modrinth/CDN 的压缩响应用 `.text()`/`.bytes()`
  解码时报错。已在 workspace `Cargo.toml` 加 `gzip`/`brotli` feature，并让 `download_bytes` /
  `get_download_url` 的错误保留底层 source（含 HTTP status）便于诊断。体积 strip 后 **7.03 → 7.20 MiB**
  （仍 < ADR-011 的 10 MiB）。
- **Phase 1.5 zip-slip 修复（2026-10-21，审计 P2-4）**：`rmccc-launch/src/natives.rs` 新增
  `safe_relative_entry()`，拒绝绝对路径、`..`、Windows 盘符前缀与空路径；`decompress_natives`
  只接受安全相对路径拼接目标目录，恶意条目记 warn 并跳过。新增正负向单测（`rmccc-launch` 50→51）。
  `cargo fmt`/`clippy` 通过。**Phase 1 开始（1.5 完成）**。
- **修复：Mod 安装 `TimedOut`（同日）**：`error decoding response body` 修好后暴露第二个问题——
  `rmccc-core` 的 reqwest 设了 **总超时 120s**（`.timeout`），慢速 CDN 下载大 jar 时 body 读到一半
  被 `TimedOut` 误杀（`kind: Body`）。改为 **`read_timeout(120s)`**（单次读间隔）+ `connect_timeout(15s)`，
  移除总超时：只要仍在收数据就能续传式读完，真正卡死才失败。

- **Phase 1.2 编排层收口（2026-10-21，审计 P1-3，最高优先级结构债）**：`rmccc-core::pipeline` 成为
  Tauri 与 CLI **唯一**的业务编排入口。
  - `install_game`：三类别下载合并为**单批次**（进度单调，百分比不再回弹）+ 磁盘空间预检（120% 余量）
    + natives 解压；返回 `SetupReport { total_files, failed_files }`。删除旧的 `repair_resources`
    （第三份分叉实现、0 调用方）。
  - 新增 `repair_instance`（版本 id 入口，Tauri `instance_repair` 与 CLI `resource repair` /
    `ensure_launch_ready` 共用）、`ensure_launch_ready` 改为其 profile 视图薄委托。
  - 新增 `commit_install`（重读 profile → 写 `game_version` → 刷索引）与 `refresh_repository`，
    两端共用同一提交路径。
  - `launch_game` 改为返回 `Arc<ManagedProcess>`（**不阻塞**），CLI 与 Tauri 各自只需提供
    `ProcessListener`；`last_played` 在启动成功处统一更新。
  - 顺带修掉的行为差异：GUI 安装原先**不解压 natives**；安装提交原先用几秒前的 profile 快照覆盖用户设置；
    Tauri 自定义 Java 路径不存在时要到 spawn 才失败（现立即 `JavaNotFound`）；
    `wait_for_batch` 通知早于最终进度回调（失败计数读取竞态），已把 `notify_one` 移到回调之后。
  - 代码量：删掉三处复刻（约 400 行），`version.rs` 309→203 行、`launch.rs` 309→168 行。
  - `cargo fmt` ✅、`clippy --workspace --all-targets -D warnings` ✅、`cargo test --workspace`
    **311 passed / 0 failed**（`rmccc-core` 新增 10 个编排/工具单测）、`pnpm typecheck` ✅、`pnpm build` ✅。

- **Phase 1.1a 平台适配层骨架（2026-10-21，指南 §5.7）**：新建 `crates/rmccc-platform`（第 11 个
  workspace crate），把"平台执行"从领域 crate 抽成可注入的 trait：
  - `PlatformDescriptor`：os/arch、`classpath_separator`、`exe_suffix`、`default_encoding`、
    `native_classifier_candidates`、`matches_rule_os/arch`（`osx`/`macos`、`arm64`/`aarch64` 均兼容）。
    领域层日后判断 Mojang `rules[].os` 一律经它，不再裸写 `cfg!`。
  - `PlatformFs`（`restrict_owner_only` / `make_executable` / `symlink` / `remove_symlink`）、
    `PlatformProcessControl`（`terminate_tree` / `set_priority` / `priority_command_prefix` /
    `configure_child_process_group`）、`PlatformShell`（`open_url` 白名单 http(s) + `open_path`）。
  - macOS 后端全部落地（`chmod 600/755`、`killpg`、`setpriority`、`nice` 前缀、`process_group(0)`、`open`）；
    Windows/Linux 后端为 `UnsupportedFs/Process/Shell`，一律返回 `UnsupportedPlatform`（deferred 待原生 CI）。
  - `Platform::current()` 是 crate 内**唯一**的 `#[cfg]` 选择器；`Platform::mock()` + `MockPlatform`
    可断言调用序列、可模拟单次失败，供领域层单测使用。
  - `CoreContext.platform: Arc<Platform>` 在组装层注入一次（指南 §5.7.1 第 5 条）。
  - `rmccc-error` 新增 `Validation` / `UnsupportedPlatform` 变体，`ErrorCode::UnsupportedPlatform`
    （CLI 退出码 11），`types.ts` 已重新生成。
  - **故意未做**：`CredentialStore`（指南要求先评估 Keychain；现有 AES vault 除 chmod 外与平台无关）、
    `JavaDetector`（`rmccc-launch/java.rs` 已是 per-OS 分文件，随 1.1d 提升为 trait）。
  - 门禁：`cargo test --workspace` **335 passed / 0 failed**（平台 crate 23 + doc-test 1）、
    `fmt` ✅、`clippy --all-targets -D warnings` ✅、`clippy --lib --bins -D panic -D unwrap_used` ✅、
    `pnpm typecheck` ✅、`pnpm build` ✅。
  - 下一步：**1.1b 数据筛选切换**（`rmccc-version` 去裸 `cfg!`）→ 1.1c 执行迁移 → 1.1d Java trait → 1.1e 分隔符。

- **Phase 1.1b 数据筛选去裸 `cfg!`（2026-10-21，指南 §5.7.1 第 3 条，审计 P2-12）**：
  `rmccc-version` 现在**零平台 `#[cfg]`**。
  - `libraries` / `arguments` / `compatibility` / `file_layout` / `completion` /
    `default_game_repository` 全部改收 `&PlatformDescriptor`；
    删掉 3 个 `cfg` 函数（`current_os_name`×2 / `current_arch`×2 / `minecraft_arch`）与 per-OS classifier 分支。
  - native classifier 的**唯一事实来源**收归 `PlatformDescriptor::native_classifier_candidates()`
    （含 `natives-osx` / `natives-windows-64` / `natives-unix` 等历史别名），新增
    `accepts_native_classifier` / `minecraft_arch()`。
  - 注入路径：`CoreContext.platform` → `pipeline` / `LaunchOptions.platform`（新增字段）/
    `DefaultGameRepository::with_platform`。
  - 测试改造：所有 `#[cfg(target_os)] assert!` 删除，改为显式描述符**双向断言**
    （同一份 manifest 喂 macOS / Windows 描述符，结果互补）——原先那些断言在非 macOS 上从未执行过。
  - **顺带修掉真实 bug（P2-12）**：`library_applies` 默认值反了。Mojang
    `Library.isRulesSatisfied` / HMCL `appliesToCurrentEnvironment` 是 **default-deny**，
    本项目原为 default-allow → 有 `rules` 但无命中的库也被选中。用真实 1.20.4 manifest 实测：
    修复前 macOS 选中 **88/88** 个 library（含 30 个 `natives-windows*` / `netty…:linux-x86_64`），
    修复后 **58** 个，异平台 natives 正确排除（每次安装少下 30 个无用 jar，classpath 不再被污染）。
    已加回归测试 `test_library_selection_matches_real_1_20_4_semantics`。
  - 门禁：`cargo test --workspace` **339 passed / 0 failed**、`fmt` ✅、
    `clippy --all-targets -D warnings` ✅、`clippy --lib --bins -D panic -D unwrap_used` ✅、
    `pnpm typecheck` ✅、`pnpm build` ✅。
  - 下一步：**1.1c 执行迁移**（`terminate_tree` / `restrict_owner_only` / `make_executable` /
    `symlink` / 进程组）→ 1.1d Java trait → 1.1e 分隔符。

- **Phase 1.1c 执行迁移（2026-10-21，指南 §5.7.2，真机烟测通过）**：把"随 OS 改变的系统调用"
  全部迁入平台适配层，领域 crate 只剩 `java.rs` 4 处 `#[cfg]`（属 1.1d）。
  - 进程终止：Tauri `launch_kill` / CLI `launch_stop` → `platform.process.terminate_tree(pid)`；
    `libc::kill` 与 `taskkill` 分支删除，`src-tauri` + `src-cli` 的 `libc` 依赖一并移除。
  - 进程组：`spawn_game(.., &Platform)` → `configure_child_process_group`。
  - 优先级：`nice -n N` 前缀 → `priority_command_prefix`；`ProcessPriority` 定义收归平台 crate。
  - 文件权限：`TokenVault::init(path, fs)` 与 argfile → `restrict_owner_only`（0600）；
    启动脚本 → `make_executable`（0755）。
  - 符号链接：natives 临时链接 → `symlink` / `remove_symlink`。
  - 平台数据去内联：`-Xdock:name` → `descriptor.extra_jvm_args()`（`{instance}` 模板）、
    `-Duser.home` → `is_windows()`、分隔符 → `classpath_separator()`、
    `-Dfile.encoding` → `default_encoding()`、非 ASCII natives 兜底 → `needs_non_ascii_natives_workaround()`。
  - 顺带：`decompress_natives` 按平台过滤 native jar → 安装日志里 **30 条**
    "Native library JAR not found" 噪音归零。
  - **真机烟测（macOS Apple Silicon）**：`rmcli` 新建实例 → `resource install` 1.20.4
    **3870 文件 / 0 失败 / natives 16 个**；`launch run` **启动到主菜单**
    （OpenAL 初始化「外置耳机」、全部纹理图集创建、正常 `exit_code 0` 退出）；
    `.key` 权限实测 `-rw-------`。
  - 门禁：`cargo test --workspace` **341 passed / 0 failed**（新增真实子进程
    `setpgid`+`killpg` 烟测）、`fmt` ✅、`clippy --all-targets -D warnings` ✅、
    `clippy --lib --bins -D panic -D unwrap_used` ✅。
  - 下一步：**1.1d Java 探测提升为 `JavaDetector` trait**（`rmccc-launch/java.rs` 最后 4 处
    `#[cfg]`）→ 1.1e 分隔符收尾 → 1.3 loader 单体系。

- **Phase 1.1d 平台适配层收尾 + 1.3 loader 单体系 + 1.4 context 拆分（2026-10-21，一口气完成）**
  - **1.1d Java 探测提升为 trait**：`JavaInstallation` + `JavaDetector` 迁入 `rmccc-platform::java`；
    `select_for()` 选版规则是纯函数（精确匹配 > 最近的更高版本 > `JavaVersionMismatch`）。
    macOS 后端 `java_home -V` + `JAVA_HOME` + Homebrew 路径；Windows/Linux 返回 `UnsupportedPlatform`。
    `rmccc-launch/src/java.rs` 265→80 行（只留 re-export），**最后 4 处 `#[cfg(target_os)]` 消失**。
  - **1.1e 分隔符**：随 1.1c 已完成，核对销账。**`crates/*/src` 除 `rmccc-platform` 外零平台 `#[cfg]`。**
  - **1.3 loader 单体系（P1-4）**：删 `ModLoaderProvider` / `create_provider` / 三个 `*Installer`
    与 `CoreContext.modloader_providers`；新增 `rmccc-modloader::installer`（官方安装器运行器）
    与 `DependencyManager::install_loader`（唯一路由）；新增 `rmccc-core::modloader::install_loader`
    （Tauri/CLI 都只调它）。顺带修两处不一致：CLI 原先写合并 JSON（丢 HMCL 回滚/叠加语义）；
    装完 loader 不下载自带库 → `--no-repair` 启动 `ClassNotFoundException: KnotClient`（已修，安装末尾补 8 个库）。
  - **1.4 context.rs 拆分（P2-1）**：1,450 行 → `context/mod.rs` **93 行** + 10 个域文件
    （`auth/config/profile/version/resource/modloader/mods/launch/system/helpers`，最大 253 行）。
    `CliContext` 仍是唯一入口，命令层零改动。
  - **真机验证**：`rmcli` 新建实例 → 安装 1.20.4（3870 文件 / 0 失败）→ 安装 Fabric 0.15.11
    （增量 JSON + 自动补 8 个 loader 库）→ `--no-repair` 启动到主菜单
    （Fabric Loader 0.15.11 / Mixin 0.8.5 / OpenAL 就绪）→ `modloader remove` 恢复正常。
  - 门禁：`cargo test --workspace` **356 passed / 0 failed**、`fmt` ✅、
    `clippy --all-targets -D warnings` ✅、`clippy --lib --bins -D panic -D unwrap_used` ✅、
    `pnpm typecheck` ✅、`pnpm build` ✅。
  - Phase 1 剩余：**1.6 IPC 契约测试**（`src-tauri` 2→≥46）、**1.7 ADR-002**（profile/instance 语义）、
    **1.8 生效值单一来源**。

- **Phase 1.6–1.8 完成（2026-10-21）—— Phase 1 全部切片收尾**
  - **1.6 IPC 契约测试（P2-2）**：新增 `handlers.rs`（`generate_handler!` 单一注册表 +
    `IPC_COMMANDS` 清单）与 `ipc_tests.rs`（37 测试）。覆盖三层契约：注册表一致性
    （含与前端 `src/ipc/api.ts` 交叉校验）、参数校验（mod 文件名 / URL scheme / loader 类型 /
    profile id 解析 / 日志行数 clamp）、错误码（19 个 `RmcccError` 变体映射 + `retryable` 语义 +
    `{code,message,retryable}` 形状）。`src-tauri` 测试 **2 → 39**。
    测试过程中修掉 3 个真实问题：`mod_install_local` 注册但前端从不调用（白名单+理由）；
    `sanitize_filename("../x")` 产出 `.._x`（现剥前导点、空名回落 `mod`）；
    `account_repo.remove(未知 id)` 实为幂等 no-op（按契约修正测试）。
    **已知缺口**：运行级 mock-app 测试需把 49 个命令泛型化到 `Runtime`，记为 deferred；
    前端组件测试 0 个（Phase 3 补）。
  - **1.7 ADR-002 定稿（P2-10）**：profile 为唯一用户实体，`instance_*` 按"是否依赖 profile 字段"
    分两组并逐条写明 id 语义。修掉一个真实功能缺陷：实例设置页用 profile id 调
    `instance_check_completion`/`instance_repair`，而它们按版本 id 查仓库 → 一直 `NotFound`，
    "完整性检查/修复"其实不可用；现在 profile 优先、版本 id 回落。
    `ProfileStore::delete(cleanup=true)` 增加**共享根保护**（否则会删掉所有实例的游戏文件）。
  - **1.8 生效值单一来源（P2-8/P2-9）**：新增 IPC `instance_resolved_settings`
    （`ResolvedInstanceSettings` 加 `Serialize+TS`，`types.ts` 重生成、`bindings.rs` 26→27）；
    实例设置页在字段继承时显示后端算出的「生效值」，TS 侧零继承规则。
    安装目标确认单一取值点（`profile.game_dir` == `resolved_game_root()`）。
  - 门禁：`cargo test --workspace` **394 passed / 0 failed**、`fmt` ✅、
    `clippy --all-targets -D warnings` ✅、`clippy --lib --bins -D panic -D unwrap_used` ✅、
    `pnpm typecheck` ✅、`pnpm build` ✅。CLI 烟测（`system info` / `resource check`）无回归。
  - **Phase 1 验收**：三链路单一编排 ✅、平台适配层 ✅、loader 单体系 ✅、`context.rs` 93 行 ✅、
    zip-slip ✅、IPC 契约 ✅、ADR-002 ✅、生效值单一来源 ✅。
    遗留：前端组件测试 0 个（Phase 3）、Windows/Linux 后端 deferred（release-blocked）。

- **Phase 2 声明式插件系统完成（2026-10-21）—— 2.1–2.6 + 2.7 IPC/UI**
  - **新增 `crates/rmccc-plugin`（第 12 个 crate，14 个 workspace 成员）**：
    `manifest`（严格 v1 + 12 条校验）、`version`（SemVer 2.0 + 受限范围）、`capability`（18 项能力/风险/host 策略）、
    `package`（`.rmcpkg` 包级限额与路径/类型检查）、`installer`（staging）、`registry`（11 状态机）、
    `permissions`（授予/撤销/每次复核/审计）、`config`（schema 子集 + 版本迁移）、`manager`（窄门面）。
  - **ADR-003 / ADR-004 accepted**：包格式 = 无加密 ZIP；`plugin-state/registry.json` 为唯一状态来源
    （**不建 `enabled.json`**）；Phase 2 无签名通道，安装一律 `trust=development` 并显示风险提示；
    `ui/`、`logic/` 与脚本/可执行文件在包校验阶段拒绝。
  - **安装事务**：staging → 校验 → `rename` 原子提交 → 原子写注册表/权限；失败删除新版本目录并恢复快照；
    升级保留 `previousVersion` 与旧目录；`plugin rollback` 交换版本并重新计算待审权限。
    升级**不自动扩张任何权限**，新权限进入 pending，插件保持 `installed_disabled`。
  - **损坏隔离**：损坏的 `registry.json`/插件 manifest 只产生诊断并备份 `.bak-<ts>`，
    `PluginManager::load` 永不返回 Err，其它插件与主应用照常启动。
  - **平台归属**：新增 `PlatformFs::atomic_replace`（Unix rename / Windows 待 ReplaceFile），
    插件 crate 零 `#[cfg]`；`PlatformDescriptor::plugin_platform_id()` 统一平台标识。
  - **CLI**：`rmcli plugin list|inspect|install|enable|disable|remove|rollback|grant|revoke|
    config|capabilities|pack`（13 个子命令）；真实烟测覆盖 pack→install→pending→grant→enable→config。
  - **IPC + UI（额外切片 2.7）**：15 条命令（注册表 49→64）；`/plugins` 页面支持安装/状态、
    权限审查（风险/作用域/授予/撤销）、依赖状态、配置编辑、回滚、卸载与诊断。
  - **样例**：`examples/plugins/theme-midnight`（纯主题）与 `server-dashboard`
    （卡片 + 设置页 + 受限 `network.request`）；`example_plugins_pack_and_install` 锁定真实样例可安装。
  - 门禁：`cargo test --workspace` **468 passed / 0 failed**（新增 74 项插件测试）、`fmt` ✅、
    `clippy --all-targets -D warnings` ✅、`clippy --lib --bins -D panic -D unwrap_used` ✅、
    `pnpm typecheck` ✅、`pnpm build` ✅（JS 150.42 kB / gzip 43.20 kB；CSS 37.47 kB / gzip 8.01 kB）。
  - 遗留（明确 deferred）：签名/可信目录（Phase 7）、隔离 UI（Phase 4）、逻辑运行时（Phase 5）、
    插件仓库与依赖自动下载（Phase 7）、Windows/Linux 安装事务实测（原生 CI）、前端组件测试（Phase 3）。
  - **下一步**：Phase 3（Dashboard 卡片与主题引擎），先建 ADR-005（Layout Schema / 冲突算法）。

- **Phase 3 Dashboard 布局与主题引擎（2026-10-21）—— 3.1–3.5 完成**
  - **新增 `crates/rmccc-dashboard`**：布局 Schema + 确定性归一化算法（去重/尺寸钳制/
    换行/排序/push-down/压缩）、Widget Registry（内置 `org.rmccc.core-ui` + 插件卡片）、
    布局持久化（损坏备份 + 默认恢复）、主题令牌与安全语义色边界。17 项单测。
  - **ADR-005 accepted** + `schemas/dashboard-layout.schema.json`：
    12 列网格、`(x,y,w,h)` 逻辑坐标、断点 wide/medium/compact（12/8/4 列）。
  - **9 条 IPC**：`dashboard_get_layout/save_layout/reset_layout/list_widgets/list_themes/
    get_theme_tokens/set_theme_preference/get_theme_preference/get_theme_json`；
    `UserSettings` 新增 `theme` 偏好（light/dark/跟随系统/插件主题 key）。
  - **前端迁移**：Dashboard 从固定 240px/1fr/260px 三列改为注册表驱动 CSS Grid；
    三块内置面板成为 `org.rmccc.core-ui` Widget；支持拖放、宽度循环、隐藏/恢复/重置、
    添加插件卡片；`ErrorBoundary` 隔离单卡片失败，卸载插件卡片显示保位占位。
  - **主题令牌**：`--rmccc-*` 新令牌 + 桥接旧 `--surface-*`/`--text-*`；
    内置 light/dark + 插件 `contributes.themes`；安全色由宿主控制，主题不可覆盖。
  - 门禁：`cargo test --workspace` **485 passed / 0 failed**（新增 17 项 Dashboard 测试）、
    `fmt`/`clippy` ✅、`pnpm typecheck` ✅、`pnpm build` ✅（JS 163.64 kB / gzip 47.35 kB；
    CSS 41.12 kB / gzip 8.55 kB）。
  - 遗留：插件卡片真实渲染在 Phase 4（当前占位）；高 DPI/Windows WebView2 差异 deferred；
    前端组件测试仍为 0。

- **修复：启动后"透明窗口"（前端完全不渲染，2026-10-21）**
  - **现象**：打开 `RMCCC.app` 后窗口是透明矩形——无标题栏、无内容，`logs/` 里也没有任何前端 IPC。
  - **根因（实证）**：`.app` 内的可执行文件**没有嵌入任何前端资源**（`strings` 查不到
    `/assets/index-*.js` 键），WebView 拿到空 asset map，`index.html` 与模块脚本全部 404。
    成因是 `tauri-build` 只对 `tauri.conf.json` / capabilities / 资源发 `rerun-if-changed`，
    **不监听 `frontendDist`**：早先一次 `dist/` 尚未生成时的 `cargo build` 把"空 asset map"
    编译进了 `rmccc`，之后的 `tauri build` 因无 Rust 源码变更不再重编译，直接复用了那个二进制。
  - **结构修复**：`src-tauri/build.rs` 增加 `cargo:rerun-if-changed=../dist`（+ `dist/index.html`），
    dist 变化必然触发 `rmccc` 重编译。
  - **失败可见化**（同类问题不再表现为"透明窗口"）：
    - 新增 `src-tauri/src/bundle.rs`：启动时用 `asset_resolver()` 校验 `index.html` 及文档引用的
      `src=`/`href=` 资源是否都在 asset map 中；缺失则写 `ERROR` 日志，并把窗口导航到内联
      `data:text/html` 错误页（列出缺失清单 + `pnpm tauri build` 提示）。用导航而非 `eval`：
      `index.html` 缺失时 WebView 没有可求值的脚本上下文。
    - `index.html` 内联 boot guard（在 bundle 之前、不依赖模块加载）：捕获资源加载失败、
      未捕获异常、未处理 Promise、4s 未挂载，绘制可读错误面板，并通过新增 IPC
      `frontend_log`（注册表 73→74）写入 `logs/rmccc.<date>.log`。
    - `src/main.tsx` 在 `render()` 后派发 `rmccc:shell-mounted`，guard 收到后撤下错误面板。
  - **验证**：移走 `dist/` 后 release 重编译 → 窗口显示"内置前端资源不完整"错误页，
    日志有 `Embedded frontend bundle is incomplete`；恢复 `dist/` 重编译 → UI 正常渲染、
    `version_list` 等 IPC 正常。前端 guard 另在 Safari 用"故意改错 JS 文件名"的 dist 验证
    （面板显示具体 404 URL）。
  - 门禁：`cargo test --workspace` **497 passed / 0 failed**（新增 6 项 bundle 校验测试 +
    3 项 `frontend_log` 契约测试）、`fmt` ✅、`clippy --all-targets -D warnings` ✅、
    严格 clippy ✅、`pnpm typecheck` ✅、`pnpm build` ✅。
  - 体积：GUI 未剥离 11.39 MiB / 手动 strip **7.79 MiB**（ADR-011 仍通过）。

- **Phase 4 隔离 UI 扩展与 SDK（2026-10-21）—— 4.1–4.5 完成**
  - **ADR-006 accepted**：容器 = 主窗口内 `<iframe sandbox src="rmcccplugin://<id>/…">`（独立源，
    不是独立 WebView：卡片要进网格）；协议 = 版本化 postMessage（`UI_API_VERSION = 1`）；
    资源 = 自定义 URI 方案只服务 `ui/` 白名单；授权 = 每条消息重新判定。
  - **新增 `crates/rmccc-plugin/src/ui.rs`**：方法表（`host.info` / `theme.get` / `settings.get` /
    `instances.list` / `network.request`，各自能力+超时）、12 个线上错误码、请求解析
    （≤64 KiB/形状/版本/方法/ID）、资源白名单、严格 CSP（`connect-src 'none'`）。16 项单测。
  - **包与 Manifest 开放 UI**：`ui/` 目录与 `entry.ui` 从"拒绝"改为"校验后允许"（必须位于 `ui/`、
    `.html`、文件存在、且与 `dashboardCards`/`settingsPages` 互相印证）；`logic/` 仍拒绝。
  - **宿主桥 `plugin_ui_invoke`（注册表 74→75）**：接收 iframe 的**原始** JSON 重新解析，
    逐条 `ensure_enabled` + `authorize`（`network.request` 以目标 host 为作用域），
    返回带线上错误码的 `BridgeReply`；`network.request` 用独立 HTTP client（禁重定向、30 s、2 MiB）。
  - **`rmcccplugin://` 方案处理器**：只服务该插件 `<install>/ui/` 内白名单文件，
    HTML 带 CSP 与 `nosniff`；未启用/无 `dashboard.cards`·`ui.pages` 一律 403。
  - **前端**：`src/plugin-ui/bridge.ts`（source/origin/大小/并发/超时/错误转发）+ `PluginFrame.tsx`
    （加载/错误/就绪三态、8 s 未 ready 判失败、主题变更推送）；`WidgetDescriptor.ui_url` 驱动
    Dashboard 自动选择 iframe 或声明式卡片。
  - **SDK `plugin-sdk/`**：`@rmccc/plugin-api`（线协议类型 + 方法表）、`@rmccc/plugin-ui`
    （运行时 client，纯 ESM/JSDoc）、`templates/dashboard-card`、`scripts/cli.mjs`
    （`validate`/`pack`，与安装器同规则）、`docs/`（入门/API/安全/发布）、
    **跨工件契约测试**（schema ↔ Rust ↔ SDK 的方法表与错误码必须一致）。
  - **样例**：`examples/plugins/server-dashboard` 升级为隔离 UI 卡片并通过桥读取实例/设置；
    同时在真实 `.app` 中验证了"未授权 → 拒绝错误态 → 授权 → 正常数据"的完整链路。
  - **顺手修掉 Phase 2 的一个权限漏洞**：升级新增低风险能力时既不在 granted 也不在 pending
    （插件能启用但永远拿不到），`pending` 已改为"所有已声明未授予"，升级仍不自动授权。
  - 门禁：`cargo test --workspace` **525 passed / 0 failed**、`fmt`/`clippy --all-targets -D warnings`/
    严格 clippy ✅、`pnpm typecheck` ✅、`pnpm build` ✅（JS 170.21 kB / gzip 49.59 kB；
    CSS 42.15 kB / gzip 8.71 kB）、`pnpm sdk:test` 4 项 ✅。
  - 体积：GUI 未剥离 11.53 MiB / 手动 strip **7.85 MiB**（ADR-011 仍通过）。
  - 遗留（deferred）：Windows/Linux 隔离 UI 行为（release-blocked）、插件页面（settingsPages）的
    iframe 路由、`plugin:dev` 热更新（与"不信任任意路径"冲突）、前端组件测试仍为 0。

**2026-10-20（第 7 轮）**：产出**全量架构与审计文档** [`AUDIT_2026-10-20.md`](AUDIT_2026-10-20.md)，
并修正本文档与 `ARCHITECTURE.md`、`DEVELOPMENT_PLAN.md` 中过期的数字。

审计新增的关键发现（此前未记录）：

1. **编排层被绕过**：`pipeline` 的 `resolve_version` / `repair_resources` / `launch_game` 均为**零调用方**；
   Tauri 的 `version_install`、`instance_repair`、`launch_start` 各自复刻了一遍流程（只有
   `ensure_launch_ready` 被两端共用）。
2. **两套"实例"概念并存**：`profile`（UUID，前端实例卡片）与 `instance`（版本 id，`versions/{id}/`）。
   `instance_rename/duplicate/remove` 操作的是后者，这解释了它们为何在前端零调用。
3. **环境回归**：Homebrew 升级 rustup 后 `/opt/homebrew/opt/rustup/toolchains/` 失效
   （实体在 `Cellar/rustup/1.29.1/toolchains/…`），且 `~/.cargo` 已不存在
   → §7.3 记的工具链路径已过期；当前 `cargo test` 需联网重建依赖缓存。
4. 数字校正：IPC **46**（非 42/45/48）、UI 直达 **27**、前端 **17 文件 4,279 行**、
   测试 **299 个 `#[test]`**（上次全量 300 passed）。

**2026-10-10（第 6 轮）**：实现**每实例独立设置**（参考 HMCL），含实例卡片入口与全局/实例分层。

**调研结论（修正了一个常见误解）**：HMCL 的"全局游戏设置"**不在 `LauncherSettings` 里**，而是可多套的
"游戏设置预设"（`config/game-settings.json`），实例通过 `parent` 引用其一；40 个游戏字段全部是
`InheritableProperty`（默认继承、可显式覆盖）。"继承 vs 覆盖"的唯一真相是
`Instance.overrideProperties` **字段名集合**——值与该标志分开存，因此来回切换不会丢值。

**落地设计**（三点，与 HMCL 一致或有据可循）：

| 层 | 类型 | 存储 |
|----|------|------|
| 全局默认值 | `AppConfig`（新增 jvm/game args、isolation、窗口、优先级、auto_memory） | `config/launcher-settings.json` |
| 实例值 + 覆盖名单 | `Profile`（新增 `override_properties`、`isolation`） | `profiles/{id}.json` |
| 生效值 | `ResolvedInstanceSettings`（`Profile::resolve`） | 不落盘 |

- **值不丢**：`override_properties` 不含某字段时该字段继承全局，但实例里已填的值仍留在磁盘上；
  已有 profile JSON 无此字段 → 全部继承，且原值保留（新增测试覆盖）。
- **隔离两层**：`isolation`（never/modded/always，默认 modded）决定**运行目录**；
  共享游戏根（versions/libraries/assets）始终不变。`rmccc_launch::instance_run_dir` 是唯一推导入口，
  启动与模组命令共用，杜绝"安装目录 ≠ 启动目录"重现。模组目录随运行目录走，隔离实例因此拥有独立 mods。
- **与 HMCL 的差异（有意）**：只有一套全局默认值（不做多预设）；实例这半存在 `profiles/{id}.json`
  而非 `{instance_root}/.hmcl/config/`——RMCCC 的实例注册表就是 ProfileStore，不扫描 `versions/`。

**后端新增**：`instance_paths`（报告生效的游戏根/运行目录/模组目录）、`ModList{enabled,disabled,mods_dir}`
（此前只返回启用的模组，禁用的一半看不见）、`ProfileUpdateOpts` 覆盖全部字段、
`SettingsUpdate` 覆盖全部全局默认值、`LoaderVersion` 导出为 TS 类型。

**前端**：实例卡片新增齿轮按钮 → 路由 `/instances/:id`；设置页新增"全局游戏默认值"分区；
`api.ts` 去掉手写的 `SettingsUpdate`/`ProfileUpdateOpts`（它们此前**遮蔽**了 ts-rs 生成的同名类型，
字段一变就报错——本次正是被它绊到），改为 `Partial<生成类型>`。

| commit | 内容 |
|--------|------|
| `5ef603b` | feat(config,profile)：全局默认值 + 实例覆盖模型 |
| `dfea619` | feat(launch,...)：消费生效值 + 共享根/运行目录分离 |
| `79a0054` | feat(...)：每实例模组目录随运行目录 |
| `d3e9a3c` | feat(tauri)：实例与全局设置的完整接口面 |
| `883f985` | feat(tauri,ui)：实例设置入口接线 |
| `4f92579` | test(launch)：隔离运行目录与生效值测试 + 文档 |

**2026-10-10（第 5 轮）**：修复"能下载但打不开游戏"，并完成创建实例的版本选择闭环。

用户实测症状：前端能创建实例、能下载，但打不开游戏。根因是**三个独立 bug 叠加**，全部修复并实机验证。

1. **安装目录 ≠ 仓库根目录（阻断）**：`profile_create` 把 `game_dir` 设为
   `{game_root}/{profile_id}`，安装写进那里；而 `DefaultGameRepository` 索引的是共享
   `{game_root}`，启动链路经 `get_resolved()` 解析版本。于是日志里装完 5262 个文件仍然
   `instances=0`，启动在第一个阶段就失败。现统一为共享游戏根（`IsolationType` 只影响运行目录），
   并新增旧布局迁移把 `instances/{id}/versions|libraries|assets` 并入共享根。
2. **response file 竞态（阻断）**：文件在 `spawn()` 后**立即删除**，而 JVM 是在解析命令行时才读
   `@argfile`，两者竞争 → 游戏以 `could not open .../.launch-args-*` 退出（exit 1）。
   现改为交给后台任务在 JVM 读完参数后删除，并在下次启动清扫陈旧文件。
3. **argfile 混入 java 可执行文件（阻断）**：`Command::jvm_args` 的第一项是 java 路径（其文档只说是
   "JVM arguments"），于是 response file 首行成了 java 路径，被 JVM 当成主类 →
   `could not find or load main class /Library/.../bin/java`。现由两个消费方各自提供可执行文件
   （`spawn_game()` 作为程序、`make_launch_script()` 前置）。

**实机验证**（临时数据目录 + 软链真实 SHA1 缓存）：

```
Game repository refreshed game_dir=.../instances instances=1        ← 修复 1
[Render thread/INFO]: Setting user: E2ETester                       ← 修复 2+3
[Render thread/INFO]: Backend library: LWJGL version 3.4.3+4
[Render thread/INFO]: Using graphics device: Apple M1 (Apple)
[Render thread/INFO]: Created window using SDL video driver: cocoa
```

（离线账号的 `Failed to fetch user properties ... 401` 属预期，不影响启动。）

**前端**：创建实例的弹窗新增**必选**游戏版本下拉（默认正式版，可勾选包含快照），
名称或版本为空时 Create 按钮禁用；创建成功后立即为该实例下载所选版本。

**$schema 命名空间**改为 `https://rmcl.snowebsite.top/json/schema/...`，旧 HMCL 命名空间仍可读，
并在**加载时**即被改写（不再等用户改设置）。已确认本机 `launcher-settings.json` /
`user-settings.json` 均已迁移且设置值完整保留。

| commit | 内容 |
|--------|------|
| `049cc26` | feat(config)：schema 命名空间迁移 |
| `15f1cf3` | fix(core,tauri,cli)：安装到共享游戏根 + 旧布局迁移 |
| `4046bfa` | fix(launch)：response file 交付修复 |
| `36c2234` | feat(ui)：创建实例必须选版本并立即下载 |
| `2270d8c` | feat(config)：加载时改写旧 `$schema` + 文档同步 |

**2026-10-10（第 4 轮）**：修复窗口外壳与交互缺陷，并新增语言设置（English / 简体中文）。

**窗口外壳缺陷**（用户实测反馈：无法拖动、启动在角落、四周有缝隙）根因三条，均已修复：

1. **无法拖动/没有窗口按钮**：`TitleBar.tsx` 与 `.app-root` 的 CSS 早已写好，但 `main.tsx` 只渲染了
   `<Router>`，**从未挂载过它们**。改为 `Router root={App}`，shell（标题栏 + 圆角容器 + toast 层）常驻。
2. **启动在角落 + 尺寸异常**：`tauri-plugin-window-state` 恢复了一份过期几何
   （**2200×1426 @ (-760, 892)**，而显示器只有 **1440×900**）——用户只看到一个巨大窗口的一角。
   已把该状态文件备份为 `.window-state.json.stale-20261010`，并在 `App.tsx` 增加启动时的
   显示器范围校验（不可达则 `center()`）。
3. **四周缝隙**：路由根元素的 `flex: 1` 找不到 flex 父容器（`.app-root` 未挂载）→ 内容塌缩、
   透明窗口露出桌面。挂载 shell 后即恢复设计意图。

顺带修复：窗口 `visible: false` 却**无人调用 `show()`**——此前仅靠 window-state 里存的
`visible: true` 侥幸可见，删掉那份状态后窗口将永不出现。现由 `App.tsx` 在 `initI18n()` 后显式
`show()`（ready-to-show）。另外 `ToastContainer` 也从未挂载，所有 `showToast` 都不可见，一并补上。
新增 capability：`core:window:allow-center`、`core:window:allow-available-monitors`。

**语言设置**：见 §9.5（`ARCHITECTURE.md`）。`UserSettings.locale` 此前从未被 IPC 读写，
现补齐 `settings_get_user` / `settings_set_user` / `settings_supported_locales` 三个命令。
默认跟随系统语言，系统语言不在支持列表内（如 `fr`/`ja`）回退英语。

> 注：窗口外壳修复与语言功能落在同两个 commit（`6609fa4` 含 capability，`371b0e1` 含 shell），
> 未单独成一个 commit。

| commit | 内容 |
|--------|------|
| `6609fa4` | feat(config,tauri)：用户级设置与 UI 语言后端 |
| `371b0e1` | feat(ui)：i18n（English / 简体中文）+ 窗口外壳修复 |
| `b7f7150` | docs：i18n 管线与新命令 |

**2026-10-10（第 3 轮）**：**第 0 步地基全部完成**——CI 的 lint 三门禁已本地跑通并修复，文档漂移已同步。

- 补齐本地工具链：`rustfmt` / `clippy` 组件（清华镜像缺这两个组件，改用 `static.rust-lang.org`
  的 1.98.1 组件包装入现有工具链）。
- CI lint job 原本**三项全红**，现已全绿：
  - `cargo fmt --all -- --check`：27 个文件未格式化 → 已 `cargo fmt --all`。
  - `cargo clippy --workspace --all-targets -- -D warnings`：15 处 lint（`derivable_impls`、
    `unnecessary_to_owned`、`needless_borrows_for_generic_args`、`question_mark`、
    `field_reassign_with_default`）→ 已修。
  - `cargo clippy --workspace --lib --bins -- -D warnings -D clippy::panic -D clippy::unwrap_used`：
    **9 处生产代码 `unwrap()`**（`MicrosoftAccount` 的 session mutex）→ 改为
    `session_lock()` / `session_snapshot()`，从锁中毒恢复而非 panic。
- 文档漂移已修：`ARCHITECTURE.md` 的 8 类漂移（§2 目录树、§4.1 加密、§7 路径、§8.3 下载粒度、
  §9.1 路由、§10.3 认证流、§11 命令表 31→42、§15.1 数据目录）已对齐实现；
  `DEVELOPMENT_PLAN.md` 已为 5 个 Phase 回填状态块；4 份历史文档已加「已被取代 / 历史快照」横幅。

| commit | 内容 |
|--------|------|
| `9ed6cf0` | fix(lint)：通过 fmt / clippy / 生产 unwrap 三门禁 |
| （本轮 docs） | 同步 ARCHITECTURE.md、DEVELOPMENT_PLAN.md 与历史文档横幅 |

**2026-10-10（第 2 轮）**：§3、§4 记录的 CLI 问题**已全部修复**，工作区**已按主题提交为 10 个 commit**。

| commit | 内容 |
|--------|------|
| `9d3d496` | chore(build)：工具链、CI/release 工作流、依赖清单 |
| `3f9c21e` | feat(auth,config)：账户与配置持久化对齐 HMCL |
| `59b86a7` | feat(version,download)：版本管理本地磁盘中心化 |
| `b4bcb09` | feat(launch,profile)：启动模块按 HMCL 结构重写 |
| `050de4d` | feat(modloader,mods,error)：加载器 provider 与 Modrinth |
| `7618ee0` | refactor(core)：编排抽取为 pipeline + 真 DI |
| `27b2447` | feat(tauri)：IPC 命令面补齐 + ts-rs 绑定 |
| `3840337` | feat(cli)：命令面重建 + 统一输出信封（含下列修复） |
| `9f59d23` | feat(ui)：SolidJS 前端接线 |
| `6d93d52` | docs：审计、整改台账、运维手册与现状交接 |

> 说明：§3 / §4 保留为**问题溯源记录**，其中"现象/根因/修法"仍有价值；"未修复"的措辞已不代表当前状态。

---

## 1. 结论速览

| 维度 | 实测结论 |
|------|----------|
| 编译 | `cargo check --workspace --all-targets` **通过**；clippy 无 warning |
| 前端类型 | `pnpm typecheck` **通过** |
| CI lint 三门禁 | `cargo fmt --check` / `clippy --all-targets -D warnings` / `clippy --lib --bins -D clippy::panic -D clippy::unwrap_used` **全部本地通过** |
| 测试 | **525 passed / 0 failed**（2026-10-21 全量复跑；插件 98 项 + Dashboard 17 项 + SDK 契约 4 项） |
| 版本控制 | HEAD = `80887ce`；Phase 1/2 变更**尚未提交**（80+ 路径，待用户决定提交粒度） |
| 发布体积 | GUI 未剥离 11.53 MiB / 手动 strip **7.85 MiB**；CLI 手动 strip **4.48 MiB**（ADR-011：stripped < 10 MiB ✅；`strip = true` 仍失效） |
| 代码规模 | Rust 183 文件 / **44,584 行**；前端 22 文件 / **7,105 行**；SDK 19 文件 / **1,306 行**（详见 [`AUDIT_2026-10-20.md`](AUDIT_2026-10-20.md)） |
| 文档同步 | `ARCHITECTURE.md` / `AUDIT` / `roadmap` 已按 Phase 1/2 实现回填；ADR-002/003/004/011 accepted |
| 阶段判断 | **Phase 0 / 0.5 / 1 / 2 / 3 完成；Phase 4.1–4.5 完成**；下一步 Phase 5（逻辑运行时，先建 ADR-007） |

**最重要的三个判断**：

1. **结构债已清偿**：pipeline 单一编排、平台适配层、loader 单体系、`context.rs` 93 行、IPC 契约测试、ADR-002 生效值单一来源——Phase 1 全部验收项完成。
2. **插件平台第一阶段已交付**：Manifest v1 + `.rmcpkg` 事务 + 生命周期 + 权限/审计 + 配置 schema + CLI + `/plugins` 页面；损坏插件隔离、安装失败回滚、升级不扩张权限均有测试。
3. **剩余缺口是"生态与分发"而非地基**：隔离 UI（Phase 4）、逻辑运行时（Phase 5）、Agent（Phase 6）、签名/目录/公证（Phase 7），以及前端组件测试（当前 0）。Dashboard 布局/主题（Phase 3）已交付。

---

## 2. 本次实测验证结果

执行环境（本机 macOS / Apple Silicon，Homebrew rustup，工具链 rustc 1.98.1）：

```bash
export PATH=/opt/homebrew/opt/rustup/toolchains/stable-aarch64-apple-darwin/bin:$PATH
export CARGO_HOME=$HOME/.cargo
```

| 命令 | 结果 |
|------|------|
| `cargo check --workspace --all-targets` | ✅ 通过，1 warning：`src-cli/tests/cli_integration.rs:68` 未使用变量 `h` |
| `cargo test --workspace` | ❌ 277 passed / 7 failed（详见 §3） |
| `pnpm typecheck` | ✅ 通过 |
| `cargo build --release -p rmccc-cli` | ✅ 通过，1 warning（`strip` 失败，环境问题） |
| `cargo fmt --check` / `cargo clippy` | ⛔ 无法执行——工具链未安装 `rustfmt` / `clippy` 组件（见 §7.2） |

crate 级单测（全部通过）：`auth 31`、`config 32`、`launch 48`、`modloader 36`、`version 58+2`、`download 18`、`mods 4`、`profile 3`、`core 3`、`cli lib 16`、`src-tauri 1(+1 E2E 默认跳过)`、`rmccc-error 0`。

---

## 3. 当前唯一的硬阻断：CLI 每次调用都 panic（已修）

**现象**：`rmcli` 任何参数都以 exit code 101 panic：

```
Command rmcli: command name `help` is duplicated
  at clap_builder-4.6.2/src/builder/debug_asserts.rs:353
```

**根因**：`src-cli/src/app.rs` 的顶层 `Commands` 枚举定义了自定义 `Help` 变体（`app.rs:87-91`），与 clap 内建的 `help` 子命令重名；clap 的 debug 断言在 debug 构建下直接 panic。

**已做修改（1 行 + 注释）**：在 `Cli` 上追加 `#[command(disable_help_subcommand = true)]`
（[`src-cli/src/app.rs:12-17`](../src-cli/src/app.rs#L12-L17)），使自定义 `help` 子命令取代内建命令。

**效果**：测试失败数 **28 → 7**。

> ⚠️ 该 bug 让此前所有 CLI 测试全部失败，因此它**掩盖了另外 7 个真实失败**。这正是"文档自述全绿但实际为红"的原因。
>
> ✅ **状态：已修复并提交**（`3840337`）。

---

## 4. 其余 7 个失败的分类与定性（均已修复）

> ✅ 本节 7 个失败在 `3840337` 中全部处理完毕，`cargo test --workspace` 现为 **284 passed / 0 failed**。以下保留现象与根因分析，供后续回归参照。

| # | 测试 | 现象 | 定性 |
|---|------|------|------|
| 1 | `e2e_version_and_help` | `rmcli --help` 输出的是 JSON 错误信封，不是帮助文本 | **生产代码缺陷** |
| 2 | `completions_generate_for_each_shell` | `completions --shell tcsh`（不支持的值）被 clap 拒绝，exit 2；测试期望"优雅处理、exit 0" | 契约分歧 |
| 3 | `local_mod_install_list_toggle_remove` | `mod toggle --enabled false` 是用法错误（exit 2） | **生产代码缺陷** |
| 4–7 | `e2e_error_exit_codes`、`json_error_emits_error_envelope_with_exit_code`、`profile_create_rejects_invalid_input`、`local_mod_install_rejects_missing_file` | 测试期望 `"ProfileNotFound"` / `"ValidationError"` / `"NotFound"`，实际输出 `"profile_not_found"` / `"validation_error"` / `"not_found"` | **测试过期** |

### 4.1 缺陷 1：`--help` / `--version` 走错分支（生产代码）—— ✅ 已修

`src-cli/src/lib.rs:45-53` 把 `Cli::try_parse_from` 的所有 `Err` 一律当作用法错误渲染：

```rust
Err(e) => {
    let out = Output::new(OutputMode::Auto, false, stdout_is_terminal); // 注意：硬编码 Auto，忽略 -o
    out.render_clap_error(&e);
    return e.exit_code();
}
```

而 clap 对 `--help` / `--version` 返回的是 `DisplayHelp` / `DisplayVersion` 这两种**非错误** kind（`exit_code()` 为 0）。结果：

```console
$ rmcli --version
{"ok":false,"error":{"code":"cli_usage_error","exit_code":2,"message":"rmcli 0.2.0",...}}
$ rmcli -o plain --version      # -o 也被忽略
{"ok":false,"error":{...同一 JSON...}}
```

**影响**：脚本/CI 无法用 `rmcli --version` 取版本；`--help` 不可读；`-o plain` 在错误路径不生效。
**修法**：在 `Err` 分支先匹配 `e.kind()`，`DisplayHelp` / `DisplayVersion` 直接 `print!("{e}")` 并返回 0；其余再走错误信封；同时把 `-o` 已解析的值传给输出层。同一问题存在于 `execute_line`（`lib.rs:140-153`）。

### 4.2 缺陷 2：`--enabled` 是 flag 而非取值参数（生产代码）—— ✅ 已修

`app.rs:405` 定义为 `enabled: bool`，clap derive 对 `bool` 生成 **SetTrue flag**，因此：

- `mod toggle --profile P --file F` → 禁用（enabled=false）
- `mod toggle --profile P --file F --enabled` → 启用（enabled=true）
- `mod toggle ... --enabled false` → **用法错误**（`false` 成为多余位置参数）

用户无法用 `--enabled false` 显式表达禁用（只能靠省略），与测试及常规 CLI 直觉不符。
**修法**：改为 `#[arg(long, action = clap::ArgAction::Set)] enabled: bool`（或 `Option<bool>` + `num_args(1)`），使取值语义生效。

### 4.3 测试过期的 4 项：错误码大小写 —— ✅ 已修（改测试断言）

`ErrorCode` 的权威定义是 `#[serde(rename_all = "snake_case")]`（[`crates/rmccc-error/src/lib.rs:113`](../crates/rmccc-error/src/lib.rs#L113)），ts-rs 生成的 IPC 契约同样是 snake_case
（[`src/ipc/types.ts:196`](../src/ipc/types.ts#L196) 起：`"profile_not_found" | ... | "validation_error" | "not_found"`）。

因此**测试期望的 PascalCase 才是错的**（很可能是 `rename_all` 加上之前写下的断言）。
**修法**：把这 4 个断言改成 snake_case，与 `types.ts` 契约对齐。

### 4.4 契约分歧：不支持 shell 的处理 —— ✅ 已修（去掉 value_parser）

`app.rs:92-96` 用 `value_parser = ["bash","zsh","fish","powershell","elvish"]` 限制了取值，而 `registry::render_completions` 本可对未知 shell 友好提示。测试期望后者。
**建议**：去掉 `value_parser`，改由 `render_completions` 处理未知值（与测试一致，也更符合"优雅降级"）。

---

## 5. 架构现状（as-built）

### 5.1 crate 分层（严格 DAG，无环）

```
rmccc-core（编排 + DI 容器 CoreContext）
├── rmccc-error        0.2k 行  统一错误 + ErrorCode（snake_case，ts-rs 导出）
├── rmccc-config       2.3k 行  $schema 版本化 / AppConfig·UserSettings / FileSaver 防抖 / paths
├── rmccc-auth         2.9k 行  AccountID + Account/AccountFactory + AccountRepository + TokenVault
├── rmccc-download     1.2k 行  并发下载 / Range 续传 / SHA1 / CacheRepository / DownloadProvider
├── rmccc-version      4.4k 行  GameRepository（本地索引 + resolve 双视图）+ manifest/assets/libraries
├── rmccc-launch       4.0k 行  DefaultLauncher（HMCL 同构命令行）+ natives/log4j/ProcessListener
├── rmccc-modloader    5.6k 行  8 种 loader（Fabric/Forge/NeoForge/Quilt/LiteLoader/OptiFine/Cleanroom/LegacyFabric）
├── rmccc-mods         0.5k 行  ModSource trait + Modrinth 实现
└── rmccc-profile      0.6k 行  Profile JSON 存储（原子写）+ 继承/覆盖模型
src-tauri  2.7k 行  46 个 IPC 命令 + AppState + 事件常量 + ts-rs 绑定导出（24 类型）
src-cli    5.3k 行  clap 命令面 + CliContext 门面 + registry（帮助/补全）+ 统一输出信封
```

### 5.2 关键设计（与 ARCHITECTURE.md 一致且已落地）

- **单一编排层**：`CoreContext` 持有全部 `dyn` provider（download / version / auth / modloader / mod source / account repo / file saver），命令层只做参数适配与输出。
  ⚠ 但 `pipeline` 的 `launch_game` / `repair_resources` / `resolve_version` **零调用方**，Tauri 侧自行复刻了安装/修复/启动——详见审计文档 §5.3、P1-3。
- **本地磁盘为中心**：`DefaultGameRepository` 扫描 `versions/` 建索引，离线 `resolve` 出 `launch` / `standalone` 双视图；网络 `MojangVersionProvider` 只在安装时取元数据。
- **启动链路 HMCL 同构**：`DefaultLauncher` 负责命令行生成、`natives-{platform}` 解压、log4j 提取、`.abnormal` 崩溃标记、环境变量注入、启动脚本生成。
- **持久化收口两条通路**：账户走 `AccountRepository`（明文 metadata + 加密 private 分离），配置/状态走 `FileSaver`（200ms 防抖）。
- **安全**：AES-256-GCM + 本地 `.key`（Windows `icacls`）；token 经 response file 传递且**延迟**删除（JVM 解析完参数后）；capability 禁通用 `fs`/`shell`。
- **类型桥**：ts-rs 自动生成 24 个类型到 `src/ipc/types.ts`（439 行），CI 用 `git diff` 断言其最新。

### 5.3 IPC 面实测

- 定义 46 个 `#[tauri::command]`，注册 46 个（`src-tauri/src/main.rs`），**差集为空**。
- **UI 可直达 27 个**（Dashboard 11 / InstanceSettings 8 / Login 3 / Settings 5）。
- 15 个仅有 `api.ts` wrapper、无页面调用：`mod_search/get_versions/get_download_url/install`、
  `download_pause/resume`、`auth_list_accounts/switch_account/remove_account/refresh_token`、
  `log_get/clear`、`instance_check_completion/repair/refresh`。
- 4 个连 wrapper 都没有：`instance_list/duplicate/rename/remove`——它们操作的是"版本 id"
  而非 profile，属概念未收敛（审计文档 §7.4）。
- 事件常量 Rust 侧 12 个、前端 11 个，缺 `INSTANCES_REFRESHED = "instances://refreshed"`（`src-tauri/src/events.rs:34`）。
- `src/ipc/api.ts` 已不再手写 `SettingsUpdate` / `ProfileUpdateOpts`，改为 `Partial<ts-rs 生成类型>`（第 6 轮修复）。

---

## 6. 进度对照 DEVELOPMENT_PLAN

| Phase | 验收标准达成情况 | 判定 |
|-------|------------------|------|
| **1 Foundation** | 编译通过、下载引擎、认证（MS device flow + 离线 + 加密 vault）、日志、IPC 框架、SolidJS 布局与登录页均就绪 | ✅ 完成 |
| **2 Core Game** | Mojang API、版本列表缓存、assets/libraries、Java 检测、response file 启动参数、进程管理、Profile CRUD 均就绪；CLI 曾端到端启动原版 26.2 | ✅ 完成（GUI 侧启动面板已在 Dashboard 中） |
| **3 Mod Loaders** | 后端 8 种 loader 安装任务齐备；**实例设置页已有 loader 选择 + 版本列表 + 安装**（Cleanroom 仍为 stub） | ✅ 后端完成，前端已接（未逐项实机点击） |
| **4 Mod Management** | Modrinth 搜索/版本/下载/安装/启停/移除 + 本地 mod 管理后端齐备；**实例页已有已装模组列表（启停/删除），仍缺 Mod 浏览/详情页** | ⚠️ 后端完成，前端缺浏览器 |
| **5 Polish & Ship** | CI（lint + 三平台 test + e2e job）、release.yml（三平台打包 + macOS 公证 + updater 签名）、运维手册、updater 配置齐备 | ⚠️ 配置齐备，但 CI 实为红、证书/Secret 未配、无体积审计记录 |

**结构性欠账**（跨 Phase）：

- `src-cli/src/context.rs` **1,511 行** God Class（历史审计 760 行，规模已翻倍）。
- `crates/rmccc-modloader` 并存两套体系：旧 `ModLoaderProvider` + `create_provider`（仅 `core` 使用）与新 `DependencyManager`（Tauri 使用）。
- `crates/rmccc-core/src/pipeline.rs:583` 的 `launch_game` **无任何调用方**——CLI（`context.rs:987` 起）与 Tauri 各自直接构造 `DefaultLauncher`，编排出现两条分叉。
- `crates/rmccc-modloader/src/cleanroom/` 整个子模块是 stub（`execute()` 直接返回 "not yet implemented"）。
- 死代码：`src/App.tsx`（33 行占位，无人 import）、`src/components/TitleBar.tsx`（92 行，0 引用）、`ToastContainer`（从未挂载）、`src/routes/index.ts`（3 行空操作）。

**已解除（第 5 轮）**：曾经的三处启动阻断——`instances=0`（安装目录与仓库根不一致）、
response file 竞态、argfile 首行混入 java 路径——全部修复；"安装 → 仓库索引 → 启动游戏"全链路
已实机跑通（Minecraft 26.3 进入渲染线程并创建窗口），创建实例的版本选择闭环也已完成。

**测试覆盖薄弱处**：`src-tauri/src/commands/` **46 个命令 0 个单测**；前端 0 测试；crate 侧 `rmccc-mods` 4、`rmccc-core` 5、`rmccc-error` 0。

---

## 7. 文档体系现状与漂移

### 7.1 现有文档

| 文档 | 性质 | 时效 |
|------|------|------|
| `ARCHITECTURE.md` | 设计基线，AGENTS.md 指定的最高权威 | ✅ 2026-10-10 已同步实现（1,728 行） |
| `DEVELOPMENT_PLAN.md` | Phase 1–5 路线图 | ✅ 2026-10-10 已回填各 Phase 状态块 |
| `AI_EDITING_RULES.md`（101 行） | 编码规范 | 有效 |
| `ARCHITECTURE_AUDIT_REPORT.md` | 2026-07-31 审计 | ✅ 已加「历史快照」横幅 |
| `ARCHITECTURE_AUDIT_REPORT_2026-08-04.md` | 2026-08-04 审计（Phase 2–4 交界） | ✅ 已加「历史快照」横幅；其 5 个 P0 均已修复 |
| `FIX_PLAN_2026-08-04.md` | 53 项整改台账（P0×5/P1×9/P2×25/P3×14） | ✅ 已加「历史台账」横幅 |
| `BACKEND_ISSUES_INDEX.md` | 51 项问题索引 | ✅ 已加「已被 `PROJECT_STATUS.md` 取代」横幅 |
| `MODLOADER_IMPLEMENTATION_PLAN.md`、`test-flow.md`、`test_log.md`、`OPERATIONS.md` | 专题 | 可用 |
| `plans/*.md`（3 份，2026-08-05） | 认证/配置重构、版本管理兼容、启动模块重写 | ✅ 已加「已落地」横幅 |

### 7.2 ARCHITECTURE.md 的漂移点 —— ✅ 已全部修正（2026-10-10）

> 下列 8 类漂移已在本轮同步到实现，此处保留为记录。

1. `:41` 称离线认证为 **UUID v5** —— 实际已改为官方算法 **UUID v3 = MD5("OfflinePlayer:"+name)**（`crates/rmccc-auth/src/offline.rs`）。
2. `:100-109` 称 `src/`（前端）为 placeholder —— 实际已是真实 Router + 3 个页面（1,134 行）。
3. `:59-64` 称 modloader 为 stubs —— 实际 8 种 loader 已实现（5.6k 行）。
4. `:65-66` 称 `rmccc-mods` 仅类型 —— 实际 Modrinth API 已实现。
5. `:1246-1276` IPC 命令表 31 条 —— 实际 42 条。
6. `:437` 引用 `rmccc-auth/src/crypto.rs`（不存在，实为 `vault.rs`）；`:785`/`:804` 引用 `rmccc-download/src/retry.rs`、`resume.rs`（不存在，该 crate 只有 `lib.rs`/`provider.rs`）。
7. `:963`/`:1268` 把 `download_cancel` 定义为"单文件取消" —— 实际按 `batch_id` 批量取消。
8. §15.1 数据目录仍写 `config.json`/`auth.json` —— 实际已拆为 `config/launcher-settings.json`、`state/launcher-state.json`、`user-settings.json`、`accounts.json` + `private/account-private-data.json`。

> AGENTS.md 规定"设计文档优先，冲突以文档为准"。当前漂移方向是**代码领先于文档**，因此必须先更新文档，否则后续开发会被错误的基线误导。

### 7.3 环境问题（非代码缺陷）

推荐的本地构建环境变量（本机实测可用）：

```bash
export PATH=/opt/homebrew/opt/rustup/toolchains/stable-aarch64-apple-darwin/bin:$PATH
export CARGO_HOME=$HOME/.cargo
```

- **`cargo` 不在 PATH**：Homebrew rustup 未安装任何工具链到 `~/.rustup`，实际工具链在
  `/opt/homebrew/opt/rustup/toolchains/stable-aarch64-apple-darwin`。直接用该路径调用即可正常工作
  （`cargo` / `rustc` / `rustfmt` / `cargo-fmt` / `clippy-driver` / `cargo-clippy` 都在该 bin 目录下）。
- **`rustfmt` / `clippy` 组件**：✅ **已安装**（2026-10-10）。注意配置的清华镜像
  （`mirrors.tuna.tsinghua.edu.cn`）**缺这两个组件的 1.98.1 包（404）**，且已安装工具链的
  `multirust-channel-manifest.toml` 写死了该镜像的绝对 URL，`RUSTUP_DIST_SERVER` 无法覆盖。
  解决方式：从 `https://static.rust-lang.org/dist/2026-09-03/` 取
  `rustfmt-1.98.1-aarch64-apple-darwin.tar.xz` 与 `clippy-1.98.1-aarch64-apple-darwin.tar.xz`，
  解包后用包内 `install.sh --prefix=<toolchain dir>` 装入。
- **`strip = true` 失效**：`rust-objcopy` 因缺 `libLLVM.dylib` SIGABRT，release 二进制未剥离符号
  → 5.74MB；手工 `strip` 后 **4.09MB**。真实体积应报 4.09MB。
  （安装 `llvm-tools` 组件可修此问题，但该组件同样在清华镜像缺失；尚未处理，不影响链接结果。）

---

## 8. 继续开发的建议顺序

### 第 0 步：地基 —— ✅ 全部完成

1. ✅ **提交现有工作**。已按主题拆为 10 个 commit（见 §0），CI 可运行、历史可追溯。
2. ✅ **修掉 §3/§4 的 CLI 问题**，`cargo test --workspace` 已 **292 passed / 0 failed**。
3. ✅ **补齐本地 lint 能力**并跑通 CI 的 lint job：安装 rustfmt/clippy 组件后，发现三门禁原本全红
   （27 文件未格式化 + 15 处 clippy lint + 9 处生产 `unwrap()`），均已修复。
4. ✅ **同步文档漂移**：`ARCHITECTURE.md` 8 类漂移已修（§7.2）、`DEVELOPMENT_PLAN.md` 已回填
   5 个 Phase 状态、4 份历史文档已加取代/快照横幅。

### 第 1 步：补齐前端（Phase 3/4 的缺口，投入产出比最高）

后端两条链路已就绪却无 UI，是当前最明显的"做完一半"：

1. `instance_*` 7 个命令：实例列表/刷新/重命名/复制/删除/完整性检查/修复 → Dashboard 的实例面板 + 修复入口。
2. `modloader_*` 2 个命令：loader 列表 + 安装进度（事件 `modloader://install-progress` 已在 Rust 侧就绪）。
3. Mod 浏览器（Phase 4 验收项）：搜索列表 + 详情 + 安装；以及已装 mod 的启停/删除面板。
4. 清理死代码：删除 `src/App.tsx`、`TitleBar.tsx` 或将其接线；挂载 `ToastContainer`；让 `api.ts` 直接 re-export ts-rs 生成的 `SettingsUpdate`/`ProfileUpdateOpts`/`InstanceInfo`，消除手工重复定义。
5. 前端事件常量补 `INSTANCES_REFRESHED`。

### 第 2 步：结构性偿债

1. 拆分 `src-cli/src/context.rs`（1,511 行）为按域的门面（auth/instance/version/mod/modloader/launch/system）。
2. 统一编排：让 CLI 与 Tauri 都经由 `pipeline::launch_game`（当前无调用方），消除两条分叉。
3. 收敛 modloader 双体系：删除旧 `ModLoaderProvider` + `create_provider`，或在 `core` 侧也改用 `DependencyManager`。
4. 补 `src-tauri/src/commands/` 的单元测试（当前 46 个命令 0 测试）。

### 第 3 步：发布工程

1. 跑一次真实 E2E 验收：登录 → 建实例 → 安装原版 → 安装 Fabric/Forge → 离线启动 → 异常退出检出 `.abnormal`。
2. `cargo bloat` 体积审计：重构后 CLI 已从文档记载的 3.1MB 增至 **4.09MB（剥离后）**，需确认 Tauri 最终产物仍 < 10MB。
3. 配置 `TAURI_SIGNING_PRIVATE_KEY_PATH` Secret 与 updater 的真实 `endpoints`；申请 Apple 证书跑通公证。

---

## 9. 未验证 / 待确认

- `cargo fmt` 与 `cargo clippy` 未能在本机执行（组件缺失），CI lint job 的真实结论未知。
- 未运行 `pnpm tauri dev` / `pnpm build`，GUI 端到端表现未验证。
- 未做任何联网行为验证（版本下载、Modrinth、MS 登录）。
- `failing` 的 7 项测试中，判定"测试过期"的 4 项依据是 `ErrorCode` 的 `rename_all = "snake_case"` 与生成的 `types.ts` 契约；若产品口径要求 PascalCase，则应改生产代码而非测试。
