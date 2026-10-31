# RMCCC Architecture Design

> Rust Minecraft Core Client — 纯 Rust 启动核心 + Tauri 2.x + SolidJS Web UI

---

## 1. Technology Stack

| Layer | Technology | Rationale |
|-------|-----------|-----------|
| Native Shell | Tauri 2.x | System WebView, ~4-5MB stripped, 原生跨平台 |
| Core Engine | Rust (stable) | 零成本抽象, 内存安全, 跨平台编译 |
| Frontend | SolidJS + Vite | ~8KB gzipped, 高性能响应式, 编译时优化 |
| Styling | Tailwind CSS v4 | Utility-first, tree-shaking, 零运行时开销 |
| IPC | Tauri invoke + events | 类型安全, async 支持 |
| Logging | tracing + tracing-appender | Structured logging, per-span tracing |
| Type Bridge | ts-rs | Rust → TypeScript 类型自动生成 |

## 2. Workspace Structure

```
RMCL/
├── Cargo.toml                    # Workspace root
├── docs/                         # Architecture & planning docs（分类索引见 docs/README.md）
│   ├── ARCHITECTURE.md           # 设计基线（最高权威）
│   ├── AI_AND_MCP_ARCHITECTURE.md # AI 服务 + 外部 Agent 控制（ADR-013）
│   ├── AI_EDITING_RULES.md / DEVELOPMENT_PLAN.md / PROJECT_STATUS.md
│   └── adr/ audits/ guides/ roadmap/ ui-design/ archive/
├── crates/                       # Rust business logic crates
│   ├── rmccc-error/              # Unified error types (leaf)
│   │   └── src/lib.rs            # RmcccError + ErrorCode (snake_case wire form)
│   ├── rmccc-config/             # Configuration, paths, logging (leaf)
│   │   └── src/
│   │       ├── lib.rs            # AppConfig / LauncherState / UserSettings
│   │       ├── schema.rs         # ConfigSchema + SchemaStatus state machine
│   │       ├── setting_file.rs   # JsonSetting trait + SettingFile<T>
│   │       ├── file_saver.rs     # Debounced background writer (200ms)
│   │       ├── file_utils.rs     # Atomic write + backup_invalid_config
│   │       ├── paths.rs          # Platform-specific paths
│   │       └── logging.rs        # tracing init + span strategy
│   ├── rmccc-auth/               # Authentication
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── account.rs        # Account trait (log_in / play_offline)
│   │       ├── account_id.rs     # AccountID ("account:<uuid>")
│   │       ├── auth_info.rs      # AuthInfo launch value object
│   │       ├── factory.rs        # AccountFactory trait (create / from_storage)
│   │       ├── microsoft.rs      # Microsoft OAuth2 device flow
│   │       ├── offline.rs        # Offline auth (official UUID v3 / MD5)
│   │       ├── repository.rs     # AccountRepository (plaintext + encrypted split)
│   │       ├── types.rs          # AuthSession, AuthProvider trait
│   │       └── vault.rs          # AES-256-GCM token encryption
│   ├── rmccc-download/           # Download engine
│   │   └── src/
│   │       ├── lib.rs            # ConcurrentDownloadManager + retry + SHA1 cache
│   │       └── provider.rs       # DownloadProvider (Mojang / BMCLAPI)
│   ├── rmccc-version/            # Version & asset management
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── game_repository.rs         # GameRepository trait
│   │       ├── default_game_repository.rs # versions/ index + offline resolve
│   │       ├── resolved.rs                # unresolved/launch/standalone views
│   │       ├── file_layout.rs             # .minecraft layout path helpers
│   │       ├── isolation.rs               # Never / Always / Modded
│   │       ├── compatibility.rs           # Unified OS/arch/features rules
│   │       ├── completion.rs              # Completeness check / repair plan
│   │       ├── manifest.rs       # MojangVersionProvider
│   │       ├── assets.rs         # AssetIndex resolution
│   │       ├── libraries.rs      # Library resolution + classpath
│   │       ├── arguments.rs      # Template vars + argument resolution
│   │       ├── types.rs          # GameVersion, Library, etc.
│   │       └── cache.rs          # Offline version list cache
│   ├── rmccc-launch/             # Game launcher
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── launcher.rs       # Launcher trait
│   │       ├── default_launcher.rs # DefaultLauncher (20-step command line)
│   │       ├── options.rs        # LaunchOptions (builder) + ProcessPriority
│   │       ├── command.rs        # Command { args, native folders }
│   │       ├── process_listener.rs # ProcessListener + ExitType
│   │       ├── log4j.rs          # log4j2.xml extraction (RCE mitigations)
│   │       ├── java.rs           # JVM detection + version mapping
│   │       ├── natives.rs        # ExtractRules-aware natives extraction
│   │       ├── managed.rs        # ManagedProcess + stdout/stderr pumps
│   │       └── process.rs        # Response file + process spawning
│   ├── rmccc-modloader/          # Mod loader installation
│   │   └── src/
│   │       ├── lib.rs            # DependencyManager（统一安装入口，原 ModLoaderProvider 已收敛）
│   │       ├── analyzer.rs       # Loader detection from libraries
│   │       ├── patch.rs          # Shared GameInstancePatch type
│   │       ├── fabric/  forge/  neoforge/  quilt/
│   │       └── liteloader/  optifine/  cleanroom/  legacyfabric/
│   ├── rmccc-mods/               # Mod management (Modrinth implemented)
│   │   └── src/
│   │       ├── lib.rs            # ModSource trait + data types
│   │       └── modrinth.rs       # Modrinth search / versions / download
│   ├── rmccc-modpack/            # HMCL-compatible modpack support
│   │   └── src/
│   │       ├── lib.rs            # inspection, Curse resolution, modpack.cfg
│   │       ├── model.rs          # format-independent Modpack model
│   │       ├── archive.rs        # UTF-8/GBK names + path safety + bounded reads
│   │       ├── extract.rs        # overrides + HMCL update-preservation rules
│   │       └── formats/          # Curse / Modrinth / MCBBS / Server / MultiMC / HMCL
│   ├── rmccc-instance/           # Game directories + version-backed instances
│   │   └── src/                  # directory registry, settings, service, migration
│   ├── rmccc-platform/           # 平台适配层（Phase 1，指南 §5.7）
│   │   └── src/                  # descriptor / fs / process / shell / java
│   │                             # macOS 后端 + Windows/Linux 占位 + mock
│   ├── rmccc-plugin/             # 声明式插件系统（Phase 2，ADR-003/004）
│   │   └── src/                  # manifest / version / capability / package /
│   │                             # installer / registry / permissions /
│   │                             # config / manager
│   ├── rmccc-dashboard/          # Dashboard 布局与主题（Phase 3，ADR-005）
│   ├── rmccc-logic/              # 逻辑插件协议与 DTO（Phase 5，ADR-007；wasm 宿主在 src-logic-host）
│   ├── rmccc-llm/                # OpenAI 兼容 AI 客户端（ADR-013：llm 流式/探测 + credentials + redact）
│   ├── rmccc-agent/              # 依赖扫描/补装 + 审计 + 两阶段确认原语（ADR-013 瘦身后）
│   └── rmccc-core/               # Core orchestrator + DI container
│       └── src/
│           ├── lib.rs            # CoreContext, re-exports
│           ├── pipeline.rs       # install / repair / launch orchestration
│           └── modpack.rs        # modpack install orchestration (game → loader → files)
├── src-cli/                      # CLI binary crate (`rmcli`)
│   ├── Cargo.toml
│   ├── src/
│   │   ├── main.rs               # Entry point (env in, exit code out)
│   │   ├── lib.rs                # run / execute_line / dispatch
│   │   ├── app.rs                # clap command tree (single parse source)
│   │   ├── registry.rs           # help/completion metadata (sync-asserted)
│   │   ├── output.rs             # Unified result envelope + plain mode
│   │   ├── error.rs              # CliError + stable exit codes
│   │   ├── context/              # CliContext — backend API facade（按域拆分）
│   │   ├── repl.rs               # Interactive REPL
│   │   └── commands/             # auth config profile instance resource
│   │                             # modloader mod launch system
│   └── tests/                    # cli_e2e (process) + cli_integration (in-process)
├── src-tauri/                    # Tauri application layer
│   ├── Cargo.toml
│   ├── build.rs
│   ├── tauri.conf.json
│   ├── capabilities/default.json
│   └── src/
│       ├── main.rs               # Entry + generate_handler! (42 commands)
│       ├── bindings.rs           # ts-rs export of 19 types → src/ipc/types.ts
│       ├── state.rs              # AppState (Tokio-backed)
│       ├── events.rs             # Event constants + ThrottledEmitter
│       └── commands/             # IPC command handlers
│           ├── mod.rs
│           ├── auth.rs  download.rs  instance.rs  launch.rs  logs.rs
│           ├── mods.rs  modloader.rs  profile.rs  settings.rs  version.rs
│           └── external.rs
├── src/                          # Frontend (SolidJS)
│   ├── index.html
│   ├── main.tsx                  # Router: / · /login · /settings
│   ├── App.tsx                   # Legacy placeholder (unreferenced)
│   ├── routes/                   # Dashboard · Login · Settings
│   ├── components/               # TitleBar · Toast
│   ├── ipc/
│   │   ├── api.ts                # Typed invoke wrappers + safeInvoke
│   │   ├── events.ts             # Event name constants
│   │   └── types.ts              # ts-rs generated — do not edit by hand
│   └── styles/app.css
└── assets/                       # Static assets (icons)
```

## 3. Rust Core Architecture

### 3.1 Crate Dependency Graph

由各 crate 的 `Cargo.toml` 实测得出（`rmccc-error` 是唯一叶子，全图无环）：

```
rmccc-error                     # leaf — 统一错误类型 + ErrorCode
├── rmccc-config                # → error
├── rmccc-download              # → error
├── rmccc-auth                  # → error, platform
└── rmccc-mods                  # → error, download

rmccc-version    → error, download, platform
rmccc-modpack    → error, download, version
rmccc-modloader  → error, download, platform, version
rmccc-instance   → error, config, platform, version
rmccc-launch     → error, config, platform, version, instance
rmccc-core       → 以上全部（编排 + DI 容器）
src-tauri (rmccc)     → rmccc-core + 全部子 crate
src-cli   (rmccc-cli) → rmccc-core + 全部子 crate
```

`rmccc-core` 不仅是 re-export，还承担两个职责：
1. **Pipeline 编排**：将多个 crate 的操作组合成完整流程（如 "安装游戏" = version fetch → download batch → verify）
2. **DI 容器**：持有所有 trait object（`Box<dyn AuthProvider>`, `Box<dyn ModSource>` 等），提供统一的 `CoreContext` 给 Tauri 层

### 3.2 Trait System (Extensibility)

#### AuthProvider

```rust
#[async_trait]
pub trait AuthProvider: Send + Sync {
    /// Unique provider identifier
    fn provider_id(&self) -> &str;

    /// Perform authentication, may require user interaction (browser)
    async fn authenticate(&self) -> Result<AuthSession, RmcccError>;

    /// Refresh an expired session without user interaction
    async fn refresh(&self, session: &AuthSession) -> Result<AuthSession, RmcccError>;

    /// Validate whether a session is still usable
    async fn validate(&self, session: &AuthSession) -> bool;
}

// Implementations:
// - MicrosoftAuthProvider  (OAuth2 device code flow)
// - OfflineAuthProvider    (UUID generation from username)
```

#### VersionProvider

```rust
#[async_trait]
pub trait VersionProvider: Send + Sync {
    fn provider_id(&self) -> &str;

    /// Fetch available game versions (with offline fallback)
    async fn fetch_versions(&self) -> Result<Vec<VersionInfo>, RmcccError>;

    /// Fetch full version JSON for a specific version
    async fn fetch_version(&self, id: &str) -> Result<GameVersion, RmcccError>;
}

// Implementations:
// - MojangVersionProvider   (Official Mojang API, caches to disk)
// - MirrorVersionProvider   (BMCLAPI / custom mirror)
```

#### ModLoader 安装入口（as-built：`DependencyManager`）

> 审计批次 3/4 修订：原设计的 `ModLoaderProvider` trait 已被
> `rmccc_modloader::DependencyManager`（统一分流 Forge/NeoForge installer JAR
> 与其余 patch 型 loader）+ `rmccc-core::modloader::install_loader` 编排取代，
> 不存在名为 `ModLoaderProvider` 的 trait。能力面如下：

```rust
// rmccc_modloader::DependencyManager（非 trait，具体类型）
pub async fn list_versions(&self, loader_type: &ModLoaderType, game_version: &str)
    -> Result<Vec<LoaderVersion>, RmcccError>;
pub async fn install_loader(&self, loader_type: &ModLoaderType, game_version: &str,
    base_version: &GameVersion, loader_version: &str, game_dir: &Path, java_path: &Path)
    -> Result<LoaderInstallOutcome, RmcccError>;

// Implementations（内部路由）:
// - Forge / NeoForge  → 官方 installer JAR 运行（InstallerJarRunner）
// - 其余 6 种         → GameInstancePatch（增量 JSON，可回滚可叠加）
```

#### ModSource

```rust
#[async_trait]
pub trait ModSource: Send + Sync {
    fn source_id(&self) -> &str;

    /// Search mods by query and filters
    async fn search(&self, query: &ModSearchQuery) -> Result<ModSearchResult, RmcccError>;

    /// List available versions of a mod
    async fn list_versions(&self, mod_id: &str) -> Result<Vec<ModVersion>, RmcccError>;

    /// Get download URL for a specific version
    async fn get_download_url(&self, version_id: &str) -> Result<String, RmcccError>;
}

// Implementations:
// - ModrinthSource     (Modrinth v2 API, open, no key needed) — 已实现
// - CurseForgeSource   (CurseForge Core API, API key from env or key file) — planned（审计差距 T1，见 docs/audits/BACKEND_AUDIT_2026-10-25/04）
```

#### DownloadManager

```rust
#[async_trait]
pub trait DownloadManager: Send + Sync {
    /// Submit a batch of items, returns a task handle
    async fn download_batch(
        &self,
        items: Vec<DownloadItem>,
        progress: ProgressCallback,
    ) -> Result<DownloadHandle, RmcccError>;

    /// Cancel an entire download batch
    async fn cancel_batch(&self, handle: &DownloadHandle) -> Result<(), RmcccError>;

    /// Cancel a single file within a batch
    async fn cancel_file(&self, handle: &DownloadHandle, item_id: &str) -> Result<(), RmcccError>;

    /// Pause a download batch (flush in-progress, stop dequeuing)
    async fn pause(&self, handle: &DownloadHandle) -> Result<(), RmcccError>;

    /// Resume a paused download batch
    async fn resume(&self, handle: &DownloadHandle) -> Result<(), RmcccError>;
}

// Implementation:
// - ConcurrentDownloadManager (tokio-based worker pool, retry, resume, SHA1 verify)
```

### 3.3 Key Data Models

```rust
// ADR-016: an instance is a version directory. There is no Profile UUID.
// The on-disk manifest is the identity; `InstanceSettings` is an overlay.
pub struct InstanceKey {
    pub directory: GameDirectoryId, // registered game root
    pub id: InstanceId,             // versions/{id}/{id}.json
}

pub struct InstanceSettings {       // {instance_root}/.rmccc/instance.json
    pub display_name: Option<String>,
    pub icon: InstanceIcon,
    pub isolation: Option<IsolationPolicy>,
    pub override_properties: BTreeSet<String>,
    // Java / JVM / memory / window / process / graphics / env / wrapper /
    // quick-play / native / diagnostics — full HMCL GameSettings set.
}

pub struct InstanceSummary {        // list row, no completeness scan
    pub key: String,
    pub game_version: String,        // base MC (root of inheritance chain)
    pub loader: Option<LoaderInfo>,
    pub path: PathBuf,               // versions/{id}
    pub run_dir: PathBuf,            // isolation-aware
    pub last_played: Option<DateTime<Utc>>,
}

pub struct ModLoaderConfig {
    pub loader_type: ModLoaderType,  // Forge | Fabric | NeoForge
    pub loader_version: String,
}

/// Authentication session (persisted, tokens encrypted at rest)
pub struct AuthSession {
    pub provider: String,            // "microsoft" | "offline"
    pub access_token: String,        // in-memory plaintext, encrypted on disk
    pub username: String,
    pub uuid: String,
    pub refresh_token: Option<String>,
    pub expires_at: Option<DateTime<Utc>>,
}

/// Game version metadata (parsed from Mojang manifest)
pub struct GameVersion {
    pub id: String,
    pub release_type: ReleaseType,   // Release | Snapshot | OldBeta | OldAlpha
    pub main_class: String,
    pub asset_index: AssetIndex,
    pub downloads: HashMap<String, DownloadEntry>,  // client, server, etc.
    pub libraries: Vec<Library>,
    pub arguments: GameArguments,
    pub java_version: Option<JavaRequirement>,
}

/// MC version → minimum Java version mapping
pub struct JavaRequirement {
    pub major_version: u32,          // e.g. 8, 16, 17, 21
    pub component: String,           // e.g. "java-runtime-gamma"
}

// Well-known mapping (used as fallback when manifest lacks info):
// 1.6 – 1.16  → Java 8
// 1.17        → Java 16
// 1.18 – 1.20.4 → Java 17
// 1.20.5+     → Java 21

#### 版本继承合并与规则求值的统一语义（2026-10-25 审计批次 3 定版）

`inheritsFrom` 合并与 OS/features 规则求值各只有**一份权威实现**；历史上曾出现三套
语义互斥的合并器与三套规则求值器（安装链/启动链/本地仓库对同一份 JSON 解析结果不同，
见 `docs/audits/BACKEND_AUDIT_2026-10-25/01_CRATES_AUDIT.md` 跨 crate #1/#2），
现收敛如下，**禁止再新增第三套**：

**合并（`rmccc_version::merge`，HMCL `GameInstanceManifest.merge` 语义）**

- 权威函数：`merge::merge_child_into_parent(parent, child)`（完整子版本）与
  `merge::merge_partial_json_into_parent(parent, json)`（缺 `assetIndex` 等
  必需字段、typed 解析失败的部分子 JSON，如安装器产物）。
- 逐字段语义：
  - `libraries`：父在前、子追加，按 `name` 去重；
  - `arguments`：**追加**（父的 jvm/game 先，子的后接——HMCL addArguments），
    不是整体替换；
  - `minecraftArguments`：子覆盖；
  - `mainClass`：子定义非默认值（≠ `net.minecraft.client.main.Main`）时覆盖；
  - `jar` / `assetIndex` / `javaVersion` / `logging`：子有则覆盖；
  - `downloads`：子的 `client` 条目覆盖（保持 HMCL 行为）；
  - `compatibilityRules`：累计。
- 链式解析（`repository::resolve_version` 网络链 / `DefaultGameRepository::resolve`
  本地链 / `pipeline::resolve_version` 本地 JSON 分支）全部折叠到上述函数；
  三条链路对同一份 JSON 的解析结果必须一致（由回归测试锁定）。
- 可完整 typed 解析的子 JSON 若声明 `inheritsFrom`，同样必须走合并（不得直接返回）。

**规则求值（`rmccc_version::compatibility::rules_allow`，HMCL `CompatibilityRule`
.appliesToCurrentEnvironment 语义）**

- 无规则 → 适用（默认放行）；
- 有规则：**最后一条命中的规则**决定结果（`allow` → 适用，`disallow` → 不适用）；
- 有规则但**一条都没命中 → 不适用**（例：`--XstartOnFirstThread` 规则
  `[allow osx]` 在 Windows 上不出现；库规则 `[allow windows]` 在 macOS 上不下载）。
- `libraries::library_applies` 与 `arguments::rules_match` 都是对该函数的薄封装，
  不得各自实现匹配逻辑。
- `compatibility::applies_to_environment`（清单可见性过滤）语义不同且保留：
  任一规则命中即视为当前平台可见（用于索引里隐藏不支持平台的快照）。

/// Mod metadata
pub struct ModInfo {
    pub id: String,
    pub source: String,              // "modrinth" | "curseforge"
    pub name: String,
    pub slug: String,
    pub authors: Vec<String>,        // multiple authors supported
    pub description: String,
    pub download_count: u64,
    pub icon_url: Option<String>,
    pub categories: Vec<String>,
    pub versions: Vec<ModVersionInfo>,
    pub dependencies: Vec<ModDependency>,  // required/optional deps
}

/// Mod dependency (e.g. Sodium requires Fabric API)
pub struct ModDependency {
    pub mod_id: String,
    pub version_constraint: Option<String>,  // semver range if applicable
    pub dependency_type: DependencyType,     // Required | Optional | Incompatible
    pub source: String,              // same source as parent mod
}

/// Modpack placeholder (for future implementation)
pub struct Modpack {
    pub id: String,
    pub name: String,
    pub version: String,
    pub source: String,              // "modrinth" | "curseforge"
    pub game_version: String,
    pub mod_loader: ModLoaderConfig,
    pub mods: Vec<ModpackEntry>,     // ordered list of mods with versions
    pub overrides: Vec<PathBuf>,     // config files to copy
}

pub struct ModpackEntry {
    pub mod_id: String,
    pub version_id: String,
    pub required: bool,
}
```

### 3.4 Settings Model: Global Defaults + Per-Instance Overrides

设置分三层，对齐 HMCL 的"全局 / 实例"分离；**启动时只消费合并后的生效值**。

| 层 | 类型 | 存储 | 作用 |
|----|------|------|------|
| 全局预设 | `rmccc_instance::GlobalGameSettings` | `config/game-settings.json` | 所有可继承字段的回退值（单默认预设，未来可扩展多预设） |
| 实例值 + 覆盖名单 | `rmccc_instance::InstanceSettings` | `{instance_root}/.rmccc/instance.json` | `override_properties` 列出真正采用实例值的字段；其余字段继承全局 |
| 启动生效值 | `rmccc_instance::EffectiveGameSettings` | 不落盘 | `InstanceSettings::resolve(&GlobalGameSettings)` 的产物，`LaunchOptions::from_instance` 的唯一输入 |

**关键取舍（照抄 HMCL `overrideProperties`）**：字段的"值"与"是否覆盖"分开存。
`override_properties` 不含某字段时该字段继承全局，但实例里已填的值**仍然保留在磁盘上**，
因此用户在实例设置页来回切换"继承/独立"不会丢掉已输入的内容。
`rmccc_instance::fields::ALL` 用常量列出全部可覆盖字段名，供后端校验、UI 与测试共用，
避免三处各写一份字符串。

> 与 HMCL 对齐：实例设置放在 `{instance_root}/.rmccc/instance.json`（HMCL 是 `.hmcl/config/instance-game-settings.json`），
> 随 `versions/{id}` 复制/重命名/删除自然移动。全局侧目前是单默认预设，`parent` 字段预留给多预设扩展。

**隔离（"独立实例文件夹"）**：`isolation` 取 `never` / `modded` / `always`（默认 `modded`，同 HMCL）。
它决定**运行目录**，而共享游戏根不变：

| 目录 | 内容 | 何时不同 |
|------|------|----------|
| `game_dir`（共享游戏根） | `versions/`、`libraries/`、`assets/`、natives、实例根 | 始终共享 |
| `run_dir`（运行目录） | `--gameDir`、进程工作目录、`${game_directory}`、`INST_MC_DIR`、`mods/`、`saves/`、`options.txt` | 实例被隔离时 = `versions/{id}` |

`rmccc_launch::instance_run_dir` / `instance_mods_dir` 是唯一的推导入口，
`LaunchOptions::from_instance` 与模组命令都经它，避免"安装目录与启动目录不一致"这类事故重现。
详见 §15.1 的目录布局。

### 3.5 Rust Dependencies (Size-Optimized)

```toml
# Core dependencies (chosen for size)
serde = { version = "1", features = ["derive"] }
serde_json = "1"
tokio = { version = "1", features = ["rt-multi-thread", "macros", "fs", "process", "sync"] }
reqwest = { version = "0.12", default-features = false, features = ["rustls-tls", "json", "stream"] }
sha1 = "0.10"
sha2 = "0.10"
zip = { version = "2", default-features = false, features = ["deflate"] }
uuid = { version = "1", features = ["v4"] }
chrono = { version = "0.4", features = ["serde"] }
thiserror = "2"
anyhow = "1"
async-trait = "0.1"
dirs = "6"
url = "2"

# Concurrency & state
dashmap = "6"                 # Lock-free concurrent HashMap (AppState)
tokio-util = { version = "0.7", features = ["rt"] }  # CancellationToken

# System
fs2 = "0.4"                   # free_space() for disk pre-check

# Auth specific
oauth2 = "5"
urlencoding = "2"
aes-gcm = "0.10"              # AES-256-GCM token encryption (~30KB)
rand = "0.8"                  # Key generation

# Logging
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter", "json"] }
tracing-appender = "0.2"

# Type bridge (dev dependency, generates TS types at build time)
ts-rs = { version = "10", features = ["serde-json"] }

# Tauri app specific
tauri = { version = "2", features = ["tray-icon"] }
tauri-build = { version = "2", features = [] }
tauri-plugin-updater = "2"          # Auto-update
tauri-plugin-process = "2"          # Process utilities
tauri-plugin-window-state = "2"     # Persist window size/position
tauri-plugin-deep-link = "2"        # rmccc:// protocol handler
```

**Size-critical choices:**

- `reqwest` with `rustls-tls` instead of `native-tls`: avoids OpenSSL/SSPI linking, slightly smaller
- `zip` with only `deflate`: skip bzip2/zstd features
- `aes-gcm` for token encryption: ~30KB, no system keychain dependency
- No `regex` in core (use `str::matches` or simple parsers)
- Avoid `clap` in GUI mode (CLI can be an optional feature)
- `reqwest` configured with `pool_max_idle_per_host: 32` for high-concurrency same-domain downloads (MC asset servers have thousands of files)

## 4. Security Architecture

### 4.1 Token Encryption (AES-256-GCM)

Token 存储采用轻量级本地加密方案，不依赖系统密钥链。账户数据分为**明文 metadata** 与**加密 private data** 两部分（对齐 HMCL `Accounts`）：

```
{data_dir}/
├── .key                          # 32-byte random AES key (Unix 600, Windows icacls)
├── accounts.json                 # Plaintext metadata list + selected AccountID
└── private/
    └── account-private-data.json # AES-256-GCM payload, keyed by AccountID
```

**加密流程：**

`TokenVault` 已收敛为**纯加密原语**（`encrypt` / `decrypt` + envelope 版本字段，为未来算法迁移留路径）；
账户的读写、工厂注册与 selected 指针由 `AccountRepository` 承担。

```rust
// rmccc-auth/src/vault.rs
use aes_gcm::{Aes256Gcm, KeyInit, Nonce};
use aes_gcm::aead::Aead;

pub struct TokenVault {
    cipher: Aes256Gcm,
}

impl TokenVault {
    /// Load or create encryption key
    pub fn init(key_path: &Path) -> Result<Self> {
        let key_bytes = if key_path.exists() {
            fs::read(key_path)?
        } else {
            let key = rand::random::<[u8; 32]>();
            fs::write(key_path, &key)?;
            // Set file permissions to owner-only (Unix)
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                fs::set_permissions(key_path, PermissionsExt::from_mode(0o600))?;
            }
            key.to_vec()
        };
        let cipher = Aes256Gcm::new_from_slice(&key_bytes)?;
        Ok(Self { cipher })
    }

    /// Encrypt an arbitrary payload → versioned envelope JSON string
    pub fn encrypt(&self, plaintext: &[u8]) -> Result<String> {
        let nonce = rand::random::<[u8; 12]>();
        let ciphertext = self.cipher.encrypt(Nonce::from_slice(&nonce), plaintext)?;
        // Store as: { "v": 1, "nonce": base64, "data": base64 }
        Ok(serde_json::to_string(&EncryptedEnvelope { v: 1, nonce, ciphertext })?)
    }

    /// Decrypt a versioned envelope JSON string → plaintext
    pub fn decrypt(&self, payload: &str) -> Result<Vec<u8>> {
        let EncryptedEnvelope { nonce, ciphertext, .. } = serde_json::from_str(payload)?;
        Ok(self.cipher.decrypt(Nonce::from_slice(&nonce), &ciphertext)?)
    }
}
```

**安全边界说明：**
- 防御目标：防止其他非特权用户、恶意脚本读取 token
- 已知局限：本机物理访问者可读 `.key` 文件（桌面应用场景可接受）
- `.key` 丢失则 `private/account-private-data.json` 不可恢复（用户需重新登录）

### 4.2 API Key 管理

敏感 API Key（如 CurseForge）不入 `config/launcher-settings.json`：

```rust
// Priority order for API keys:
// 1. Environment variable (RMCCC_CURSEFORGE_KEY=xxx)
// 2. .key file in data_dir (separate from token key)
// 3. Prompt user at runtime (store in .key file)
pub fn get_api_key(name: &str, data_dir: &Path) -> Result<Option<String>> {
    // Check env first
    if let Ok(key) = std::env::var(format!("RMCCC_{}", name.to_uppercase())) {
        return Ok(Some(key));
    }
    // Then check key file
    let key_path = data_dir.join(format!(".api_keys/{}", name));
    if key_path.exists() {
        return Ok(Some(fs::read_to_string(key_path)?.trim().to_string()));
    }
    Ok(None)
}
```

### 4.3 Tauri Capability Model

最小化 IPC 暴露面，每个 capability 明确声明允许的命令：

```json
// capabilities/default.json
{
  "identifier": "default",
  "description": "Default capabilities for the main window",
  "windows": ["main"],
  "permissions": [
    "core:window:default",
    "core:window:allow-set-focus",
    "core:window:allow-minimize",
    "core:window:allow-close",
    "core:event:default",
    "core:event:allow-listen",
    "core:event:allow-emit"
  ]
}
```

IPC commands通过 Tauri 的 `#[tauri::command]` 宏注册，天然只暴露在主窗口 context 中。不允许 `fs:default`、`shell:default` 等通用权限——所有文件/网络操作都在 Rust 侧完成。

**Note**: P2 阶段引入 system tray、deep link、updater 时，需在 `capabilities/` 中添加对应的 plugin permission 声明（如 `updater:default`、`deep-link:default`）。

### 4.4 Process Argument Security

`--accessToken` 在进程列表中明文可见是一个已知风险（`ps aux` 可查看）。解决方案：

```rust
// rmccc-launch/src/process.rs
//
// Instead of: java ... --accessToken REAL_TOKEN
// Use a response file: java @/tmp/rmccc-launch-args-XXXX

pub fn build_response_file(args: &[String], game_dir: &Path) -> Result<PathBuf> {
    let path = game_dir.join(format!(".launch-args-{}", uuid::Uuid::new_v4()));
    // Write one arg per line
    fs::write(&path, args.join("\n"))?;
    // Restrict permissions
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&path, PermissionsExt::from_mode(0o600))?;
    }
    Ok(path)
}

// Spawn with: java @response_file_path
//
// Deletion timing matters: the JVM reads the @argfile while parsing its command
// line, i.e. AFTER spawn() returns. Deleting it at spawn time races that read
// and the JVM dies with "could not open .../.launch-args-<uuid>". Schedule the
// removal (detached task) once the JVM has certainly consumed it, and sweep
// stale files (older than the grace period) at the next launch.
```

## 5. Error Architecture

### 5.1 Unified Error Enum

```rust
// rmccc-core/src/error.rs
use thiserror::Error;

#[derive(Debug, Error)]
pub enum RmcccError {
    // ── Network ──
    #[error("Network request failed: {0}")]
    Network(String),
    #[error("HTTP {status}: {url}")]
    HttpStatus { status: u16, url: String },
    #[error("Connection timeout after {0}s")]
    Timeout(u64),

    // ── Auth ──
    #[error("Authentication failed: {0}")]
    AuthFailed(String),
    #[error("Token expired and refresh failed")]
    TokenExpired,
    #[error("Device code auth pending: visit {uri} and enter code {user_code}")]
    DeviceCodePending { uri: String, user_code: String },

    // ── Download ──
    #[error("Download failed: {url} — {reason}")]
    DownloadFailed { url: String, reason: String },
    #[error("SHA1 mismatch for {path}: expected {expected}, got {actual}")]
    HashMismatch { path: String, expected: String, actual: String },
    #[error("Insufficient disk space: need {needed} bytes, have {available}")]
    InsufficientDiskSpace { needed: u64, available: u64 },

    // ── Java ──
    #[error("Java not found")]
    JavaNotFound,
    #[error("Java version {found} too old, need {required}+")]
    JavaVersionMismatch { found: u32, required: u32 },

    // ── Launch ──
    #[error("Game process exited with code {0}")]
    GameCrashed(i32),
    #[error("Profile not found: {0}")]
    ProfileNotFound(String),

    // ── Mod ──
    #[error("Mod dependency missing: {0}")]
    MissingDependency(String),
    #[error("Mod version conflict: {mod_name} requires {constraint}")]
    ModVersionConflict { mod_name: String, constraint: String },

    // ── IO / System ──
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("JSON parse error: {0}")]
    Json(#[from] serde_json::Error),

    // ── Generic ──
    #[error("{0}")]
    Other(String),
}

impl RmcccError {
    /// Whether this error is worth retrying
    pub fn is_retryable(&self) -> bool {
        matches!(self,
            RmcccError::Network(_) |
            RmcccError::Timeout(_) |
            RmcccError::HttpStatus { status, .. } if *status >= 500
        )
    }
}
```

### 5.2 IPC Error Bridge

所有 IPC command 返回 `Result<T, IpcError>`，前端根据 `code` + `retryable` 决定 UI 行为：

```rust
// src-tauri/src/ipc_error.rs
use serde::Serialize;
use ts_rs::TS;

#[derive(Debug, Serialize, TS)]
#[ts(export)]
pub struct IpcError {
    pub code: ErrorCode,
    pub message: String,
    pub retryable: bool,
}

#[derive(Debug, Serialize, TS)]
#[ts(export)]
#[serde(rename_all = "snake_case")]
pub enum ErrorCode {
    NetworkError,
    AuthFailed,
    TokenExpired,
    DownloadFailed,
    HashMismatch,
    InsufficientDiskSpace,
    JavaNotFound,
    JavaVersionMismatch,
    GameCrashed,
    ProfileNotFound,
    MissingDependency,
    ModVersionConflict,
    IoError,
    Unknown,
}

impl From<RmcccError> for IpcError {
    fn from(err: RmcccError) -> Self {
        let retryable = err.is_retryable();
        let code = match &err {
            RmcccError::Network(_) | RmcccError::Timeout(_) | RmcccError::HttpStatus { .. } =>
                ErrorCode::NetworkError,
            RmcccError::AuthFailed(_) => ErrorCode::AuthFailed,
            RmcccError::TokenExpired => ErrorCode::TokenExpired,
            RmcccError::DownloadFailed { .. } => ErrorCode::DownloadFailed,
            RmcccError::HashMismatch { .. } => ErrorCode::HashMismatch,
            RmcccError::InsufficientDiskSpace { .. } => ErrorCode::InsufficientDiskSpace,
            RmcccError::JavaNotFound => ErrorCode::JavaNotFound,
            RmcccError::JavaVersionMismatch { .. } => ErrorCode::JavaVersionMismatch,
            RmcccError::GameCrashed(_) => ErrorCode::GameCrashed,
            RmcccError::ProfileNotFound(_) => ErrorCode::ProfileNotFound,
            RmcccError::MissingDependency(_) => ErrorCode::MissingDependency,
            RmcccError::ModVersionConflict { .. } => ErrorCode::ModVersionConflict,
            RmcccError::Io(_) => ErrorCode::IoError,
            RmcccError::Json(_) => ErrorCode::IoError,        // parse errors → IO category
            RmcccError::DeviceCodePending { .. } => ErrorCode::AuthFailed,  // informational, not a true error
            _ => ErrorCode::Unknown,
        };
        IpcError {
            code,
            message: err.to_string(),
            retryable,
        }
    }
}
```

**前端错误处理模式：**

```typescript
// hooks/useIpc.ts
import type { IpcError } from "../ipc/types";

export async function safeInvoke<T>(
  cmd: string,
  args?: Record<string, unknown>
): Promise<{ ok: true; data: T } | { ok: false; error: IpcError }> {
  try {
    const data = await invoke<T>(cmd, args);
    return { ok: true, data };
  } catch (e) {
    return { ok: false, error: e as IpcError };
  }
}

// Frontend decides UI based on error.code + error.retryable:
// - retryable=true → show "Retry" button
// - code=JavaNotFound → show "Install Java" link
// - code=TokenExpired → auto-refresh or re-login prompt
```

## 6. Logging Architecture

### 6.1 Tracing Setup (Phase 1)

Logging 前置到 Phase 1，不等 Phase 5：

```rust
// rmccc-config/src/logging.rs (or rmccc-core)
use tracing_subscriber::{fmt, EnvFilter, prelude::*};
use tracing_appender::rolling;

pub fn init_logging(log_dir: &Path) -> Result<()> {
    let file_appender = rolling::daily(log_dir, "rmccc");
    let (non_blocking, _guard) = tracing_appender::non_blocking(file_appender);

    tracing_subscriber::registry()
        // Console: colored, INFO level
        .with(fmt::layer().with_target(false))
        // File: JSON structured, DEBUG level
        .with(fmt::layer().json().with_writer(non_blocking))
        // Filter: RUST_LOG env var or default INFO
        .with(EnvFilter::from_default_env().add_directive("rmccc=info".parse()?))
        .init();

    Ok(())
}
```

### 6.2 Span Strategy

```rust
// Every IPC command gets a span
#[tauri::command]
#[tracing::instrument(skip(state), fields(cmd = "version_install"))]
async fn version_install(
    state: tauri::State<'_, AppState>,
    version: String,
    profile_id: String,
) -> Result<(), IpcError> { ... }

// Every download batch gets a span
#[tracing::instrument(skip(items, progress), fields(batch_size = items.len()))]
async fn download_batch(
    &self,
    items: Vec<DownloadItem>,
    progress: ProgressCallback,
) -> Result<DownloadHandle, RmcccError> { ... }
```

## 7. Resilience & Robustness

### 7.1 Download Retry

```rust
// rmccc-download/src/lib.rs — retry policy of ConcurrentDownloadManager
pub struct RetryConfig {
    pub max_retries: u32,        // default: 3
    pub base_delay: Duration,    // default: 1s
    pub max_delay: Duration,     // default: 30s
}

impl RetryConfig {
    /// Exponential backoff: 1s → 2s → 4s (capped at max_delay)
    pub fn delay_for_attempt(&self, attempt: u32) -> Duration {
        let delay = self.base_delay * 2u32.pow(attempt);
        delay.min(self.max_delay)
    }
}
```

### 7.2 Download Resume (HTTP Range)

```rust
// rmccc-download/src/lib.rs — Range resume inside ConcurrentDownloadManager
/// Check existing partial file, request with Range header
pub async fn resume_download(
    client: &reqwest::Client,
    url: &str,
    dest: &Path,
) -> Result<reqwest::RequestBuilder, RmcccError> {
    let existing_size = if dest.exists() {
        fs::metadata(dest)?.len()
    } else {
        0
    };

    let mut builder = client.get(url);
    if existing_size > 0 {
        builder = builder.header("Range", format!("bytes={}-", existing_size));
    }
    Ok(builder)
}
// Server returns 206 Partial Content → append to existing file
// Server returns 200 OK → server doesn't support Range, re-download from start
```

### 7.3 Disk Space Pre-Check

```rust
// Before starting any download batch:
pub fn check_disk_space(dest: &Path, required_bytes: u64) -> Result<(), RmcccError> {
    let available = fs2::free_space(dest)?;
    if available < required_bytes {
        return Err(RmcccError::InsufficientDiskSpace {
            needed: required_bytes,
            available,
        });
    }
    Ok(())
}
```

### 7.4 Offline Fallback Cache

```rust
// rmccc-version/src/cache.rs
/// Cache version list locally, serve stale on network failure
pub struct VersionCache {
    cache_dir: PathBuf,
    ttl: Duration,  // default: 1 hour
}

impl VersionCache {
    pub async fn fetch_versions(&self, provider: &dyn VersionProvider) -> Result<Vec<VersionInfo>> {
        match provider.fetch_versions().await {
            Ok(versions) => {
                // Update cache
                self.write_cache(&versions)?;
                Ok(versions)
            }
            Err(e) if e.is_retryable() => {
                // Network failure → try stale cache
                if let Some(cached) = self.read_cache()? {
                    tracing::warn!("Using stale version cache: {}", e);
                    Ok(cached)
                } else {
                    Err(e) // No cache available either
                }
            }
            Err(e) => Err(e), // Non-retryable error, don't use cache
        }
    }
}
```

### 7.5 Event Throttling

```rust
// src-tauri/src/events.rs
/// Throttle download progress events to avoid flooding frontend
pub struct ThrottledEmitter {
    app: AppHandle,
    last_emit: Arc<Mutex<Instant>>,
    throttle_interval: Duration,  // default: 100ms
}

impl ThrottledEmitter {
    pub fn emit_progress(&self, payload: DownloadProgress) {
        let mut last = self.last_emit.lock().unwrap();
        if last.elapsed() >= self.throttle_interval {
            let _ = self.app.emit(events::DOWNLOAD_PROGRESS, &payload);
            *last = Instant::now();
        }
        // Always emit 100% completion
        if payload.progress >= 1.0 {
            let _ = self.app.emit(events::DOWNLOAD_PROGRESS, &payload);
        }
    }
}
```

## 8. Concurrency & State Management

### 8.1 AppState Structure

```rust
// src-tauri/src/state.rs
use std::sync::Arc;
use tokio::sync::RwLock;
use dashmap::DashMap;

pub struct AppState {
    /// Core orchestrator (trait registry + pipelines)
    pub core: Arc<RwLock<rmccc_core::CoreContext>>,

    /// Shared HTTP client (connection pooling)
    pub http_client: reqwest::Client,

    /// Active download tasks (handle_id → task state)
    pub downloads: Arc<DashMap<String, DownloadTaskState>>,

    /// Running game processes (instance key → child process)
    pub running_games: Arc<DashMap<String, tokio::process::Child>>,

    /// Token vault for encryption/decryption
    pub vault: Arc<rmccc_auth::TokenVault>,
}

pub struct DownloadTaskState {
    pub status: DownloadStatus,        // Running | Paused | Completed | Failed | Cancelled
    pub total_bytes: u64,
    pub downloaded_bytes: u64,
    pub cancel_token: tokio_util::sync::CancellationToken,
}

impl AppState {
    pub fn new(data_dir: &Path) -> Result<Self> {
        let http_client = reqwest::Client::builder()
            .pool_max_idle_per_host(32)  // MC asset servers: thousands of files on same host
            .connect_timeout(Duration::from_secs(10))
            .build()?;

        Ok(Self {
            core: Arc::new(RwLock::new(rmccc_core::CoreContext::new(data_dir)?)),
            http_client,
            tasks: Arc::new(rmccc_tasks::TaskRegistry::new()),
            running_games: Arc::new(DashMap::new()),
            vault: Arc::new(rmccc_auth::TokenVault::init(&data_dir.join(".key"))?),
        })
    }
}
```

### 8.2 Multi-Instance Policy

- **允许**：同一时间运行多个 MC 实例（不同 `directory/id` key）
- **限制**：同一实例 key 不可同时运行两个进程（`running_games` map 检查）
- **内存预检**：启动前估算所需内存，超出系统可用内存时警告

### 8.3 Unified Task Control

Download batches are an implementation detail inside a task (ADR-019). The
control surface is task-scoped; there is no per-file cancel command.

| IPC command | Scope | Behavior |
|-------------|-------|----------|
| `task_list` | 全部 | 活动任务 + 最近完成（刷新恢复） |
| `task_pause` / `task_resume` | 单任务 | 调任务的 pause action（当前下载批次） |
| `task_cancel` | 单任务 | 置协作取消标志 + 取消当前批次；终态由 worker 落定 |

## 9. Frontend Architecture

### 9.1 Pages & Routing

当前实现（`src/main.tsx`）：

```
/              → Dashboard（布局网格：账户/实例/版本 Widget + 插件卡片；启动/停止/日志）
/login         → Login (Microsoft device code / offline)
/settings      → Settings (launcher settings, language, theme, Java detection, global defaults)
/instances/:id → InstanceSettings (per-instance overrides, isolation, Java, args, loader, mods)
/mods          → ModBrowser (Modrinth search / versions / install)
/plugins       → Plugins (declarative plugin manager, Phase 2)
```

### 9.2 State Management

```typescript
// SolidJS stores — one store per domain
// stores/auth.ts
export function createAuthStore() {
  const [session, setSession] = createStore<AuthSession | null>(null);
  const [loading, setLoading] = createSignal(false);

  const login = async () => { /* invoke IPC */ };
  const logout = async () => { /* invoke IPC */ };
  const refresh = async () => { /* invoke IPC */ };

  return { session, loading, login, logout, refresh };
}

// stores/tasks.ts — the only task store (ADR-019)
export const tasksStore = createRoot(() => {
  const [active, setActive] = createStore<Record<string, TaskSnapshot>>({});
  const [recent, setRecent] = createSignal<TaskSnapshot[]>([]);

  // One snapshot per task; terminal snapshots move into `recent`.
  listen<TaskSnapshot>("task://updated", (event) => apply(event.payload));
  // Lag recovery / reload: replace the whole board.
  listen<TaskBoard>("task://sync", (event) => applyBoard(event.payload));
  void tasks.list().then((res) => res.ok && applyBoard(res.data));

  return { active, recent, activeList, aggregate, cancel, pause, resume };
});
```

### 9.3 IPC Type Bridge (Auto-Generated Types + Manual API Wrapper)

```typescript
// ipc/types.ts — AUTO-GENERATED by ts-rs, do not edit manually
// Rust structs with #[derive(TS)] produce matching TypeScript interfaces:
// interface InstanceDetail { summary: InstanceSummary; settings: InstanceSettings; ... }
// interface IpcError { code: ErrorCode; message: string; retryable: boolean; }

// ipc/api.ts — MANUAL API wrapper, imports types from types.ts
import { invoke } from "@tauri-apps/api/core";
import type { InstanceDetail, GameDirectory, VersionInfo, ... } from "./types";

// Auth
export const auth = {
  microsoftLogin: () => invoke<AuthSession>("auth_microsoft_login"),
  offlineLogin: (username: string) => invoke<AuthSession>("auth_offline_login", { username }),
  logout: () => invoke<void>("auth_logout"),
  getSession: () => invoke<AuthSession | null>("auth_get_session"),
};

// Game directories + instances (ADR-016). Every instance command takes a
// composite `{directory_id}/{instance_id}` key.
export const gameDirectories = {
  list: () => invoke<GameDirectory[]>("game_directory_list"),
  overview: () => invoke<GameDirectoryOverview[]>("game_directory_overview"),
  selected: () => invoke<GameDirectory>("game_directory_selected"),
  scan: () => invoke<DirectoryCandidate[]>("game_directory_scan"),
  reveal: (id: string) => invoke<void>("game_directory_reveal", { id }),
  add: (path: string, name?: string) => invoke<GameDirectory>("game_directory_add", { path, name }),
  update: (id: string, name?: string, path?: string) =>
    invoke<GameDirectory>("game_directory_update", { id, name, path }),
  select: (id: string) => invoke<GameDirectory>("game_directory_select", { id }),
  remove: (id: string) => invoke<void>("game_directory_remove", { id }),
};

export const instances = {
  list: () => invoke<InstanceDetail[]>("instance_list"),
  get: (key: string) => invoke<InstanceDetail>("instance_get", { key }),
  install: (directoryId: string, mcVersion: string, instanceId?: string) =>
    invoke<void>("instance_install", { directoryId, mcVersion, instanceId }),
  updateSettings: (key: string, opts: InstanceSettingsPatch) =>
    invoke<InstanceDetail>("instance_update_settings", { key, opts }),
  paths: (key: string) => invoke<InstancePaths>("instance_paths", { key }),
  resolvedSettings: (key: string) => invoke<ResolvedInstanceSettings>("instance_resolved_settings", { key }),
  rename: (key: string, newId: string) => invoke<InstanceDetail>("instance_rename", { key, newId }),
  duplicate: (key: string, newId: string, copySaves: boolean) =>
    invoke<InstanceDetail>("instance_duplicate", { key, newId, copySaves }),
  remove: (key: string, deleteFiles: boolean) => invoke<void>("instance_remove", { key, deleteFiles }),
  repair: (key: string) => invoke<number>("instance_repair", { key }),
};

// Versions (remote catalogue only; installation is instance_install)
export const versions = {
  list: () => invoke<VersionInfo[]>("version_list"),
};

// Mod Loaders (unified): install creates a new version-backed instance.
export const modloaders = {
  list: (loaderType: string, gameVersion: string) =>
    invoke<LoaderVersion[]>("modloader_list", { loaderType, gameVersion }),
  install: (key: string, loaderType: string, loaderVersion: string) =>
    invoke<string>("modloader_install", { key, loaderType, loaderVersion }),
};

// Mods (key = directory/instance)
export const mods = {
  search: (query: string, source: string) => invoke<ModSearchResult>("mod_search", { query, source }),
  install: (key: string, version: ModVersion) => invoke<string>("mod_install", { key, version }),
  remove: (key: string, modFilename: string) => invoke<void>("mod_remove", { key, modFilename }),
  list: (key: string) => invoke<ModList>("mod_list", { key }),
  toggle: (key: string, modFilename: string, enabled: boolean) =>
    invoke<void>("mod_toggle", { key, modFilename, enabled }),
};

// Launch
export const launch = {
  start: (key: string) => invoke<void>("launch_start", { key }),
  getStatus: () => invoke<string[]>("launch_status"),
  kill: (key: string) => invoke<void>("launch_kill", { key }),
};

// Unified tasks (all task-scoped, ADR-019)
export const tasks = {
  list: () => invoke<TaskBoard>("task_list"),
  log: (taskId: string) => invoke<string[]>("task_log", { taskId }),
  pause: (taskId: string) => invoke<void>("task_pause", { taskId }),
  resume: (taskId: string) => invoke<void>("task_resume", { taskId }),
  cancel: (taskId: string) => invoke<void>("task_cancel", { taskId }),
  clearHistory: () => invoke<void>("task_clear_history"),
};

// Settings & Logs
export const settings = {
  get: () => invoke<GlobalSettings>("settings_get"),
  set: (opts: Partial<GlobalSettings>) => invoke<void>("settings_set", { opts }),
  detectJava: () => invoke<JavaInfo[]>("java_detect"),
};

export const logs = {
  get: (lines?: number) => invoke<string>("log_get", { lines }),
  clear: () => invoke<void>("log_clear"),
};
```

### 9.4 Event System (Rust → Frontend)

```rust
// src-tauri/src/events.rs
pub mod events {
    // Unified task center (ADR-019)
    pub const TASK_UPDATED: &str = "task://updated";
    pub const TASK_SYNC: &str = "task://sync";

    // Auth
    pub const AUTH_STATE_CHANGED: &str = "auth://state-changed";
    pub const AUTH_DEVICE_CODE: &str = "auth://device-code";

    // Launch
    pub const LAUNCH_STARTED: &str = "launch://started";
    pub const LAUNCH_OUTPUT: &str = "launch://output";    // coalesced stdout/stderr lines (batched)
    pub const LAUNCH_EXITED: &str = "launch://exited";

}

// All event names and error codes are string constants.
// ts-rs exports them as TypeScript const enums for type safety.
```

### 9.5 Internationalisation

界面文案全部经 `src/i18n/` 的 `t(key)` 取用，语言包在 `src/i18n/locales/`：

| 文件 | 作用 |
|------|------|
| `locales/en.ts` | 参考语言包（`as const`）。`TranslationKey` 由它推导，因此其他语言包缺键/多键都会在 `pnpm typecheck` 时报错 |
| `locales/zh-CN.ts` | 简体中文，类型为 `Messages = Record<TranslationKey, string>` |
| `locales/index.ts` | 语言包注册表 + `normalizeLocale`（与 Rust 侧 `resolve_locale` 行为一致） |
| `index.ts` | `t()`、`locale()`、`setLocale()`、`initI18n()`、系统语言探测 |

**生效顺序**（`resolve_locale` 与 `normalizeLocale` 两侧同构）：

1. `UserSettings.locale` 指定了语言 → 用它；
2. 否则用 WebView 上报的系统语言（`navigator.languages`）；
3. 系统语言不在支持列表内（如 `fr` / `ja`）→ 回退 `DEFAULT_LOCALE`（英语）。

匹配按主语言子标签进行：`en-US` → `en`，`zh` / `zh-TW` / `zh-Hans-CN` → `zh-CN`。
持久化值只会是 `None`（跟随系统）、`"en"` 或 `"zh-CN"`——后端 `settings_set_user`
写入前会做规范化，未知标签降级为英语而不是报错。

窗口 `visible: false`，`initI18n()` 在 `App.tsx` 里于 `show()` **之前**完成，
所以首次绘制就是正确语言，不会闪一下英文。

### 9.6 Virtual Scrolling

Mod 搜索结果和下载列表使用虚拟滚动（前端可见行数固定，只渲染可视区域）：

```typescript
// components/VirtualList.tsx
// Renders only visible rows, handles thousands of items smoothly
// Key props: items, rowHeight, overscan (default 5 rows buffer)
```

## 10. Data Flow

### 10.1 Game Installation Flow

```
User clicks "Install MC 1.20.4"
  │
  ├─ Frontend: invoke("version_install", { version: "1.20.4", profileId })
  │
  ├─ Rust: Check disk space (RmcccError::InsufficientDiskSpace if insufficient)
  │
  ├─ Rust: VersionProvider::fetch_version("1.20.4")
  │   └─ GET https://piston-meta.mojang.com/v1/packages/.../1.20.4.json
  │   └─ On failure: try local cache (offline fallback)
  │
  ├─ Rust: Resolve all dependencies
  │   ├─ Client JAR download URL
  │   ├─ Asset index → individual asset files (~4000 files for 1.20)
  │   ├─ Library JARs (with OS-native filtering)
  │   └─ Native libraries (zip extraction)
  │
  ├─ Rust: DownloadManager::download_batch(items)
  │   ├─ Concurrent downloads (configurable workers, default 16)
  │   ├─ Exponential backoff retry (1s → 2s → 4s, max 3 attempts)
  │   ├─ HTTP Range resume for interrupted downloads
  │   ├─ SHA1 verification per file
  │   └─ Throttled progress events (100ms interval) → Frontend
  │
  └─ Frontend: Update progress bar, show completion
```

### 10.2 Mod Loader Installation Flow

```
User selects "Install Fabric 0.15.6" for profile
  │
  ├─ Frontend: invoke("modloader_install", { profileId, loaderType: "fabric", loaderVersion })
  │
  ├─ Rust: DependencyManager::list_versions(loader_type, game_version)
  │   └─ Fabric: GET https://meta.fabricmc.net/v2/versions/loader/{gameVersion}
  │   └─ Forge:  GET https://files.minecraftforge.net/net/minecraftforge/forge/promotions_slim.json
  │
  ├─ Rust: DependencyManager::install_loader(...) / core::modloader::install_loader
  │   ├─ Fabric: Generate profile JSON + download fabric-loader JAR
  │   └─ Forge:  Download installer JAR + execute slim install (no GUI)
  │
  ├─ Rust: Update profile's mod_loader field + version JSON
  │
  └─ Frontend: Show "Fabric installed" + refresh UI
```

### 10.3 Microsoft Authentication Flow

```
User clicks "Microsoft Login"
  │
  ├─ Frontend: invoke("auth_microsoft_login")
  │
  ├─ Rust: MicrosoftAuthProvider::authenticate()
  │   ├─ Start OAuth2 device code flow
  │   │   └─ POST https://login.microsoftonline.com/consumers/oauth2/v2.0/devicecode
  │   ├─ Emit "auth://device-code" with { user_code, verification_uri }
  │   ├─ Frontend shows: "Go to microsoft.com/link, enter code XXXX"
  │   ├─ Poll for token (user completes browser auth)
  │   ├─ Exchange for Xbox Live token
  │   ├─ Exchange for XSTS token
  │   ├─ Exchange for Minecraft access token
  │   ├─ GET https://api.minecraftservices.com/minecraft/profile
  │   ├─ Encrypt private fields via TokenVault (AES-256-GCM)
  │   ├─ Write plaintext metadata to accounts.json
  │   ├─ Write encrypted private data to private/account-private-data.json
  │   └─ Return AuthSession
  │
  └─ Frontend: Show logged-in username, store session
```

### 10.4 Game Launch Flow

```
User clicks "Launch" on a profile
  │
  ├─ Frontend: invoke("launch_start", { profileId })
  │
  ├─ Rust: Resolve launch context
  │   ├─ Load profile + version JSON
  │   ├─ Detect/validate Java installation
  │   │   └─ Check MC version → Java version mapping
  │   ├─ Build classpath (libraries + client JAR + mod loader JARs)
  │   ├─ Construct argument list:
  │   │   ├─ JVM args: -Xmx, -Xms, -XX:+UseG1GC, ...
  │   │   ├─ Main class (from version JSON or mod loader override)
  │   │   └─ Game args: --username, --uuid, --accessToken, --version, --gameDir, ...
  │   ├─ Write args to response file (avoid ps aux token leak)
  │   └─ Merge mod loader specific args (Forge tweaks, Fabric mixins)
  │
  ├─ Rust: Spawn process
  │   ├─ Command::new(java_path).arg(format!("@{}", response_file)).spawn()
  │   ├─ Capture stdout/stderr → emit "launch://output"
  │   ├─ Schedule response-file removal once the JVM has read its args
  │   └─ Wait for exit → emit "launch://exited" with exit code
  │
  └─ Frontend: Show game log console, "Kill" button
```

## 11. IPC Command Table

125 commands, all defined in `src-tauri/src/commands/` and registered via
`IPC_COMMANDS` + `generate_handler!` in `src-tauri/src/handlers.rs`
（数量由 `ipc_tests.rs` 的计数断言守护；ADR-013 迁移后 agent_* 9 条已由
ai_* 5 条 + mods 纯逻辑 2 条取代，S4/S5 再加 mcp_* 9 条）。

| Command | Handler | Description |
|---------|---------|-------------|
| `open_external_url` | `external.rs` | Open a URL in the system browser |
| `dialog_pick_background_video` | `external.rs` | Pick and canonicalize a local MP4 wallpaper |
| `background_video_prepare` | `external.rs` | Probe, prepare native-resolution HEVC when needed, validate and authorize one asset |
| `dialog_pick_background_image` | `external.rs` | Pick and canonicalize a local still-image wallpaper |
| `background_image_prepare` | `external.rs` | Validate image format/size and authorize one asset path |
| `auth_microsoft_login` | `auth.rs` | Start MS OAuth2 device flow (emits `auth://device-code`) |
| `auth_offline_login` | `auth.rs` | Create offline session |
| `auth_logout` | `auth.rs` | Clear session + remove private data |
| `auth_get_session` | `auth.rs` | Decrypt + return cached session |
| `auth_refresh_token` | `auth.rs` | Silent refresh with a profile-UUID consistency check |
| `auth_list_accounts` | `auth.rs` | List accounts (plaintext metadata only) |
| `auth_switch_account` | `auth.rs` | Change the selected account |
| `auth_remove_account` | `auth.rs` | Remove an account and its private data |
| `task_list` | `tasks.rs` | Active tasks + recent history (reload recovery) |
| `task_get` / `task_log` | `tasks.rs` | One task snapshot / full detail log |
| `task_pause` / `task_resume` / `task_cancel` | `tasks.rs` | Task-scoped controls (ADR-019) |
| `task_clear_history` | `tasks.rs` | Clear recent completed tasks |
| `launch_start` | `launch.rs` | Launch game through `DefaultLauncher` |
| `launch_kill` | `launch.rs` | Kill game process (process group on Unix) |
| `launch_status` | `launch.rs` | Get running game status |
| `log_get` | `logs.rs` | Get recent launcher log lines |
| `log_clear` | `logs.rs` | Clear/truncate log file |
| `frontend_log` | `ui.rs` | Frontend runtime message → launcher log (release 仅有日志通道) |
| `ai_config_get` | `ai.rs` | AI 服务配置视图 `{configured, baseUrl, model, hasKey}` |
| `ai_config_set` | `ai.rs` | 保存 AI 配置（字段可选，留空沿用已存值；key 入 vault，ADR-013） |
| `ai_models_probe` | `ai.rs` | `GET {base}/models` 自动检测可用模型（未传参用已存配置） |
| `ai_analyze_log` | `ai.rs` | 游戏日志一键分析（脱敏尾部 → 流式事件 `ai://delta/done/error`） |
| `ai_analyze_cancel` | `ai.rs` | 取消进行中的分析 |
| `mcp_status` | `mcp.rs` | MCP 服务器状态（开关/端口/令牌/会话/待确认数） |
| `mcp_set_enabled` | `mcp.rs` | 启停 MCP 服务器（默认关闭；开启即监听回环） |
| `mcp_regenerate_token` | `mcp.rs` | 重新生成 Bearer Token 并断开全部会话 |
| `mcp_tools_list` / `mcp_tools_set` | `mcp.rs` | 30 工具目录与逐工具策略覆盖（allow/confirm/deny） |
| `mcp_sessions_disconnect` | `mcp.rs` | 断开指定（或全部）外部会话 |
| `mcp_confirmations_list` | `mcp.rs` | 待确认的破坏性操作（确认卡数据） |
| `mcp_confirmation_resolve` | `mcp.rs` | 用户放行/拒绝一次破坏性调用 |
| `mcp_recent_calls` | `mcp.rs` | 最近 MCP 调用审计（脱敏视图） |
| `modloader_list` | `modloader.rs` | List loader versions (`loader_type` param) |
| `modloader_install` | `modloader.rs` | Install loader; creates a new version-backed instance and returns its key |
| `modpack_inspect` | `modpack.rs` | Parse a local `.zip`/`.mrpack`; returns format, game/loader/file counts, icon, warnings |
| `modpack_pick_file` | `modpack.rs` | Native `.zip`/`.mrpack` picker |
| `modpack_install` | `modpack.rs` | Background one-click install (game → loader → overrides → files → metadata); creates a unified task |
| `task_list` / `task_get` / `task_log` | `tasks.rs` | Task board / snapshot / full detail log |
| `task_pause` / `task_resume` / `task_cancel` | `tasks.rs` | Task-scoped controls (ADR-019) |
| `task_clear_history` | `tasks.rs` | Clear recent completed tasks |
| `mod_search` | `mods.rs` | Search mods (Modrinth) |
| `mod_get_versions` | `mods.rs` | List versions of a mod |
| `mod_get_download_url` | `mods.rs` | Resolve a download URL |
| `mod_list` | `mods.rs` | List installed mods (active + disabled) |
| `mod_toggle` | `mods.rs` | Enable/disable mod (move to/from `mods/disabled/`) |
| `mod_install` | `mods.rs` | Download + install mod (remote) |
| `mod_remove` | `mods.rs` | Remove mod from an instance run dir |
| `mod_install_local` | `mods.rs` | Install mod from a local file path |
| `mod_pick_and_install` | `mods.rs` | System file picker + install (local mod) |
| `mods_dependency_scan` | `mods.rs` | 纯逻辑依赖扫描（缺失必需/冲突/未解析，ADR-013 §4.4） |
| `mods_apply_plan` | `mods.rs` | 一键补装缺失前置（Modrinth 匹配 + SHA1 校验，纯逻辑无 AI） |
| `game_directory_list` | `instance.rs` | List registered game roots |
| `game_directory_overview` | `instance.rs` | Sidebar rows: layout markers + instance count + selected flag |
| `game_directory_selected` | `instance.rs` | The active game root |
| `game_directory_scan` | `instance.rs` | Discover HMCL/PCL/official `.minecraft` directories |
| `game_directory_reveal` | `instance.rs` | Open a game root in Finder/Explorer |
| `game_directory_add` / `update` / `remove` / `select` | `instance.rs` | Registry CRUD (never deletes files; empty root gets `versions/`) |
| `storage_overview` | `instance.rs` | Launcher directories + game directories with per-instance sizes (blocking walk on a worker thread) |
| `storage_clean_cache` | `instance.rs` | Remove `{data_dir}/cache` only; returns freed bytes |
| `instance_sizes` | `instance.rs` | Per-instance sizes for the instance cards |
| `instance_list` | `instance.rs` | Detailed instances for a directory (settings + paths, no completeness scan) |
| `instance_get` | `instance.rs` | Instance detail by `{directory}/{id}` |
| `instance_refresh` | `instance.rs` | Rescan a directory, emit `instances://refreshed` |
| `instance_update_settings` | `instance.rs` | Settings/override/display-name/icon/isolation patch |
| `instance_resolved_settings` | `instance.rs` | Effective values (global preset + overrides) |
| `instance_paths` | `instance.rs` | Game root / run dir / mods dir / natives / settings file |
| `instance_select` | `instance.rs` | Persist the selected instance key |
| `instance_install` | `instance.rs` | Download + create an instance (optional loader) |
| `instance_import` | `instance.rs` | Register an already-installed version |
| `instance_rename` | `instance.rs` | Rename id + manifest + child `inheritsFrom` |
| `instance_duplicate` | `instance.rs` | Duplicate (optional `saves/`) |
| `instance_remove` | `instance.rs` | Hide (`delete_files=false`) or delete `versions/{id}` |
| `instance_check_completion` | `instance.rs` | Report missing files (hash-level) |
| `instance_repair` | `instance.rs` | Download missing files |
| `instance_global_settings` | `instance.rs` | Global game-settings preset |
| `instance_update_global_settings` | `instance.rs` | Replace the global preset |
| `settings_get` | `settings.rs` | Get global settings |
| `settings_set` | `settings.rs` | Update global settings (debounced `FileSaver` write) |
| `settings_get_user` | `settings.rs` | Get user-level settings (UI language, update check) |
| `settings_set_user` | `settings.rs` | Update user settings; `locale: ""` = follow the system |
| `settings_supported_locales` | `settings.rs` | List shipped UI languages with their native names |
| `java_detect` | `settings.rs` | Auto-detect Java installations |
| `version_list` | `version.rs` | List available MC versions (with cache) |
| `instance_list` | `instance.rs` | List instances from the local repository index |
| `instance_refresh` | `instance.rs` | Rescan `versions/`, emit `instances://refreshed` |
| `instance_rename` | `instance.rs` | Rename an instance |
| `instance_duplicate` | `instance.rs` | Duplicate an instance (jar + patched JSON) |
| `instance_remove` | `instance.rs` | Remove an instance |
| `instance_check_completion` | `instance.rs` | Report missing files |
| `instance_repair` | `instance.rs` | Download missing files |
| `instance_paths` | `instance.rs` | Effective game root / run dir / mods dir (depends on isolation) |
| `instance_resolved_settings` | `instance.rs` | Resolve per-launch effective settings (defaults + profile) |
| `plugin_list` | `plugin.rs` | List installed plugins |
| `plugin_inspect` | `plugin.rs` | Inspect package before install (manifest + checks) |
| `plugin_install` | `plugin.rs` | Install plugin package (transactional, ADR-003) |
| `plugin_pick_and_install` | `plugin.rs` | System file picker + install |
| `plugin_enable` / `plugin_disable` | `plugin.rs` | Toggle plugin enabled state |
| `plugin_remove` | `plugin.rs` | Uninstall plugin (rollback data retained) |
| `plugin_rollback` | `plugin.rs` | Roll back to previous install state |
| `plugin_grant` / `plugin_revoke` | `plugin.rs` | Grant/revoke capabilities |
| `plugin_config_get/set/reset` | `plugin.rs` | Plugin-scoped config CRUD |
| `plugin_capabilities` | `plugin.rs` | Capability catalog for a plugin |
| `plugin_contributions` | `plugin.rs` | Dashboard/theme contributions of a plugin |
| `plugin_ui_invoke` | `plugin_ui.rs` | Isolated UI container bridge invoke (ADR-006) |
| `plugin_logic_invoke` | `plugin_logic.rs` | Read-only logic call behind `logic.invoke` (ADR-007) |
| `plugin_logic_cancel` | `plugin_logic.rs` | Cancel a running logic call |
| `dashboard_list_widgets` | `dashboard.rs` | List available widgets (incl. plugin contributions) |
| `dashboard_list_themes` | `dashboard.rs` | List themes (builtin + plugin) |
| `dashboard_get_theme_tokens` | `dashboard.rs` | Resolve active theme tokens |
| `dashboard_set_theme_preference` | `dashboard.rs` | Set theme preference |
| `dashboard_get_theme_preference` | `dashboard.rs` | Get theme preference |
| `dashboard_get_theme_json` | `dashboard.rs` | Read raw theme JSON (plugin theme viewer) |

> 前端调用覆盖（wrapper / 页面实际调用）与规划目标见
> [`ui-design/11_IPC_COVERAGE.md`](ui-design/11_IPC_COVERAGE.md)；
> 前端整体将按 [`ui-design/`](ui-design/) 文档重构。

## 12. Size Budget

### 12.1 Target: < 20 MiB Total（ADR-011，2026-10-24 放宽）

> 度量口径：**strip 后的主可执行文件**（1 MiB = 1,048,576 B）；strip 失效时门禁判
> release-blocked，不得用未剥离数字宣称通过。

| Component | Target Size | Notes |
|-----------|------------|-------|
| Tauri 2.x binary (stripped + LTO) | ~4.5 MB | Rust code + WebView glue |
| SolidJS frontend (gzipped) | ~80 KB | JS + Tailwind CSS |
| Static assets (icons) | ~100 KB | PNG icons, splash |
| **Total** | **~4.7 MB** | **远低于 20 MiB 红线** |

### 12.2 Binary Optimization Strategy

```toml
# Cargo.toml (workspace root)
[profile.release]
opt-level = "s"          # Optimize for size
lto = true               # Link-time optimization
codegen-units = 1        # Single codegen unit (better LTO)
strip = true             # Strip debug symbols
panic = "abort"          # Abort on panic (smaller binary)

[profile.release.package."*"]
opt-level = "s"
strip = true
```

### 12.3 Frontend Build Optimization

```typescript
// vite.config.ts
export default defineConfig({
  build: {
    target: "esnext",
    minify: "esbuild",
    cssMinify: true,
    rollupOptions: {
      output: {
        manualChunks: undefined,  // single chunk (small enough)
      },
    },
  },
  plugins: [solid(), tailwindcss()],
});
```

### 12.4 Dependency Audit Checklist

Before adding any crate, evaluate:

1. **Size impact**: `cargo bloat --release` to measure
2. **Alternatives**: Prefer lighter crates (e.g., `ureq` vs `reqwest` if async not needed)
3. **Feature flags**: Disable default features, enable only what's needed
4. **Duplication**: Avoid multiple crates doing the same thing

## 13. Cross-Platform Strategy

### 13.1 Platform Matrix

| Feature | Windows | macOS | Linux |
|---------|---------|-------|-------|
| WebView | WebView2 (built-in Win10+) | WebKit (system) | WebKit2GTK |
| Java detection | Registry + PATH + common paths | `/usr/libexec/java_home` + PATH | `update-alternatives` + PATH |
| Game directory | `%APPDATA%/.minecraft` | `~/Library/Application Support/minecraft` | `~/.minecraft` |
| Process spawn | `CreateProcess` | `fork+exec` | `fork+exec` |
| File separator | `\` | `/` | `/` |
| Executable | `.exe` | (no ext) | (no ext) |
| Min version | Windows 10 | macOS 11+ | Ubuntu 22.04+ |
| .key permissions | ACL (icacls) | chmod 600 | chmod 600 |

### 13.2 Conditional Compilation

```rust
// Platform-specific Java detection
#[cfg(target_os = "windows")]
mod java_windows {
    use winreg::enums::*;
    use winreg::RegKey;
    // Scan registry for JDK/JRE installations
}

#[cfg(target_os = "macos")]
mod java_macos {
    // Use /usr/libexec/java_home -V
}

#[cfg(target_os = "linux")]
mod java_linux {
    // Scan /usr/lib/jvm/ + update-alternatives
}
```

### 13.3 CI/CD Cross-Compilation

```yaml
# .github/workflows/build.yml (conceptual)
strategy:
  matrix:
    target:
      - os: windows-latest
        rust-target: x86_64-pc-windows-msvc
      - os: macos-latest
        rust-target: aarch64-apple-darwin    # Apple Silicon
      - os: macos-latest
        rust-target: x86_64-apple-darwin     # Intel
      - os: ubuntu-22.04
        rust-target: x86_64-unknown-linux-gnu
  steps:
    - name: Build
      run: cargo build --release --target ${{ matrix.rust-target }}
    - name: Test
      run: cargo test --workspace --target ${{ matrix.rust-target }}
    - name: Clippy
      run: cargo clippy --workspace -- -D warnings
    - name: Package (Tauri)
      run: pnpm tauri build
```

## 14. Tauri Platform Features (P2)

| Feature | Plugin / Approach |
|---------|------------------|
| Auto-update | `tauri-plugin-updater` — GitHub Releases as update source |
| System tray | Game running → minimize to tray, show status |
| Window state | `tauri-plugin-window-state` persists window size/position |
| Deep link | `rmccc://profile/{id}` — browser one-click import |
| Linux min version | Ubuntu 22.04+ (WebKit2GTK 4.0+) |

## 15. File System Layout

### 15.1 User Data Directory

```
{data_dir}/                          # Platform data dir (CLI `-d` overrides)
├── config/
│   ├── launcher-settings.json       # Launcher behavior (paths/download/UI)
│   ├── game-settings.json           # Global game-settings preset
│   └── game-directories.json        # Registered game roots + selected instance
├── state/
│   ├── launcher-state.json          # Runtime state (loss-tolerant)
│   └── instance-migration-v2.json   # One-shot legacy profile migration marker
├── user-settings.json               # User-level preferences
├── .key                             # AES-256 encryption key (600 / icacls)
├── accounts.json                    # Plaintext account metadata + selected
├── private/                         # Encrypted account/agent credentials
├── .api_keys/                       # API keys (separate from settings)
├── cache/                           # Offline caches + SHA1 object cache
├── profiles/                        # LEGACY read-only profiles (migration only)
├── instances/                       # Default managed game root (.minecraft equivalent)
│   ├── versions/                    # Instance source of truth
│   ├── libraries/  assets/          # Shared across instances
│   └── launcher_profiles.json       # Created on demand (Forge/LiteLoader need it)
├── agent/                           # Audit directory (ADR-010; ADR-013)
└── logs/                            # Launcher logs

{game_dir}/                          # Any registered game root (managed or external)
├── versions/
│   └── 1.20.4-fabric/
│       ├── 1.20.4-fabric.json       # Manifest (also the instance identity)
│       ├── 1.20.4-fabric.jar        # Optional (inherits parent JAR when absent)
│       ├── natives-{platform}/      # Extracted natives
│       ├── .abnormal                # Crash marker, cleared on normal exit
│       └── .rmccc/
│           ├── instance.json        # Per-instance settings overlay
│           └── instance.json.tmp    # Atomic write staging (transient)
├── libraries/  assets/              # Shared across instances
├── mods/                            # Run-dir mods (isolated instances use versions/{id}/mods)
│   └── disabled/
├── resourcepacks/
└── logs/
```

### 15.2 Instance Settings Schema

`{instance_root}/.rmccc/instance.json` (instance-local, moves with the version):

```json
{
  "$schema": "https://rmcl.snowebsite.top/json/schema/instance-settings/1.0.0",
  "display_name": "Survival Pack",
  "icon": { "type": "auto" },
  "isolation": "always",
  "override_properties": ["max_memory_mb", "jvm_args"],
  "java_type": "custom",
  "custom_java_path": "/Library/Java/JavaVirtualMachines/jdk-21.jdk/Contents/Home",
  "jvm_args": ["-XX:+UseG1GC"],
  "game_args": [],
  "max_memory_mb": 4096,
  "min_memory_mb": null,
  "metaspace_mb": null,
  "process_priority": "normal",
  "window_type": "windowed",
  "width": 854,
  "height": 480,
  "running_directory": null,
  "launcher_visibility": "hide",
  "graphics_backend": "default",
  "quick_play": "none",
  "env_vars": {},
  "created_at": "2026-07-29T10:00:00Z",
  "last_played": null,
  "play_time_seconds": 45240
}
```

> `isolation`（`never`/`modded`/`always`，`null` = 跟随全局预设）只决定运行目录；
> `override_properties`（覆盖名单，见 §3.4）决定哪些字段取实例值，其余取
> `config/game-settings.json`。**实例里填过的值即使不覆盖也保留在文件中**。
>
> `running_directory` 是 HMCL `runningDirectory` 的对等物：非空时覆盖隔离推导
> （绝对路径，或相对 `{game_dir}`）。旧字段 `external_root` 已取消——实例归属由
> `game-directories.json` 的目录注册表表达。
>
> 旧 `profiles/{uuid}.json` 仅在首启迁移时读取：根 = `external_root ? game_dir : 默认目录`，
> 实例 id = `game_version`；版本 JSON 不存在的 profile 跳过并记诊断。迁移完成后旧文件
> 保留但不再读取。`state/instance-migration-v2.json` 是实现幂等的标记。

## 16. Testing Strategy

### 16.1 Test Pyramid

| Level | Scope | Tools | When |
|-------|-------|-------|------|
| Unit | Argument builder, classpath resolution, SHA1 verify, version comparison | `#[cfg(test)]` + `assert_eq!` | Every commit |
| Integration | API interactions (mocked) | `wiremock` crate | Every PR |
| Trait Mock | Orchestrator pipeline logic | `mockall` or hand-written mocks | Every PR |
| E2E | Login → create profile → launch | `tauri-driver` | Release CI |

### 16.2 Key Test Cases

```rust
// Unit: Argument builder
#[test]
fn test_jvm_args_default() {
    let args = build_jvm_args(4096, &[]);
    assert!(args.contains(&"-Xmx4096M".to_string()));
    assert!(args.contains(&"-Xms256M".to_string()));
}

// Unit: Java version mapping
#[test]
fn test_java_version_mapping() {
    assert_eq!(java_version_for_mc("1.16.5"), 8);
    assert_eq!(java_version_for_mc("1.17"), 16);
    assert_eq!(java_version_for_mc("1.20.4"), 17);
    assert_eq!(java_version_for_mc("1.20.5"), 21);
}

// Integration: Mock Mojang API
#[tokio::test]
async fn test_version_fetch_with_mock() {
    let mock_server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/mc/game/version_manifest_v2.json"))
        .respond_with(ResponseTemplate::new(200).set_body_json(fixture_manifest()))
        .mount(&mock_server)
        .await;

    let provider = MojangVersionProvider::new(mock_server.uri());
    let versions = provider.fetch_versions().await.unwrap();
    assert!(!versions.is_empty());
}

// Trait Mock: Pipeline orchestration
#[tokio::test]
async fn test_install_pipeline_calls_download() {
    let mut mock_version = MockVersionProvider::new();
    mock_version.expect_fetch_version().returning(|_| Ok(fixture_version()));

    let mut mock_download = MockDownloadManager::new();
    mock_download.expect_download_batch().returning(|_, _| Ok(fixture_handle()));

    // Test pipeline wires providers correctly
}
```

### 16.3 CI Pipeline

```yaml
# Every PR:
- cargo test --workspace --all-features
- cargo clippy --workspace -- -D warnings
- cargo fmt --check
- cd src/ && pnpm typecheck   # Frontend type check

# Release:
- cargo test --workspace --release
- tauri-driver E2E suite
- cargo bloat --release (size regression check)
```

## 17. Engineering Standards

### 17.1 Type Generation (ts-rs)

All IPC-related Rust structs derive `TS`, build script auto-generates TypeScript types：

```rust
// Every IPC type derives TS
#[derive(Debug, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Profile { ... }

#[derive(Debug, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct IpcError { ... }

// build.rs in src-tauri triggers ts-rs export
// Output: src/ipc/types.ts (auto-generated, gitignored)
```

### 17.2 Event & Error Constants

```rust
// Event names: centralized in events.rs as &str constants
// Error codes: ErrorCode enum (serialized as snake_case)
// Frontend imports auto-generated const enums — no stringly-typed events
```

### 17.3 Hot-Reloadable Config

```rust
// Some settings can be changed without restarting the app:
pub struct HotReloadConfig {
    pub download_concurrency: u32,    // change worker pool size
    pub mirror_url: Option<String>,   // switch download mirror
    pub event_throttle_ms: u64,       // adjust throttle interval
}

// Other settings require restart:
pub struct ColdConfig {
    pub data_dir: PathBuf,
    pub default_java_path: Option<String>,
}
```

## 18. Implementation Phases

### Phase 1: Foundation (Week 1-2)
- Workspace setup (Cargo workspace, Tauri project init, ts-rs config)
- `rmccc-config`: Path resolution, global settings, **logging init (tracing)**
- `rmccc-core`: RmcccError enum, IpcError bridge, CoreContext skeleton
- `rmccc-download`: Concurrent download engine + SHA1 verification + retry + resume
- `rmccc-auth`: Microsoft OAuth2 device flow + offline auth + **AES-256-GCM encryption**
- Frontend: Project scaffold + layout + login page + **error handling UI**

### Phase 2: Core Game (Week 3-4)
- `rmccc-version`: Mojang manifest parsing, version listing + **offline cache**
- `rmccc-launch`: Classpath resolution, argument builder (**response file**), process spawn + **Java version mapping**
- Game download pipeline (client JAR + assets + libraries) + **disk pre-check**
- Frontend: Profile creation + version install + launch button

### Phase 3: Mod Loaders (Week 5-6)
- `rmccc-modloader`: Fabric installer (meta API + profile generation)
- `rmccc-modloader`: Forge installer (installer JAR execution)
- `rmccc-modloader`: NeoForge installer
- Frontend: Mod loader selection + install UI

### Phase 4: Mod Management (Week 7-8)
- `rmccc-mods`: Modrinth API integration + **dependency resolution**
- `rmccc-mods`: CurseForge API integration + **API key management**
- Local mod file management (install/remove/toggle)
- Frontend: Mod browser + search + install/manage UI + **virtual scrolling**

### Phase 5: Polish & Ship (Week 9-10)
- Binary size optimization (LTO, stripping, feature audit)
- **Integration tests** (wiremock) + **E2E tests** (tauri-driver)
- Cross-platform CI/CD pipeline
- Auto-update plugin + system tray + deep link
- UI polish + dark theme + responsive layout
- **Performance audit**: `cargo bloat`, `pool_max_idle_per_host` tuning

## 19. Key Design Decisions

### Why Tauri 2.x over Electron?
Electron bundles Chromium (~150MB+). Tauri uses system WebView, keeping binary at ~4-5MB. For the 20 MiB target (ADR-011), this is non-negotiable.

### Why SolidJS over React?
SolidJS compiles to fine-grained reactive DOM updates with no virtual DOM overhead. Bundle is ~8KB vs React's ~40KB. For a launcher that's mostly a management UI (not complex interactive apps), SolidJS is more than sufficient.

### Why workspace with many small crates?
Compile-time isolation: changing one module doesn't recompile everything. Also enforces clean boundaries — each crate has a clear public API. Future option: publish `rmccc-core` as a standalone library for other launchers.

### Why reqwest over ureq?
Concurrent downloads need async. reqwest with rustls-tls gives us async HTTP without system OpenSSL dependency. ureq is sync-only and would require thread-per-download.

### Why JSON for profile storage over SQLite?
Profiles are simple, rarely concurrent, and human-debuggable. SQLite adds ~500KB to binary. JSON is zero-cost (already using serde_json). Can migrate to SQLite later if needed.

### Why AES-256-GCM over system keychain?
System keychain (`keyring` crate) adds cross-platform complexity (different backends per OS, potential D-Bus issues on Linux). AES-256-GCM with a random key file is ~30KB, pure Rust, zero system dependencies. Security boundary: protects against non-privileged users and scripts. Desktop app threat model accepts local physical access risk.

### Why tracing from Phase 1 instead of Phase 5?
Logging is infrastructure, not polish. Adding it late means retrofitting spans into every function, and missing critical debugging data during development. Every IPC command and download batch gets a span from day one.

### Why response files for game launch arguments?
`--accessToken` in process args is visible via `ps aux` / Task Manager. Writing args to a temporary file and passing `@filepath` to Java keeps tokens out of process listings. File is deleted immediately after spawn.

## 20. 插件系统（Phase 2 已实现）

> 目标架构与阶段路线见指南 §6–§14；协议细节以
> [ADR-003](adr/ADR-003-plugin-package-and-install-transaction.md) /
> [ADR-004](adr/ADR-004-plugin-manifest-and-versioning.md) 为准。本节只记录落地形态。

### 20.1 边界

- **Phase 2 不执行第三方代码**：只支持数据型插件（主题、翻译、声明式卡片/页面）。
  `entry.ui` / `entry.logic` 在 Manifest 校验阶段拒绝；包内出现 `ui/`、`logic/`、
  脚本或可执行文件直接拒绝。隔离 UI 在 Phase 4，逻辑运行时在 Phase 5。
- 插件**拿不到 `CoreContext`/Token**；权限由后端 `PluginManager`/`PermissionStore` 判定。
- 损坏插件/注册表只产生诊断，不阻断主应用启动。

### 20.2 目录与事实来源

```text
<data_dir>/
  plugins/<plugin-id>/<version>/...     # 不可变版本目录（可并存，支持回滚）
  plugin-data/<plugin-id>/config.json   # 用户配置（卸载默认保留）
  plugin-state/registry.json            # 安装/启用状态唯一来源
  plugin-state/permissions.json         # 授权（与 Manifest 声明分离）
  plugin-state/audit.jsonl              # 权限审计（只追加）
  plugin-state/staging/<uuid>/          # 安装暂存（成功后删除）
```

**不维护 `enabled.json`**：启用状态是 `registry.json` 生命周期状态的一部分。

### 20.3 安装事务

`inspect(.rmcpkg)` → staging 解压 → Manifest/限额/路径/类型校验 → 依赖/循环检查 →
`rename` 到 `plugins/<id>/<version>` → 原子写注册表 → 原子写权限 → 删除 staging。
任一步失败：删除新版本目录并恢复注册表/权限内存快照。升级保留旧版本目录与
`previousVersion`；新权限一律进入 pending，插件回到 `installed_disabled` 等待审查。

### 20.4 IPC / CLI / UI

- CLI：`rmcli plugin …`（13 子命令）。
- IPC：15 条 `plugin_*` 命令；`/plugins` 页面负责安装、权限审查、配置、回滚、卸载。
- 贡献项（`dashboardCards` / `settingsPages` / `themes`）只汇总**已启用**插件，
  注册键为 `<pluginId>/<localId>`，供 Phase 3 的 Dashboard 布局引擎消费。

### 20.5 平台归属

路径/zip 校验是 OS 无关逻辑；**原子替换**与权限收紧经
[`PlatformFs`](../crates/rmccc-platform/src/fs.rs)（`atomic_replace` / `restrict_owner_only`），
`rmccc-plugin` 内零 `#[cfg]`。Windows/Linux 后端返回 `UnsupportedPlatform`，原生验证 deferred。

## 21. Dashboard 布局引擎与主题（Phase 3）

> 协议与算法以 [ADR-005](adr/ADR-005-dashboard-layout-and-theme.md) 为准。本节只记录落地形态。

### 21.1 分层

| 模块 | 职责 |
|---|---|
| `rmccc-dashboard::layout` | 布局 Schema、确定性归一化（去重/钳制/换行/push-down/压缩）、断点 |
| `rmccc-dashboard::widget` | `WidgetRegistry`：内置 `org.rmccc.core-ui` + 插件 `dashboardCards` |
| `rmccc-dashboard::theme` | 主题令牌 + 安全语义色边界 |
| `rmccc-dashboard::store` | `dashboard/layouts/<id>.json` 读取/保存/损坏恢复 |

### 21.2 布局文件

`<data_dir>/dashboard/layouts/<layoutId>.json`：

```json
{ "schemaVersion": 1, "layoutId": "default",
  "grid": { "columns": 12, "gap": 16 },
  "widgets": [{ "instanceId": "core-launcher", "pluginId": "org.rmccc.core-ui",
                "widgetId": "instance-launcher", "enabled": true,
                "position": { "x": 0, "y": 0 }, "size": { "w": 8, "h": 4 } }] }
```

写入前归一化；损坏 → `.bak-<ts>` + 默认布局；未知版本拒绝。

### 21.3 主页卡片系统 v3（2026-10-31）

- 前端主页已从后端持久化网格切换为 **12 列五档宽 × 四档高**卡片系统，权威契约见
  [`docs/ui-design/14_CARD_SYSTEM.md`](ui-design/14_CARD_SYSTEM.md)。宽度档位
  `3/4/6/8/12`，高度档位 `1/2/3/4`（92px 行高 + 14px 间隔），布局存
  `rmccc.softDashboard.v3`；旧 `rmccc.softDashboard.v2` 自动迁移。
- 内置卡与插件卡共用 `.soft-module` 外壳、尺寸约束和编辑操作。插件 manifest 的
  `dashboardCards[]` 支持多卡，全局实例 id 为 `plugin:<pluginId>/<cardId>`；
  `defaultSize/minSize/maxSize` 会吸附到合法档位。
- 插件卡通过 UI API v1 Host Bridge 读取主题、公开设置、实例、任务、启动状态，
  或经声明的 `network.request` 查询服务器/HTTP；每次调用在 Rust 重新授权。
- 默认卡：`server` 走 Rust SLP（`server_status`）；`launch://progress` 由启动编排
  和游戏窗口就绪日志驱动，`100%` 只在窗口打开后出现；`downloads` 直接消费统一任务
  中心快照，不再维护第二套下载状态。

### 21.3 渲染与交互

- 前端 CSS Grid（`repeat(columns, 1fr)`，120px 行高）；`grid-column/row` 由 `position/size` 计算。
- 断点：≥1280 wide（声明列数）/ 768–1279 medium（≤8）/ <768 compact（≤4）。
- **显式编辑模式**（默认关闭）：工具栏最右的「编辑布局 / 完成」是唯一开关。
  - 视图态：不渲染任何卡片头、不接受 drag/drop，卡片（含插件 iframe）完全可交互；
  - 编辑态：每张卡片显示 `⠿` 拖动 / `↔` 循环宽度 / `×` 隐藏，槽位虚线框，
    槽位内容与 iframe `pointer-events: none`（否则 iframe 会吞掉拖放事件）；
    工具栏才出现「添加插件卡片 / 恢复隐藏 / 恢复默认」。
  - 卡片头**只在编辑态渲染**：`opacity: 0` 不会关闭命中测试，早期版本因此留下
    一层看不见的覆盖物，让整页都点不动。
- 每卡片 `ErrorBoundary`；插件卸载/禁用后 descriptor 缺失 → 保位占位。

### 21.4 主题（Noctis v3，ADR-021）

- `--rmccc-*` 令牌（background/surface/surfaceElevated/text/textMuted/accent/accentContrast/border），
  同时桥接旧 `--surface-*`/`--text-*` 变量。公开 schema 仍只有这 8 个颜色。
- 宿主派生 `ThemeScene`（`src/theme/presets.ts`）：8 套内置主题各有 atmosphere、
  secondary/tertiary 与粒子 motif；未知插件主题按 accent 的 HSL 派生确定性场景。
  场景通过 `<html data-atmosphere>` 与 `--scene-*` 发布，主题不能提供 CSS/JS。
- 视觉层：`theme-core.css`（token/控件/页面）→ `theme-skin.css`（v2 皮肤槽）→
  `ambient-stage.css`（默认动态背景）→ `home-stage.css`（主页/dock）。旧
  `visual-system.css` / `soft-dashboard.css` 已删除。
- 没有用户图片/视频壁纸且没有 v2 app 背景时，`AmbientStage` 渲染主题化渐变、
  veil、地平线、网格和粒子；有壁纸时不创建。动效统一受 `data-motion` 与
  `prefers-reduced-motion` 闸门。
- 自适应主题 v2：图片多区域 / 视频多帧 → OKLab k-means（k=6）→ 代表性角色色；
  dark/light 跟随当前主题，所有关键对比度 ≥4.5:1；存储 `rmccc.adaptivePalette.v2`。
- `follow_system` 仍监听 `prefers-color-scheme` 以兼容旧 `null` 偏好；设置页只列
  显式主题卡。未知/无效主题回退安全主题。
- 安全语义色（danger/success/warning）由宿主控制，主题 JSON 中出现时忽略。

## 22. 前端引导与"透明窗口"防护（启动链路）

窗口是 `decorations: false` + `transparent: true` + `visible: false`，由 `main.rs` 在 `setup`
末尾 `show()`。这个组合把"前端整包没跑起来"退化成**透明矩形**：没有标题栏、没有报错、没有日志。
2026-10-21 的一次真实故障就属于这一类——`.app` 里的可执行文件**没有嵌入任何前端资源**，
WebView 收到空 asset map，`index.html` 与模块脚本全 404。

三道防线（按失效位置从早到晚）：

| 层 | 位置 | 作用 |
|----|------|------|
| **构建** | `src-tauri/build.rs` | `cargo:rerun-if-changed=../dist`。`tauri-build` 自己不监听 `frontendDist`，缺这行时"先 `cargo build`（dist 不存在）→ 后 `tauri build`（不重编译）"会把空 asset map 固化进二进制 |
| **原生自检** | `src-tauri/src/bundle.rs` | 启动时用 `asset_resolver()` 校验 `/index.html` 与文档里 `src=`/`href=` 引用的资源；缺失 → `ERROR` 日志 + 把窗口**导航**到内联 `data:text/html` 错误页（列出缺失清单和 `pnpm tauri build`）。用导航而非 `eval`，因为 `index.html` 缺失时没有可求值的脚本上下文 |
| **前端守卫** | `index.html` 内联脚本 | 位于 bundle 之前、不依赖模块加载：捕获资源加载失败、未捕获异常、未处理 Promise、4s 未挂载，绘制深色错误面板，并通过 IPC `frontend_log` 写入 `logs/rmccc.<date>.log` |

`src/main.tsx` 在 `render()` 后派发 `rmccc:shell-mounted`，守卫收到后撤下错误面板（只在
**挂载前**才全屏绘制，挂载后仅写日志，避免遮住可用的 UI）。

`frontend_log(level, message)` 是注册表里唯一的"前端 → 日志"通道（8 KiB 上限、level 白名单），
因为 release 构建没有 Web Inspector，WKWebView 的 console 在进程外不可见。

排障开关：`RMCCC_WEBVIEW_PROBE=1` 运行主可执行时，4s 后把 WebView 实际加载的 URL、
`#root` 子节点数、脚本/样式数量写进日志并画在页面上（正常启动无副作用）。

## 23. 隔离 UI 扩展与 SDK（Phase 4）

第三方 UI 不允许在主窗口的 JS 环境里执行（同环境即可拿到全部 Tauri API，指南 §6.2）。
Phase 4 的边界是三层叠加，任何一层失效都还有下一层：

| 层 | 实现 | 保证 |
|----|------|------|
| **独立源** | `<iframe sandbox src="rmcccplugin://<pluginId>/ui/…">`；`src-tauri` 注册该 URI 方案，只服务插件安装目录 `ui/` 内的白名单文件 | 与主窗口（`tauri://localhost`）不同源：无宿主 DOM、无 `__TAURI_INTERNALS__` |
| **CSP** | HTML 响应头：`default-src 'none'; script-src 'self'; style-src 'self' 'unsafe-inline'; img-src 'self' data:; font-src 'self'; connect-src 'none'; form-action 'none'; base-uri 'none'; object-src 'none'` | 不能加载远程脚本/样式，**不能自己发网络请求**（只能走桥） |
| **逐条授权** | `plugin_ui_invoke` 每条消息重新 `ensure_enabled` + `authorize`；`network.request` 以目标 host 为作用域 | 撤销能力后下一条消息立即被拒 |

### 23.1 协议（UI API v1）

`postMessage` 传 **JSON 字符串**（不是对象）：宿主可以对长度、形状、版本一次校验。

```text
frame → host   { v:1, type:"ready" }
host  → frame  { v:1, type:"init", pluginId, cardId?, theme, limits }
frame → host   { v:1, type:"cancel", id }          # 放弃 in-flight 请求，迟到结果丢弃
frame → host   { v:1, id, method, params? }
host  → frame  { v:1, id, ok:true, result } | { v:1, id, ok:false, error:{ code, message, retryable } }
host  → frame  { v:1, type:"theme", theme } / { v:1, type:"revoke", capability }
```

| method | 能力 | 说明 |
|--------|------|------|
| `host.info` | — | `uiApiVersion` / `appVersion` / `pluginId` / `cardId` / `platform` |
| `theme.get` | — | 当前生效令牌（shell 直接应答：只有它知道"跟随系统"的结果） |
| `settings.get` | `settings.read` | 语言、主题偏好、默认内存（无路径/凭据） |
| `instances.list` | `instance.read` | 实例概要（id/name/version/loader/memory/lastPlayed） |
| `network.request` | `network.request` + 声明 host | 宿主 HTTP client：GET/POST、无重定向、2 MiB、30 s |

超时 5–30 s（按方法），请求 ≤64 KiB，响应 ≤512 KiB，同插件并发 ≤8；取消有两条路径：
插件主动 `cancel`（SDK 以 `AbortSignal` 暴露）与卡片卸载/客户端 dispose（回 `cancelled`）。
错误码是桥自己的封闭枚举（12 项），不复用应用内 `ErrorCode`——线上契约要稳定。

### 23.2 前端容器

`src/plugin-ui/bridge.ts`（宿主侧传输层，**不做授权**）+ `PluginFrame.tsx`（三态容器）：

- 只接受 `event.source === iframe.contentWindow` 的消息；origin 必须是 `null`（sandbox 不透明源）
  或 `rmcccplugin://`；
- 非字符串、超长、缺 `id/method` 的消息丢弃并记日志；
- 请求超时/卡片卸载时清理计时器并回 `timeout`/`cancelled`；
- `init` 到达才算 ready；8 s 未 ready 显示错误态（插件抛异常时的兜底）。

`WidgetDescriptor.ui_url` 决定渲染路径：有 `uiUrl` → iframe；否则宿主绘制的声明式卡片。

### 23.3 SDK 与工具

```text
plugin-sdk/
├── packages/plugin-api/    # 线协议类型 + 方法表（@rmccc/plugin-api）
├── packages/plugin-ui/     # 运行时 client（@rmccc/plugin-ui，纯 ESM，可 vendored）
├── templates/dashboard-card/
├── scripts/cli.mjs         # validate / pack（pack 调 rmcli，与安装器同规则）
├── test/                   # schema ↔ Rust ↔ SDK 三处方法表/错误码必须一致
└── docs/                   # getting-started / api-reference / security / publishing
```

根 `package.json` 暴露 `pnpm plugin:validate` / `pnpm plugin:pack` / `pnpm sdk:test`。

### 23.4 平台与遗留

- **Windows/Linux**：WebView2 / WebKitGTK 对自定义方案与 CSP 的行为未实测，
  隔离 UI 的跨平台一致性标记 **deferred（release-blocked）**，不得宣称可用。
- **页面容器**：`settingsPages` 已经带 `uiUrl`，但 Settings 里的 iframe 路由留到后续切片；
  当前只在 Dashboard 卡片渲染隔离 UI。
- **不是进程隔离**：iframe 与宿主同 WebView 进程；逻辑插件的进程/WASM 运行时是 Phase 5。

## 24. 逻辑插件运行时（Phase 5）

决策与实测数据见 [ADR-007](adr/ADR-007-logic-runtime.md)。三条硬边界：

1. **主程序不链接 WASM 引擎**。引擎只存在于 helper 进程 `rmccc-logic-host` 里，
   因此 ADR-011 的主可执行文件体积不受影响（实测：嵌入式 wasmtime +5.89 MiB 会直接顶破红线）。
2. **进程隔离**：helper 崩溃/死循环不会带走启动器；超时由宿主 `kill` 子进程强制实现，
   不依赖模块配合。
3. **模块沙箱**：只允许导入 `rmccc.log`，线性内存由 `StoreLimits` 封顶，指令数由 fuel 计量。

### 24.1 组件

| 组件 | 位置 | 职责 |
|------|------|------|
| `rmccc-logic` | `crates/rmccc-logic` | ABI 常量、JSON-RPC 协议、`LogicLimits`、`LogicHost` 监管者（spawn/超时/重启） |
| `rmccc-logic-host` | `src-logic-host` | 唯一链接 wasmi 的二进制：校验导入 → 实例化 → 执行 `rmccc_invoke` |
| PoC | `poc/logic-runtime`（非 workspace 成员） | ADR-007 的体积/延迟/内存实测与复现脚本 |

### 24.2 协议

stdin/stdout，一行一个 JSON：

```text
{"v":1,"id":1,"method":"ping"}
{"v":1,"id":2,"method":"load","pluginId":"…","path":"…/logic/plugin.wasm","limits":{…}}
{"v":1,"id":3,"method":"invoke","pluginId":"…","call":"compat.check","input":{…},"limits":{…}}
{"v":1,"id":4,"method":"unload","pluginId":"…"}
→ {"v":1,"id":3,"ok":true,"result":{…}} | {"v":1,"id":3,"ok":false,"error":{"code":"…","message":"…"}}
```

错误码是封闭枚举（`import_not_allowed` / `module_too_large` / `module_invalid` /
`fuel_exhausted` / `trap` / `timeout` / `payload_too_large` / `not_loaded` / …），
与 UI 桥（§23）同一条原则：**线上格式独立于应用内 `ErrorCode`**。

### 24.3 WASM ABI（v1）

```wat
(import "rmccc" "log" (func (param i32 i32 i32)))            ;; 唯一允许的导入
(func (export "rmccc_alloc")  (param i32) (result i32))      ;; len -> ptr
(func (export "rmccc_invoke") (param i32 i32) (result i64))  ;; in_ptr, in_len -> (out_ptr << 32 | out_len)
```

宿主把 `{"method": <call>, "input": <input>}` 写进线性内存，读取返回值并解析成 JSON。
选择 core module 而非 Component Model：后者需要 wasmtime 的组件工具链，与本阶段的体积预算冲突。

### 24.4 限制与生命周期

| 限制 | 默认 | 上限 |
|------|------|------|
| 单次调用墙钟 | 5 s | 30 s（超时即 kill helper） |
| 线性内存 | 16 MiB | 256 MiB |
| fuel | 5 000 万 | 20 亿 |
| 并发 | 1 | 1（v1 串行，避免共享内存竞争） |
| 模块体积 | — | 2 MiB |
| 载荷 | — | 256 KiB |

一个 helper 同一时刻只加载一个模块（v1）；`load` 会替换上一个。超时/崩溃后
`LogicHost::restart()` 拉起新进程，下一次调用即可继续。

### 24.5 平台与遗留

- Windows/Linux：`wasmi` 是纯 Rust，但进程/管道/kill 语义需原生 runner 验证 → **deferred**。
- helper 的分发（打进 `.app` / 独立安装）与签名在 Phase 7；当前通过 `RMCCC_LOGIC_HOST`
  环境变量或与启动器同目录解析。
- 逻辑插件与权限系统已接线：`entry.logic`（`logic/*.wasm`、≤2 MiB、与 `logic/` 目录双向印证）
  → 能力 `logic.invoke`（medium，需显式授予）→ 命令 `plugin_logic_invoke` / `plugin_logic_cancel`。
  v1 只放行**只读**调用白名单：`compat.check`、`mods.source.search`；每条调用依次检查
  启用状态 → 能力 → 调用白名单 → 入口/路径，撤销在下一次调用即生效。
- helper 尚未打进 `.app`：通过 `RMCCC_LOGIC_HOST` 或与启动器同目录解析；
  打包/签名/公证与三平台 runner 一起留到 Phase 7。

## 25. AI 服务与外部 Agent 控制（Phase 6 方向重定义，ADR-013）

> 决策记录：[ADR-013](adr/ADR-013-lightweight-ai-and-external-mcp.md)（轻干预 · 轻 AI）。
> 完整设计（含 MCP 服务器 30 工具目录、确认流、部署方式）见
> [`AI_AND_MCP_ARCHITECTURE.md`](AI_AND_MCP_ARCHITECTURE.md)；本节只记录已落地形态。
> 历史：原「内建 Agent 平台」方向（执行循环 / 工具注册表 / 会话存储，ADR-010 的会话部分）
> 已于 2026-10-23 退役，当时的全景记录见 [`audits/`](audits/)。

### 25.1 定位与边界

- **启动器不内建 Agent**：AI 编排交给外部工具（ZCode 等，经 MCP 接入；MCP 尚未实现），
  启动器内部只保留一个轻量 OpenAI 兼容 AI 服务。
- **AI 是能力不是主体**：一次 AI 调用 = 发文本 → 拿流式回复；没有循环、没有工具注册表、
  没有会话状态机。
- **不是什么都要 AI**：依赖缺失检测（`mods_dependency_scan`）、一键补装（`mods_apply_plan`）、
  完整性体检（`instance_check_completion/repair`）都是纯本地逻辑 + 网络请求。
- **一次配置全局使用**：端点 / 模型 / 密钥在设置页配置一次（vault service=`llm`），
  启动器内置功能与（未来的）插件桥共用同一凭据。

### 25.2 组件

| 模块 | 职责 |
|------|------|
| `rmccc-llm::llm` | `LlmProvider` trait + `OpenAiCompatibleProvider`（SSE 流、超时、指数退避重试、最小间隔限流、`CancelToken`）+ `probe_models`（`GET {base}/models`） |
| `rmccc-llm::credentials` | `CredentialStore` trait + `FileCredentialStore`（AES-256-GCM，`private/agent-credentials.json` + `private/agent.key`，0600） |
| `rmccc-llm::redact` | 发送 / 落盘前脱敏（敏感键名 / `Bearer` / 凭据前缀 / URL 查询参数 / home 路径 / 邮箱） |
| `rmccc-agent::mods` | 只读扫描 `mods/*.jar` 的 `fabric.mod.json`（`depends`/`suggests`/`breaks`）→ `DependencyReport` + `ModInstallPlan`；`PlanApplier` trait 由宿主注入执行器 |
| `rmccc-agent::audit` | `AuditLog` 追加写（字段白名单 + 脱敏；MCP 服务器落地时启用） |
| `rmccc-agent::confirm` | `TaskSnapshot` 两阶段确认原语（proposed→approved→applied/failed，供 MCP 确认流复用） |
| `src-tauri/commands/ai.rs` | `ai_*` IPC 5 条（配置 / 探测 / 日志分析） |

### 25.3 IPC 与事件

命令 5 条 + Mod 纯逻辑 2 条（详见 §11）：

- `ai_config_get` / `ai_config_set`：配置视图永不回传 key；set 字段可选，
  留空沿用已存值（vault 键 `llm` 沿用 agent 时代的文件名，旧配置无缝迁移）。
- `ai_models_probe`：`GET {base}/models` 自动检测可用模型；未传参时用已存配置。
- `ai_analyze_log` / `ai_analyze_cancel`：取实例 `logs/latest.log` 尾部 ≤ 12,000 字符
  → `redact`（home → `~`、凭据遮蔽）→ `chat_stream` → 事件
  `ai://delta` / `ai://done` / `ai://error`（payload 键 `analysisId`）。
  并发上限 2、整体超时 180s；返回 `{analysisId, sentLines}` 保持外发透明。
- `mods_dependency_scan` / `mods_apply_plan`：纯逻辑依赖扫描与一键补装
  （宿主 `ModrinthPlanApplier`：搜索 → game+loader 版本匹配 → SHA1 校验下载）。

### 25.4 数据与隐私

- 密钥只存 vault（service=`llm`）；`AiConfigView` 只暴露 `hasKey` 布尔。
- 日志发送前强制脱敏；错误信息不包含密钥。
- 远程端点必须 HTTPS；本地模型仅允许 loopback HTTP（ADR-009 决策 5）。
- `agent/audit.jsonl` 审计结构与 ADR-010 的脱敏 / 保留规则继续有效，
  当前无生产写入方（MCP S4/S5 落地时启用）；会话 / 任务存储已退役。

### 25.5 MCP 服务器（S4/S5，已落地）与遗留

- **服务器**：`src-tauri/src/mcp/`——Streamable HTTP（JSON 模式），仅回环 +
  Bearer Token（`<data>/mcp-token` 0600）；默认**关闭**，`mcp_set_enabled` 启停。
  传输基于 hyper 1.x（reqwest 依赖树已有，启用 server feature；未引入 axum/tower）。
- **31 工具**：只读 15 / 操作 11 / 破坏性 5；dispatch 直接调用与 GUI 同一份
  `commands::*`（下载/启动事件天然可见），操作类另发 `mcp://call` 归因事件。
- **确认流**：破坏性工具复用 `rmccc_agent::confirm::TaskSnapshot` 两阶段语义
  （`kind = mcp:<tool>`），GUI 经 `mcp_confirmation_resolve` 放行/拒绝，
  120s 超时即拒；破坏性操作审计 fail-closed。
- **策略**：默认 按风险（只读/操作 allow，破坏性 confirm），可逐工具覆盖
  （`<data>/mcp-policies.json`）；会话 ≤ 4，60 次/分钟/会话，
  `instance.check_completion` 全局并发 1。
- **stdio 桥**：`rmccc mcp [--launch]`（src-cli）——stdin/stdout MCP 转发到
  HTTP 端点（`<data>/mcp-endpoint`），启动器未运行时返回明确 JSON-RPC 错误。
- **S6 插件桥**：能力 `ai.chat`（medium）+ 桥方法 `llm.chat`（限流 6/min/插件，
  流式经 `plugin://push` 中继 `llm.delta/done/error`，密钥由宿主代持）。
- **随前端重构落地**：设置页 MCP 分区、确认卡、归因徽标/toast（`mcp://call`
  与 `mcp://confirmation` 的消费方）、`plugin://push` 的 iframe 中继、
  事件 payload `origin` 字段（当前以 `mcp://call` 替代）。
- Windows/Linux：vault 的 `restrict_owner_only` 为 `UnsupportedPlatform`
  → **deferred / release-blocked**（权限语义与账号 vault 相同）。
- 旧面清理：`/agent` 路由重定向到 `/settings`；`agent_*` 9 条 IPC 移除，
  前端无引用（`ipc_tests` 计数断言 93）。

## 26. Native-resolution wallpaper engine (ADR-020)

Local MP4 wallpaper uses the system WebView as the GPU decoder and compositor;
there is no bundled multimedia runtime and no custom native texture pipeline.
The design supersedes ADR-017's 1080p H.264 proxy.

| Layer | Responsibility |
|-------|----------------|
| `rmccc-platform::video` shared engine | MP4 probe, direct/transcode decision, recipe-keyed cache, unique staging and strict output validation |
| macOS backend | `avconvert PresetHEVCHighestQuality`, native-resolution HEVC |
| Windows backend | WinRT `MediaTranscoder`, native-resolution HEVC, hardware acceleration required |
| Tauri `background_video_prepare` | Validate source, prepare on a blocking worker, grant the returned asset and return one DTO |
| Tauri `background_image_prepare` | Validate a still image (format/size), grant the exact asset path and return it |
| SolidJS `BackgroundVideo` | One persistent muted/looping/inline video source, composited into a DOM canvas at display resolution; frame watchdog only for decode stalls |
| SolidJS `BackgroundImage` | One static `object-fit: cover` image layer above the video; no copy or image runtime |

H.264 at or below 4096×2304 and HEVC play directly. Oversized H.264 is converted
to HEVC without changing dimensions or timing. Validation requires HEVC, exact
source width/height, exact rational frame rate and frame count; duration rounding
is bounded to 200 ms. Preparation errors are visible and do not fall back to a
known software-decoding source. Unsupported MP4 codecs are explicit errors.

The single video prepare IPC removes the separate asset-allow step. Frontend
preparation starts during bootstrap and deduplicates concurrent calls by source
path. The old poster/IndexedDB cache, proxy-upgrade source swap, route compositor
nudges, focus recovery timers and forced `load()` reset are prohibited by ADR-020.

A still-image wallpaper reuses the asset-scope contract through
`background_image_prepare`: the backend canonicalizes the file, accepts PNG/JPEG/
WebP/GIF/BMP/AVIF up to 32 MiB and grants only that path. `BackgroundImage` layers
above the video, sampling it first for the adaptive palette. Image and video are
mutually exclusive in Appearance; enabling one turns the other off. No image
codec runtime or frontend bitmap cache is added.

Native macOS feasibility is verified with 4126×2596 HEVC and VideoToolbox.
Windows HEVC codec/GPU/driver availability remains a runtime prerequisite;
native Windows compilation and WebView2 GPU validation are deferred /
release-blocked under platform guide §1.4.

