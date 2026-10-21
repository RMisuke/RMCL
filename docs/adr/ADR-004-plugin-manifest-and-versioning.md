# ADR-004: 插件 Manifest Schema、ID 与版本兼容

- **状态**：accepted（2026-10-21 定稿）
- **日期**：2026-10-21
- **阶段**：Phase 2（切片 2.1）
- **关联**：指南 §6.4、§7.2、§7.3、§8.1、§11.2；审计 §1.3；ADR-003

## 背景

指南 §7.2 的 Manifest 示例明确标注为"设计草案，不能当作最终协议"。Phase 2 要写解析、
校验、权限审查和依赖解析，必须先固定：

- `manifestVersion` 的兼容策略（未知字段、未知版本）；
- 插件 ID 的字符集与命名空间；
- 版本与版本范围的语法（依赖、`pluginApi`、`core` 都要用）；
- 平台标识（Mojang 的 `osx`、`aarch64` 与包生态惯用的 `darwin-x86_64` 不同，不能混用）；
- 第一版能力清单与贡献类型。

## 决策

### 1. `manifestVersion` 与字段

- 当前支持 `manifestVersion = 1`（整数）。`> 1` 一律拒绝，错误信息提示升级启动器；
  `0`/缺失/非整数拒绝。
- v1 的已知对象**严格拒绝未知字段**（`deny_unknown_fields`）。理由：Manifest 是协议文件，
  拼错字段静默忽略会导致"看起来装好了但权限没生效"；未来版本通过 `manifestVersion = 2`
  扩展，而不是在同版本里放宽。
- 完整字段：

```json
{
  "manifestVersion": 1,
  "id": "dev.rmccc.server-dashboard",
  "name": "Server Dashboard",
  "version": "1.0.0",
  "description": "…",
  "publisher": { "id": "dev.rmccc", "name": "RMCCC Examples" },
  "compatibility": {
    "pluginApi": "1.x",
    "core": ">=0.1.0 <0.2.0",
    "platforms": ["darwin-aarch64", "windows-x86_64", "linux-x86_64"]
  },
  "entry": {},
  "permissions": [
    { "capability": "dashboard.cards" },
    { "capability": "ui.pages" },
    { "capability": "network.request", "hosts": ["status.example.net"] }
  ],
  "contributes": {
    "dashboardCards": [
      {
        "id": "server-status",
        "title": "Server Status",
        "defaultSize": { "w": 4, "h": 2 },
        "minSize": { "w": 2, "h": 1 }
      }
    ],
    "settingsPages": [
      { "id": "server-dashboard-settings", "title": "Server Dashboard" }
    ],
    "themes": [
      { "id": "midnight", "name": "Midnight", "path": "themes/midnight.json" }
    ]
  },
  "extensionDependencies": [
    { "id": "dev.rmccc.shared-ui", "version": "^1.0.0", "optional": false }
  ]
}
```

- `entry.ui` / `entry.logic` 字段在 schema 中存在（为 Phase 4/5 预留），但 Phase 2 解析到
  任一非空值即拒绝安装：本阶段不执行第三方代码（ADR-003 决策 1）。
- `publisher.id` 与插件 ID 前缀**不强制绑定**（发布者可以发布多个命名空间），但会出现在审查 UI 中。

### 2. 插件 ID

- 反向域名：`^[a-z][a-z0-9-]*(\.[a-z][a-z0-9-]*)+$`，至少两段，总长 ≤ 128。
- 只允许小写 ASCII 字母、数字、`-`、`.`；不允许连续点、首尾点、`_`、大写。
- 安装目录、数据目录、日志字段都用该 ID；ID 不随显示名变化。
- `org.rmccc.` 前缀保留给官方内置贡献；Phase 2 没有签名通道，第三方安装直接拒绝（ADR-003 决策 5）。

### 3. 版本与版本范围

- 版本 = 严格 SemVer 2.0.0（`MAJOR.MINOR.PATCH[-prerelease][+build]`），不接受 `v1.2.3`、
  两位版本、前导零。
- 比较忽略 build metadata，prerelease 按 SemVer 规则排序。
- 范围语法（自实现，避免为一个小语法引入 `semver` crate）：

  | 写法 | 含义 |
  |---|---|
  | `1.2.3` / `=1.2.3` | 精确 |
  | `^1.2.3` | `>=1.2.3 <2.0.0`（`^0.2.3` → `<0.3.0`，`^0.0.3` → `<0.0.4`） |
  | `~1.2.3` | `>=1.2.3 <1.3.0` |
  | `>=1.0.0 <2.0.0` | 空格分隔 = 逻辑与 |
  | `1.x` / `1.*` / `1.2.x` | 主/次版本通配 |
  | `*` | 任意 |

  不支持 `||`、`,` 分隔、`-` 区间；解析失败即拒绝（不允许"解析失败就放行"）。

- `compatibility.pluginApi` 用 `MAJOR.x` 形式（如 `1.x`），表示"宿主插件 API 主版本为 1"。
  当前常量 `PLUGIN_API_VERSION = 1.0.0`；要求 `1.x`，其它值拒绝。
- `compatibility.core` 是相对启动器版本（当前 `0.1.0`）的范围，缺省为 `*`（不推荐，示例都显式写）。
- `compatibility.platforms` 为空 = 全平台；否则必须命中
  `darwin-aarch64` / `darwin-x86_64` / `windows-x86_64` / `windows-aarch64` /
  `linux-x86_64` / `linux-aarch64` 之一。未知平台字符串直接拒绝（防止拼错导致"哪都装不上"却无提示）。
  当前平台 ID 由 `PlatformDescriptor::plugin_platform_id()` 给出，不在插件层裸写 `cfg!`。

### 4. 第一版能力清单（`permissions[].capability`）

| 能力 | 风险 | 作用域 | Phase 2 状态 |
|---|---|---|---|
| `dashboard.cards` | low | — | 安装时自动授予（声明式） |
| `ui.pages` | low | — | 自动授予 |
| `themes.apply` | low | — | 自动授予 |
| `settings.read` | low | 字段 allowlist | 自动授予 |
| `instance.read` | low | 实例 allowlist | 自动授予 |
| `mods.read` | low | 实例 allowlist | 自动授予 |
| `mods.search` | medium | — | 需显式授予 |
| `mods.plan_install` | medium | 实例 allowlist | 需显式授予 |
| `mods.install` | high | 实例 allowlist | 需显式授予 + 每次确认（Phase 4） |
| `mods.manage` | high | 实例 allowlist | 需显式授予 + 每次确认（Phase 4） |
| `downloads.submit` | medium | — | 需显式授予 |
| `downloads.manage` | medium | 自己的任务 | 需显式授予 |
| `game.launch` | high | 实例 allowlist | 需显式授予 + 确认（Phase 4） |
| `logs.read` | medium | 脱敏/限量 | 需显式授予 |
| `network.request` | high | `hosts` 必需 | 需显式授予 |
| `agent.tools.register` | medium | — | Phase 6 前只声明 |
| `agent.context.read` | high | 任务范围 | Phase 6 前只声明 |
| `credentials.use:<service>` | high | service | Phase 6 前只声明 |

- 未知能力名一律拒绝（"未知权限不能默认为允许"，指南 §7.3 第 6 条）。
- `credentials.use:<service>` 的 service 必须是 `^[a-z][a-z0-9-]{1,31}$`。
- `network.request` 的 `hosts`：精确主机名，或 `*.example.com`（至少两段、只允许最左一个 `*`）；
  拒绝裸 `*`、协议、端口、路径、IP 字面量、`localhost`、私网段与云 metadata 地址。
  这些限制在权限模型层先拒绝声明，执行层（Phase 4）再按每次请求复核。

### 5. 贡献类型 v1

- `dashboardCards[]`：`id`、`title`、可选 `description`、`defaultSize{w,h}`、`minSize{w,h}`。
  `w/h` 取 `1..=24`，`defaultSize >= minSize`。
- `settingsPages[]`：`id`、`title`、可选 `description`。
- `themes[]`：`id`、`name`、`path`（包内相对路径，必须存在且位于 `themes/`）。
- 每个贡献的本地 `id` 匹配 `^[a-z][a-z0-9-]*$`；同一插件内同类贡献 ID 不得重复。
- 全局注册键 = `<pluginId>/<localId>`（指南 §11.3）；`org.rmccc/` 保留。
- Phase 2 不提供菜单项/命令贡献，避免在无执行通道时定义半成品协议。

### 6. 依赖

- `extensionDependencies[]`：`{ id, version, optional? }`。
- 安装时要求必需依赖已安装且版本满足；可选依赖存在但版本不满足只记诊断，不阻断。
- 依赖图在**已安装集合 + 待装插件**上做 DFS 环检测；存在环拒绝安装。
- 卸载被依赖的插件时拒绝并返回依赖链。
- 不自动下载依赖（Phase 2 没有插件仓库协议，指南 §15/Phase 7）。

### 7. 12 条校验规则落地

| # | 规则 | 实现位置 |
|---|---|---|
| 1 | 文件 schema（字段/类型/未知字段） | `manifest::PluginManifest`（serde） |
| 2 | 插件 ID 与包目录 ID 一致 | `installer`（目标目录 = manifest.id） |
| 3 | 版本字符串合法 | `version::Version::parse` |
| 4 | entry 路径在包内、无 `..`/绝对/symlink escape | `package::validate_entry_path` + 存在性检查 |
| 5 | 贡献 ID 插件内唯一 / 全局冲突 | `manifest::validate_manifest` + `contributions` |
| 6 | 权限名已知 | `capability::Capability::parse` |
| 7 | hosts 精确或受控通配 | `capability::validate_hosts` |
| 8 | pluginApi/core/platform 匹配 | `manifest::validate_manifest` |
| 9 | 依赖存在、范围满足、无环 | `manager::install` + `dependency` |
| 10 | 解压体积/数量/单文件/路径/压缩比 | `package::inspect_archive` |
| 11 | 重复名/Windows 保留名/大小写冲突/文件类型 | `package::inspect_archive` |
| 12 | staging 校验后原子切换 | `installer`（ADR-003 决策 4） |

## 后果

- 正向：协议严格、可测试；未知字段/未知权限不会被静默接受。
- 负向：Manifest 演进必须升 `manifestVersion`；Phase 2 的示例不能带 UI；自实现 SemVer
  范围只覆盖实际需要的子集（文档已列）。
- 生态：`schemas/manifest.schema.json` 与 Rust 校验器必须保持一致；示例插件同时被两者校验。

## 迁移影响

- 首次落地，无历史 Manifest。
- 未来 `manifestVersion = 2` 需要新增解析器和迁移说明；v1 数据按"向后读取兼容"保留。

## 跨平台验证方式

- 平台/架构匹配用 `PlatformDescriptor` 的单元测试覆盖 6 种组合；
- Windows/Linux 的真实安装验证 deferred（指南 §1.4）。

## 状态变更记录

- 2026-10-21：proposed（Phase 2 启动）
- 2026-10-21：accepted（切片 2.1 实现前定稿）
