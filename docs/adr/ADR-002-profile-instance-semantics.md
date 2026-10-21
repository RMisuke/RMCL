# ADR-002: Profile 与 Instance 的语义分界

- **状态**：accepted（2026-10-21 定稿）
- **日期**：2026-10-21
- **阶段**：Phase 1（切片 1.7）
- **关联**：审计 P2-10、§7.4、P2-9；指南 §3.4、§4

## 背景

RMCCC 同时存在两套"实例"概念：

| 概念 | 标识 | 存储 | 谁在用它 |
|---|---|---|---|
| **profile** | UUID | `profiles/{id}.json` | 用户可见的实例卡片；启动、设置、mod 管理都以它为单位 |
| **instance**（= 版本 id） | 版本字符串（`1.20.4`、`1.20.4-fabric-0.15.11`） | `versions/{id}/` + `GameRepository` 索引 | 磁盘布局与仓库解析 |

Phase 0.5 把 UI 接上 `instance_*` 之后暴露了一个真实缺陷：`InstanceSettings` 用 **profile id**
调用 `instance_check_completion` / `instance_repair`，而这两个命令把参数当**版本 id** 去
`GameRepository` 里查 → 永远 `NotFound`，页面的"完整性检查/修复"按钮实际不可用。

同时 `profile.game_dir` 已在 Phase 1 前收敛为**共享游戏根**（`resolved_game_root()`），
使得 `profile_delete(cleanup_game_dir=true)` 会删掉**所有**实例共用的 `versions/`、`assets/`、
`libraries/`——一个被文档描述为"选项"的参数实际上是全库删除。

## 决策

### 1. profile 是唯一用户可见实体

- 用户视角的一切操作（启动、设置、mod、隔离、导入导出）以 **profile id** 为标识。
- **instance / 版本 id 只在磁盘布局与仓库解析层面存在**，不作为用户概念暴露。

### 2. `instance_*` 命令按"是否依赖 profile 字段"分成两组

| 命令 | 参数语义 | 理由 |
|---|---|---|
| `instance_paths` | **profile id** | 返回值包含 `java_path` / `isolation` 等 profile 字段 |
| `instance_check_completion` | **profile id，回落版本 id** | 完整性属于某个实例的运行配置 |
| `instance_repair` | **profile id，回落版本 id** | 同上 |
| `instance_list` / `instance_refresh` | 无参 | 返回**版本仓库**视图（`missing_count` 按版本统计） |
| `instance_rename` / `instance_duplicate` / `instance_remove` | **版本 id** | 操作的是 `versions/{id}/` 目录本身 |

"回落版本 id"指：先按 profile id 查 profile，命中则用它的 `game_version`；未命中再当作版本 id
直接查仓库；两者都不存在才返回 `ProfileNotFound`。这样 UI 传 profile id 正确，
脚本/调试传版本 id 也仍然可用。

### 3. `profile_delete` 永不删除共享游戏根

`ProfileStore::delete(id, cleanup_game_dir)` 的 `cleanup` 语义收紧为：

- **只**删除该 profile **自己的**运行目录（`outbound`/隔离实例的 `versions/{id}/`），
  且必须在 `game_dir != resolved_game_root()` 时才允许；
- 指向共享游戏根时 `cleanup` 被忽略（记 warn），只删 `profiles/{id}.json`。

删除实例不应影响其它实例的游戏文件；清理资源请用 `instance_remove`（按版本 id 删单个版本目录）。

### 4. Core Service 门面边界（Phase 2+ 目标）

对外门面按**能力**划分，不按 crate 划分：

| 门面 | 职责 | 现状 |
|---|---|---|
| `GameService` | 安装 / 修复 / 启动 / 版本解析 | 已收敛为 `rmccc_core::pipeline`（1.2） |
| `LoaderService` | mod loader 版本与安装 | 已收敛为 `rmccc_core::modloader`（1.3） |
| `ModService` | mod 搜索 / 安装 / 启停 | 部分在 `commands/mods.rs`，待 2.x 下沉 |
| `InstanceService` | profile 生命周期与生效值 | `commands/profile.rs` + `commands/instance.rs`，待 2.x 下沉 |

Tauri 与 CLI 只允许调用这些门面；`pipeline` / `modloader` 已经是这个形态。

## 后果

**正面**
- UI 的"完整性检查/修复"恢复可用（此前静默失败）。
- 不可能通过删实例误删整个游戏库。
- 用户概念只剩一个，文档与 UI 文案统一。

**负面 / 代价**
- 同一个 `instance_*` 前缀下存在两种 id 语义（已在表中逐条写明；Phase 2 重命名为
  `version_*` 可彻底消除）。
- 回落查找多一次 `profile_store.get`，可忽略。

## 状态变更记录

- 2026-10-20：proposed（占位）。
- 2026-10-21：**accepted**。落地改动：`instance_check_completion` / `instance_repair` 改为
  profile 优先 + 版本 id 回落；`ProfileStore::delete` 增加共享根保护；文档与命令注释同步。
