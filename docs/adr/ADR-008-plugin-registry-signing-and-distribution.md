# ADR-008: 插件目录、发布者签名与分发

- **状态**：proposed（2026-10-26 草案，待用户拍板；未 accepted 前不据此改协议）
- **日期**：2026-10-26
- **阶段**：Phase 7（切片 7.1；7.2–7.5 依赖本 ADR）
- **关联**：指南 §6.4、§14.4、§15.3、§16.3、§20；ADR-003（包格式/安装事务/决策 5）、ADR-004（Manifest/ID/版本）、ADR-011（体积门禁）、ADR-012（macOS Intel 产物）；审计 §13.7；[`roadmap/09_phase7_distribution.md`](../roadmap/09_phase7_distribution.md)

## 背景

Phase 2 已交付声明式插件管理器，但**刻意没有签名通道**（ADR-003 决策 5）：

- `.rmcpkg` 是无加密 ZIP，包内 `sha256` 只用于诊断/去重，不是来源认证；
- 所有安装一律记为 `trust = development`，UI 显示"未签名（开发模式）"；
- `org.rmccc.*` 命名空间保留给官方贡献，第三方不允许进入；
- `crates/rmccc-plugin` 的 `TrustLevel` 只有 `official` / `development` 两档，`PluginSourceKind::registry` 已预留但从未生产；
- 安装入口只有本地路径（CLI/IPC 传路径），没有任何网络索引客户端；`plugin-sdk/scripts/cli.mjs` 只做本地校验与打包。

Phase 7 要开放第三方插件生态，必须在不伪造信任的前提下回答四个问题：

1. 用户如何确认"这个包确实来自某个发布者"（身份，而不只是哈希）；
2. 索引被篡改/降级/替换时如何失败关闭；
3. 升级、撤回、回滚与"新增权限"如何不静默放行；
4. 上述机制不能把应用本体拖过 ADR-011 的体积红线，也不能强依赖常驻服务。

指南 §6.4 已给出方向（包 URL + SHA-256 + 体积 + 兼容性 + 签名；hash ≠ 来源认证），§14.4 规定安装/升级事务顺序（先验索引签名与哈希，再 staging），§15.3 要求升级可回滚、插件更新与应用更新分离。本 ADR 只定"目录与签名"的协议与信任根，不实现。

## 目标

- **可验证身份**：每个从目录安装的包都能追到一个发布者密钥，且该密钥被某个信任根背书；
- **失败关闭**：索引/包/签名任一不符即拒绝安装，错误可诊断；离线时行为确定；
- **多级信任可见**：官方、已审核、社区自签、本地开发四档在安装与启用 UI 上可区分；
- **可轮换、可撤回**：密钥有有效期与重叠窗口；撤回不静默卸载，但阻止启用；
- **协议最小**：静态文件即可分发（无动态服务端），包格式不变（ADR-003 不重开）；
- **体积可控**：新增依赖经 `cargo bloat` 实测，且只在安装/启用路径上验证。

**非目标（本 ADR 明确不做）**：完整 TUF（阈值角色、快照/时间戳角色、透明日志）；发布者法律身份核验；付费/私有商店与账号体系；插件沙箱与运行时权限（分别由 ADR-007/权限系统负责）；自动更新推送服务；Windows/Linux 原生验收（服从指南 §1.4，记 deferred）。

## 选项

| 方案 | 做法 | 优点 | 缺点 |
|---|---|---|---|
| **A（推荐）静态签名索引 + 分离式包签名** | `registry.json` 由索引密钥签名；每个版本条目携带发布者对"包摘要 + 元数据"的 Ed25519 签名 | 实现小、无状态、可镜像、包格式零改动、离线可验证 | 需自管两把密钥与轮换；索引新鲜度需 TTL |
| B 包内签名（`signature.json` 随包） | 签名文件进 `.rmcpkg` | 单文件自证 | 改 ADR-003 包白名单与旧包兼容；撤销/密钥轮换要重打包 |
| C 只签索引 | 发布者身份 = 索引签名者 | 最简单 | 无法区分发布者；索引密钥泄露即全站伪造 |
| D Sigstore/cosign keyless | 依赖 OIDC 身份与透明日志 | 无自管密钥、身份强 | 引入重依赖/联网登录，与"静态、轻量"目标冲突 |

TUF 完整角色模型是最强方案，但需要元数据服务器与客户端状态机；当前只有单一官方目录、发布者数量未知，先做"两把密钥 + 分离签名"的轻量层级，保留将来升级到 TUF 的路径（索引格式带 `schemaVersion`）。

## 决策

### 1. 信任根：两把离线/在线分离的密钥

| 密钥 | 用途 | 存放 | 轮换 |
|---|---|---|---|
| **root key**（离线） | 签 `publishers.json` 与 `revocations.json`（发布者白名单、密钥状态、撤回） | 离线介质；仅发布者审核时使用 | 年度或泄露时 |
| **index key**（在线） | 签 `registry.json`（每日/每发布生成） | 发布 CI Secret | 季度，重叠 ≥ 2 个应用版本 |

- 公钥随应用发布（实现时新增，建议路径 `src-tauri/assets/trust/root.pub` / `registry.pub`，各自含 `keyId`、`notBefore`/`notAfter`，支持多把并存）。
- 轮换：新公钥先进应用（重叠窗口），旧密钥到期后索引改用新密钥签名；应用只接受 `notBefore ≤ now ≤ notAfter` 的密钥。
- 私钥**不得**进入 Git、日志、构建产物、崩溃报告（指南 §14.2 第 14 条）；index key 仅存在于发布流水线 Secret；root key 不进入 CI。

### 2. 目录 = 静态文件（HTTPS）

发布物是三个已签名 JSON，可托管在任意静态源（GitHub Pages / 对象存储 / 镜像）：

```text
registry.json      + registry.json.sig      # 索引（index key）
publishers.json    + publishers.json.sig    # 发布者白名单与密钥（root key）
revocations.json   + revocations.json.sig   # 撤回清单（root key）
```

- **签名对象是文件原始字节**（不做 JSON 规范化）：客户端先验签、再解析，避免 canonicalization 分歧。
- 拉取约束：仅 HTTPS；重定向 ≤ 3 跳且不得跨到非 HTTPS；索引 ≤ 4 MiB、超时 10s；不携带账号/遥测。
- 索引示例（`schemaVersion: 1`）：

```json
{
  "schemaVersion": 1,
  "generatedAt": "2026-10-26T00:00:00Z",
  "plugins": [{
    "id": "top.snow.server-dashboard",
    "publisher": { "id": "top.snow", "name": "Snow" },
    "versions": [{
      "version": "1.2.0",
      "url": "https://plugins.example.net/top.snow/server-dashboard-1.2.0.rmcpkg",
      "sha256": "<64 hex>",
      "size": 48213,
      "pluginApi": "1.x",
      "core": ">=0.1.0 <0.2.0",
      "platforms": ["darwin-aarch64", "windows-x86_64"],
      "publishedAt": "2026-10-26T00:00:00Z",
      "yanked": false,
      "signature": { "alg": "ed25519", "keyId": "pub:top.snow:2026-1", "value": "<base64>" }
    }]
  }]
}
```

- `publishers.json`：`publisher.id` → `keys[{keyId, publicKey, status, notBefore, notAfter}]`；发布者 ID 复用 ADR-004 校验规则。
- `revocations.json`：`{id, versions:[...] | "*", reason, revokedAt}`；按 root key 验签后生效。

### 3. 包签名对象：绑定摘要、身份与元数据

分离式签名覆盖一段**规范化语句**（UTF-8，`\n` 分隔，**含末尾换行**），而不是裸文件字节，以阻止"换个版本号重放同一签名"：

```text
rmccc-plugin-v1
id: <manifest.id>
version: <version>
sha256: <64 hex>
size: <十进制字节数>
publisher: <索引条目 publisher.id>
published: <RFC3339>
```

- 签名 = Ed25519(sign(statement))，base64 放索引条目；客户端用 `keyId` 选公钥。
- 校验顺序：包大小 → `sha256` → 语句重建 → 验签 → 包内 `manifest.id` / `manifest.version` / `manifest.publisher.id`
  与索引条目逐字一致（任一不一致即拒绝，防止索引错配或张冠李戴）。
- 为什么不签包内文件：保持 `.rmcpkg` 布局与 ADR-003 决策 1/5 不变，旧包仍可按 development 安装；撤回/轮换无需重打包。

### 4. 信任级别与 UI 映射

扩展 `TrustLevel`（本地 `plugin-state/registry.json` 的 `schemaVersion` 升到 2——与目录索引的
`schemaVersion: 1` 是两个独立版本号；旧记录缺省映射为 `development`）：

| 级别 | 判定 | UI |
|---|---|---|
| `official` | 包由 root 白名单中的 `org.rmccc` 发布者密钥签名 | "官方"，无风险提示 |
| `verified` | 发布者密钥在 root 签名的 `publishers.json` 白名单内 | "已验证发布者"，显示发布者名 |
| `community` | 签名有效，但密钥仅本地首次固定（TOFU）；密钥变更需用户重新确认 | "社区（自签）"，安装前一次性风险确认 |
| `development` | 本地路径安装、未签名 | 现有"未签名（开发模式）"提示；永不自动启用 |

`PluginSource.kind = registry` 时 `reference` 记录目录 URL 与版本；本地安装仍为 `local`。`org.rmccc.*` 仅允许 `official`，第三方无法通过自签进入。

### 5. 安装/升级/撤回流水线（失败关闭）

```text
取索引（HTTPS 限额）
  → 验 registry.json.sig（index key，有效期检查）
  → 验 revocations.json.sig 并过滤已撤回条目
  → 兼容性预检（pluginApi / core / 平台 / 版本）
  → 下载到 staging（≤ 16 MiB，边下边算 sha256）
  → 大小 + sha256 比对
  → 重建语句 + 验包签名（keyId 属于 publishers.json 或本地 TOFU 固定）
  → 包内 manifest.id / version / publisher.id 与条目一致
  → 走 ADR-003 既有安装事务（解压校验 / 原子 rename / registry+permissions 提交）
  → 权限审查（新增权限必须用户确认后才可启用）
```

- **离线**：缓存最近一次验签通过的索引与三个 JSON（`plugin-state/registry-cache/`），TTL 24h；过期且无网络时，安装/更新明确失败并提示"离线，使用已安装版本"；**不**因离线而跳过验签。
- **升级**：新版本走同一流水线；`previousVersion` 保留（ADR-003），失败自动恢复；新版本请求更多权限时保持 `installed_disabled`，待审查。
- **撤回**：启动时与启用前检查撤回清单；命中则置 `disabled` 并记录原因，**不自动删除**用户数据与旧版本，由用户决定卸载/回滚。
- **降级**：目录安装不允许降级（沿用现有规则），回滚走 `previousVersion`。

### 6. 更新策略

- 插件更新**默认手动**（用户点"检查更新"）；可按插件开启自动更新，默认关闭。
- 自动更新只允许在同一 `publisher.id`、同一插件 ID 内进行，且不静默扩张权限（扩张即停在待审）。
- 应用本体更新走 Tauri updater，与插件目录**密钥、endpoint、开关全部分离**（指南 §16.3）；两套私钥互不复用。
- 目录索引拉取不进入启动关键路径：仅在插件页可见/用户操作时拉取（避免离线启动被网络阻塞）。

### 7. 发布流程

- 目录内容维护在独立仓库（建议 `rmccc-plugins`），发布 = PR：新增/更新 `registry.json` 条目 + 提交 `.rmcpkg` 与签名材料。
- CI 校验：`rmcli plugin pack`/SDK validate、清单 schema、包摘要与签名语句一致性、平台/兼容性字段、重复 ID/版本、撤回冲突；通过后由 index key 重签 `registry.json` 并发布。
- 发布者密钥登记（进入 `verified`）= PR 修改 `publishers.json` + 人工审核；root key 签名在离线环境完成。
- 撤回 = 提 PR 修改 `revocations.json`，root key 签署后发布。

### 8. 体积与依赖

- 签名验证选 **`ed25519-dalek`**（RustCrypto；`sha2` 已在依赖树，`base64` 已在锁文件）；仅安装/启用时调用，无热路径开销。
- 预计 stripped 增量 **< 300 KiB**；合并前必须 `cargo bloat` 实测，并把实测值补进本 ADR（accepted 时）。未实测不得宣称达标；若增量 > 500 KiB，重新评估选型（`ring` 或更小的曲线实现）。
- 不引入常驻网络服务、后台轮询或系统密钥链依赖。

## 后果

- **正面**：来源可验证、索引篡改可检测、离线可验证、包格式与现有安装事务零改动；信任级别可直接映射到现有 `TrustLevel` 与 UI 徽标。
- **负面**：多两把密钥与轮换流程；索引新鲜度依赖 TTL；`verified` 审核是人工瓶颈；新增 crypto 依赖需守体积红线；社区 TOFU 首次使用仍有风险（UI 必须明示）。
- **对 CI**：release 工作流新增"签名索引"步骤，私钥来自 Secret；密钥缺失时目录发布判 `release-blocked`，不得用未签名索引冒充通过。
- **对数据**：`plugin-state/registry.json` schemaVersion 2；新增 `registry-cache/`；旧记录迁移为 `development`。
- **对文档**：`PLUGIN_AUTHORING.md`、SDK `publishing.md`、`OPERATIONS.md` 需补签名与发布步骤；`ARCHITECTURE.md` §13/§14 同步目录协议。

## 迁移影响

- **代码**：`TrustLevel` 增加 `verified`/`community`；`PluginSourceKind::Registry` 开始生产；新增目录客户端（拉取/验签/缓存）与 IPC（建议 `plugin_registry_list`、`plugin_registry_refresh`、`plugin_registry_install`，具体命令面在实现 ADR 时定稿并过 `ipc_tests` 计数门禁）；安装器复用 ADR-003 事务，不改 `.rmcpkg` 解析。
- **数据**：注册表 schemaVersion 1 → 2 的显式迁移；失败可回滚到旧文件（原子写 + 备份）。
- **回滚**：目录功能由设置项 `plugins.registry.enabled`（默认关闭）门控；关闭后本地安装与既有组件不受影响，已装插件的 `trust` 字段保留但 UI 退回开发提示。
- **兼容性**：客户端遇到索引 `schemaVersion: 1` 仍可解析（字段只增不改语义）；未知 `schemaVersion` 拒绝并提示升级应用。
  本地注册表 `schemaVersion: 2` 由迁移代码读取，不与索引版本号混用。

## 跨平台验证方式

- 签名/摘要/语句重建为纯计算，平台无关；下载与原子安装复用既有平台适配层（`PlatformFs`）。
- macOS Apple Silicon 本机：完整验收（目录拉取、验签、篡改拒绝、撤回、离线缓存、升级回滚）。
- Windows/Linux：本机**不**交叉编译、不实测（指南 §1.4），在原生 CI runner 触达后补做；在此之前相关验收项记 **deferred**，完成度报告不得填 pass。
- 测试向量：固定测试密钥对 + 4 类负例（改包一字节 / 改索引条目 / 过期密钥 / 已撤回版本）+ 1 类离线正例，纳入 `cargo test` 与 SDK 校验。

## 待拍板问题（accepted 前必须确定）

1. 信任级别集合与命名（`official` / `verified` / `community` / `development`）是否接受？
2. 自动更新默认关闭（本草案建议）还是默认开启？
3. v1 是否允许 `community`（自签 + TOFU），还是先只允许 `official` / `verified`，把社区自签推迟到有审核流程之后？
4. 目录托管与仓库命名（GitHub Pages + `rmccc-plugins` 仓库，或自建对象存储）？
5. `ed25519-dalek` 与 `ring` 的最终选型，以 `cargo bloat` 实测增量与许可为准。

## 状态变更记录

- 2026-10-26：proposed（草案入库；决策 1–8 为建议方案，待用户拍板后转 accepted）。
