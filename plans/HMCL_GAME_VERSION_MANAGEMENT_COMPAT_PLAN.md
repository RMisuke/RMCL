# 游戏版本管理系统改进报告（以与 HMCL 相同/兼容为目标）

> ✅ **状态：已落地（2026-08-05 完成，2026-10-10 复核）**。阶段 A–E 的主要模块
> （`game_repository` / `default_game_repository` / `resolved` / `file_layout` / `isolation` /
> `compatibility` / `completion`）均已实现并接入启动链路。
> 当前状态与验证结果见 [`../docs/PROJECT_STATUS.md`](../docs/PROJECT_STATUS.md)。

> 分析入口：`HMCL-main/explain/game_version_management_游戏版本管理模块.md` + HMCL 源码（只读）
> 当前项目：RMCL / RMCCC（Rust workspace + Tauri 2.x + SolidJS）
> 日期：2026-08-05 | 状态：待确认，未改任何代码

---

## 一、HMCL 文档要点提炼

### 1.1 模块定位
游戏版本管理是 HMCL 的**数据核心层**，为启动模块与下载模块提供游戏实例的结构化视图，职责覆盖：版本元数据加载、版本 JSON 解析、继承链合并（resolve）、文件系统布局管理、资源索引与 Asset 管理、原生库处理、版本隔离。

### 1.2 核心数据模型（GameInstanceManifest Record）
| 字段 | 说明 |
|------|------|
| `id` | 实例唯一标识（GameInstanceID 值对象，非空字符串） |
| `inheritsFrom` | 继承的父版本 ID |
| `jar` | jar 文件标识（默认取自身 id） |
| `assetIndex` / `assets` | 资源索引信息 / 资源版本名（含 legacy 内置哈希表回退） |
| `libraries` | 库列表 |
| `compatibilityRules` | 兼容性规则（manifest 级 OS/features 判定） |
| `downloads` | client/server 下载信息 |
| `logging` | 日志配置（log_configs） |
| `patches` | 补丁列表（Modloader 注入，含 priority） |
| `root` / `hidden` | 根版本标记 / 隐藏标记 |
| `minecraftArguments` / `arguments` / `mainClass` / `javaVersion` | 新旧参数与入口 |
| `type` / `time` / `releaseTime` / `minimumLauncherVersion` / `complianceLevel` | 版本元信息 |
| `rawJson` | 保留原始 JSON 便于精确回写 |

### 1.3 关键机制
- **继承 + 补丁模型**：版本可 `inheritsFrom` 父版本，Modloader 以 `patches` 注入；`resolve()` 沿链合并产生 `launchManifest`（启动用，无 inheritsFrom 无 patches）与 `standaloneManifest`（编辑用，保留 patches）。**原始 JSON 永不修改，回滚 = 删除 patch。**
- **Resolved 双视图**：`unresolved / launchManifest / standaloneManifest` 三视图。
- **仓库层**：`GameRepository` 接口（索引 + 路径 + resolve）→ `DefaultGameRepository`（文件系统实现，refresh 从 `versions/` 扫描建索引并缓存 resolved）→ `HMCLGameRepository`（隔离策略 + 实例设置 + launcher_profiles.json）。
- **隔离策略**：`NEVER / ALWAYS / MODDED` 三级，经 `getRunDirectory()` 实现。
- **Asset 两级结构**：`assets/indexes/{id}.json` + `assets/objects/{hash[0:2]}/{hash}`；兼容 `assets/virtual/legacy` 旧版。
- **生命周期**：RemoteVersion → GameDownloadTask → Installed → installLibraryAsync(patch) → resolve → checkGameCompletionAsync → launch。
- **事件**：RefreshingInstancesEvent / RefreshedGameInstancesEvent / GameJsonParseFailedEvent（EventBus 发布）。

---

## 二、HMCL 关键机制总结（与本次改造直接相关的）

1. **目录布局（.minecraft）**：
   ```
   versions/{id}/{id}.json + {id}.jar
   libraries/{group}/{artifact}/{version}/{artifact}-{version}.jar
   assets/indexes/{assetIndex}.json
   assets/objects/{hash[0:2]}/{hash}
   assets/virtual/legacy            # 旧版兼容
   versions/{id}/natives-{platform} # 原生库解压目录
   launcher_profiles.json           # Mojang 启动器兼容，Forge/LiteLoader 安装器依赖
   ```
2. **路径方法**：`getInstanceRoot / getInstanceJson / getInstanceJar / getLibrariesDirectory / getLibraryFile / getRunDirectory / getNativeDirectory / getAssetIndexFile / getActualAssetDirectory / getModsDirectory / getResourcePackDirectory / getSavesDirectory / getBackupsDirectory`。
3. **resolve 合并策略**：libraries 叠加、arguments 合并、mainClass 子覆盖父、assetIndex 子覆盖父、javaVersion 子覆盖父、patches 按 priority 排序后逐个 merge、循环依赖检测、`jar` 默认回退自身 id。
4. **Library 完整字段**：artifact / url / downloads（artifact+classifiers）/ checksums / extract(ExtractRules) / natives / rules / hint(`MMC-hint`) / filename(`MMC-filename`)；含 TLauncher 兼容变体。
5. **实例设置**：`{instanceRoot}/.hmcl/config/instance-game-settings.json`，逐实例覆盖运行目录/JVM/内存等。
6. **容忍性加载**：json 文件名与目录名不一致时自动 rename；解析失败发事件可纠正；继承缺失的版本从索引剔除；`launcher_profiles.json` 缺失时自动创建。

---

## 三、当前项目架构问题清单

> 依据：`crates/rmccc-version/`、`crates/rmccc-launch/`、`crates/rmccc-profile/`、`crates/rmccc-modloader/`、`crates/rmccc-config/src/paths.rs`、`src-tauri/src/commands/{version,modloader,launch}.rs`、`crates/rmccc-core/src/lib.rs`。

| # | 问题 | 位置 | 严重度 |
|---|------|------|--------|
| G1 | **启动时从网络 `fetch_version` 解析版本**，不读本地已安装 `versions/{id}/{id}.json`；离线无法启动；modloader 落盘的 patched JSON 启动时被忽略 | [`launch.rs`](src-tauri/src/commands/launch.rs:73) | 🔴 阻断 |
| G2 | **无本地 GameRepository / 实例索引**：无 refresh、无索引缓存、无实例 rename/duplicate/remove 原子操作；`resolve_version` 面向网络而非磁盘 | [`repository.rs`](crates/rmccc-version/src/repository.rs:21) | 🔴 严重 |
| G3 | **数据模型不完整**：`GameVersion` 缺 `jar/compatibilityRules/logging/root/hidden/minimumLauncherVersion/complianceLevel/time/releaseTime`；`Library` 缺 `extract/natives/checksums/hint/filename`；无 Resolved 双视图 | [`types.rs`](crates/rmccc-version/src/types.rs:34) | 🔴 严重 |
| G4 | **Patch 语义与 HMCL 不一致**：modloader_install 把 `patch.merge_into` 结果整体写死到 `versions/{id}/{id}.json`，丢失增量语义；回滚/卸载困难；`resolve_version` 不消费 patch 的 arguments/compatibilityRules | [`modloader.rs`](src-tauri/src/commands/modloader.rs:167) | 🔴 严重 |
| G5 | **启动时版本 JSON 的 JVM arguments 未使用**（`&[]` TODO），版本 JSON 的 jvm 参数丢失 | [`launch.rs`](src-tauri/src/commands/launch.rs:184) | 🟡 中等 |
| G6 | **natives 目录命名不一致**：HMCL 用 `versions/{id}/natives-{platform}`，当前用 `versions/{id}/natives` | [`launch.rs`](src-tauri/src/commands/launch.rs:174) | 🟡 中等 |
| G7 | **目录半兼容、缺 `launcher_profiles.json`**；无 `assets/virtual/legacy` 兼容；无 `.hmcl` 实例设置目录 | `rmccc-config/src/paths.rs` | 🟡 中等 |
| G8 | **共享/隔离策略缺失**：默认 `{data_dir}/rmccc/instances` 每实例独立 game_dir，无共享 libraries/assets，多实例重复下载；无 IsolationType | [`paths.rs`](crates/rmccc-config/src/paths.rs:16) | 🟡 中等 |
| G9 | **Profile 与版本强耦合**：`Profile.game_version` 单值；同一 game_dir 多版本时 profile 仅指向一个；HMCL 是"游戏目录下所有 manifest 即实例集合" | [`lib.rs`](crates/rmccc-profile/src/lib.rs:11) | 🟡 中等 |
| G10 | **兼容规则判定分散**：`library_applies` 与 `rules_match` 两套逻辑；`LibraryRule` 无 features 支持；manifest 级 `compatibilityRules` 未处理 | [`libraries.rs`](crates/rmccc-version/src/libraries.rs:14) | 🟡 中等 |
| G11 | **Asset/natives 兼容不完整**：assets 无 legacy virtual 兼容；natives 按扩展名提取而非 ExtractRules | [`assets.rs`](crates/rmccc-version/src/assets.rs:29)、[`natives.rs`](crates/rmccc-launch/src/natives.rs:44) | 🟡 中等 |
| G12 | **无版本完整性检查/补全**：一次性下载，无 checkGameCompletion 等价物、无增量补全 | `version_install` | 🟡 中等 |
| G13 | **版本解析为手写 `serde_json::Value` 手工解析**，非完整类型化，易漏字段 | [`manifest.rs`](crates/rmccc-version/src/manifest.rs:174) | 🟡 中等 |

---

## 四、与 HMCL 的兼容性差距分析

| 维度 | HMCL | RMCCC 现状 | 差距等级 |
|------|------|-----------|----------|
| 版本清单 | GameInstanceManifest 全字段 + rawJson | GameVersion 缺 8+ 字段 | 🔴 模型不完整 |
| 继承/补丁 | inheritsFrom + patches 动态合并，原始 JSON 不变 | resolve 网络化 + patch 写死合并 | 🔴 语义不一致 |
| 解析视图 | Resolved 双视图（launch/standalone） | 无 | 🔴 缺失 |
| 本地仓库 | DefaultGameRepository 扫描 versions/ 建索引 | 无 | 🔴 缺失 |
| 启动数据源 | 本地磁盘 manifest → resolve → launchManifest | 网络 fetch_version | 🔴 阻断 |
| 目录布局 | 共享 .minecraft + 可选隔离 | 每实例独立 game_dir（全隔离） | 🟡 半兼容 |
| natives 命名 | versions/{id}/natives-{os} | versions/{id}/natives | 🟡 不一致 |
| launcher_profiles.json | 自动生成 | 无 | 🟡 缺失 |
| 兼容规则 | CompatibilityRule 统一判定 | 两套分散逻辑 | 🟡 不一致 |
| Library 字段 | extract/natives/checksums/MMC hint/filename | 缺 | 🟡 不完整 |
| 隔离策略 | NEVER/ALWAYS/MODDED | 无 | 🟡 缺失 |
| 完整性检查 | checkGameCompletionAsync | 无 | 🟡 缺失 |

**兼容性风险点**：
1. **启动与安装的数据源分裂**：安装写本地 JSON，启动却从网络取，导致 modloader 结果、离线启动、本地修复全部失效——这是当前最大的兼容性风险。
2. **patch 语义**：HMCL 生态（Forge/LiteLoader 安装器、其他启动器）按"inheritsFrom + patches + 原版 JSON 不被篡改"工作；当前写死合并结果虽能运行但与 HMCL/官方启动器行为不符，且无法增量叠加多层 loader。
3. **natives 目录**：HMCL 官方布局为 `natives-{platform}`，不一致会与外部工具/文档冲突。
4. **共享 vs 隔离**：当前全隔离导致 libraries/assets 无法跨实例复用，与 HMCL 默认共享 .minecraft 的磁盘效率与兼容目标相悖。

---

## 五、改进方案设计

### 5.1 总体目标
将版本管理从**"网络为中心"**转变为**"本地磁盘为中心"**：引入与 HMCL 对齐的 **GameRepository（实例索引 + 本地 resolve）**，版本 JSON 落盘采用 HMCL 兼容的"原始 JSON + inheritsFrom + patches"语义，启动/修复/补全一律从本地解析，网络仅用于安装时的元数据获取与下载。

### 5.2 目标目录结构（默认共享 .minecraft 布局 + 可选隔离）
```
{game_dir}/                          # 默认 = 共享 .minecraft；隔离模式 = 实例根目录
├── versions/
│   └── {id}/
│       ├── {id}.json                # 版本清单（原版 JSON 或 inheritsFrom + patches）
│       ├── {id}.jar                 # 客户端 jar
│       └── natives-{platform}/      # 原生库解压目录（对齐 HMCL）
├── libraries/{group}/{artifact}/{version}/...
├── assets/
│   ├── indexes/{assetIndex}.json
│   ├── objects/{hash[0:2]}/{hash}
│   ├── virtual/legacy/              # 旧版（1.6 前）资源兼容
│   └── log_configs/                 # logging 对象
├── launcher_profiles.json           # Mojang/HMCL 兼容（Forge/LiteLoader 安装器依赖）
└── .hmcl/ 或 .rmccc/                # 实例设置（隔离模式位于实例根下）
    ├── config/instance-game-settings.json
    └── state/
```

### 5.3 核心模块拆分
建议在 `rmccc-version` crate 内扩展（保持 crate 边界，避免新建 crate 的迁移成本），新增以下模块：

| 模块 | 对标 HMCL | 职责 |
|------|-----------|------|
| `game_repository.rs` | `GameRepository` 接口 | 索引/路径/resolve 契约：`resolve / has_instance / get_manifest / get_resolved / refresh / get_run_directory / get_library_file / get_native_directory / get_instance_jar / get_game_version / rename / asset 系列` |
| `default_game_repository.rs` | `DefaultGameRepository` | 文件系统实现：扫描 `versions/` 建索引、容忍性加载（json 改名、id 不一致修复）、缓存 resolved、compatibilityRules 过滤、循环依赖检测 |
| `file_layout.rs` | `get*Directory` 路径方法 | 统一 `.minecraft` 布局路径解析（供 install/launch/repository 共用） |
| `isolation.rs` | `DefaultIsolationType` | `Never / Always / Modded` 枚举 + 运行目录选择逻辑 |
| `manifest.rs` 重构 | `GameInstanceManifest.Adapter` | 类型化解析（serde derive）全字段，保留 raw_json |
| `resolved.rs` | `GameInstanceManifest.Resolved` | `unresolved / launch / standalone` 三视图 + 校验 |
| `compatibility.rs` | `CompatibilityRule` | 统一 OS/arch/features 判定（合并 library 与 argument 两套逻辑） |
| `patch.rs` 上移 | `GameInstancePatch` | 将 `rmccc-modloader::patch::GameInstancePatch` 提升为版本层共享类型（含 priority + merge） |

### 5.4 关键接口 / 数据流 / 状态管理
- **状态**：`CoreContext` 增加 `game_repository: Arc<RwLock<DefaultGameRepository>>`；`refresh()` 后通过 Tauri 事件（`instances://refreshed`）通知前端；前端凭实例列表渲染（对标 RefreshedGameInstancesEvent）。
- **数据流（安装）**：`version_install` → fetch 元数据 → 下载 client/libraries/assets → 写 `versions/{id}/{id}.json`（原始 JSON）+ `{id}.jar` → `repository.refresh()` → 发射完成事件。
- **数据流（modloader）**：`modloader_install` → fetch 基础版本 → 生成 patch → 写**新版本 JSON**（`inheritsFrom: {game_version}` + `patches: [...]`，不再写死合并）→ refresh → 更新 profile。
- **数据流（启动）**：`launch_start` → `repository.get_resolved_instance_manifest(id)`（本地解析，离线可用）→ 取 launchManifest 的 libraries/arguments/mainClass/javaVersion/assetIndex → 构建参数。
- **数据流（修复/补全）**：`check_completion(manifest)` 对比磁盘缺失项 → 生成补全下载项（对标 checkGameCompletionAsync + GameDownloadTask）。

### 5.5 现有代码集成方式
- `rmccc-version`：补全 `GameVersion`/`Library` 字段；新增 repository/layout/isolation/compatibility 模块；`resolve_version` 改造为基于本地索引（保留网络 fetch 作为"安装时元数据来源"）。
- `rmccc-modloader`：`GameInstancePatch` 类型上移/共享；安装流程改为生成 inheritsFrom+patches 版本 JSON。
- `rmccc-launch`：`launch_start` 改为从 repository 取 resolved manifest；接入版本 JSON jvm args；natives 目录改 `natives-{platform}`。
- `rmccc-profile`：`Profile` 保留（作为"实例显示名/引用"），版本解析职责移交 repository；支持同一 game_dir 多版本。
- `rmccc-config`：`paths.rs` 增加 `.minecraft` 布局 helper；`default_game_dir` 收敛为共享目录 + 可选隔离。
- `rmccc-core`：`CoreContext` 注入 `game_repository`；`refresh_session` 等不受影响。
- `src-tauri`：`version.rs / modloader.rs / launch.rs` 改走 repository；新增 `instance_*` 命令（list/refresh/rename/duplicate/remove/check_completion）。

### 5.6 与 HMCL 版本目录的兼容策略
1. **直接读取**现有 `{game_dir}/versions/{id}/{id}.json`（格式本身与 Mojang/HMCL 兼容）。
2. **生成** `launcher_profiles.json`（缺省时创建默认内容）。
3. **natives 目录迁移**：识别旧 `versions/{id}/natives`，重命名/兼容读取为 `natives-{platform}`。
4. **patch 迁移**：对已写死合并的版本，提供"重新按 inheritsFrom+patches 落盘"的一键修复（基于已记录的 `GamePatch` 元数据，或提示重装 loader）。
5. **assets**：兼容读取 `assets/indexes/{id}.json` 与 objects 哈希分桶；新增 legacy virtual 兜底。
6. **共享优先**：默认共享 libraries/assets（同一 game_dir 内多版本复用），隔离作为显式策略（Modded 自动 / Always / Never）。

### 5.7 迁移路径
- 阶段 A（模型与路径）：纯新增字段（全 Optional 向后兼容）+ FileLayout + 类型化解析。
- 阶段 B（仓库）：新增 GameRepository + DefaultGameRepository + refresh + 本地 resolve，接入 CoreContext。
- 阶段 C（消费方改造）：launch / version_install / modloader_install 迁移到 repository。
- 阶段 D（语义统一）：patch 增量落盘 + 双视图 + compatibility 统一 + natives 命名 + launcher_profiles.json。
- 阶段 E（高级能力）：隔离策略 + 完整性检查/补全 + 实例 CRUD + asset legacy + logging。

---

## 六、分步实施计划（供 Code 模式执行）

> 见 `update_todo_list` 中的执行清单；每一步均以现有测试 + 新增单元/集成测试通过为完成标准。

1. **数据模型补全（G3/G10/G13）**
   - `GameVersion` 增加 `jar/compatibility_rules/logging/root/hidden/minimum_launcher_version/compliance_level/time/release_time`（全 `Option`，向后兼容）。
   - `Library` 增加 `extract/natives/checksums/hint/filename`。
   - 新增 `CompatibilityRule` 类型并统一 `library_applies` / `rules_match`（含 features）。
   - `manifest.rs` 重构为类型化 serde 解析并保留 `raw_json`。
2. **文件布局 helper（G6/G7/G8）**
   - 新增 `rmccc-version/src/file_layout.rs`：`instance_root / instance_json / instance_jar / libraries_dir / library_file / run_dir / native_dir / mods_dir / asset_index_file / asset_object / launcher_profiles` 等路径函数，命名对齐 HMCL。
   - `paths.rs` 接入 `.minecraft` 布局；natives 统一 `natives-{platform}`（含旧 `natives` 迁移读取）。
3. **GameRepository + DefaultGameRepository（G1/G2）**
   - 定义 `GameRepository` trait（resolve/has_instance/get_manifest/get_resolved/refresh/路径方法/rename/asset 系列）。
   - 实现 `DefaultGameRepository`：扫描 `versions/`、容忍性加载（json 改名、id 不一致修复、解析失败告警、继承缺失剔除、compatibility 过滤）、缓存 resolved、循环依赖检测。
   - `CoreContext` 注入 `game_repository`；`refresh()` 发射实例刷新事件。
4. **Resolved 双视图 + resolve 重构（G4）**
   - 新增 `Resolved { unresolved, launch, standalone }` 及校验（launch 无 inheritsFrom 无 patches）。
   - `resolve()` 沿本地索引合并 libraries/arguments/mainClass/assetIndex/javaVersion/compatibilityRules/complianceLevel；patches 按 priority 排序逐个 merge；standalone 保留 patches。
5. **启动流程改造（G1/G5）**
   - `launch_start` 改为 `repository.get_resolved_instance_manifest`；接入 launchManifest 的 jvm/game arguments；natives 目录用新布局；保证离线可启动。
6. **modloader 安装语义改造（G4）**
   - `GameInstancePatch` 提升为版本层共享类型（含 priority + merge）。
   - `modloader_install` 改为写 `inheritsFrom + patches` 增量版本 JSON（非写死合并）；提供已写死版本的一键迁移/重装提示。
7. **兼容产物补齐（G7/G11）**
   - `launcher_profiles.json` 自动生成（默认内容，对标 HMCL PROFILE 常量）。
   - assets legacy virtual 兼容；logging log_configs 支持。
8. **隔离策略 + 完整性检查 + 实例 CRUD（G8/G9/G12）**
   - `IsolationType { Never, Always, Modded }` + 运行目录选择。
   - `check_completion` 补全流程（对标 checkGameCompletionAsync）。
   - 实例 list/rename/duplicate/remove 命令（Tauri + CLI），Profile 收敛为实例引用。

---

## 七、风险说明

| 风险 | 等级 | 缓解 |
|------|------|------|
| 现有已装版本目录兼容 | 低 | `versions/{id}/{id}.json` 布局本就兼容；FileLayout 只读不破坏 |
| patch 语义迁移（写死合并 → 增量） | 中 | 提供一键重落盘/重装；保留旧合并结果读取兼容 |
| natives 目录命名变更 | 低 | 迁移读取旧 `natives`，新安装用 `natives-{platform}` |
| 启动流程改造回归 | 中 | 以离线启动原版 + Fabric + Forge 为验收用例；保留 fetch 兜底 |
| 共享 libraries/assets 后多实例一致性 | 中 | 以 SHA1 校验 + 只追加不删改策略；隔离模式显式可选 |
| 数据模型新增字段影响 IPC/前端类型 | 低 | 全 `Option` 向后兼容；ts-rs 重新生成前端类型 |
| 引入 repository 后与现有 CLI 流程重复 | 中 | 统一走 repository；CLI/Tauri 共用编排（rmccc-core pipeline） |

---

## 附：验证方式
- 单元测试：`resolve()` 合并（继承链、patch 优先级、循环、双视图）、FileLayout 路径、compatibility 判定、manifest 全字段解析。
- 集成测试（wiremock）：version_install 写盘 → repository.refresh → resolve → launch 参数构建全链路。
- E2E 验收：共享 .minecraft 下安装原版 → 安装 Fabric（生成 inheritsFrom+patches）→ 离线启动成功；重复安装第二版本复用 libraries/assets；`launcher_profiles.json` 生成。
