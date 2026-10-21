# ADR-003: 插件包归档、目录布局与安装事务

- **状态**：accepted（2026-10-21 定稿）
- **日期**：2026-10-21
- **阶段**：Phase 2（切片 2.2/2.3）
- **关联**：指南 §6.2、§7.1、§11.1、§12.1、§12.3、§14.4；审计 §1.3；ADR-004

## 背景

Phase 2 要交付"声明式插件管理器"，但插件系统此前完全不存在（指南 §1.3）。在写代码前必须固定：

1. 插件包是什么格式、允许装什么；
2. 安装到磁盘的哪个位置、如何原子提交与回滚；
3. 注册表/启用状态/权限分别存在哪里，谁是单一事实来源；
4. 签名与信任在 Phase 2 到底做到什么程度——**不能假装做了**。

当前平台适配层（`crates/rmccc-platform`）已提供 `PlatformFs`，但只有权限收紧/可执行位/符号链接。macOS 上 `rename(2)` 可原子覆盖，Windows 的 `rename` 在目标存在时会失败，因此"原子替换"本身是平台差异，需要进入适配层。

## 决策

### 1. 包格式：`.rmcpkg` = 无加密 ZIP

- 固定扩展名 `.rmcpkg`；内容为 ZIP（deflate，不接受加密条目）。
- 安装入口只接受 `.rmcpkg`；开发/测试直接调 `install_bytes`，不走"改扩展名"的捷径。
- 包根必须有 `manifest.json`；除此之外**只允许**以下顶层条目（大小写敏感）：

  | 顶层条目 | 说明 |
  |---|---|
  | `manifest.json` | 必需，见 ADR-004 |
  | `README.md` / `CHANGELOG.md` / `LICENSE` / `NOTICE` | 文档（可选） |
  | `config.schema.json` | 可选，插件配置 schema（§2.5） |
  | `assets/` | 静态资源（图片/字体等） |
  | `locales/` | 翻译 JSON |
  | `themes/` | 主题 JSON + 资源 |

- **Phase 2 明确拒绝 `ui/` 与 `logic/`**，错误信息指向 Phase 4（隔离 UI）/ Phase 5（逻辑运行时）。
  第三方代码一行都不执行，这是本阶段的安全边界（指南 §6.2 阶段 A）。

### 2. 限制（防 zip bomb / 路径攻击）

| 限制 | 值 |
|---|---|
| 包文件大小 | 16 MiB |
| 解压后总大小 | 64 MiB |
| 单条目解压大小 | 16 MiB |
| 条目数 | 512 |
| 路径长度 | 180 字节 |
| 目录深度 | 12 |
| 单条目压缩比（解压/压缩） | 200:1 |
| 总压缩比 | 100:1 |

路径规则（任一违反即拒绝整个包）：

- 拒绝绝对路径、`..`、盘符（`C:`）、UNC（`\\`）、反斜杠、NUL/控制字符；
- 拒绝 Windows 保留名（`CON`/`PRN`/`AUX`/`NUL`/`COM1-9`/`LPT1-9`，含带扩展名形式）、
  结尾点或空格；
- 拒绝符号链接、硬链接、非普通文件/目录的 Unix mode 条目；
- 拒绝**大小写不敏感重名**（为跨平台可移植性，即便在 macOS 上也按不敏感判定）；
- 文件类型默认拒绝可执行/脚本：`.js .mjs .cjs .html .htm .wasm .exe .dll .dylib .so .bat .cmd .ps1 .sh .command .app .jar .class .py .pl .rb`；
  允许 `.json .md .txt .css .png .jpg .jpeg .gif .webp .svg .ttf .otf .woff .woff2` 与无扩展名的
  `LICENSE`/`NOTICE`。

### 3. 目录布局与单一事实来源

沿用指南 §12.1 的分区，但**收紧一处**：

```text
<data_dir>/
  plugins/<plugin-id>/<version>/...        # 不可变版本目录，可并存多个版本
  plugin-data/<plugin-id>/config.json      # 用户配置（卸载默认保留）
  plugin-state/registry.json               # 安装/启用状态的唯一事实来源
  plugin-state/permissions.json            # 授权（与 Manifest 声明分开）
  plugin-state/audit.jsonl                 # 权限审计（只追加）
  plugin-state/staging/<uuid>/             # 安装暂存区（成功后删除）
```

- **不创建 `enabled.json`**。启用状态是生命周期状态的一部分，已存于 `registry.json`；
  再维护一份投影会产生两个真相。指南中的 `enabled.json` 是建议布局，这里显式偏离并记录。
- `registry.json` 与 `permissions.json` 都带 `schemaVersion`，写入走"临时文件 + 原子替换"。
- 版本目录只增不改：升级保留旧版本目录，`previousVersion` 用于回滚；清理旧版本是单独动作
  （Phase 2 不自动删除，避免回滚失效）。

### 4. 安装事务

```text
read package
  → 包大小/扩展名检查
  → 解压到 plugin-state/staging/<uuid>/unpacked（逐条目校验 + 限额）
  → 解析并校验 manifest（ADR-004 12 条）
  → 依赖/循环/ID/权限审查
  → rename(staging/unpacked, plugins/<id>/<version>)   # 同一卷内原子
  → 原子写 registry.json（安装记录 + installed_disabled/enabled）
  → 原子写 permissions.json（低风险自动授予；新权限待审）
  → 删除 staging
```

任一步失败：

- 未 rename：只删 staging；
- 已 rename、注册表未提交：删除新版本目录；
- 注册表已提交、权限写入失败：把注册表回滚到前一版本（旧 `registry.json` 内存快照重写）。

**升级**：新版本写入新目录，注册表 `previousVersion = 旧版本`，旧目录保留。
**回滚**：把 `installedVersion`/`previousVersion` 对调，状态回到 `installed_disabled`，
再按新（旧）Manifest 重新计算待审权限。旧目录不存在则拒绝回滚。

### 5. 签名与信任：Phase 2 不做，且不伪装

- Phase 2 **没有签名验证**。所有通过 CLI/IPC 安装的包一律记为 `trust = development`，
  UI 显示"未签名（开发模式）"风险提示。
- `org.rmccc.*` 命名空间保留给官方内置贡献，**拒绝第三方安装**（因为没有可信签名通道）。
- 包内 `sha256` 只用于诊断/去重，不作为来源认证。签名与可信目录在 Phase 7（指南 §14.4/§15.3）。

### 6. 卸载语义

- 存在**必需依赖者**时拒绝卸载，并返回依赖链；不级联卸载。
- 卸载 = 状态 `uninstalling` → 删除该插件所有版本目录 → 注册表墓碑 `removed`（保留 id/时间，供审计）→
  删除其权限记录。
- **默认保留 `plugin-data/<id>/`**；只有显式 `delete_data = true` 才删除配置。
- 卸载不是"删文件夹"：注册表、权限、审计必须一起收敛。

### 7. 平台归属

- 目录布局、路径校验、zip 解压是 OS 无关逻辑，用 `std::fs` 实现；
- **原子替换**属于平台差异（Unix rename 覆盖 / Windows ReplaceFile），新增
  `PlatformFs::atomic_replace`，由 macOS 后端实现，Windows/Linux 返回 `UnsupportedPlatform`；
- 配置文件权限收紧（`0600`）经 `PlatformFs::restrict_owner_only`；
- 不在本机宣称 Windows 安装事务可用（指南 §1.4）。

## 后果

- 正面：包格式与目录布局可测试、可回滚；Phase 4/5 只需扩展顶层目录白名单与执行通道。
- 负面：Phase 2 无法安装带自定义 UI 的示例；旧版本目录不会自动清理；`enabled.json` 与指南建议不一致。
- 体积：新增 `rmccc-plugin` 依赖 `zip`/`sha2`，已在 workspace 中（无新增第三方 crate）。

## 迁移影响

- 无既有插件数据（首次落地）。
- `PlatformFs` 增加一个必选方法：3 个后端 + mock 需同步；Windows/Linux 明确 `UnsupportedPlatform`。

## 跨平台验证方式

- macOS：本机 `cargo test -p rmccc-plugin` + 真实安装样例包；
- Windows/Linux：后端为占位，安装事务的路径/大小写/保留名规则由与平台无关的单元测试覆盖；
  原生 CI runner 上的实测记为 deferred（指南 §16.1）。

## 状态变更记录

- 2026-10-21：proposed（Phase 2 启动）
- 2026-10-21：accepted（切片 2.2 实现前定稿）
