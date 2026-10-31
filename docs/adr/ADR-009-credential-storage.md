# ADR-009: 凭据存储 —— AES-256-GCM 文件 vault 为首选，系统密钥链留作可替换后端

- **状态**：accepted
- **日期**：2026-10-22
- **阶段**：Phase 6
- **关联**：指南 §10.4、§12.1；ADR-011（体积门禁）；`rmccc-auth::TokenVault`（现有加密原语）；`docs/roadmap/08_phase6_agent.md` 6.2

## 背景

Phase 6 的 Agent 要访问云端模型（OpenAI 兼容端点）与未来的第三方服务，需要保存
**API Key / endpoint / 账号凭据**。现有的 Minecraft 账号令牌已经有一套可用的加密实现
（`crates/rmccc-auth/src/vault.rs`）：随机 32 字节 key 文件 + AES-256-GCM + 版本化信封，
key 文件经 `PlatformFs::restrict_owner_only` 收紧为 `0600`。

指南 §10.4 的硬要求：

- API Key 不能进普通配置 JSON、前端 `localStorage`、崩溃报告或日志。
- 插件不得读取任意其他插件的模型凭据，只能请求"某个服务的受控使用授权"。
- 用户可配云端与本地端点；两者的网络安全策略分开处理。

需要决定的是：**要不要为了 Agent 凭据引入 macOS Keychain / Windows Credential Manager**，
还是复用文件 vault。判据是安全收益、跨平台一致性、体积与可测试性——不是"系统密钥链听起来更安全"。

## 选项

### A. 系统密钥链（macOS Keychain / Windows Credential Manager / libsecret）

优点：密钥不落用户目录，受 OS 访问控制与（macOS）解锁状态保护；单条凭据粒度。

缺点：

- 三套原生 API + 三套错误语义；Windows 已由文件 vault + `icacls` ACL 适配（原生实测 deferred），Linux 本机无法验证（指南 §1.4 → deferred / release-blocked）。
- `security-framework` / `keyring` 类依赖会进入主可执行文件；ADR-011 的 stripped 余量只剩
  ~2 MiB（当前 7.96 MiB），一个跨平台 keyring 栈通常 +0.3–1 MiB，且无法用 `opt-level=s` 压掉。
- 需要处理"钥匙串被锁定 / 用户拒绝授权 / 无 GUI 会话（CLI）"的分支，失败模式比文件 vault 多。
- 测试无法在 CI 无头环境稳定跑（会弹系统授权框）。

### B. 复用文件 vault（AES-256-GCM + `0600` key 文件），按服务分条

优点：与账号令牌同一套原语，代码与威胁模型已经在用；纯 Rust，无平台分支；
可在临时目录里完整测试；体积增量接近 0（`aes-gcm`/`base64`/`rand` 已在依赖里）。

缺点：安全性取决于"同机同用户"边界——能读用户目录的攻击者也能读 key 文件；
没有 OS 级的"每次访问弹窗"；key 文件丢失即全部凭据不可解密（可重新录入）。

### C. 明文配置 / 前端存储

直接排除：违反指南 §10.4，且 `localStorage` 在 WebView 里可被任意前端代码读取。

## 决策

采用 **B**，并把它定义成一个可替换的 trait：

1. **`CredentialStore` trait** 是唯一入口：`put / get / delete / list`。Agent 与未来的插件授权层
   只依赖 trait；系统密钥链如果以后要做，是新增一个后端，不改调用方。
2. **落盘位置**：`<ApplicationData>/rmccc/private/agent-credentials.json`（单个加密信封）+
   `private/agent.key`（32 字节随机 key，`PlatformFs::restrict_owner_only` 收紧为 `0600`，
   Windows ACL 走同一 trait，待原生 CI 验证）。信封格式沿用 `TokenVault` 的版本化结构，
   便于将来换 KDF/算法时迁移。
3. **条目粒度**：`service`（如 `openai`、`local-llm`）→ `{ api_key, base_url, model }`。
   **不支持**"一个 key 全局通用"；不同服务的凭据互不可见。
4. **绝不出圈**：凭据只能由 Rust 侧在发起请求时读取；IPC 只返回 `has_key` 与
   `base_url`/`model` 这类非秘密字段，**没有任何命令返回 api_key 明文**。
   日志与审计写 `service` 名，不写 key；`Debug` 实现对 `api_key` 打码。
5. **网络策略分开**：
   - 远程端点必须 `https://`；
   - 本地端点只允许 loopback（`http://127.0.0.1` / `http://localhost` / `[::1]`），
     非 loopback 的明文 `http://` 一律拒绝（`RmcccError::Validation`）。
   这条校验放在 adapter 构造处，而不是 UI。
6. **插件访问模型（Phase 6 不做执行，只定义边界）**：插件永远拿不到 key 明文；
   未来由 Host Broker 以"服务 + 能力"授权后代发请求，授权记录进 `plugin-state/permissions.json`，
   与 Phase 2 的能力审查同一条链路。

## 后果

**正面**

- 与账号令牌共用一套加密原语与权限收紧路径，审计面小。
- 纯 Rust、可单测（临时目录 + 随机 key），macOS 本机即可完整验收。
- 主可执行体积不增加（依赖已在 workspace），ADR-011 余量不受影响。
- 后端可替换：Keychain / Credential Manager 作为 Phase 7+ 的可选增强，不影响 Agent 代码。

**负面 / 代价**

- 安全边界是"同用户"，不能防御同机同用户恶意进程；需在隐私说明与 ADR-010 中如实写明。
- 没有 OS 级访问提示；"是否允许 Agent 使用某服务"必须由应用层 UI 明示（Phase 6.5）。
- key 文件损坏/丢失会同时废掉所有 Agent 凭据（不影响 Minecraft 账号 vault，两者 key 独立）。

## 迁移影响

- 新增 `crates/rmccc-agent`（`credentials` 模块），依赖 `aes-gcm`/`base64`/`rand`/`rmccc-platform`。
- `rmccc-auth::TokenVault` 不改：账号令牌与 Agent 凭据使用**各自的 key 文件**，
  避免一次泄露同时波及游戏账号与模型账单。
- CLI/GUI 尚无凭据 UI；本 ADR 只约束存储层，录入入口随 Phase 6.5 落地。

## 跨平台验证方式

- macOS ARM64（本机）：单元测试覆盖 roundtrip、key 文件权限（`0600`）、错误 key 拒绝、
  条目隔离、非 loopback 明文端点的 URL 策略拒绝。
- Windows / Linux：加密与文件 API 是纯 Rust，但 ACL 收紧（`restrict_owner_only`）为
  `UnsupportedPlatform` → **deferred / release-blocked**，不得宣称已实现。

## 状态变更记录

- 2026-10-21：proposed（Phase 6 前置，指南 §10.4）
- 2026-10-22：accepted（文件 vault + trait 后端；Keychain/Credential Manager 记为可选增强）
