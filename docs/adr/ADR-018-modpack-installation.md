# ADR-018: 整合包安装子系统（HMCL 格式兼容 + 版本即实例）

- **状态**：accepted（2026-10-30）
- **日期**：2026-10-30
- **阶段**：Phase 4+（整合包）
- **关联**：[ADR-016](ADR-016-instance-management-v2.md)（版本即实例）；`ARCHITECTURE.md` §2/§11；`crates/rmccc-modpack`；`crates/rmccc-core/src/modpack.rs`

## 背景

整合包是 RMCL 与 HMCL 差距最大的一块：此前只有下载页占位卡（`RmEmptyState`），
后端 0 实现。用户要求按 HMCL 的功能面 1:1 复刻：

- 支持 HMCL 能装的整合包类型：CurseForge、Modrinth、MCBBS、Server、MultiMC/Prism、HMCL 自有导出、手动打包的 `.minecraft` zip；
- 从游戏本体、loader 到 mods/config 的一键安装；
- 主界面拖入 `.zip`/`.mrpack` 即可开始安装；
- 覆盖文件解压、`modpack.cfg` 元数据、图标、内存/JVM/窗口等设置。

HMCL 的实现分层是 `ModpackProvider`（格式解析）+ `ModpackInstallTask`（overrides）
+ `MinecraftInstanceTask`（配置）+ 各 provider 的 completion task（Curse/Modrinth/
Server/MCBBS 文件下载）+ `GameBuilder`（原版与 loader）。RMCL 需要保留同样的职责
分离，但不能照搬 Java 代码。

## 目标

1. 在 Rust 侧覆盖 HMCL 的全部本地整合包格式与安装主流程；
2. 安装结果必须能直接启动：原版文件、loader 库、overrides、manifest 文件全部就位；
3. 复用 RMCL 已有的游戏安装管线、loader 安装器、下载引擎、实例设置与 IPC 事件体系；
4. 拖拽安装不绕过安全边界（zip-slip、symlink、非法路径）；
5. 保持体积红线（< 20 MiB）与跨平台约束。

**不解决**：整合包导出、在线整合包仓库浏览、整合包更新 UI（overrides 的更新保留
语义已实现，供后续接入）、多 loader 叠加安装（Forge + OptiFine 等在 RMCL 中只安装
主 loader，其余以 warning 呈现）、HMCL MultiMC JSON-Patch 的逐字段无损变换（RMCL
通过既有 loader 安装器达成同样的主流结果）。

## 选项

### A. 全部塞进 `rmccc-core`

- 优点：不新增 crate。
- 缺点：格式解析、zip 安全、Curse API、解压策略约 2k 行，会把 core 变成杂物间；
  `rmccc-core` 的职责是编排与 DI，不是格式实现。

### B. 新建 `rmccc-modpack` + core 编排（采纳）

- 优点：格式/解压/元数据可独立单测；core 只负责“按顺序调用已有能力”；
  与 HMCL 的 provider/install task 分层同构。
- 缺点：多一个 crate 的编译单元（约 2k 行，zip/encoding_rs 已是现有依赖树）。

### C. 只支持 Modrinth `.mrpack`

- 优点：最小可用。
- 缺点：不满足“所有 HMCL 支持的整合包类型”的要求。

## 决策

采纳 **B**，并确定以下契约：

### 1. 格式识别顺序

与 HMCL `ModpackHelper.readModpackManifest` 一致：

`mcbbs.packmeta`/MCBBS `manifest.json` → Curse `manifest.json` →
`modrinth.index.json` → HMCL `modpack.json` + `minecraft/pack.json` →
MultiMC `instance.cfg` + `mmc-pack.json` → `server-manifest.json` →
手动打包（含 `versions/` 的 `.minecraft` 目录）。

### 2. 安装目标

整合包落成一个**版本即实例**（ADR-016）：

- 无 loader：`install_game_into(mc, target_id)` 直接把实例写到 `versions/{target_id}`；
- 有 loader：先在标准 id 下装原版，装完 loader 后 `rename_instance` 到用户命名；
- 实例运行目录固定为 `versions/{id}`（`isolation = always`），与 HMCL 给整合包设置
  `runningDirectory` 的语义一致；
- `modpack.cfg` 写在实例根，字段与 HMCL `ModpackConfiguration` 对齐
  （`manifest`/`type`/`name`/`version`/`overrides[{path,hash,downloadURL?}]`）。

### 3. 覆盖文件与用户修改

`overrides/`、`client-overrides/`、`.minecraft/`、`minecraft/` 均按 HMCL
`ModpackInstallTask` 的更新语义处理：

- 新包新增的文件：写入；
- 旧包有、新包也有：磁盘文件仍等于旧 hash 才覆盖（用户改过的保留）；
- 用户删掉的：保持缺失；
- 新包删掉的文件：删除；
- HMCL 格式的 `minecraft/pack.json` 不进入 overrides。

### 4. 文件下载

- Modrinth：按 `files[].downloads`，跳过 `env.client == unsupported`；
- Curse：优先用 manifest 的 `fileName` 拼 `edge.forgecdn.net`；缺失时查
  CurseForge 元数据（`CURSEFORGE_API_KEY`，无 key 时回退到 HMCL BMCLAPI 规则里
  已使用的 `mod.mcimirror.top/curseforge` 公开镜像）；
- MCBBS/Server：`fileApi + /overrides/{path}`，带 SHA-1 校验；
- 所有 URL 再经 `DownloadProvider` 注入镜像候选；
- 下载失败按 HMCL 语义**容忍**：实例保留，warning 交给 UI，后续可修复。

### 5. 设置与图标

- MultiMC `instance.cfg` 的 Override* 字段 → `InstanceSettingsPatch`；
- MCBBS `launchInfo.minMemory/javaArgument/launchArgument` → 内存/JVM/游戏参数；
- zip 根 `icon.*` 或 MultiMC `iconKey` → `versions/{id}/icon.png` + `InstanceIcon::File`；
- 包声明的额外库（MCBBS `libraries`）以 `mcbbs` patch 追加到版本 JSON。

### 6. IPC 与拖拽

- `modpack_inspect` / `modpack_pick_file` / `modpack_install`；
- 进度走统一任务模型（`task://updated`，ADR-019）；`modpack_cancel` 已由 `task_cancel` 取代；
- 前端在 `App` 根节点监听 HTML5 拖放（`dragDropEnabled=false`，与插件安装一致），
  松开后先 inspect、再弹向导确认实例名/目录；
- 手动整合包解压到 `{data_dir}/externalgames/{name}` 并注册为外部游戏目录。

## 后果

正面：

- 七类 HMCL 整合包都有解析与安装路径；
- 复用现有 loader 安装器/下载器/实例设置，不产生第二套游戏安装逻辑；
- 格式解析、zip 安全、更新语义均有 crate 内单测；
- 所有新依赖（`encoding_rs`）只影响 GBK 文件名兼容，zip 已在体积预算内。

负面/代价：

- 单 loader 限制：多 loader 包只装主 loader 并 warning；
- MultiMC 的 JSON-Patch 无损变换没有复刻，未知组件只保留 metadata；
- Curse 无 API key 时依赖第三方只读镜像；
- `modpack.cfg` 的 `manifest` 对 Curse 在解析后会被补全文件名/URL（与 HMCL
  `CurseCompletionTask` 的行为一致，但不再是原始字节）。

## 迁移影响

- 新增 workspace crate `rmccc-modpack` 与 core 模块 `modpack`；
- `src-tauri` 新增 4 个 IPC 命令、1 个事件、4 个 TS DTO；
- 前端新增 `stores/modpacks.ts`、`ModpackInstallDialog`、App 级拖放与任务中心行；
- 不修改既有 profile/instance/launch 数据结构；回滚只需移除新模块与命令注册。

## 跨平台验证方式

- 单元测试覆盖七种格式、zip 路径逃逸、GBK 名称、更新保留语义；
- macOS 本机手测：拖入 Curse/Modrinth 包 → 安装 → 启动；
- 路径处理只使用 `std::path` 与 `/` 归一化，Windows 保留盘符/保留名拒绝；
- 非 macOS 的 GUI 拖放/落地验证按指南 §1.4 记为 deferred。
