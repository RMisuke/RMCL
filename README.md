# RMCCC — Rust Minecraft Core Client

> 纯 Rust 实现的 Minecraft 启动器（RMCL / RMCLI，显示名 SNGL）。跨平台、轻量（strip 后主可执行 < 20 MiB，ADR-011）、可扩展。

RMCCC（Rust Minecraft Core Client）是一个使用 **Rust** 编写的 Minecraft 启动器，采用 **Tauri 2.x + SolidJS** 构建桌面前端，并提供完整功能的命令行工具 `rmcli`。项目按 `docs/DEVELOPMENT_PLAN.md` 的 Phase 1→5 规划推进，所有架构决策记录于 `docs/ARCHITECTURE.md`。

## 主要功能

- **认证**：Microsoft OAuth2（设备码流程）与离线模式登录，会话持久化。
- **实例管理**：创建、列出、删除游戏实例，每个实例独立版本与加载器。
- **资源管理**：游戏文件完整性检查、按需下载、SHA1 校验与自动修复。
- **模组加载器**：安装/列出版本，支持 Fabric、Forge、NeoForge、Quilt、LiteLoader、OptiFine、LegacyFabric（Cleanroom 仍为 stub，未在 UI 暴露）。
- **智能启动**：Java 版本自适应（`find_java` 自动选择最接近的兼容版本）、响应文件（Response file）传递 JVM 参数防止 `ps aux` 泄露 token、`--add-opens`/`--add-exports` 可重复参数正确保留。
- **桌面 GUI（Tauri）**：SolidJS 前端，主页（hero 双栏 + 主题氛围）/ 登录 / 设置 / 实例设置 / Mod 浏览器 / 插件页面，通过 IPC 命令调用后端。
- **声明式插件（Phase 2）**：`.rmcpkg` 包（无第三方代码）、权限审查与审计、生命周期状态机、schema 校验配置；`rmcli plugin …` 与 `/plugins` 页面。
- **主题引擎（Phase 3）**：8 token 主题 schema、内置 8 套主题 + 插件主题令牌、明暗/跟随系统；原 12 列拖放网格已由 hero 双栏主页取代（ADR-005 主题体系不变）。

## 技术栈

| 层 | 技术 |
|----|------|
| 语言 | Rust（workspace 多 crate）、TypeScript / SolidJS |
| 桌面壳 | Tauri 2.x（系统 WebView，约 4-5MB） |
| 前端 | SolidJS + Tailwind CSS v4（utility-first，tree-shaking） |
| 类型桥接 | ts-rs（Rust struct → TypeScript interface 自动生成） |
| 认证加密 | AES-256-GCM + 本地 key file |
| 日志 | tracing + tracing-appender（结构化日志） |
| 版本存储 | JSON 文件（可调试，后续可迁移 SQLite） |

## 项目目录结构

```
RMCL/
├── AGENTS.md               # AI 协作规范（顶层指引）
├── Cargo.toml              # Rust workspace 清单
├── package.json            # 前端（SolidJS）工程配置
├── docs/                   # 架构与计划文档（分类索引见 docs/README.md）
│   ├── ARCHITECTURE.md     # 完整架构设计（最高权威）
│   ├── AI_AND_MCP_ARCHITECTURE.md  # AI 服务 + 外部 Agent 控制（ADR-013）
│   ├── DEVELOPMENT_PLAN.md # 开发路线图（Phase 1-5）
│   ├── adr/ audits/ guides/ roadmap/ ui-design/ archive/
├── crates/                 # Rust workspace crates
│   ├── rmccc-auth/         # 认证（Microsoft OAuth2 / 离线）
│   ├── rmccc-config/       # 配置管理与默认路径
│   ├── rmccc-core/         # 核心上下文（Context）
│   ├── rmccc-download/     # 并发下载与批量任务
│   ├── rmccc-error/        # 错误码体系
│   ├── rmccc-launch/       # Java 检测、响应文件、进程管理
│   ├── rmccc-modloader/    # Mod 加载器安装
│   ├── rmccc-mods/         # Mods 相关 API
│   ├── rmccc-profile/      # 实例（Profile）管理
│   ├── rmccc-platform/     # 平台适配层
│   ├── rmccc-plugin/       # 声明式插件系统
│   ├── rmccc-dashboard/    # Dashboard 布局与主题
│   ├── rmccc-logic/        # 逻辑插件协议（ADR-007）
│   ├── rmccc-llm/          # OpenAI 兼容 AI 客户端（ADR-013）
│   ├── rmccc-agent/        # 依赖扫描/补装 + 审计 + 确认原语（ADR-013 瘦身后）
│   └── rmccc-version/      # Minecraft 版本元数据、库解析
├── src/                    # SolidJS 前端源码
├── src-cli/                # CLI 二进制（rmcli）
└── src-tauri/              # Tauri 应用层（IPC 命令、配置、图标）
```

## 环境要求

- **Rust**：stable toolchain（aarch64-apple-darwin / x86_64 均可）。
- **Java**：游戏运行时（启动时自动检测，支持 8/17/21/25，按游戏版本自动选择，如 MC 1.20.1 推荐 Java 17）。
- **Node.js + pnpm**：仅前端开发/构建需要，纯 CLI 使用无需安装。

## 安装方式

```bash
# 方式一：安装 CLI 到 PATH（生成 ~/.cargo/bin/rmcli）
cargo install --path src-cli

# 方式二：仅构建 release 二进制（target/release/rmcli）
cargo build --release -p rmccc-cli

# 构建桌面应用（Tauri）
cargo build --release -p rmccc-app   # 或使用 pnpm tauri dev / build
```

## 使用方法

`rmcli` 提供 6 个子命令：`auth`、`instance`、`resource`、`launch`、`config`、`modloader`。不带子命令启动时自动进入交互式 REPL 模式。

```bash
# 认证
rmcli auth login                     # 选择 Microsoft OAuth2 或离线登录
rmcli auth status                    # 查看登录状态
rmcli auth logout

# 实例管理
rmcli instance create -n MyMC -v 1.20.1 -l fabric
rmcli instance list
rmcli instance delete -i <instance_id>

# 资源下载与校验
rmcli resource check -i <instance_id>
rmcli resource download -i <instance_id> -v 1.20.1
rmcli resource verify -i <instance_id>

# 模组加载器
rmcli modloader list -l fabric -g 1.20.1
rmcli modloader install -p <instance_id> -l neoforge -lv 47.1.105

# 启动游戏
rmcli launch run -i <instance_id>

# 配置
rmcli config get
rmcli config set mirror_url "https://bmclapi2.bangbang93.com"

# 交互模式
rmcli
```

> 提示：可用 `-d <data_dir>` 覆盖数据目录（测试与多环境切换时使用），`-i` 强制进入交互模式。

## 已支持的游戏版本与 Loader

- **游戏版本**：通过 Mojang 官方 manifest 获取全部版本；实测覆盖 26.2（最新）、1.20.1、1.12.2 等。
- **加载器**：Fabric、Forge、NeoForge、Quilt、LiteLoader、OptiFine、LegacyFabric（Cleanroom stub）。
- **NeoForge 1.20.1 专项适配**：版本号标准化（`1.20.1-forge-XX` → `XX`）、`META-INF/NEOFORGE.RSA` 签名检测、`net.neoforged:forge` Maven 坐标切换，与 HMCL 行为对齐。

## 测试与验证结果摘要

以下为 2026-07-30 CLI 全功能测试的摘要（完整日志已随历史文档清理，可在 git 历史查阅；当前测试现状见 [`docs/PROJECT_STATUS.md`](docs/PROJECT_STATUS.md)）。关键结论：

| 指标 | 结果 |
|------|------|
| 单元/集成测试 | **468 passed / 0 failed**（含插件 74 项、IPC 契约 37 项） |
| 编译 | `cargo build --release` 通过；`strip = true` 仍因缺 `libLLVM.dylib` 告警，需手动 strip（ADR-011） |
| 全量安装 | `cargo install --path src-cli` 成功，`rmcli` 位于 `~/.cargo/bin` |
| 安装后验证 | 版本/帮助、实例列表、认证状态、Fabric/NeoForge 版本列表均正常 |
| Bug 统计 | 累计发现 19 个，已修复 18 个（修复率 94.74%） |
| 端到端启动 | Fabric / NeoForge（26.2）可正常启动；NeoForge 1.20.1 ModLauncher 启动成功 |

### 关键修复记录（2026-07-30 ~ 07-31）

- **Bug #15**：`find_java(17)` 误选 Java 25，导致 NeoForge 1.20.1 模块路径溢出 → 改为选择最接近的兼容版本，并修复 vendor 名含空格的解析。
- **Bug #17/#18/#19**：Forge profile 测试 JSON 序列化、NeoForge 版本号标准化、`NEOFORGE.RSA` 签名检测，均与 HMCL 实现对齐。

## 已知问题与后续计划

- **NeoForge 1.20.1 游戏窗口**：ModLauncher 启动成功后，NeoForge 内部 `ImmediateWindowHandler` 在 macOS 上抛出 NPE（NeoForge 自身兼容性问题，与启动器无关），作为已知限制记录（Bug #16）。
- **NeoForge 双 Maven API**：当前通过 BMCLAPI 代理获取版本，后续可对接 `net/neoforged/forge` + `net/neoforged/neoforge` 双端点。
- **Forge/NeoForge 安装路径**：NeoForge 已改为进程内逐 processor 执行（对齐 HMCL，含 maven 内嵌提取与依赖并发下载）；Forge 仍走 `java -jar installer.jar` 整包安装。
- **体积红线**：strip 后的主可执行文件 < **20 MiB**（ADR-011，2026-10-24 由 10 MiB 放宽）；新增依赖须评估体积，`strip = true` 于 2026-10-26 实测生效。
- **后续 Phase**：Phase 0–5 已完成；AI/MCP S1–S7 后端完成；剩余 **Phase 7**（插件目录/签名、平台产物与分发）需先落 ADR-008，见 `docs/roadmap/09_phase7_distribution.md`。

## 文档索引

> 完整分类索引见 [`docs/README.md`](docs/README.md)。

- [`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md) — 设计基线（架构决策的权威来源，IPC 表 84 条）
- [`docs/AI_AND_MCP_ARCHITECTURE.md`](docs/AI_AND_MCP_ARCHITECTURE.md) — AI 服务 + 外部 Agent 控制（ADR-013，S1–S7 实施切片）
- [`docs/audits/AUDIT_2026-10-20.md`](docs/audits/AUDIT_2026-10-20.md) — **系统架构 + 实现 + 功能矩阵 + 技术债审计（全景，先读这个）**
- [`docs/audits/AUDIT_2026-10-23.md`](docs/audits/AUDIT_2026-10-23.md) — 全量复核 + 增量审计台账
- [`docs/PROJECT_STATUS.md`](docs/PROJECT_STATUS.md) — 现状交接与按轮次的变更记录
- [`docs/DEVELOPMENT_PLAN.md`](docs/DEVELOPMENT_PLAN.md) — 开发路线图
- [`docs/guides/PLUGIN_AUTHORING.md`](docs/guides/PLUGIN_AUTHORING.md) — 插件编写指南（Manifest/权限/配置/打包）
- [`docs/guides/OPERATIONS.md`](docs/guides/OPERATIONS.md) — 运维手册
- [`docs/roadmap/`](docs/roadmap/) — Phase 0–7 细化执行计划（含平台适配）
- [`docs/ui-design/`](docs/ui-design/) — 前端重设计（15 份，后续前端重构依据）
- [`docs/adr/`](docs/adr/) — 架构决策记录（ADR-002–007、009–011、013 accepted；001/012 proposed）
- [`docs/archive/`](docs/archive/) — 历史审计快照（仅问题溯源）
- [`AGENTS.md`](AGENTS.md) — AI 协作规范
