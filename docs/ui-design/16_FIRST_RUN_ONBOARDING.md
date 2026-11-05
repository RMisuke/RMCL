# 初次启动引导（Onboarding）

> 版本：v1（2026-11-04）。实现：`src/components/Onboarding.tsx`、
> `src-tauri/src/commands/onboarding.rs`、`rmccc_instance::compat::detect_launcher_configs`。

## 1. 触发条件

- 启动时在 `AppState::new`（core 初始化之前）取样：标准数据根与便携数据根都不存在
  `settings.json` / `config/launcher-settings.json` / `user-settings.json` /
  `state/launcher-state.json`，且没有 `.onboarded` 标记 → `needs_onboarding = true`。
- `game-directories.json` 由 core 启动时自动生成，**不作为**已配置依据，否则首次运行会被误判。
- 本地验收：设置 `SNGL_FORCE_ONBOARDING=1` 可直接重开向导（不删除数据）。
- 设置 → 高级 → 「初次启动引导」按钮：写入 localStorage 开关，**下一次启动**显示向导；
  便携模式切换会重启，开关在完成向导前一直保留（模式切换仍会真实生效）。
- 完成向导后向所选数据根写入 `.onboarded`；之后不再弹出。

## 2. 页面流程

```
[1 数据模式] ──标准──> [3 登录账户] ──> [完成：欢迎使用 SNGL 启动器！]
      │
      └──便携──> 写入 .portable 标记并重启 ──> [2 便携 .minecraft] ──> [3 登录账户] ...
```

### 2.1 数据模式

- 两张可选卡片：标准模式 / 便携模式，附说明与目标路径。
- 选标准：留在当前进程，直接进入登录页。
- 选便携：调用 `data_mode_switch(mode=portable, migrate=false)`，进程自动重启；
  重启前把选择写入 localStorage（`rmccc.onboarding.pendingMode`），重启后从第 2 页续接。

### 2.2 便携 .minecraft

- 使用启动时快照判断「本次启动前是否已存在」，避免 core 创建空目录后误报。
- 存在：提示将作为默认实例文件夹，并显示已有实例数。
- 不存在：提示继续后会在程序旁创建 `.minecraft` 并作为默认实例文件夹。

### 2.3 导入 HMCL / PCL 配置

检测范围仅限程序根目录与其 `.minecraft`，**不递归** `versions/{id}/...`（那里的 `.hmcl`/`PCL` 是实例元数据，不是目录注册表）。

**HMCL（3.16+ 与旧版）**，按原版读取顺序：

| 来源 | 路径 / 结构 |
|---|---|
| 新版本地 store | `<root>/.hmcl/config/game-directories.json` |
| 新版用户 store | `<root>/.hmcl/config/user-game-directories.json` |
| 当前选中 | `<root>/.hmcl/config/launcher-settings.json` → `selectedGameDirectory` |
| 旧版 | `<root>/.hmcl/hmcl.json`、`<root>/hmcl.json`、`<root>/.hmcl.json` → `configurations.<name>.gameDir` + `last` |

- 相对路径按 HMCL 语义相对于工作目录（程序根目录）解析；绝对路径原样保留。
- `name` 支持字符串或 locale 映射；内置 `Default`/`Home` 不显示自定义名。
- 新版 store 有内容时忽略旧版文件（与 HMCL 迁移行为一致）。

**PCL**，按原版读取顺序：

| 来源 | 路径 / 结构 |
|---|---|
| 文件夹列表 | Windows 注册表 `HKCU\Software\PCL` / `PCLDebug` → `LaunchFolders`（`名称>路径|…`） |
| 当前选中 | `<exe>/PCL/Setup.ini` → `LaunchFolderSelect`（`$` = exe 目录） |
| 兼容旧版 | `Setup.ini` 内的 `LaunchFolders`（部分旧版/改版） |
| 文件夹标记 | `<.minecraft>/PCL.ini`（仅顶层，证明 PCL 使用过该目录） |

- PCL 的 `Setup.ini` 正常位于 exe 旁的 `PCL` 文件夹，而不是 `.minecraft` 内部；
  只有 PCL 被解压进 `.minecraft` 时才读取 `.minecraft/PCL/Setup.ini`。
- `LaunchFolders` 在 Windows 读注册表；非 Windows 平台（或复制来的配置）退回
  `Setup.ini` 的 `LaunchFolders`、默认目录发现与 `.minecraft/PCL.ini` 标记。
- 默认发现与 PCL 一致：exe 目录（含 `versions/`）、其直接子目录（含 `versions/` 或名为 `.minecraft`）、
  Windows `%APPDATA%\.minecraft`；不会进入 `.minecraft/versions/*`。
- INI 支持 UTF-8（含 BOM）、UTF-16 BOM 与 GB18030 回退；键值按第一个 `:` 切分。
- `LaunchFolders` 路径按 PCL 规则以 `\` 结尾；导入时容忍缺失并统一去掉尾部分隔符。
- 选中标记、自定义名称会在卡片上显示；导入仍只登记目录（`game_directory_add`），不复制文件。
- 未检测到任何配置时自动跳过本页。

> 逆向来源：HMCL `SettingsManager.java`（store 路径）、`GameDirectoryManager.java`（合并/选中）、
> `LegacyConfigMigrator.java`（旧版 `configurations`/`last`）、`PortablePath.java`（相对路径）；
> PCL `Modules/Minecraft/ModMinecraft.vb`（目录发现/`$` 选择）、`Modules/Base/ModBase.vb`（INI）、
> `Pages/PageSetup/Settings.vb`（`LaunchFolders` 来源为注册表）、`Modules/ModSecret.vb`（`PCLDebug`）。

### 2.4 登录账户

- Microsoft 设备码登录（复用 `authStore.startMicrosoftLogin`，页内展示验证码）。
- 离线账户登录（用户名）。
- 「稍后登录」跳过。
- 登录成功或跳过后调用 `onboarding_complete` 写入 `.onboarded`，进入欢迎页。

### 2.5 欢迎页

显示「欢迎使用 SNGL 启动器！」，点「开始使用」关闭向导并刷新实例/账户数据。

## 3. 边界与决策

- **不提前写设置**：首次运行跳过 `ensure_dirs` 与文件日志；core 仍可能创建空目录与
  `game-directories.json`，因此检测只认真正的设置文件与 `.onboarded`。
- **模式切换即重启**：避免在内存中同时维护两套数据根；重启后向导自动续接。
- **检测快照**：`.minecraft` 是否存在、配置列表都在 core 初始化前取样，保证向导文案描述的是
  用户机器原有状态，而不是本进程刚创建的文件。
- **IPC**：`onboarding_probe` / `onboarding_complete` 两条，计数 135。
