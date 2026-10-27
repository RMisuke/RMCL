# ADR-016: 实例管理 v2 —— 以 HMCL 版本仓库为中心的实例、游戏目录与设置模型

- **状态**：accepted（2026-10-27，取代 [ADR-002](ADR-002-profile-instance-semantics.md)）
- **日期**：2026-10-27
- **阶段**：Phase 2+（实例子系统的结构性重构）
- **关联**：ADR-002（被取代）、ADR-011（体积门禁）、
  `ARCHITECTURE.md` §3.3/§3.4/§15、`ui-design/03_INSTANCE_LIST.md`、`ui-design/04_INSTANCE_DETAIL.md`

## 背景

当前 RMCCC 的"实例"有两套互不相干的事实来源，用户与代码都被迫同时理解：

| 层 | 标识 | 存储 | 现状 |
|---|---|---|---|
| `Profile` | UUID（`p.id`） | `profiles/{id}.json` | 用户卡片、设置、启动、Mod 都以它为单位 |
| 版本仓库 | 版本 id（`1.20.4-fabric`） | `versions/{id}/{id}.json` | `GameRepository` 索引，只用于解析与启动 |

由此产生的具体问题（2026-10-27 代码核对）：

1. **列表口径分裂**：`profile_list` 返回 profile，`instance_list` 返回版本仓库；前端实例页用前者，
   版本仓库页用后者。用户删掉 `versions/{id}` 后 profile 仍在，列表出现"能点但启动必失败"的幽灵实例。
2. **id 语义分裂**：`instance_check_completion` / `instance_repair` 接 profile id 再回落到版本 id，
   `instance_rename` / `instance_duplicate` / `instance_remove` 只接版本 id（ADR-002 决策 2 的历史补丁）。
   同一前缀两套参数，前端与 MCP 都容易传错。
3. **目录模型错误**：`Profile.game_dir` 同时承担"共享游戏根""外部游戏根""实例运行目录"三种语义，
   再用 `external_root` 布尔标记兜底。多游戏目录（`extra_game_roots`）只是只读浏览，不能成为实例归属，
   同一版本 id 出现在两个根下时无法区分。
4. **设置存储位置错误**：实例设置存在 `profiles/{uuid}.json`，而版本/存档/mods 在 `versions/{id}/`。
   复制、重命名、删除版本目录都不会带动设置，设置与实例生命周期脱节。
5. **隔离运行目录取错 id**：`rmccc_launch::instance_run_dir` 用 `profile.game_version` 推导
   `versions/{game_version}`；对 `1.20.4-fabric` 这类实例会落到父版本目录，隔离实际指向错误位置。
6. **配置分层混乱**：全局默认值（`AppConfig.default_*`）、实例值（`Profile` 的平铺字段）、
   覆盖名单（`override_properties`）混在同一个文件/结构里；新增字段要同时改三处，容易漏。
7. **Loader 安装会原地改 profile 的 `game_version`**：旧 profile 指向新版本 id，旧版本目录变成
   仓库里的孤立条目；用户看到的是一个"改名了"的实例，而不是 HMCL 的"新版本 = 新实例"。

UI 设计文档也已把"Profile 与磁盘 versions/ 两套概念混用"列为必须解决的头号问题
（`docs/ui-design/00_OVERVIEW.md` §6、`03_INSTANCE_LIST.md`）。

## HMCL 事实基线（2026-10 代码库）

本 ADR 的模型直接对照 `HMCL-main` 的以下实现，不凭印象设计：

### 1. 实例就是版本目录

- `GameInstanceID` 只是**非空字符串**，即 `versions/{id}` 的目录名；没有独立 UUID
  （`HMCLCore/.../game/GameInstanceID.java`）。
- `DefaultGameRepository.refreshImpl()` 扫描 `{game_dir}/versions/*`：
  - 只接受目录；id 来自目录名；
  - 优先读 `{id}.json`；找不到时，若目录内**恰好只有一个** json，会把它改名为 `{id}.json`
    （jar 同理）——自动修复 id↔文件名不一致；
  - manifest 的 `id` 与目录名不一致时，`moveInstanceFiles()` 把目录/JSON/JAR 一并改成 manifest id；
  - JSON 解析失败、继承父版本不存在、`compatibilityRules` 不适用当前平台 → 该实例不进入索引。
- `resolve()` 产出 `Resolved { unresolved, launchManifest, standaloneManifest }`：
  `launch` 已折叠继承与 patches，`standalone` 保留 patches 供编辑/回写。继承链缺失父版本即
  `NoSuchGameInstanceException`，循环依赖会中断并降级为独立 manifest。

### 2. 游戏目录是实例的一等容器

- `GameDirectories`（`config/local-game-directories.json` + `config/user-game-directories.json`）
  持久化 `GameDirectory { id, name, path, legacyGameSettings }`；`GameDirectoryManager` 合并两份存储，
  维护 `selectedGameDirectory` / `selectedRepository` / `selectedInstance`。
- 每个 `GameDirectory` 对应一个 `HMCLGameRepository`；实例只在**当前选中的目录**内可见。
  这解决了"同一版本 id 出现在多个根"的歧义，也让外部 `.minecraft` 成为正式目录而不是特殊 profile。
- 仓库路径布局（`DefaultGameRepository`）：`getInstanceRoot(id) = {base}/versions/{id}`；
  `getRunDirectory(id)` 默认 `{base}`，由 `HMCLGameRepository` 依据实例设置覆盖。

### 3. 实例设置独立于版本 JSON

- 每个实例：`{instance_root}/.hmcl/config/instance-game-settings.json`
  （`HMCLGameRepository.INSTANCE_METADATA_DIRECTORY`），带 `$schema`、`parent`（设置预设 id）、
  `icon`、`overrideProperties` 与全部字段。读失败/只读时降级为不可写设置，不阻断实例加载。
- 全局侧是**可多套的 `GameSettings.Preset`**（`config/game-settings-presets.json`），实例用
  `parent` 引用其一；`GameSettings.resolve(preset, instance)` 逐字段合并：
  `overrideProperties` 含该字段 → 实例值，否则 preset 值。
- 字段"值"与"是否覆盖"分开存：取消覆盖不会丢掉用户已填的值（`InheritableProperty` 语义）。

### 4. 实例选项全集（`GameSettings`）

HMCL 当前实例可配置项（`HMCL/.../setting/GameSettings.java`）：

| 分组 | 字段 |
|---|---|
| Java | `javaType`（AUTO/CUSTOM/VERSION/DETECTED）、`customJavaVersion`、`customJavaPath`、`detectedJava`（version + path hash） |
| JVM | `jvmOptions`、`noJVMOptions`、`noOptimizingJVMOptions`、`notCheckJVM`、`notCheckGame` |
| 内存 | `autoMemory`、`minMemory`、`maxMemory`、`permSize` |
| 窗口 | `windowType`（WINDOWED/FULLSCREEN/MAXIMIZED）、`width`、`height` |
| 运行目录 | `runningDirectory`（实例覆盖时为 `versions/{id}`，否则共享根） |
| 进程 | `processPriority`、`launcherVisibility`（CLOSE/HIDE/KEEP/HIDE_AND_REOPEN） |
| Java Agent | `allowAutoAgent` |
| 游戏参数 | `gameArguments`、`disableAutoGameOptions` |
| 图形 | `graphicsBackend`（DEFAULT/OPENGL/VULKAN）、`openGLRenderer`、`vulkanRenderer` |
| 环境 | `environmentVariables`、`commandWrapper`、`preLaunchCommand`、`postExitCommand` |
| 快速进入 | `quickPlay`（NONE/MULTIPLAYER/SINGLEPLAYER/REALMS）+ 三个目标字段 |
| 日志 | `showLogs`、`enableDebugLogOutput` |
| 原生库 | `notPatchNatives`、`useCustomNatives`、`nativesDirectory`、`useNativeGLFW`、`useNativeOpenAL` |
| 实例专属 | `parent`、`icon`、`overrideProperties` |

### 5. 生命周期与目录约定

- **新建**：安装向导生成版本 JSON / 下载资源；`applyDefaultIsolationSettingForNewInstance`
  按预设的 `defaultIsolationType`（ALWAYS/MODDED/NEVER）决定是否给新实例加 `runningDirectory` 覆盖。
- **重命名**：`renameInstance()` 移动 `versions/{from}` → `versions/{to}`，改内部 json/jar 名，
  把 manifest 的 `id`/`jar` 改写为 to，并**扫描所有实例**把 `inheritsFrom == from` 的子实例改为 to。
- **复制**：`duplicateInstance(src, dst, copySaves)` 复制目录（黑名单排除源 json/jar、可选 saves），
  改写新 manifest 的 `id/jar`，复制实例设置，并把 `runningDirectory` 覆盖置空（避免仍指向源目录）。
- **删除**：`removeInstanceFromDisk` 先把目录改名为 `{id}_removed`，再尝试移入系统回收站；
  失败才逐文件删除。实例设置缓存同步清理。
- **导入**：拖入 JSON → 解析为 `GameInstanceManifest`，以新 id 下载资源、`saveAsync` 后刷新；
  整合包目录、外部目录都只是"某个 game directory 下的 versions"。
- **图标**：`versions/{id}/icon.{png,jpg,...}`；`icon` 字段是内置图标枚举（DEFAULT 时按 loader/
  版本自动选），自定义文件优先。

## 决策

### 1. 取消 Profile 实体，实例 = 版本仓库条目

- **唯一标识**：`InstanceKey { directory_id, instance_id }`。`instance_id` 就是
  `versions/{instance_id}` 目录名与 manifest id；不再生成 UUID。
- 实例是否存在**只由磁盘决定**：`{game_dir}/versions/{id}/{id}.json` 可解析且继承链完整。
  设置缺失/损坏不影响实例存在，只影响可配置性。
- `display_name` 是设置里的可选显示名（默认 = `instance_id`），不参与路径与身份；
  重命名接口同时支持改显示名与改 id（改 id = HMCL `renameInstance` 语义）。
- 删除 profile 与删除版本目录不再是两个动作：删除实例默认**只删设置**（保留游戏文件），
  显式 `delete_files` 才删 `versions/{id}`；外部目录默认永删文件。

### 2. 游戏目录注册表（`GameDirectoryStore`）

- 新文件 `config/game-directories.json`：
  ```json
  {
    "$schema": "https://rmcl.snowebsite.top/json/schema/game-directories/1.0.0",
    "selected": "default",
    "directories": [
      { "id": "default", "name": "默认目录", "path": "...", "kind": "managed" },
      { "id": "hmcl-...", "name": "HMCL", "path": "...", "kind": "external" }
    ]
  }
  ```
- 首次启动从 `AppConfig.game_root_dir` + `extra_game_roots` **迁移生成**；此后以注册表为准。
  `AppConfig` 保留旧字段仅用于读取旧配置，不再参与实例解析。
- 目录可增删改、可切换；删除目录只移除注册，不删磁盘文件。
- 同一物理目录（canonicalize 后）只允许注册一次。
- 实例列表默认返回**当前选中目录**的实例；传 `directory_id` 或 `all=true` 可跨目录查询。
  跨目录时 `InstanceKey` 必填，杜绝 id 冲突。

### 3. 设置分层：全局预设 + 实例覆盖

- `config/game-settings.json` 保存**唯一默认预设**（`GlobalGameSettings`），首启从旧
  `AppConfig.default_*` 迁移。未来可扩展为多预设；实例设置预留 `parent` 字段。
- 实例设置写入 `{instance_root}/.rmccc/instance.json`（HMCL `.hmcl` 的同构位置）：
  - 随版本目录复制/重命名/删除自然移动，不再依赖 UUID 映射；
  - 外部只读目录不可写时，实例仍可用默认值启动，设置页显示只读原因；
  - 旧 `profiles/*.json` 在首启时迁移为实例设置，原文件保留但不再读取。
- 字段"值 + 覆盖名单"模型保留（ADR-002 的正面部分）：`overrides` 命中字段取实例值，
  否则取全局预设；取消覆盖保留值。
- 设置模型覆盖 HMCL 选项全集（见上表），并补齐 RMCCC 已有但 HMCL 未单列的开关
  （`use_native_glfw/openal`、`no_generated_*`、`enable_debug_log_output`、`wrapper`、
  `java_agents`、`env_vars`）。不支持的字段在启动层显式记录"尚未消费"，不静默装作生效。

### 4. 运行目录

`run_dir` 的推导顺序（单一入口 `InstanceService::paths`）：

1. 实例设置 `running_directory` 覆盖（绝对路径或相对 game_dir）；
2. 否则 `isolation`（实例值，缺省取全局预设 `default_isolation`）：
   `always` / `modded && modded` → `versions/{instance_id}`；
3. 否则 `game_dir`。

这修正了旧实现用 `game_version`（父版本 id）推导隔离目录的缺陷。

### 5. 命令面（v2）

统一 `instance_*` 前缀，全部接 `directory_id + instance_id`（或 `key`）：

| 命令 | 语义 |
|---|---|
| `game_directory_list/add/update/remove/select` | 目录注册表 |
| `game_directory_overview` | 侧边栏行：目录 + 标准布局标记 + 实例数 + 是否选中 |
| `game_directory_scan` | 读取 HMCL / PCL / 官方 `.minecraft` 位置，返回待导入候选 |
| `game_directory_reveal(id)` | 在 Finder/资源管理器中打开目录 |
| `instance_list(directory_id?, all?)` | 聚合视图（版本 + 设置 + loader + 路径），**不做完整性全量扫描** |
| `instance_get(key)` | 详情（summary + 存储设置 + 生效设置 + 路径 + manifest 诊断） |
| `instance_update_settings(key, patch)` | 设置/覆盖名单/显示名/图标/隔离 |
| `instance_resolved_settings(key)` | 生效值（UI 唯一来源） |
| `instance_paths(key)` | 全部目录 |
| `instance_install(directory_id, mc_version, instance_id?, loader?)` | 安装向导（异步事件） |
| `instance_import(directory_id, version_id)` | 把已有版本登记为实例（目录内已有文件） |
| `instance_rename(key, new_id)` / `instance_duplicate(key, new_id, copy_saves)` / `instance_remove(key, delete_files)` | 生命周期（HMCL 语义） |
| `instance_check_completion(key)` / `instance_repair(key)` | 按需完整性 |
| `launch_start/kill/status(key)` | 运行态 |
| `modloader_install(key, loader, version)` | 生成新版本 = 新实例，不再原地改旧实例 |
| `mod_*` / `mods_*` | 入参改为 key，内部用实例 run_dir |

旧 `profile_*` 命令与 `instance_*` 旧语义已在前端切换完成后删除。

### 6. 核心门面

新增 crate `rmccc-instance`，作为 ADR-002 §4 中 `InstanceService` 的落地：

- `GameDirectoryStore` / `InstanceService` 是唯一实例读写入口；
- `rmccc-core` 只做编排（安装/修复/启动），不再直接读写 `profiles/`；
- Tauri / CLI / MCP 都经 `CoreHandle.instances`；
- 旧 `rmccc-profile` crate 与 `profile_*` 命令已删除；legacy JSON 解析内联在 `rmccc_instance::migrate`，只服务一次性迁移。

### 7. 前端对齐

实例列表只显示实例（不再有"版本仓库"平行页），实例详情五 tab 的数据全部来自
`instance_get` / `instance_resolved_settings` / `instance_paths`；"新建实例"进入安装向导，
完成后自动选中新实例。运行态 key 使用 `{directory_id}/{instance_id}`，事件 payload 同步改键。

### 8. 目录发现与 HMCL/PCL 兼容

实例目录本身**不引入私有格式**：一个游戏目录就是标准 `.minecraft`：
`versions/{id}/{id}.json` + `{id}.jar`、`libraries/`、`assets/`、`mods/`、`saves/` 等。
RMCCC 的私有物只有两层，且都放在启动器自己的配置区/版本目录内，其他启动器会直接忽略：

| 层 | RMCCC | HMCL | PCL |
|---|---|---|---|
| 目录注册表 | `{data_dir}/config/game-directories.json` | `{HMCL_HOME}/config/game-directories.json` | `PCL/Setup.ini` 的 `LaunchFolders` |
| 实例覆盖设置 | `versions/{id}/.rmccc/instance.json` | `versions/{id}/.hmcl/config/instance-game-settings.json` | `versions/{id}/PCL/Setup.ini` |
| 目录选择 | `selected` 字段 | `launcher-settings.json` 的 `selectedGameDirectory` | `LaunchFolderSelect` |

因此目录结构完全互通：HMCL/PCL 建的游戏目录可以直接在 RMCCC 打开（`game_directory_add`），
RMCCC 建的 `versions/{id}` 也能被 HMCL/PCL 识别。为了让互换更省事：

- `game_directory_scan` 读取 HMCL 的 `game-directories.json` / `user-game-directories.json`
  （含本地化 `name`）与旧 `hmcl.json` 的 `gameDir`，读取 PCL `PCL/Setup.ini` 的
  `LaunchFolders`（`名称>路径|…`，`$` 展开为 PCL 根目录）以及官方启动器位置；
- 扫描结果带 `sources`（rmccc/hmcl/pcl/mojang）、`registered`、`preferred`、标准布局标记
  和实例数；导入只登记路径，不复制/移动文件；
- 添加空目录时补建 `versions/`，使其立刻成为标准游戏目录（PCL 同样会创建
  `.minecraft/versions`）；
- 目录行显示 HMCL（`.hmcl/`）与 PCL（`PCL/`）实例元数据标记，便于用户确认来源；
- 不写 HMCL/PCL 的注册表，避免两个启动器同时改写对方配置。

`DirectoryLayout`（`has_versions/libraries/assets/launcher_profiles/hmcl/pcl`）是"能否被
其他启动器读取"的唯一判定来源，前端只展示、不猜测。

## 选项

- **A. 继续修补 Profile**（保留 UUID + `external_root`）：改动小，但无法解决多目录歧义、
  设置位置与版本生命周期脱节；已证伪。
- **B. 只把 `Profile.game_version` 改成版本 id，仍以 Profile 为注册表**：id 语义统一一半，
  外部目录/只读目录/同 id 多根仍无解。
- **C. HMCL 同构：目录注册表 + 版本即实例 + `.rmccc` 设置覆盖**（本 ADR 采纳）：
  一次性改动大，但把"实例是什么"收敛为磁盘事实，后续安装/复制/重命名/删除/导入全部同构。

## 后果

**正面**

- 消除"幽灵实例"与两套 id；所有命令参数统一。
- 多游戏目录、外部 HMCL 目录、同版本 id 多根都有明确语义。
- 设置随实例目录移动，复制/重命名/删除不再需要同步 UUID 记录。
- Loader 安装 = 新版本目录，不破坏基础版本，可回滚。
- 实例列表不再做数千次 stat，首屏更快。

**负面 / 代价**

- 数据迁移必须一次成功；旧 `profiles/*.json` 需保留只读副本以便回滚。
- 实例 id 成为用户可见名称，改名会改目录名（与 HMCL 相同，但和旧"只改显示名"不同）。
- 外部目录只读时设置不可持久化（明确报错，而非静默丢失）。
- 旧 `profile_*` 命令在一个开发周期内直接切换（无外部稳定 API 承诺）；CLI/GUI 同步改为 key。

## 迁移影响

1. 首启：
   - 若无 `game-directories.json`：由 `AppConfig.game_root_dir` 生成 `default` 目录，
     `extra_game_roots` 逐个生成 external 目录。
   - 若无 `game-settings.json`：把 `AppConfig.default_*`（含旧 `Profile` 里出现过的字段默认值）
     迁移为默认预设。
   - 若无 `state/instance-migration-v2.json`：扫描 `profiles/*.json`：
     - 根 = `external_root ? game_dir : 默认目录`；找不到则新建 external 目录；
     - 版本 id = `profile.game_version`；若 `versions/{id}/{id}.json` 不存在，跳过并记诊断；
     - 设置 = 旧 Profile 字段（含 `override_properties`、`isolation`、`icon`、游玩时长）；
     - 写入 `{instance_root}/.rmccc/instance.json`；
     - 迁移完成写标记；旧文件不删除。
2. `LauncherState.selected_profile_id` 映射到选中目录 + 实例 key（按版本 id 匹配）。
3. 回滚：删除 v2 标记与两个新配置文件即可回到旧读取路径；旧 `profiles/` 未被修改。

## 跨平台验证方式

- 目录/文件名校验覆盖 Windows 保留名、大小写冲突、结尾空格/点、路径分隔符与控制字符；
- 所有路径拼接经 `rmccc_version::file_layout` 与 `rmccc_platform`，不内联 `cfg`；
- 本机仅 macOS 实测；Windows/Linux 的 trash/权限差异记 deferred（存量平台约束）。

## 状态变更记录

- 2026-10-27：accepted，取代 ADR-002；作为实例管理 v2 的设计基线。
- 2026-10-27：补充 §8 目录发现与 HMCL/PCL 兼容；新增 `game_directory_overview/scan/reveal`，
  前端改为左侧目录抽屉 + 右侧实例列表（卡片/列表同数据）。
