# Modloader 安装模块 — HMCL 架构分析与 Rust 实现方案

> 基于对 `/Users/chensinuo/Codex/HMCL-main/HMCLCore/` 源码的逐文件阅读与分析，
> 本文档产出适合 RMCL（Rust + Tauri）项目的 Modloader 安装模块设计。

---

## 一、HMCL 架构提炼

### 1.1 三层抽象

HMCL 将 Modloader 安装拆分为三个正交层次：

| 层次 | 对应类 | 职责 |
|------|--------|------|
| **版本发现层** | `VersionList<T: RemoteVersion>` | 从远程 API 拉取可用版本，按 gameVersion 索引 |
| **安装执行层** | `Task<GameInstancePatch>` | 下载安装器、解析元数据、生成 patch |
| **依赖管理层** | `DefaultDependencyManager` | 统一入口，路由到 VersionList → InstallTask |

三者通过 `RemoteVersion.getInstallTask()` 工厂方法桥接：版本对象知道如何创建自己的安装任务。

### 1.2 核心数据流

```
用户请求安装 "forge 49.0.50 for MC 1.21"
  │
  ▼
DependencyManager.installLibraryAsync("1.21", baseManifest, "forge", "49.0.50")
  │
  ├─► getVersionList("forge") → ForgeVersionList
  ├─► versionList.loadAsync("1.21")      // 懒加载版本列表
  ├─► versionList.getVersion("1.21", "49.0.50") → ForgeRemoteVersion
  ├─► removeLibraryAsync(baseManifest, "forge")  // 先卸载旧版
  └─► remoteVersion.getInstallTask(dm, manifest) // 创建安装任务
        │
        ├─► preExecute: 下载安装器 JAR（带 ZIP 完整性校验 + 缓存）
        ├─► execute: 解析 install_profile.json，执行 Processor
        │     ├─► 提取 maven/ 内嵌库
        │     ├─► 下载 Processor 依赖
        │     ├─► 依次执行 Processor（含模板变量替换、SHA-1 校验）
        │     └─► 生成 GameInstancePatch
        └─► postExecute: 清理临时文件
            │
            ▼
          manifest.addPatch(patch)  → 新的 GameInstanceManifest
```

### 1.3 Patch 系统（关键设计）

HMCL 不直接修改 Minecraft 的 `version.json`，而是通过 **Patch 叠加** 实现非破坏性安装：

```
GameInstanceManifest {
    id: "1.21",
    mainClass: "net.minecraft.client.main.Main",
    libraries: [...vanilla libs...],
    patches: [
        GameInstancePatch {              // ← Forge 安装后追加
            id: "forge",
            version: "49.0.50",
            priority: 30000,            // 高优先级覆盖
            mainClass: "cpw.mods.modlauncher.Launcher",
            libraries: [...forge libs...],
            arguments: Arguments { ... },
        }
    ]
}
```

- `priority` 决定合并顺序：数值越大越优先
- 预定义优先级：`PRIORITY_MC=0` < `PRIORITY_LOADER=30000` < OptiFine=10000 < LiteLoader=60000
- 回滚只需移除对应 patch

### 1.4 各 Modloader 安装策略对比

| Modloader | 版本数据源 | 安装方式 | 复杂度 |
|-----------|-----------|----------|--------|
| **Fabric** | `meta.fabricmc.net/v2` | 拉取 launchMeta JSON，构建 patch（mainClass + libraries） | 低 |
| **Quilt** | Quilt Meta API | 同 Fabric，额外处理 hashed/intermediary 映射 + Maven 仓库路由 | 低 |
| **Forge (旧)** | Forge Maven Metadata | 解析 `install_profile.json` → 提取 universal JAR → 直接 patch | 中 |
| **Forge (新)** | HMCL/BMCLAPI 元数据 | 解析 `install_profile.json` → 下载 Processor 依赖 → 执行 Processor → patch | 高 |
| **NeoForge** | BMCLAPI | 复用 Forge 的 `ForgeNewInstallProfile` 结构，识别 NEOFORGE.RSA 签名 | 中 |
| **OptiFine** | BMCLAPI | 下载 JAR → 运行 `optifine.Patcher` 或直接复制 → 注入 launchwrapper tweaker | 中 |
| **LiteLoader** | LiteLoader 仓库 | 直接构建 patch（library + tweakClass 参数） | 低 |
| **Cleanroom** | Cleanroom Maven | 类似 Forge 模式 | 中 |
| **LegacyFabric** | LegacyFabric Meta | 类似 Fabric 模式 | 低 |

### 1.5 类型检测与冲突处理 (`LibraryAnalyzer`)

`LibraryAnalyzer` 通过 Maven 坐标模式匹配识别已安装的 Modloader：

```
net.minecraftforge:forge|fmlloader  → FORGE
net.fabricmc:fabric-loader          → FABRIC
org.quiltmc:quilt-loader            → QUILT
net.neoforged.fancymodloader:core|loader → NEO_FORGE
com.mumfrey:liteloader              → LITELOADER
(net.)?optifine:OptiFine            → OPTIFINE
com.cleanroommc:cleanroom           → CLEANROOM
net.legacyfabric:fabric-loader      → LEGACY_FABRIC
```

特殊规则：
- **LegacyFabric vs Fabric**：LegacyFabric 还检查是否存在 `net.legacyfabric` group 的其他库
- **Forge vs NeoForge**：Forge 检测前先排除 NeoForge（因为 NeoForge 也包含 `forge` 字样）
- **版本提取**：Forge/NeoForge 从 library version（格式 `MCVER-FORGEVER`）和 `--fml.neoForgeVersion` 参数中提取真实版本号

### 1.6 关键兼容性规则

从源码中提取的安装顺序与兼容性约束：

| 规则 | 说明 |
|------|------|
| Fabric/Quilt 必须先装 | `preExecute` 检查 mainClass 必须是 vanilla |
| OptiFine 必须最后装 | 优先级 10000，覆写 mainClass 为 LaunchWrapper |
| LiteLoader 在 Forge 之后 | 优先级 60000，依赖已有的 LaunchWrapper 环境 |
| Forge 1.13+ 不兼容 Fabric | 检查 mainClass 必须在 `FORGE_OPTIFINE_MAIN` 集合内 |
| OptiFine H1 Pre2+ 才兼容 Forge 1.17 | `buildof.txt` 版本检查 |
| Forge 48.0.0~49.0.50 与 OptiFine 不兼容 | `FORGE_OPTIFINE_BROKEN_RANGE` |

---

## 二、与现有 Rust 实现的差距分析

当前 `crates/rmccc-modloader/` 实现了基础的「运行官方安装器」模式：

| 能力 | 现有实现 | 缺失 |
|------|---------|------|
| 版本列表 | Forge: HMCL metadata → 解析；Fabric: `/versions/loader/{game}`；NeoForge: Maven API | 缺少版本缓存、懒加载、ReadWriteLock 保护 |
| 安装 | Forge/NeoForge: `java -jar --installClient`；Fabric: `/profile/json` | 没有 headless 安装（ForgeNewInstallTask 的 Processor 执行）；没有 patch 系统；没有版本兼容性检查 |
| 类型检测 | 无 | 完全没有 `LibraryAnalyzer` 等价物 |
| 多版本共存 | 无 | 没有 patch 叠加、优先级、回滚 |
| 安装器自动识别 | 无 | 无法拖入安装器文件自动识别类型 |
| OptiFine/LiteLoader/Quilt 等 | 无 | 未实现 |

---

## 三、Rust 实现方案

### 3.1 模块拆分

```
crates/rmccc-modloader/
├── Cargo.toml
├── src/
│   ├── lib.rs              # 公开接口 + ModLoaderType 枚举 + 工厂函数
│   ├── version_list.rs     # VersionList trait + 通用实现（缓存、懒加载）
│   ├── remote_version.rs   # RemoteVersion trait + 通用数据结构
│   ├── install_task.rs     # InstallTask trait + 通用安装流程
│   ├── patch.rs            # GameInstancePatch 数据结构 + merge 逻辑
│   ├── analyzer.rs         # LibraryAnalyzer — 类型检测
│   ├── dependency.rs       # DependencyManager — 统一入口
│   │
│   ├── fabric/
│   │   ├── mod.rs
│   │   ├── version_list.rs   # FabricVersionList
│   │   ├── remote_version.rs # FabricRemoteVersion
│   │   └── install_task.rs   # FabricInstallTask
│   │
│   ├── forge/
│   │   ├── mod.rs
│   │   ├── version_list.rs   # ForgeVersionList
│   │   ├── remote_version.rs # ForgeRemoteVersion
│   │   ├── install_task.rs   # ForgeInstallTask（路由到 new/old）
│   │   ├── new_install.rs    # ForgeNewInstallTask（Processor 执行）
│   │   ├── old_install.rs    # ForgeOldInstallTask
│   │   └── profile.rs        # ForgeNewInstallProfile / ForgeInstallProfile 数据结构
│   │
│   ├── neoforge/
│   │   ├── mod.rs
│   │   ├── version_list.rs
│   │   ├── remote_version.rs
│   │   └── install_task.rs
│   │
│   ├── quilt/
│   │   └── ...（结构同 fabric）
│   │
│   ├── optifine/
│   │   └── ...（含 Patcher 执行 + launchwrapper 提取）
│   │
│   └── liteloader/
│       └── ...
```

### 3.2 核心 Trait 设计

```rust
// ===== version_list.rs =====

/// 版本列表的通用行为。每种 Modloader 实现自己的数据源。
#[async_trait]
pub trait VersionList: Send + Sync {
    /// 该 Modloader 是否有版本类型分类（release/snapshot/old）
    fn has_type(&self) -> bool;

    /// 从远程刷新版本列表，按 game_version 分组存储。
    async fn refresh(&self, game_version: Option<&str>) -> Result<()>;

    /// 懒加载：如果已加载则跳过，否则刷新。
    async fn load(&self, game_version: &str) -> Result<()> {
        if !self.is_loaded(game_version) {
            self.refresh(Some(game_version)).await
        } else {
            Ok(())
        }
    }

    /// 获取特定 game version 下的所有 modloader 版本。
    fn get_versions(&self, game_version: &str) -> Vec<Arc<dyn RemoteVersion>>;

    /// 查找特定版本。
    fn get_version(&self, game_version: &str, remote_version: &str)
        -> Option<Arc<dyn RemoteVersion>>;

    /// 是否已加载指定 game version 的列表。
    fn is_loaded(&self, game_version: &str) -> bool;
}

// ===== remote_version.rs =====

/// 远程版本信息。
pub trait RemoteVersion: Send + Sync {
    fn library_id(&self) -> &str;       // "forge", "fabric", ...
    fn game_version(&self) -> &str;     // "1.21"
    fn self_version(&self) -> &str;     // "49.0.50"
    fn urls(&self) -> &[String];        // installer URLs
    fn release_date(&self) -> Option<DateTime<Utc>>;
    fn version_type(&self) -> VersionType; // Release, Snapshot, Old

    /// 创建此版本的安装任务。
    fn create_install_task(
        &self,
        dm: &DependencyManager,
        manifest: &GameInstanceManifest,
    ) -> Result<Box<dyn InstallTask>>;
}

// ===== install_task.rs =====

/// 安装任务的异步生命周期。
#[async_trait]
pub trait InstallTask: Send + Sync {
    /// 返回依赖的前置任务（如下载安装器文件）。
    fn dependents(&self) -> Vec<Arc<dyn InstallTask>>;

    /// 返回依赖的后置任务（如下载 patch 的 libraries）。
    fn dependencies(&self) -> Vec<Arc<dyn InstallTask>>;

    /// 执行安装逻辑，返回 GameInstancePatch。
    async fn execute(&mut self) -> Result<GameInstancePatch>;

    /// 清理临时文件。
    async fn cleanup(&self) -> Result<()>;
}
```

### 3.3 关键数据结构

```rust
// ===== patch.rs =====

/// 安装补丁 — 叠加到 GameInstanceManifest 上。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GameInstancePatch {
    pub id: String,                        // "forge", "fabric", ...
    pub version: String,                   // "49.0.50"
    pub priority: i32,                     // 0=MC, 30000=loader, 10000=optifine, 60000=liteloader
    pub main_class: Option<String>,
    pub arguments: Option<VersionArguments>, // game + jvm args
    pub libraries: Vec<Library>,
    pub asset_index: Option<AssetIndex>,
    pub java_version: Option<JavaRequirement>,
    pub inherits_from: Option<String>,
    pub downloads: Option<HashMap<DownloadType, DownloadInfo>>,
    pub logging: Option<HashMap<DownloadType, LoggingInfo>>,
    pub compatibility_rules: Vec<CompatibilityRule>,
    pub hidden: bool,
}

impl GameInstancePatch {
    pub const PRIORITY_MC: i32 = 0;
    pub const PRIORITY_LOADER: i32 = 30000;

    /// 合并 patch 到 manifest：高优先级覆盖低优先级。
    pub fn merge_into(&self, manifest: &mut GameInstanceManifest);
}

// ===== forge/profile.rs =====

/// 新版 Forge install_profile.json
#[derive(Debug, Deserialize)]
pub struct ForgeNewInstallProfile {
    pub spec: i32,
    pub minecraft: String,
    pub json: String,       // version.json 在 JAR 内的路径
    pub version: String,
    pub path: Option<Artifact>,
    pub libraries: Vec<Library>,
    pub processors: Vec<Processor>,
    pub data: HashMap<String, DataEntry>,
}

#[derive(Debug, Deserialize)]
pub struct Processor {
    pub sides: Option<Vec<String>>,   // ["client", "server"]
    pub jar: Artifact,
    pub classpath: Vec<Artifact>,
    pub args: Vec<String>,            // 含模板变量 {key}, [artifact], 'literal'
    pub outputs: HashMap<String, String>, // 文件路径 → SHA-1
}

#[derive(Debug, Deserialize)]
pub struct DataEntry {
    pub client: String,
}

/// 旧版 Forge install_profile.json
#[derive(Debug, Deserialize)]
pub struct ForgeInstallProfile {
    pub install: ForgeInstall,
    #[serde(rename = "versionInfo")]
    pub version_info: GameInstanceManifest,
}

#[derive(Debug, Deserialize)]
pub struct ForgeInstall {
    pub minecraft: String,
    pub path: Artifact,
    #[serde(rename = "filePath")]
    pub file_path: String,
}

// ===== analyzer.rs =====

/// Modloader 类型检测器 — 扫描已安装的 libraries 识别 Modloader。
pub struct LibraryAnalyzer {
    /// library_id → (Library, version_string)
    markers: HashMap<String, (Library, String)>,
    manifest: GameInstanceManifest,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LibraryType {
    Minecraft,
    Forge,
    Fabric,
    Quilt,
    NeoForge,
    LiteLoader,
    OptiFine,
    Cleanroom,
    LegacyFabric,
    // API 子类型
    FabricApi,
    QuiltApi,
    LegacyFabricApi,
}

impl LibraryType {
    /// 通过 Maven groupId + artifactId 模式匹配判断 library 类型。
    pub fn detect(library: &Library, all_libraries: &[Library]) -> Option<Self>;

    /// 从 patch_id 字符串反向查找。
    pub fn from_patch_id(id: &str) -> Option<Self>;

    /// 是否为 modloader（而非 API）。
    pub fn is_modloader(&self) -> bool;
}

impl LibraryAnalyzer {
    /// 扫描 manifest 和 patches 中的所有 library，建立类型标记。
    pub fn analyze(manifest: &GameInstanceManifest, game_version: &str) -> Self;

    /// 检查是否安装了指定的 modloader。
    pub fn has(&self, ty: LibraryType) -> bool;

    /// 获取已安装 modloader 的版本号。
    pub fn get_version(&self, ty: LibraryType) -> Option<&str>;

    /// 移除指定 modloader 的 library 和 patch。
    pub fn remove(&mut self, library_id: &str) -> &mut Self;

    /// 检测主类是否来自任何 modloader（非 vanilla）。
    pub fn is_modded(main_class: &str) -> bool;
}

// ===== dependency.rs =====

/// 统一依赖管理器。
pub struct DependencyManager {
    game_repo: Arc<GameRepository>,
    download_provider: Arc<dyn DownloadProvider>,
    cache_repo: Arc<CacheRepository>,
    client: reqwest::Client,
    /// library_id → VersionList 的映射
    version_lists: HashMap<String, Arc<dyn VersionList>>,
}

impl DependencyManager {
    /// 安装 Modloader（通过 libraryId + 版本号）。
    pub async fn install_library(
        &self,
        game_version: &str,
        base_manifest: &GameInstanceManifest,
        library_id: &str,
        library_version: &str,
    ) -> Result<GameInstanceManifest> {
        // 1. 获取 VersionList
        let vl = self.version_lists.get(library_id)
            .ok_or_else(|| anyhow!("Unknown library: {library_id}"))?;

        // 2. 加载版本列表
        vl.load(game_version).await?;

        // 3. 查找指定版本
        let rv = vl.get_version(game_version, library_version)
            .ok_or_else(|| anyhow!("Version {library_version} not found for {library_id}"))?;

        // 4. 先移除旧版本
        let cleaned = self.remove_library(base_manifest, library_id).await?;

        // 5. 创建并执行安装任务
        let mut task = rv.create_install_task(self, &cleaned)?;
        let patch = task.execute().await?;

        // 6. 叠加 patch
        Ok(cleaned.add_patch(patch))
    }

    /// 自动检测安装器文件类型并安装。
    pub async fn install_from_file(
        &self,
        manifest: &GameInstanceManifest,
        installer_path: &Path,
    ) -> Result<GameInstanceManifest> {
        // 按优先级尝试每种 modloader 的检测逻辑
        for detector in &[
            CleanroomInstaller::detect,
            NeoForgeInstaller::detect,
            ForgeInstaller::detect,
            OptiFineInstaller::detect,
        ] {
            if let Ok(task) = detector(self, manifest, installer_path) {
                let patch = task.execute().await?;
                return Ok(manifest.add_patch(patch));
            }
        }
        Err(anyhow!("Unrecognized installer file"))
    }

    /// 移除已安装的 library。
    pub async fn remove_library(
        &self,
        manifest: &GameInstanceManifest,
        library_id: &str,
    ) -> Result<GameInstanceManifest> {
        let game_version = self.game_repo.get_game_version(manifest)?;
        let mut analyzer = LibraryAnalyzer::analyze(manifest, &game_version);
        analyzer.remove(library_id);
        Ok(analyzer.build())
    }
}
```

### 3.4 安装流程对比：现有 vs 目标

#### Fabric 安装（当前实现 — 简单但脆弱）

```
GET /v2/versions/loader/{game}/{loader}/profile/json
→ 保存 JSON 到 versions/{id}/{id}.json
→ 完成
```

问题：不下载实际的 library JAR 文件，依赖启动时自动下载。

#### Fabric 安装（目标实现 — HMCL 对齐）

```
1. GET /v2/versions/loader/{game}/{loader}
   → 解析 FabricInfo { loader.maven, intermediary.maven, launcherMeta }
2. 从 launcherMeta 提取:
   - mainClass（支持 {client, server} 对象格式）
   - libraries（common + server 侧）
   - launchwrapper tweakers（如有）
3. 追加 loader + intermediary 的 Library 条目（含 Maven URL）
4. 构建 GameInstancePatch { id="fabric", priority=30000, mainClass, arguments, libraries }
5. 异步下载 libraries → 写入 libs_dir
6. manifest.addPatch(patch)
```

#### Forge 安装（目标实现 — Headless 模式）

```
1. 获取 installer URL（从 HMCL metadata 或 BMCLAPI）
2. 下载 installer JAR（带 ZIP 完整性校验 + SHA-1 缓存）
3. 打开 JAR 为只读 ZipFileSystem:
   a. 解析 install_profile.json → ForgeNewInstallProfile
   b. 检测 installer 类型:
      - "spec" 字段存在 → 新版 (ForgeNewInstallTask)
      - "install" + "versionInfo" → 旧版 (ForgeOldInstallTask)
4a. 【新版】:
   a. 从 JAR 的 maven/ 目录提取内嵌 libraries → libs_dir
   b. 下载 Processor 依赖 libraries
   c. 构建模板变量表:
      {SIDE}="client", {MINECRAFT_JAR}=<path>, {ROOT}=<game_dir>,
      {INSTALLER}=<installer_path>, {LIBRARY_DIR}=<libs_dir>
   d. 解析 data map（[artifact]/'literal'/{entry} 三种格式）
   e. 依次执行 Processor:
      - 检查 outputs 是否已存在且 SHA-1 匹配（跳过已完成步骤）
      - 构建 classpath（processor dependencies + jar）
      - 解析 args（模板替换）
      - 执行 java -cp <classpath> <mainClass> <args>
      - 验证 output 文件的 SHA-1（zlib-ng 兼容回退）
   f. 特殊处理: DOWNLOAD_MOJMAPS task → 从 version.json 获取 client_mappings URL
   g. 读取 version.json → forgeVersion manifest
   h. 生成 GameInstancePatch { id="forge", priority=30000 }
4b. 【旧版】:
   a. 解析 install.minecraft 校验版本匹配
   b. 从 JAR 中提取 install.filePath 指向的 universal jar → libs_dir
   c. 直接使用 install_profile.versionInfo 作为 patch 的元数据源
5. 下载 forgeVersion 的 libraries
6. manifest.addPatch(patch)
```

#### OptiFine 安装（目标实现）

```
1. 下载 OptiFine installer JAR
2. 检测 installer:
   - 读取 Config.class 字节码常量池
   - 提取 MC_VERSION, OF_EDITION, OF_RELEASE
   - 验证 MC 版本匹配
3. 兼容性检查:
   - 检查 mainClass 是否在 FORGE_OPTIFINE_MAIN 集合内
   - 如有 buildof.txt，检查与 BootstrapLauncher 的兼容性
4. 安装:
   a. 复制 installer → maven 路径 (optifine:OptiFine:{version})
   b. 如果 JAR 含 Patcher.class:
      执行 java -cp installer optifine.Patcher <mc_jar> <installer> <output>
   c. 否则直接复制 installer 作为 optifine library
   d. 删除 META-INF/mods.toml（避免 Forge 冲突）
   e. 提取 launchwrapper 变体:
      - launchwrapper-2.0.jar → optifine:launchwrapper:2.0
      - launchwrapper-of-{ver}.jar → optifine:launchwrapper-of:{ver}
   f. 如果无 launchwrapper，添加 net.minecraft:launchwrapper:1.12
5. 构建 GameInstancePatch:
   - id="optifine", priority=10000
   - mainClass="net.minecraft.launchwrapper.Launch"
   - gameArgs: ["--tweakClass", "optifine.OptiFineTweaker"]
   - libraries: [optiFineLib, launchwrapper, ...]
6. 下载 libraries
7. manifest.addPatch(patch)
```

### 3.5 异步流程编排

使用 `tokio` 的异步运行时，结合 `JoinSet` 实现并行下载：

```rust
impl ForgeNewInstallTask {
    async fn execute_processors(
        &self,
        processors: &[Processor],
        vars: &HashMap<String, String>,
        manifest: &GameInstanceManifest,
        game_repo: &GameRepository,
    ) -> Result<()> {
        for (i, processor) in processors.iter().enumerate() {
            if !processor.is_client_side() {
                continue;
            }

            // 检查 outputs 是否已存在且有效（幂等性）
            if self.all_outputs_valid(processor, vars)? {
                tracing::info!(processor = i, "Skipping (outputs already valid)");
                continue;
            }

            // 解析 classpath
            let classpath = processor.classpath.iter()
                .map(|a| game_repo.get_artifact_path(manifest, a))
                .collect::<Result<Vec<_>>>()?;

            // 解析 args（模板替换）
            let args = processor.args.iter()
                .map(|a| Self::parse_literal(a, vars, |s| Ok(s.to_string())))
                .collect::<Result<Vec<_>>>()?;

            // 执行 Java 进程
            let output = tokio::process::Command::new(&self.java_binary)
                .arg("-cp").arg(classpath.join(&PATH_SEPARATOR))
                .arg(&processor.main_class)
                .args(&args)
                .output()
                .await?;

            if !output.status.success() {
                return Err(anyhow!(
                    "Processor {} failed: {}",
                    i,
                    String::from_utf8_lossy(&output.stderr)
                ));
            }

            // 验证 outputs SHA-1
            self.verify_outputs(&processor.outputs, vars)?;
        }
        Ok(())
    }

    /// 模板变量解析（对齐 HMCL 的 parseLiteral）：
    /// - {key} → vars[key]
    /// - [group:name:version] → artifact 文件路径
    /// - 'text' → 字面量
    fn parse_literal(
        literal: &str,
        vars: &HashMap<String, String>,
        plain_converter: impl Fn(&str) -> Result<String>,
    ) -> Result<String> {
        if literal.starts_with('{') && literal.ends_with('}') {
            let key = &literal[1..literal.len()-1];
            vars.get(key).cloned()
                .ok_or_else(|| anyhow!("Unknown variable: {key}"))
        } else if literal.starts_with('[') && literal.ends_with(']') {
            // artifact path resolution
            let artifact = Artifact::from_descriptor(&literal[1..literal.len()-1])?;
            Ok(game_repo.get_artifact_path(manifest, &artifact)?.display().to_string())
        } else if literal.starts_with('\'') && literal.ends_with('\'') {
            Ok(literal[1..literal.len()-1].to_string())
        } else {
            plain_converter(literal)
        }
    }
}
```

### 3.6 测试策略

#### 单元测试

```rust
// 1. 数据结构序列化/反序列化
#[test]
fn test_parse_forge_new_install_profile() {
    let json = include_str!("../../testdata/forge_install_profile.json");
    let profile: ForgeNewInstallProfile = serde_json::from_str(json).unwrap();
    assert_eq!(profile.spec, 1);
    assert_eq!(profile.minecraft, "1.21");
}

// 2. LibraryAnalyzer 类型检测
#[test]
fn test_analyzer_detects_forge() {
    let lib = Library {
        name: "net.minecraftforge:forge:1.21-49.0.50".into(),
        downloads: LibraryDownloads { artifact: Some(...) },
        ..Default::default()
    };
    assert_eq!(LibraryType::detect(&lib, &[]), Some(LibraryType::Forge));
}

#[test]
fn test_analyzer_distinguishes_forge_vs_neoforge() {
    let neoforge_lib = Library { group_id: "net.neoforged.fancymodloader".into(), ... };
    let forge_lib = Library { group_id: "net.minecraftforge".into(), ... };
    // NeoForge 库存在时，forge library 不应被检测为 Forge
    assert_eq!(LibraryType::detect(&forge_lib, &[neoforge_lib]), None);
}

// 3. 模板变量解析
#[test]
fn test_parse_literal_variable() {
    let vars = HashMap::from([("SIDE".into(), "client".into())]);
    assert_eq!(parse_literal("{SIDE}", &vars, identity), Ok("client".into()));
}

#[test]
fn test_parse_literal_artifact() {
    // [net.minecraftforge:forge:1.21-49.0.50] → path
}

// 4. Patch 合并优先级
#[test]
fn test_patch_merge_priority() {
    let mut manifest = make_vanilla_manifest();
    let forge_patch = GameInstancePatch {
        priority: 30000,
        main_class: Some("cpw.mods.modlauncher.Launcher".into()),
        ..Default::default()
    };
    let optifine_patch = GameInstancePatch {
        priority: 10000,
        main_class: Some("net.minecraft.launchwrapper.Launch".into()),
        ..Default::default()
    };
    manifest = manifest.add_patch(forge_patch).add_patch(optifine_patch);
    // OptiFine 优先级更高，应覆写 mainClass
    assert_eq!(manifest.main_class, Some("net.minecraft.launchwrapper.Launch".into()));
}
```

#### 集成测试

```rust
// 5. 完整安装流程（使用 mock 服务器）
#[tokio::test]
async fn test_fabric_install_integration() {
    let mut server = mockito::Server::new_async().await;
    // Mock Fabric Meta API responses
    let dm = DependencyManager::new(
        game_repo,
        Arc::new(TestDownloadProvider::new(server.url())),
        cache_repo,
        client,
    );

    let manifest = make_vanilla_manifest("1.21");
    let result = dm.install_library("1.21", &manifest, "fabric", "0.16.0").await.unwrap();

    let analyzer = LibraryAnalyzer::analyze(&result, "1.21");
    assert!(analyzer.has(LibraryType::Fabric));
    assert_eq!(analyzer.get_version(LibraryType::Fabric), Some("0.16.0"));
}

// 6. 安装器自动检测
#[tokio::test]
async fn test_auto_detect_forge_installer() {
    let installer = create_test_jar(&[
        ("install_profile.json", r#"{"spec":1,"minecraft":"1.21",...}"#),
        ("maven/net/minecraftforge/forge/...", b"fake jar content"),
    ]);
    let manifest = make_vanilla_manifest("1.21");
    let result = dm.install_from_file(&manifest, &installer).await.unwrap();
    assert!(LibraryAnalyzer::analyze(&result, "1.21").has(LibraryType::Forge));
}

// 7. 幂等性：重复安装不报错
#[tokio::test]
async fn test_install_idempotent() { ... }

// 8. 错误场景：版本不匹配
#[tokio::test]
async fn test_version_mismatch_error() { ... }
```

---

## 四、实施优先级建议

### Phase 1：核心抽象（1-2 天）

- [x] 已有基础 `ModLoaderProvider` trait
- [ ] 重构为 `VersionList` + `RemoteVersion` + `InstallTask` 三层 trait
- [ ] 实现 `GameInstancePatch` 数据结构 + `merge_into` 逻辑
- [ ] 实现 `LibraryAnalyzer` 类型检测
- [ ] 实现 `DependencyManager` 统一入口

### Phase 2：Fabric + Quilt（最容易，1 天）

- [ ] `FabricVersionList`（从 `/v2/versions/loader` + `/game` 拉取）
- [ ] `FabricInstallTask`（解析 launchMeta，构建 patch）
- [ ] `QuiltVersionList` + `QuiltInstallTask`（同模式 + Maven 仓库路由）

### Phase 3：Forge（最复杂，2-3 天）

- [ ] `ForgeVersionList`（HMCL metadata + BMCLAPI fallback）
- [ ] `ForgeInstallProfile` / `ForgeNewInstallProfile` 数据结构
- [ ] `ForgeOldInstallTask`（universal jar 提取 + 直接 patch）
- [ ] `ForgeNewInstallTask`（Processor 执行 + 模板替换）
- [ ] `ForgeInstallTask`（路由到 new/old）

### Phase 4：NeoForge + OptiFine（1-2 天）

- [ ] `NeoForgeVersionList` + `NeoForgeInstallTask`（复用 Forge 结构）
- [ ] `OptiFineVersionList` + `OptiFineInstallTask`（Patcher + launchwrapper 提取）

### Phase 5：LiteLoader + 其他（1 天）

- [ ] `LiteLoaderVersionList` + `LiteLoaderInstallTask`
- [ ] `CleanroomVersionList` + `CleanroomInstallTask`
- [ ] `LegacyFabricVersionList` + `LegacyFabricInstallTask`

### Phase 6：高级特性（1-2 天）

- [ ] 安装器文件自动检测（拖入安装）
- [ ] 版本兼容矩阵验证
- [ ] 幂等安装（已安装则跳过）
- [ ] 完整测试套件（mock 服务器 + 测试夹具）

---

## 五、依赖关系

```
rmccc-modloader
  ├── rmccc-error        # 错误类型
  ├── rmccc-version       # Library, GameVersion, Artifact 等类型
  ├── rmccc-download      # 下载引擎（复用 ConcurrentDownloadManager）
  ├── rmccc-launch        # Java 检测（运行 Processor 需要 Java）
  ├── reqwest             # HTTP 客户端
  ├── serde / serde_json  # JSON 序列化
  ├── tokio               # 异步运行时
  ├── zip                 # JAR/ZIP 文件读取
  └── sha1 / sha2         # SHA-1 校验（或复用 rmccc-download 的实现）
```

---

## 六、与现有代码的衔接

现有的 `crates/rmccc-modloader/src/lib.rs` 中的 `ModLoaderProvider` trait 目前被 `src-cli/src/commands/modloader.rs` 和 `crates/rmccc-core/src/lib.rs` 使用。重构时应保持向后兼容的过渡期：

1. 新的 `DependencyManager` 作为主要 API 入口
2. 旧的 `ModLoaderProvider` trait 保留但标记为 `#[deprecated]`
3. CLI 和 Tauri 命令逐步迁移到新 API
4. `Profile` 结构中的 `modloader_config` 字段增加 `patches: Vec<GameInstancePatch>` 字段
