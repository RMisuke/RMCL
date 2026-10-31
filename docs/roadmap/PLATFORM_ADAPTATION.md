# 平台适配层细化（指南 §5.7 配套）

> **目的**：把指南 §5.7 的"平台特定能力归档到适配层、核心只定义 trait"落为可逐项销账的清单。
> **当前事实来源**：对 `crates/` 与 `src-tauri/` 的 `#[cfg(target_os`/`cfg!(windows`/`cfg(unix` 全量 grep（2026-10-20）。
> **执行约束**：本机仅 macOS；Windows/Linux 后端 deferred，但 trait 与 macOS 后端须先行。

## 1. 归属规则（重申，服从指南 §5.7.1）

1. 启动器核心（Core Service、领域 crate、`pipeline`）只定义 trait 与 DTO，不承担平台特定执行。
2. 凡随 OS/arch 改变的"执行"归档到平台适配层（`crates/rmccc-platform/` 或 `rmccc-core::platform`），`#[cfg]` 只在适配层后端文件内部与选择器出现。
3. **唯一例外**：基于 Mojang 清单 `rules[].os` 的数据筛选属领域规则，但应经注入的 `PlatformDescriptor` 判断，不在领域函数里裸写 `cfg!`。

## 2. 能力域 → trait 清单

| 能力域 | trait（概念） | 关键方法 |
|---|---|---|
| 路径解析 | `PlatformPaths` | `data_dir/config_dir/state_dir/log_dir/cache_dir/game_root_dir` |
| 文件系统耐久 | `PlatformFs` | `atomic_replace`（Unix rename / Windows ReplaceFile）、`sync`、`staging` |
| 归档解压 | `ArchiveExtractor` | `extract_safe`（zip-slip 防护，统一） |
| 文件权限 | `FilePermissions` | `restrict_owner_only`（0600/icacls） |
| 进程控制 | `PlatformProcessControl` | `terminate_tree`/`set_priority`/`new_process_group` |
| 命令行生成 | `ProcessLauncher` | 程序+参数数组+cwd+env 分离；response file 生命周期 |
| Java 探测 | `JavaDetector` | `detect`/`find`（各 OS 路径/注册表/java_home/alternatives） |
| 凭据存储 | `CredentialStore` | `get/set/delete_secret` |
| shell/打开 | `PlatformShell` | `open_url`(http/s)/`open_path`/`run_script`(sh/bat/ps1) |
| 原生分辨率视频壁纸 | `VideoEngine` / `VideoTranscoder` | `prepare`（决策/缓存/校验）/ `transcode`（各平台编码器） |
| 安装器/更新 | `InstallerBackend`/`UpdaterBackend` | 平台打包 + updater target 架构 |
| 平台描述符 | `PlatformDescriptor` | `os/arch/features` 供领域规则筛选 |

## 3. 当前 `#[cfg]` 散点与迁移映射（grep 实测）

| 现有位置（文件:行） | 类别 | 当前行为 | 迁移目标 | 状态 |
|---|---|---|---|---|
| `rmccc-config/paths.rs:10` `dirs::data_dir()` | 路径 | 已用 `dirs` crate | **良好范例**，保留；归 `PlatformPaths` | ✅ 无需迁 |
| `rmccc-auth/vault.rs` `.key` 权限（原 `#[cfg(unix)]` chmod / `#[cfg(windows)] icacls`） | 文件权限 | Unix 权限位 | ✅ `TokenVault::init(path, fs)` 调 `PlatformFs::restrict_owner_only`（Unix 0600 / Windows ACL 待 1.1c-Win） | ✅ 1.1c 完成 |
| `rmccc-launch/process.rs` argfile 0600 + `process_group(0)` | 进程+权限 | Unix 权限位 / 进程组 | ✅ `build_response_file(.., &Platform)` → `fs.restrict_owner_only`；`spawn_game(.., &Platform)` → `process.configure_child_process_group` | ✅ 1.1c 完成 |
| `rmccc-launch/process.rs` / `default_launcher.rs` classpath 分隔符 | 命令行 | `;` vs `:` | ✅ `PlatformDescriptor::classpath_separator()` / `path_list_separator()` | ✅ 1.1c 完成 |
| `rmccc-launch/default_launcher.rs` `cfg!(windows)` 文件分隔符 | 命令行 | 文件/路径分隔符 | ✅ `descriptor.is_windows()` | ✅ 1.1c 完成 |
| `rmccc-launch/default_launcher.rs` `nice -n` 前缀 | 进程优先级 | Unix nice 前缀 | ✅ `process.priority_command_prefix(priority)`（Unix `nice -n N` / 其他空） | ✅ 1.1c 完成 |
| `rmccc-launch/default_launcher.rs` symlink / 0755 / `-Xdock:name` / 非 ASCII natives 兜底 | 文件+数据 | symlink、Unix 权限、macOS dock 参数 | ✅ `fs.symlink`/`fs.remove_symlink`/`fs.make_executable`；`descriptor.extra_jvm_args()`；`descriptor.needs_non_ascii_natives_workaround()` | ✅ 1.1c 完成 |
| `rmccc-launch/java.rs` 的 4 处 `#[cfg(target_os)]`（`java_home` / 注册表 / `/usr/lib/jvm` / 常见路径） | Java 探测 | per-OS `mod` | ✅ `JavaDetector` trait + `JavaInstallation` DTO 迁入 `rmccc-platform::java`；macOS 后端 `MacosJavaDetector`，Windows/Linux 返回 `UnsupportedPlatform`；`launch/java.rs` 只剩 re-export | ✅ 1.1d 完成 |
| `rmccc-version/file_layout.rs` 的 `platform_classifier()` + natives 目录命名 | 路径/数据 | `natives-{platform}` | ✅ `platform_classifier(desc)` / `native_dir_name(desc)` / `native_dir(.., desc)` / `resolve_native_dir(.., desc)` | ✅ 1.1b 完成 |
| `rmccc-version/arguments.rs` 的 `current_os_name/current_arch` + classpath 分隔符 | 数据筛选 | 应用 Mojang `rules[].os/arch` | ✅ 已删除两个 `cfg` 函数；`build_template_vars`/`resolve_arguments`/`rules_match` 全部收 `&PlatformDescriptor` | ✅ 1.1b 完成（2026-10-21） |
| `rmccc-version/libraries.rs` 的 `cfg!` OS/arch + `minecraft_arch()` + per-OS classifier 分支 | 数据筛选 | library OS/arch 过滤 | ✅ `library_applies(lib, desc)` / `resolve_library_items(.., desc)` / `collect_classpath(.., desc)` / `is_current_platform_classifier(.., desc)`；classifier 事实来源收归 `PlatformDescriptor::native_classifier_candidates()` | ✅ 1.1b 完成 |
| `rmccc-version/compatibility.rs` 的 `current_os_name/current_arch` | 数据筛选 | 当前平台规则 | ✅ `os_condition_matches(os, desc)` / `rule_applies(.., desc)` / `rules_allow(.., desc)` / `applies_to_environment(.., desc)`；`DefaultGameRepository` 持注入的描述符 | ✅ 1.1b 完成 |
| `src-tauri/commands/launch.rs` `libc::kill(-pid)` + `taskkill` / `src-cli` `kill_process_group` | 进程终止 | Unix 进程组 + Windows taskkill | ✅ `core.platform.process.terminate_tree(pid)`；两个应用 crate 的 `libc` 依赖已删除 | ✅ 1.1c 完成 |
| `src-tauri/commands/external.rs` 的 `/usr/bin/avconvert` + macOS `#[cfg]` | 视频硬解 | macOS 专用转码 | ✅ 策略/缓存/编码器下沉 `rmccc-platform::video`；macOS `avconvert`、Windows WinRT `MediaTranscoder`，Tauri 层零平台分支 | ✅ macOS 实测（2026-10-30）；Windows 原生 CI deferred |

## 4. 迁移优先级（与 Phase 1 切片对应）

1. ✅ **先定义 trait + `PlatformDescriptor`**（Phase 1.1a，已完成 2026-10-21）：
   `crates/rmccc-platform` + `CoreContext.platform` 注入 + mock 后端 + 24 个单测。
2. ✅ **数据筛选切换**（Phase 1.1b，已完成 2026-10-21）：`rmccc-version` 的
   `libraries/arguments/compatibility/file_layout/completion/default_game_repository`
   全部改为从注入的描述符取值。**`rmccc-version` 现在零平台 `#[cfg]`**（只剩文档注释里提到 `cfg!`）。
   描述符注入路径：`CoreContext.platform` → `pipeline` / `LaunchOptions.platform` /
   `DefaultGameRepository::with_platform`。
   顺带修掉一个真实 bug（审计 P2-12）：`library_applies` 的默认值反了（应 default-deny），
   实测 1.20.4 在 macOS 上因此少选 30 个异平台 natives。
3. ✅ **进程控制迁移**（Phase 1.1c，已完成 2026-10-21）：`terminate_tree`（Tauri `launch_kill` + CLI
   `launch_stop`）、`configure_child_process_group`（`spawn_game`）、`priority_command_prefix`（nice 前缀）。
4. ✅ **文件权限迁移**（Phase 1.1c）：`restrict_owner_only`（vault `.key` + argfile）、
   `make_executable`（启动脚本 0755）、`symlink`/`remove_symlink`（natives 临时链接）。
5. ✅ **Java 探测提升为 trait**（Phase 1.1d，已完成 2026-10-21）：`JavaDetector` +
   `JavaInstallation` 迁入平台 crate；`select_for` 选版规则成为可单测的纯函数
   （精确匹配 > 最近的更高版本 > `JavaVersionMismatch`）。
6. ✅ **命令行生成（分隔符）迁移**（Phase 1.1e）：1.1c 已顺带完成，核对销账
   （`classpath_separator()` / `path_list_separator()`）。

## 4.1 迁移完成度

`crates/*/src` 内（除 `rmccc-platform` 自身）**已无任何平台 `#[cfg]`**：

```
grep -rn 'cfg(unix)|cfg(windows)|cfg(target_os|cfg!(target_os' crates/*/src --include=*.rs \
  | grep -v rmccc-platform/src      → 0（只剩文档注释里提到）
```

## 5. 后端实现状态

**crate 已落地**：`crates/rmccc-platform`（2026-10-21，Phase 1.1a）

| trait / DTO | 文件 | macOS 后端 | Windows | Linux |
|---|---|---|---|---|
| `PlatformDescriptor`（os/arch/分隔符/exe 后缀/native classifier/规则匹配） | `descriptor.rs` | ✅ 数据层 | ✅ 数据层（本机不验） | ✅ 数据层（本机不验） |
| `PlatformFs`（`restrict_owner_only` / `make_executable` / `symlink` / `remove_symlink` / `atomic_replace`） | `fs.rs` | ✅ `chmod 600/755` + symlink + `rename(2)` | ⛔ `UnsupportedPlatform` | ⛔ 同上（实现可直接复用 macOS） |
| `PlatformProcessControl`（`terminate_tree` / `set_priority` / `priority_command_prefix` / `configure_child_process_group`） | `process.rs` | ✅ `killpg` + `setpriority` + `nice` 前缀 + `process_group(0)` | ⛔ `UnsupportedPlatform`（Job Object 待 CI） | ⛔ 同上 |
| `PlatformShell`（`open_url` 仅 http(s) / `open_path`） | `shell.rs` | ✅ `open` | ⛔ `UnsupportedPlatform` | ⛔ 同上 |
| `ProcessPriority` | `process.rs` | ✅ 数据 | ✅ 数据 | ✅ 数据 |
| `JavaDetector`（`detect_all` / `find` + `select_for` 纯函数选版） | `java.rs` | ✅ `java_home -V` + `JAVA_HOME` + 常见路径 | ⛔ `UnsupportedPlatform`（注册表 + `Program Files` 待 CI） | ⛔ 同上（`/usr/lib/jvm` + `update-alternatives`） |
| `VideoEngine` / `VideoTranscoder`（MP4 探测、原生分辨率 HEVC 缓存、精确帧率/帧数校验） | `video.rs` / `video/` | ✅ `avconvert PresetHEVCHighestQuality`（超规格 H.264 转 HEVC，尺寸与时序严格校验） | ⚠️ WinRT `MediaTranscoder::CreateHevc` 代码已落地（强制硬件加速），原生 Windows CI 待验 | ⛔ `UnsupportedPlatform`（不静默回退可能软解的原片；VA-API/GStreamer deferred） |
| `CredentialStore` | — | **未定义**（延后到评估 Keychain 时，见下） | — | — |
| 插件包解压 | `rmccc-plugin::package` | ✅ 纯 OS 无关 + `PlatformFs::atomic_replace` | ⛔ 依赖 `atomic_replace`（deferred） | ⛔ 同上 |

后端选择器只有一处：`Platform::current()`（`lib.rs`）。测试替身：`Platform::mock()` 返回
`Arc<MockPlatform>`，可断言调用序列并模拟单次失败。

> **`CredentialStore` 暂不定义**：指南 §5.7 要求"先评估 Keychain / Credential Manager"，
> 而现有 AES-256-GCM 文件 vault 除 `chmod 600` 外与平台无关。等真正引入 LLM Key 等新场景
> 时再定接口，避免先造一个没人用的抽象。当前唯一平台相关部分（`chmod`）随 1.1c 迁入
> `PlatformFs::restrict_owner_only`。

| 后端 | 状态 | 说明 |
|---|---|---|
| macOS (`aarch64-apple-darwin`) | ✅ 本机实现 + **真机实测** | 含真实子进程 `setpgid`+`killpg` 烟测、真实 Java 探测、MP4 探测与原生分辨率 HEVC 转码；完整链路烟测：安装 1.20.4（3870 文件）→ 装 Fabric 0.15.11 → 启动到主菜单（Mixin + OpenAL 就绪） |
| Windows (`x86_64-pc-windows-msvc`) | **deferred / release-blocked** | 文件/进程/shell/Java 仍返回 `UnsupportedPlatform`；`video::WindowsVideoTranscoder` 代码已落地（WinRT `MediaTranscoder` + 强制硬件加速），需原生 CI 首次编译，以及本机 HEVC 编解码器/GPU/驱动和 WebView2 D3D11 硬解实测 |
| Linux (`x86_64-unknown-linux-gnu`) | **deferred** | 同上；视频准备返回 `UnsupportedPlatform` 并向前端显式报错，不回退可能软解的原视频 |
| macOS Intel (`x86_64-apple-darwin`) | **deferred** | 不在本机构建（Apple Silicon 单架构） |

> **Windows 隔离插件 UI：release-blocked（2026-10-27）**：wry 0.55 的 WebView2 后端把
> Tauri 初始化脚本注入所有子帧，而 Tauri 2.11 把已注册自定义协议视为 local origin——
> 插件 iframe 可能直接拿到 `__TAURI_INTERNALS__`。当前在 `platform_id` 以 `windows`
> 开头时不生成 `uiUrl`（插件卡片/页面降级为声明式），解除条件与替代方案见
> [ADR-006 补充](../adr/ADR-006-isolated-ui-container-and-host-bridge.md)。
> WebView2 实测通过前不得宣称 Windows 插件 UI 可用。

## 5.1 1.1c 真机烟测记录（2026-10-21）

```
rmcli -d /tmp/rmccc-smoke2 system info          # .key 权限 -rw-------（0600，经 PlatformFs）
rmcli -d /tmp/rmccc-smoke2 resource install ... # 3870 文件 / 0 失败 / natives 16 个，0 条异平台告警
rmcli -d /tmp/rmccc-smoke2 launch run ...       # 启动到主菜单：OpenAL 初始化、纹理图集创建、exit_code 0
```

修复的副作用噪音：`decompress_natives` 原先对本平台之外的 native jar 逐条 `warn`
（现在那些 jar 本来就不再下载），安装日志有 30 条 "Native library JAR not found"。
1.1c 让解压端也按 `PlatformDescriptor::accepts_native_classifier` 过滤，告警归零。

## 6. 测试策略

- 适配层 trait 用 mock 后端单测（不依赖真实 OS）。
- **数据筛选测试不再依赖宿主平台**：1.1b 之后，`rmccc-version` 的规则测试改为显式传
  `PlatformDescriptor::macos_arm64()` / `Windows+X86_64` / `Linux` 并双向断言
  （同一份 manifest 喂不同描述符 → 结果互补）。原先散落的 `#[cfg(target_os)] assert!`
  已全部删除——那些断言在非 macOS 上从未被执行过，正是 P2-12 那个 bug 藏身之处。
- macOS 后端：真实 macOS 烟测（启动/解压/权限位）。
- Windows/Linux 后端：原生 CI 触达后补；当前以"trait 存在 + 返回 unsupported_on_platform"为最低验收。
- zip-slip：正负向单测（恶意条目被拒）。
- 数据筛选：用不同 `PlatformDescriptor` 喂同一 manifest，断言 library/asset 选择正确。

## 7. 与指南的同步

- 本文件是指南 §5.7.2 表的展开；指南 §5.7.3 落地策略在此执行。
- 每完成一项迁移，在本文 §3 表"状态"列勾选，并在 `AUDIT`/`PROJECT_STATUS` 回填。
- 若某项迁移涉及行为变化，先提 ADR（如 `terminate_tree` 在 Windows 改用 Job Object）。
