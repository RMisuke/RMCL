# Phase 2 — 插件协议与声明式插件管理器

> **对应指南**：§6/§7/§8/§11/§12/§14，§17 Phase 2。**状态**：✅ completed（2026-10-21）。
> **决策**：[ADR-003](../adr/ADR-003-plugin-package-and-install-transaction.md)、
> [ADR-004](../adr/ADR-004-plugin-manifest-and-versioning.md)（均 accepted）。
> **当前事实来源**：`crates/rmccc-plugin`、`schemas/manifest.schema.json`、`examples/plugins/`。

## 1. 目标

定义并实现**声明式**插件管理：Manifest/权限/生命周期/`.rmcpkg` 安装事务/配置 schema。
**本阶段只支持数据型插件与内置 UI 贡献，不执行第三方代码**（指南 §6.2 阶段 A）。
损坏插件不得阻断主应用启动。

## 2. 前置条件（已完成）

- Phase 1 Core Service 窄门面 + 平台文件系统适配器（1.1–1.8）。
- ADR-003（包归档/目录/事务/信任）与 ADR-004（Manifest/ID/版本兼容）**accepted**。

## 3. 实际产出

| 指南要求 | 产出 | 位置 |
|---|---|---|
| Manifest Schema | JSON Schema 2020-12 + 严格 Rust 解析 + 12 条校验 | `schemas/manifest.schema.json`、`crates/rmccc-plugin/src/manifest.rs` |
| 插件 ID/SemVer/兼容性 | 反向域名 ID、严格 SemVer、受限范围语法（`^`/`~`/`>=`/`1.x`） | `src/path.rs`、`src/version.rs` |
| 注册表/状态机 | `plugin-state/registry.json` + 11 状态转移表 | `src/registry.rs` |
| `.rmcpkg` 安装事务 | staging → verify → commit + 失败回滚 | `src/installer.rs`、`src/manager.rs` |
| 权限模型 | 18 项能力目录、风险分级、授予/撤销/每次复核/审计 | `src/capability.rs`、`src/permissions.rs` |
| 插件配置 | `plugin-data/<id>/config.json` + schema 子集 + 版本不兼容保护 | `src/config.rs` |
| 数据型样例 | 主题包 + 声明式卡片包 | `examples/plugins/theme-midnight`、`server-dashboard` |
| 管理入口 | `rmcli plugin …`（13 个子命令）+ IPC（15 条）+ `/plugins` 页面 | `src-cli`、`src-tauri`、`src/routes/Plugins.tsx` |

## 4. 任务切片

### 2.1 Manifest Schema 定稿（ADR-004）✅

- `manifestVersion = 1`；未知版本拒绝，v1 已知对象**拒绝未知字段**（拼错不静默）。
- 字段：`manifestVersion/id/name/version/publisher/compatibility/entry/permissions/contributes/extensionDependencies`。
- 12 条校验逐项落地（见 ADR-004 §7 映射表）。
- 平台 ID 由 `PlatformDescriptor::plugin_platform_id()` 提供
  （`darwin-aarch64` / `windows-x86_64` / …），领域层无 `cfg!`。
- ID 命名空间：`org.rmccc.*` 保留；Phase 2 无签名通道，第三方安装直接拒绝。
- **测试**：`manifest` 7 项、`path` 5 项、`version` 7 项、`capability` 5 项。

### 2.2 包归档与安装事务（ADR-003）✅

- `.rmcpkg` = 无加密 ZIP；固定扩展名；包级限制：
  16 MiB 包 / 64 MiB 解压 / 16 MiB 单文件 / 512 条目 / 180 字节路径 / 12 层 / 200:1 压缩比。
- 路径规则：拒绝绝对路径、`..`、盘符、UNC、反斜杠、NUL、Windows 保留名、结尾点/空格、
  大小写不敏感重名、符号链接/特殊文件。
- Phase 2 **拒绝 `ui/`、`logic/` 与脚本/可执行文件**；只允许
  `manifest.json`、文档、`config.schema.json`、`assets/`、`locales/`、`themes/`。
- 事务：解压到 `plugin-state/staging/<uuid>/` → 校验 → `rename` 到
  `plugins/<id>/<version>/` → 原子写 `registry.json` → 写 `permissions.json`；
  任一步失败删除新版本目录并恢复旧注册表/权限快照。
- **测试**：`package` 12 项（zip-slip、重名、保留名、脚本、压缩比、条目上限、
  缺 manifest/主题、打包 roundtrip）。

### 2.3 注册表与生命周期状态机 ✅

- `registry.json` 是安装/启用状态的**唯一事实来源**；**不创建 `enabled.json`**
  （避免两个真相，ADR-003 决策 3 记录该偏离）。
- 状态机：`not_installed → downloading → verifying → staged → installed_disabled →
  enabled → disabling → disabled → uninstalling → removed`，任一阶段 → `failed`；
  升级/回滚显式允许 `enabled/disabled → installed_disabled`（回到待审查）。
- 升级保留旧版本目录与 `previousVersion`；`plugin rollback` 交换版本并重新计算待审权限。
- 卸载：存在已安装依赖者时拒绝；默认保留 `plugin-data/<id>/`；显式 `--delete-data` 才删。
- 损坏注册表/插件：备份 `.bak-<ts>` + 诊断，**启动继续**；其余插件正常加载。
- **测试**：`registry` 3 项 + `manager` 20 项（损坏隔离、墓碑、升级/回滚、依赖环、卸载保护）。

### 2.4 权限模型（Host Broker 基础）✅

- 第一版 18 项能力（含 `credentials.use:<service>`），风险分 low/medium/high；
  未知能力一律拒绝。
- low 风险安装时自动授予；medium/high 必须显式 `grant`，否则 `enable` 返回
  `PluginPermissionPending`。
- **升级不自动扩张权限**（包括 low）：`reconcile_upgrade` 只清理已移除的授权，
  新能力全部进入 pending。
- `PermissionStore::authorize(plugin, capability, host, instance)` 每次调用重新检查，
  支持 host 通配（`*.example.com`）与实例 allowlist；撤销后立即失败。
- 审计 `plugin-state/audit.jsonl`：时间、插件、能力、决定、作用域；不写 token/密钥。
- Phase 2 没有执行通道（`executable=false`），模型与拒绝路径先被测试锁定。
- **测试**：`capability` 5 项 + `permissions` 4 项 + `manager::enable_requires_permission_review`。

### 2.5 插件配置与 schema 迁移 ✅

- `plugin-data/<id>/config.json` = `{ schemaVersion, values, updatedAt }`。
- `config.schema.json` 为**严格子集**：`type/properties/required/additionalProperties/
  enum/minimum/maximum/minLength/maxLength/items/default`；
  未知 keyword 在安装阶段报错（不静默忽略）。
- 版本规则：
  - 存储版本 > schema：只读拒绝，不覆盖新字段；
  - 存储版本 < schema：先按新 schema 校验，通过才升版本号；失败**保留原文件**并报错，
    不写默认值、不覆盖用户配置；
  - 损坏 JSON：备份后报错，插件保持禁用。
- 显式 `plugin config reset --id …` 恢复默认值。
- **测试**：`config` 8 项 + `manager::config_is_validated_and_reset`。

### 2.6 数据型样例插件 + CLI ✅

- `examples/plugins/theme-midnight/`：纯主题 JSON + schema + LICENSE，零代码。
- `examples/plugins/server-dashboard/`：Dashboard 卡片 + 设置页 + 受限 `network.request`。
- `rmcli plugin` 子命令：`list / inspect / install / enable / disable / remove / rollback /
  grant / revoke / config get|set|reset|path / capabilities / pack`。
- `plugin pack` 用**管理器自己的 core 版本**打包（避免 CLI crate 版本不同步导致
  “自己都装不上”）。
- **测试**：`example_plugins_pack_and_install`（真实样例打包→安装→启用→贡献登记）。

### 2.7 IPC + 插件管理 UI（额外切片）✅

- 15 条 IPC：`plugin_list/inspect/install/pick_and_install/enable/disable/remove/rollback/
  grant/revoke/config_get/config_set/config_reset/capabilities/contributions`。
- `/plugins` 页面：安装、状态徽章、权限审查（风险/作用域/授予/撤销）、依赖状态、
  配置编辑（JSON + 后端校验）、卸载/删除数据、回滚、诊断横幅。
- 贡献项只汇总**已启用**插件，注册键 `<pluginId>/<localId>`，供 Phase 3 消费。

## 5. 验收标准

- [x] 包损坏/ID 冲突/路径穿越/未知权限/依赖循环/安装中断都有自动化测试。
- [x] 损坏插件不影响主应用启动（`broken_plugin_does_not_block_others`）。
- [x] Manifest Schema 正负向覆盖；权限未授权调用明确失败。
- [x] 安装/升级/回滚/卸载残余数据策略明确；staging→verify→commit 有测试。
- [x] 升级请求新权限时保持 disabled，等待用户审查（不自动扩张）。
- [x] `cargo test` 468 passed；CI 三门禁本机全绿；Windows = deferred。

## 6. 验证命令与结果（2026-10-21）

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo clippy --workspace --lib --bins -- -D panic -D unwrap_used
cargo test --workspace           # 468 passed / 0 failed
pnpm typecheck && pnpm build     # JS 150.42 kB (gzip 43.20 kB), CSS 37.47 kB (gzip 8.01 kB)
```

真实 CLI 烟测（`-d /tmp/rmccc-plugin-smoke`）：

```text
plugin capabilities                      → 18 项能力
plugin pack theme-midnight               → 2151 bytes
plugin install theme-midnight            → installed_disabled, 无待审权限
plugin enable                            → enabled（themes.apply 已自动授予）
plugin config get                        → {"accent":"violet"} + schema
plugin install server-dashboard          → installed_disabled, pending network.request
plugin enable                            → plugin_permission_pending（exit 3）
plugin grant network.request + enable    → enabled
plugin inspect                           → 依赖/贡献/配置齐全
```

## 7. 平台策略

- 包校验/解压是 OS 无关逻辑（`std::fs`）；**原子替换**经新增的
  `PlatformFs::atomic_replace`（Unix rename / Windows ReplaceFile），
  macOS 后端实现，Windows/Linux 返回 `UnsupportedPlatform`。
- 路径/保留名/大小写冲突规则按跨平台最严格标准执行（在 macOS 上也拒绝）。
- 不在本机宣称 Windows 安装事务可用（指南 §1.4）；原生验证 deferred 到 Phase 7。

## 8. 风险、偏离与回滚

- **偏离**：未创建 `enabled.json`（状态唯一来源 `registry.json`）。
- **偏离**：Phase 2 签名/信任未实现，所有安装记为 `trust=development` 并显示风险提示；
  不把 SHA-256 当作来源认证。
- **偏离**：不自动下载依赖（没有插件仓库协议）；只校验本地已安装依赖。
- 包格式/权限模型变更必须走 ADR 修订；已安装数据无迁移（首次落地）。
- 安装失败：删除新版本目录 + 内存快照恢复；注册表写入是原子替换。

## 9. 文档同步

- `schemas/manifest.schema.json` 建立；ADR-003/004 accepted。
- `ARCHITECTURE.md` 增插件系统章节；`AUDIT` §1.3 更新；`PROJECT_STATUS` 回填。
- `docs/roadmap/00_ROADMAP_OVERVIEW.md` 更新状态与测试数。

## 10. 明确 deferred

- 签名/可信目录/官方插件通道（Phase 7）。
- 隔离 UI（`entry.ui`，Phase 4）与逻辑运行时（`entry.logic`，Phase 5）。
- 插件仓库协议与依赖自动解析下载（Phase 7）。
- Windows/Linux 原生安装事务实测（原生 CI runner）。
- 前端组件测试仍为 0（Phase 3 随 Dashboard 重构补齐）。
