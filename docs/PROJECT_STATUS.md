# RMCCC 项目现状与继续开发交接（2026-10-10 实测）

> 本文档是**当前状态的单一入口**，用实测结果取代此前各审计快照对"当前状态"的表述。
> 历史审计与整改台账仍然有效作为**问题溯源**（见 §6），但其"当前状态"结论已过期。
> 本文所有数字均由本次在本机实际执行命令得到，未采信文档自述。

---

## 0. 更新记录

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
| 测试 | **292 passed / 0 failed**（修复 CLI 致命 bug 前为 256 / 28，见 §0/§3/§4） |
| 版本控制 | ✅ **已按主题提交为 10 个 commit**，工作区干净（原为 110 改 + 52 新增、零提交） |
| 发布体积 | CLI release 二进制 5.74MB；手工 strip 后 **4.09MB**（`strip = true` 在本机失效，见 §7.3） |
| 代码规模 | Rust 133 文件 / **29,284 行**（不含 `tests/` 为 28,431 行）；前端 12 文件 / 3,203 行 |
| 文档同步 | `ARCHITECTURE.md` / `DEVELOPMENT_PLAN.md` 已对齐实现；历史文档已加取代横幅 |
| 阶段判断 | **Phase 1–2 完成；Phase 3 后端完成、前端未做；Phase 4 后端基本完成、UI 未做；Phase 5 配置齐备但端到端未验证** |

**最重要的三个判断**：

1. **代码质量高于文档所描述的状态**。后端在 2026-08-05 已完成两轮大重构（启动模块重写、版本管理本地仓库化、认证/配置对齐 HMCL），crate 级单测全绿。
2. **工程管理风险已清除**：数千行改动此前零提交、CI 实际为红、文档与代码漂移。现均已处理：工作区已分主题提交、284 项测试全绿、CI lint 三门禁本地通过、`ARCHITECTURE.md` 漂移已同步。
3. **前后端严重不对称**：后端 45 个 IPC 命令已实现并注册，前端只调用 36 个，整个"实例管理/修复"与"ModLoader 安装"两条链路在 GUI 上完全不存在。

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
├── rmccc-config       2.0k 行  $schema 版本化 / SettingFile / FileSaver 防抖 / paths
├── rmccc-auth         2.9k 行  AccountID + Account/AccountFactory + AccountRepository + TokenVault
├── rmccc-download     1.2k 行  并发下载 / Range 续传 / SHA1 / CacheRepository / DownloadProvider
├── rmccc-version      4.2k 行  GameRepository（本地索引 + resolve 双视图）+ manifest/assets/libraries
├── rmccc-launch       3.8k 行  DefaultLauncher（HMCL 20 步命令行）+ natives/log4j/ProcessListener
├── rmccc-modloader    5.6k 行  8 种 loader（Fabric/Forge/NeoForge/Quilt/LiteLoader/OptiFine/Cleanroom/LegacyFabric）
├── rmccc-mods         0.5k 行  ModSource trait + Modrinth 实现
└── rmccc-profile      0.3k 行  Profile JSON 存储（原子写）
src-tauri  2.6k 行  45 个 IPC 命令 + AppState + 事件常量 + ts-rs 绑定导出（21 类型）
src-cli    4.3k 行  clap 命令面 + CliContext 门面 + registry（帮助/补全）+ 统一输出信封
```

### 5.2 关键设计（与 ARCHITECTURE.md 一致且已落地）

- **单一编排层**：`CoreContext` 持有全部 `dyn` provider（download / version / auth / modloader / mod source / account repo / file saver），命令层只做参数适配与输出。
- **本地磁盘为中心**：`DefaultGameRepository` 扫描 `versions/` 建索引，离线 `resolve` 出 `launch` / `standalone` 双视图；网络 `MojangVersionProvider` 只在安装时取元数据。
- **启动链路 HMCL 同构**：`DefaultLauncher` 负责命令行生成、`natives-{platform}` 解压、log4j 提取、`.abnormal` 崩溃标记、环境变量注入、启动脚本生成。
- **持久化收口两条通路**：账户走 `AccountRepository`（明文 metadata + 加密 private 分离），配置/状态走 `SettingFile` + `FileSaver`（200ms 防抖）。
- **安全**：AES-256-GCM + 本地 `.key`（Windows `icacls`）；token 经 response file 传递且 spawn 后立即删除；capability 禁通用 `fs`/`shell`。
- **类型桥**：ts-rs 自动生成 19 个类型到 `src/ipc/types.ts`（259 行），CI 用 `git diff` 断言其最新。

### 5.3 IPC 面实测

- 定义 42 个 `#[tauri::command]`，注册 42 个（`src-tauri/src/main.rs:65-108`），**差集为空**。
- 前端实际调用 36 个；**9 个后端命令前端零调用**：`instance_list/refresh/rename/duplicate/remove/check_completion/repair`（`commands/instance.rs`，253 行）与 `modloader_list/install`（`commands/modloader.rs`，237 行）。
- 事件常量 Rust 侧 11 个、前端 10 个，缺 `INSTANCES_REFRESHED = "instances://refreshed"`（`src-tauri/src/events.rs:34`）。
- `src/ipc/api.ts` 手工重定义了 `SettingsUpdate` / `ProfileUpdateOpts`（`api.ts:39-55`），**遮蔽**了 ts-rs 已生成的同名类型；`InstanceInfo` / `MissingFileInfo` / `LauncherState` / `UserSettings` 未 re-export，前端拿不到实例类型。

---

## 6. 进度对照 DEVELOPMENT_PLAN

| Phase | 验收标准达成情况 | 判定 |
|-------|------------------|------|
| **1 Foundation** | 编译通过、下载引擎、认证（MS device flow + 离线 + 加密 vault）、日志、IPC 框架、SolidJS 布局与登录页均就绪 | ✅ 完成 |
| **2 Core Game** | Mojang API、版本列表缓存、assets/libraries、Java 检测、response file 启动参数、进程管理、Profile CRUD 均就绪；CLI 曾端到端启动原版 26.2 | ✅ 完成（GUI 侧启动面板已在 Dashboard 中） |
| **3 Mod Loaders** | 后端 8 种 loader 安装任务齐备；**前端无 loader 选择/安装 UI** | ⚠️ 后端完成，前端 0% |
| **4 Mod Management** | Modrinth 搜索/版本/下载/安装/启停/移除 + 本地 mod 管理后端齐备；**无 Mod 浏览/详情/管理 UI** | ⚠️ 后端 ~完成，前端 0% |
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

**测试覆盖薄弱处**：`src-tauri/src/commands/` 45 个命令 **0 个单测**；`rmccc-mods` 4、`rmccc-profile` 3、`rmccc-core` 3、`rmccc-error` 0。

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
4. 补 `src-tauri/src/commands/` 的单元测试（当前 45 个命令 0 测试）。

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
