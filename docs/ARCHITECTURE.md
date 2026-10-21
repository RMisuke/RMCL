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
├── docs/                         # Architecture & planning docs
│   ├── ARCHITECTURE.md
│   ├── ARCHITECTURE_IMPROVEMENT_PLAN.md
│   ├── AI_EDITING_RULES.md
│   └── DEVELOPMENT_PLAN.md
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
│   │       ├── lib.rs            # ModLoaderProvider + DependencyManager
│   │       ├── analyzer.rs       # Loader detection from libraries
│   │       ├── patch.rs          # Shared GameInstancePatch type
│   │       ├── fabric/  forge/  neoforge/  quilt/
│   │       └── liteloader/  optifine/  cleanroom/  legacyfabric/
│   ├── rmccc-mods/               # Mod management (Modrinth implemented)
│   │   └── src/
│   │       ├── lib.rs            # ModSource trait + data types
│   │       └── modrinth.rs       # Modrinth search / versions / download
│   ├── rmccc-profile/            # Profile/instance management
│   │   └── src/lib.rs            # ProfileStore JSON CRUD + launch options
│   ├── rmccc-platform/           # 平台适配层（Phase 1，指南 §5.7）
│   │   └── src/                  # descriptor / fs / process / shell / java
│   │                             # macOS 后端 + Windows/Linux 占位 + mock
│   ├── rmccc-plugin/             # 声明式插件系统（Phase 2，ADR-003/004）
│   │   └── src/                  # manifest / version / capability / package /
│   │                             # installer / registry / permissions /
│   │                             # config / manager
│   └── rmccc-core/               # Core orchestrator + DI container
│       └── src/
│           ├── lib.rs            # CoreContext, re-exports
│           └── pipeline.rs       # install / repair / launch orchestration
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
├── rmccc-auth                  # → error
└── rmccc-mods                  # → error, download

rmccc-version    → error, download
rmccc-modloader  → error, download, version
rmccc-profile    → error, config, modloader
rmccc-launch     → error, config, version, profile
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

#### ModLoaderProvider

```rust
#[async_trait]
pub trait ModLoaderProvider: Send + Sync {
    fn loader_type(&self) -> ModLoaderType;

    /// List available loader versions for a game version
    async fn list_versions(&self, game_version: &str) -> Result<Vec<LoaderVersion>, RmcccError>;

    /// Install the loader, returns the version ID used as --version
    async fn install(
        &self,
        game_version: &str,
        loader_version: &str,
        game_dir: &Path,
        progress: ProgressCallback,
    ) -> Result<String, RmcccError>;
}

// Implementations:
// - ForgeInstaller     (Forge installer JAR execution / slim installer)
// - FabricInstaller    (Fabric meta API + profile generation)
// - NeoForgeInstaller  (NeoForge installer)
```

#### ModSource

```rust
#[async_trait]
pub trait ModSource: Send + Sync {
    fn source_id(&self) -> &str;

    /// Search mods by query and filters
    async fn search(&self, query: &ModSearchQuery) -> Result<ModSearchResult, RmcccError>;

    /// Get detailed mod info by slug/ID
    async fn get_mod(&self, id: &str) -> Result<ModDetail, RmcccError>;

    /// List available versions of a mod
    async fn list_versions(&self, mod_id: &str) -> Result<Vec<ModVersion>, RmcccError>;

    /// Get download URL for a specific version
    async fn get_download_url(&self, version_id: &str) -> Result<String, RmcccError>;
}

// Implementations:
// - ModrinthSource     (Modrinth v2 API, open, no key needed)
// - CurseForgeSource   (CurseForge Core API, API key from env or key file)
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
/// A game instance (profile)
pub struct Profile {
    pub id: String,                  // UUID
    pub name: String,
    pub game_version: String,        // e.g. "1.20.4"
    pub mod_loader: Option<ModLoaderConfig>,
    pub java_path: Option<String>,   // auto-detect if None
    pub jvm_args: Vec<String>,       // extra JVM args
    pub game_args: Vec<String>,      // extra game args
    pub memory_mb: u32,              // heap size
    pub game_dir: PathBuf,           // instance directory
    pub created_at: DateTime<Utc>,
    pub last_played: Option<DateTime<Utc>>,
    pub icon: Option<String>,        // custom icon path
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
| 全局默认值 | `rmccc_config::AppConfig` | `config/launcher-settings.json` | 所有可继承字段的回退值；`config` 是叶子 crate，不依赖 version |
| 实例值 + 覆盖名单 | `rmccc_profile::Profile` | `profiles/{id}.json` | `override_properties` 列出真正采用实例值的字段；其余字段继承全局 |
| 启动生效值 | `rmccc_profile::ResolvedInstanceSettings` | 不落盘 | `Profile::resolve(&AppConfig)` 的产物，`LaunchOptions::from_profile` 的唯一输入 |

**关键取舍（照抄 HMCL `overrideProperties`）**：字段的"值"与"是否覆盖"分开存。
`override_properties` 不含某字段时该字段继承全局，但实例里已填的值**仍然保留在磁盘上**，
因此用户在实例设置页来回切换"继承/独立"不会丢掉已输入的内容。
`rmccc_profile::fields::ALL` 用常量列出全部可覆盖字段名，供后端校验、UI 与测试共用，
避免三处各写一份字符串。

> 与 HMCL 的差异：HMCL 把实例这半放在 `{instance_root}/.hmcl/config/instance-game-settings.json`，
> 且全局侧是**可多套的"游戏设置预设"**（实例用 `parent` 引用其一）。RMCCC 目前只有一套全局默认值，
> 实例这半存在 `profiles/{id}.json`——因为 RMCCC 的实例注册表就是 ProfileStore，而不是扫描 `versions/`；
> 设置随实例复制/删除的语义不变。多预设是后续可选扩展。

**隔离（"独立实例文件夹"）**：`isolation` 取 `never` / `modded` / `always`（默认 `modded`，同 HMCL）。
它决定**运行目录**，而共享游戏根不变：

| 目录 | 内容 | 何时不同 |
|------|------|----------|
| `game_dir`（共享游戏根） | `versions/`、`libraries/`、`assets/`、natives、实例根 | 始终共享 |
| `run_dir`（运行目录） | `--gameDir`、进程工作目录、`${game_directory}`、`INST_MC_DIR`、`mods/`、`saves/`、`options.txt` | 实例被隔离时 = `versions/{id}` |

`rmccc_launch::instance_run_dir` / `instance_mods_dir` 是唯一的推导入口，
`LaunchOptions::from_profile` 与模组命令都经它，避免"安装目录与启动目录不一致"这类事故重现。
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

    /// Running game processes (profile_id → child process)
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
            downloads: Arc::new(DashMap::new()),
            running_games: Arc::new(DashMap::new()),
            vault: Arc::new(rmccc_auth::TokenVault::init(&data_dir.join(".key"))?),
        })
    }
}
```

### 8.2 Multi-Instance Policy

- **允许**：同一时间运行多个 MC 实例（不同 profile）
- **限制**：同一 profile 不可同时运行两个实例（`running_games` map 检查）
- **内存预检**：启动前估算所需内存，超出系统可用内存时警告

### 8.3 Download Control Granularity

All three IPC commands operate on a **batch** (`batch_id`); there is no
per-file cancel command.

| IPC command | Scope | Behavior |
|-------------|-------|----------|
| `download_cancel` | 整批 | 取消整批（`cancel_batch(batch_id)`），清理已下载的 partial 文件 |
| `download_pause` | 整批 | 等待当前 worker 完成，停止 dequeuing 新任务 |
| `download_resume` | 整批 | 从断点继续（HTTP Range） |

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

// stores/download.ts — tracks all active downloads
export function createDownloadStore() {
  const [tasks, setTasks] = createStore<Map<string, DownloadTask>>(new Map());

  // Listen to Tauri events for progress (throttled at 100ms on Rust side)
  onMount(() => {
    listen<DownloadProgress>("download://progress", (event) => {
      setTasks(event.payload.taskId, (prev) => ({
        ...prev,
        progress: event.payload.progress,
      }));
    });
  });

  return { tasks };
}
```

### 9.3 IPC Type Bridge (Auto-Generated Types + Manual API Wrapper)

```typescript
// ipc/types.ts — AUTO-GENERATED by ts-rs, do not edit manually
// Rust structs with #[derive(TS)] produce matching TypeScript interfaces:
// interface Profile { id: string; name: string; ... }
// interface IpcError { code: ErrorCode; message: string; retryable: boolean; }
// etc.

// ipc/api.ts — MANUAL API wrapper, imports types from types.ts
import { invoke } from "@tauri-apps/api/core";
import type { AuthSession, Profile, VersionInfo, ... } from "./types";

// Auth
export const auth = {
  microsoftLogin: () => invoke<AuthSession>("auth_microsoft_login"),
  offlineLogin: (username: string) => invoke<AuthSession>("auth_offline_login", { username }),
  logout: () => invoke<void>("auth_logout"),
  getSession: () => invoke<AuthSession | null>("auth_get_session"),
};

// Profiles
export const profiles = {
  list: () => invoke<Profile[]>("profile_list"),
  create: (opts: CreateProfileOpts) => invoke<Profile>("profile_create", { opts }),
  delete: (id: string) => invoke<void>("profile_delete", { id }),
  update: (id: string, opts: UpdateProfileOpts) => invoke<void>("profile_update", { id, opts }),
  import: (filePath: string) => invoke<Profile>("profile_import", { filePath }),
  export: (id: string, destPath: string) => invoke<void>("profile_export", { id, destPath }),
};

// Versions
export const versions = {
  list: () => invoke<VersionInfo[]>("version_list"),
  install: (version: string, profileId: string) => invoke<void>("version_install", { version, profileId }),
};

// Mod Loaders (unified)
export const modloaders = {
  list: (loaderType: string, gameVersion: string) =>
    invoke<LoaderVersion[]>("modloader_list", { loaderType, gameVersion }),
  install: (profileId: string, loaderType: string, loaderVersion: string) =>
    invoke<void>("modloader_install", { profileId, loaderType, loaderVersion }),
};

// Mods
export const mods = {
  search: (query: string, source: string) => invoke<ModSearchResult>("mod_search", { query, source }),
  install: (profileId: string, modId: string, versionId: string) =>
    invoke<void>("mod_install", { profileId, modId, versionId }),
  remove: (profileId: string, modId: string) => invoke<void>("mod_remove", { profileId, modId }),
  list: (profileId: string) => invoke<InstalledMod[]>("mod_list", { profileId }),
  toggle: (profileId: string, modId: string, enabled: boolean) =>
    invoke<void>("mod_toggle", { profileId, modId, enabled }),
};

// Launch
export const launch = {
  start: (profileId: string) => invoke<void>("launch_start", { profileId }),
  getStatus: () => invoke<LaunchStatus>("launch_status"),
  kill: (profileId: string) => invoke<void>("launch_kill", { profileId }),
};

// Downloads (all batch-scoped)
export const downloads = {
  cancel: (batchId: string) => invoke<void>("download_cancel", { batchId }),
  pause: (batchId: string) => invoke<void>("download_pause", { batchId }),
  resume: (batchId: string) => invoke<void>("download_resume", { batchId }),
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
    // Download
    pub const DOWNLOAD_PROGRESS: &str = "download://progress";
    pub const DOWNLOAD_COMPLETE: &str = "download://complete";
    pub const DOWNLOAD_ERROR: &str = "download://error";
    pub const DOWNLOAD_PAUSED: &str = "download://paused";
    pub const DOWNLOAD_RESUMED: &str = "download://resumed";

    // Auth
    pub const AUTH_STATE_CHANGED: &str = "auth://state-changed";
    pub const AUTH_DEVICE_CODE: &str = "auth://device-code";

    // Launch
    pub const LAUNCH_STARTED: &str = "launch://started";
    pub const LAUNCH_OUTPUT: &str = "launch://output";    // stdout/stderr stream
    pub const LAUNCH_EXITED: &str = "launch://exited";

    // Mod Loader
    pub const MODLOADER_INSTALL_PROGRESS: &str = "modloader://progress";
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
  ├─ Rust: ModLoaderProvider::list_versions(game_version)
  │   └─ Fabric: GET https://meta.fabricmc.net/v2/versions/loader/{gameVersion}
  │   └─ Forge:  GET https://files.minecraftforge.net/net/minecraftforge/forge/promotions_slim.json
  │
  ├─ Rust: ModLoaderProvider::install(game_version, loader_version, game_dir)
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

46 commands, all registered in `src-tauri/src/main.rs` `generate_handler!`.
Definition count and registration count are verified equal (2026-10-20 实测：
`grep -c 'commands::' main.rs` = 46，与 `commands/*.rs` 中 `pub fn` 数一致).

| Command | Handler | Description |
|---------|---------|-------------|
| `auth_microsoft_login` | `auth.rs` | Start MS OAuth2 device flow (emits `auth://device-code`) |
| `auth_offline_login` | `auth.rs` | Create offline session |
| `auth_logout` | `auth.rs` | Clear session + remove private data |
| `auth_get_session` | `auth.rs` | Decrypt + return cached session |
| `auth_refresh_token` | `auth.rs` | Silent refresh with a profile-UUID consistency check |
| `auth_list_accounts` | `auth.rs` | List accounts (plaintext metadata only) |
| `auth_switch_account` | `auth.rs` | Change the selected account |
| `auth_remove_account` | `auth.rs` | Remove an account and its private data |
| `profile_list` | `profile.rs` | List all profiles |
| `profile_create` | `profile.rs` | Create new profile |
| `profile_update` | `profile.rs` | Update profile settings |
| `profile_delete` | `profile.rs` | Delete profile + cleanup game dir |
| `instance_list` | `instance.rs` | List instances from the local repository index |
| `instance_refresh` | `instance.rs` | Rescan `versions/`, emit `instances://refreshed` |
| `instance_rename` | `instance.rs` | Rename an instance |
| `instance_duplicate` | `instance.rs` | Duplicate an instance (jar + patched JSON) |
| `instance_remove` | `instance.rs` | Remove an instance |
| `instance_check_completion` | `instance.rs` | Report missing files |
| `instance_repair` | `instance.rs` | Download missing files |
| `instance_paths` | `instance.rs` | Effective game root / run dir / mods dir (depends on isolation) |
| `version_list` | `version.rs` | List available MC versions (with cache) |
| `version_install` | `version.rs` | Install MC version; returns a real `batch_id` |
| `modloader_list` | `modloader.rs` | List loader versions (`loader_type` param) |
| `modloader_install` | `modloader.rs` | Install mod loader (emits install progress) |
| `mod_search` | `mods.rs` | Search mods (Modrinth) |
| `mod_get_versions` | `mods.rs` | List versions of a mod |
| `mod_get_download_url` | `mods.rs` | Resolve a download URL |
| `mod_install` | `mods.rs` | Download + install mod (remote or local file) |
| `mod_remove` | `mods.rs` | Remove mod from profile |
| `mod_list` | `mods.rs` | List installed mods (active + disabled) |
| `mod_toggle` | `mods.rs` | Enable/disable mod (move to/from `mods/disabled/`) |
| `launch_start` | `launch.rs` | Launch game through `DefaultLauncher` |
| `launch_kill` | `launch.rs` | Kill game process (process group on Unix) |
| `launch_status` | `launch.rs` | Get running game status |
| `download_cancel` | `download.rs` | Cancel an entire download batch |
| `download_pause` | `download.rs` | Pause download batch |
| `download_resume` | `download.rs` | Resume paused download batch |
| `settings_get` | `settings.rs` | Get global settings |
| `settings_set` | `settings.rs` | Update global settings (debounced `FileSaver` write) |
| `settings_get_user` | `settings.rs` | Get user-level settings (UI language, update check) |
| `settings_set_user` | `settings.rs` | Update user settings; `locale: ""` = follow the system |
| `settings_supported_locales` | `settings.rs` | List shipped UI languages with their native names |
| `java_detect` | `settings.rs` | Auto-detect Java installations |
| `log_get` | `logs.rs` | Get recent launcher log lines |
| `log_clear` | `logs.rs` | Clear/truncate log file |
| `open_external_url` | `external.rs` | Open a URL in the system browser (deep-link) |

> 前端现状（2026-10-20 实测）：**UI 可直达 27 个**（Dashboard 11 / InstanceSettings 8 /
> Login 3 / Settings 5）；另有 15 个只在 `src/ipc/api.ts` 有 wrapper 但无页面调用
> （`mod_search` 等 4 个、`download_pause/resume`、4 个账户管理、`log_get/clear`、
> `instance_check_completion/repair/refresh`）；`instance_list/duplicate/rename/remove`
> 连 wrapper 都没有——它们操作的是"版本 id"而非 profile，见审计文档 §7.4。

## 12. Size Budget

### 12.1 Target: < 10MB Total

| Component | Target Size | Notes |
|-----------|------------|-------|
| Tauri 2.x binary (stripped + LTO) | ~4.5 MB | Rust code + WebView glue |
| SolidJS frontend (gzipped) | ~80 KB | JS + Tailwind CSS |
| Static assets (icons) | ~100 KB | PNG icons, splash |
| **Total** | **~4.7 MB** | **Well under 10MB** |

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
│   └── launcher-settings.json       # Global settings ($schema versioned)
├── state/
│   └── launcher-state.json          # Runtime state (loss-tolerant)
├── user-settings.json               # User-level preferences
├── .key                             # AES-256 encryption key (600 / icacls)
├── accounts.json                    # Plaintext account metadata + selected
├── private/
│   └── account-private-data.json    # AES-256-GCM encrypted private data
├── .api_keys/                       # API keys (separate from settings)
│   └── curseforge                   # CurseForge API key
├── cache/                           # Offline caches
│   ├── versions.json                # Cached version list (TTL)
│   └── objects/{sha1[0:2]}/{sha1}   # SHA1-keyed download cache
├── profiles/                        # Profile definitions
│   ├── {profile-id-1}.json
│   └── {profile-id-2}.json
├── instances/                       # Default game root (.minecraft equivalent)
│   ├── versions/                    # Local instance index source
│   ├── libraries/  assets/          # Shared across instances
│   └── launcher_profiles.json       # Created on demand (Forge/LiteLoader need it)
└── logs/                            # Launcher logs
    ├── rmccc-2026-07-29.log         # Daily rolling, 10 kept
    └── rmccc-2026-07-28.log

{game_dir}/                          # Per-profile game directory
├── versions/
│   └── 1.20.4/
│       ├── 1.20.4.json              # Version metadata
│       ├── 1.20.4.jar               # Client JAR
│       ├── natives-{platform}/      # Extracted natives (HMCL layout)
│       └── .abnormal                # Crash marker, cleared on normal exit
├── libraries/                       # Shared libraries
│   └── com/google/...
├── assets/                          # Game assets
│   ├── indexes/
│   └── objects/
├── mods/                            # Installed mods
│   ├── sodium-0.5.5.jar
│   ├── lithium-0.12.0.jar
│   └── disabled/                    # Disabled mods (toggled)
│       └── iris-1.7.0.jar.disabled
├── resourcepacks/
└── logs/
```

### 15.2 Profile JSON Schema

> `game_dir` 指向**共享游戏根**（`.minecraft` 等价目录），版本/依赖库/资源在其中只存一份、
> 跨实例复用；安装、`GameRepository` 索引与启动器一律以该目录为准。
> 每实例隔离属于 `IsolationType`，只影响运行目录（存档/mods/options.txt）。

```json
{
  "id": "550e8400-e29b-41d4-a716-446655440000",
  "name": "Survival Pack",
  "game_version": "1.20.4",
  "mod_loader": {
    "loader_type": "fabric",
    "loader_version": "0.15.6"
  },
  "isolation": null,
  "override_properties": ["java_path", "memory_mb"],
  "java_path": null,
  "jvm_args": ["-XX:+UseG1GC", "-XX:+ParallelRefProcEnabled"],
  "game_args": [],
  "memory_mb": 4096,
  "game_dir": "/Users/xxx/Library/Application Support/rmccc/instances",
  "created_at": "2026-07-29T10:00:00Z",
  "last_played": null,
  "icon": null
}
```

> `isolation`（`never`/`modded`/`always`，`null` = 跟随全局 `default_isolation`）与
> `override_properties`（覆盖名单，见 §3.4）是继承模型的一部分：名单内的字段取实例值，
> 其余取全局默认值；**实例里填过的值即使不覆盖也保留在文件中**。
>
> 扩展字段（全部 `#[serde(default)]`，旧 JSON 可无损加载）：`min_memory_mb`、
> `metaspace_mb`、`process_priority`、`width` / `height` / `fullscreen`、
> `use_native_glfw` / `use_native_openal` / `use_custom_natives`、
> `no_generated_jvm_args` / `no_generated_optimizing_jvm_args`、
> `enable_debug_log_output`、`wrapper`、`java_agents`、`override_jvm_args`、
> `pre_launch_command`、`post_exit_command`、`env_vars`。

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
Electron bundles Chromium (~150MB+). Tauri uses system WebView, keeping binary at ~4-5MB. For a 10MB target, this is non-negotiable.

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

### 21.3 渲染与交互

- 前端 CSS Grid（`repeat(columns, 1fr)`，120px 行高）；`grid-column/row` 由 `position/size` 计算。
- 断点：≥1280 wide（声明列数）/ 768–1279 medium（≤8）/ <768 compact（≤4）。
- 卡片头：拖动（HTML5 drag/drop）、宽度循环、隐藏；工具栏：添加插件卡片、恢复隐藏、恢复默认。
- 每卡片 `ErrorBoundary`；插件卸载/禁用后 descriptor 缺失 → 保位占位。

### 21.4 主题

- `--rmccc-*` 令牌（background/surface/surfaceElevated/text/textMuted/accent/accentContrast/border），
  同时桥接旧 `--surface-*`/`--text-*` 变量。
- 内置 light/dark + 已启用插件的 `contributes.themes`（JSON，无 JS）；偏好存入 `UserSettings.theme`。
- `follow_system` 监听 `prefers-color-scheme`；未知/无效主题回退安全主题。
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
