# 11 · IPC 接口覆盖矩阵

> 验收基准：后端全部命令 + 事件在新 UI 中的唯一归属。
> 权威命令清单以 `src-tauri/src/handlers.rs` 为准。**状态（2026-10-23，ADR-013 S4–S7 完成）**：
> 注册表已达设计目标 **93 条**（`agent_*` 9 条移除；`ai_*` 5 + mods 2 +
> `mcp_*` 9 全部落地，见 §1.5）。设置页 MCP 分区 / 确认卡的 UI 随整体前端重构接线。
> 实施后端新增/变更命令时必须同步本表。

图例：「页面」引用 docs/ui-design/ 对应文档；**加粗** = 该命令在新 UI 中首次获得入口
（现状 UI 未用或无 wrapper）。

## 1. 命令矩阵（86 条）

### 认证/账号（auth.rs · 8 条）

| 命令 | 页面 | 触发点 |
|---|---|---|
| `auth_get_session` | 00 Shell / 02 主页 / 06 账号 | App 启动 authStore 恢复会话；快速启动卡状态判定 |
| `auth_microsoft_login` | 06 账号 | 「+ Microsoft 账号」弹窗发起设备码流程 |
| `auth_offline_login` | 06 账号 / 02 主页 | 离线账号弹窗；快速启动卡「以离线身份启动」 |
| `auth_logout` | 06 账号 / 02 主页 | 账号卡菜单、账号页 |
| `auth_refresh_token` | 06 账号 | 账号行 [⋯] → 刷新（仅 Microsoft） |
| `auth_list_accounts` | 06 账号 | 账号列表 |
| `auth_switch_account` | 06 账号 / 02 主页 | 账号卡 [切换 ▾]、列表「设为当前」 |
| `auth_remove_account` | 06 账号 | 账号行 [⋯] → 移除（确认） |

### 实例 Profile（profile.rs · 4 条）

| 命令 | 页面 | 触发点 |
|---|---|---|
| `profile_list` | 02/03/04/05/07 | profilesStore 唯一入口：主页卡、实例列表、详情、向导重名校验、Agent 上下文选择器 |
| `profile_create` | 05 下载中心 | 安装向导步骤 ③ |
| `profile_update` | 04 实例详情 / 03 列表 | 设置 tab 保存；列表右键重命名 |
| `profile_delete` | 04 / 03 | 管理菜单/右键删除（确认） |

### 版本（version.rs · 2 条）

| 命令 | 页面 | 触发点 |
|---|---|---|
| `version_list` | 05 向导① / 02 速装卡 | 版本列表（类型过滤+搜索）；快捷版本芯片 |
| `version_install` | 05 向导④ | 提交安装（后台执行，事件跟踪） |

### 下载控制（download.rs · 3 条）

| 命令 | 页面 | 触发点 |
|---|---|---|
| `download_cancel` | 10 任务中心 / 02 下载卡 | 批次 [✕ 取消]（确认） |
| `download_pause` | **10 任务中心** | 批次 [⏸]（现状 UI 只做 cancel，首次接通） |
| `download_resume` | **10 任务中心** | 批次 [▶] |

### 启动与进程（launch.rs · 3 条）

| 命令 | 页面 | 触发点 |
|---|---|---|
| `launch_start` | 02/03/04 | 快速启动卡、列表行内、详情工具条 |
| `launch_kill` | 02/03/04/10 | 运行态 [■ 停止] |
| `launch_status` | 00 Shell | App 启动恢复运行态（launchStore 初始化） |

### 版本仓库/完整性（instance.rs · 9 条）

| 命令 | 页面 | 触发点 |
|---|---|---|
| `instance_list` | **08 设置-高级-版本仓库** | 表格数据（首次接通） |
| `instance_refresh` | **08 版本仓库** | [重新扫描]；`instances://refreshed` 事件兜底 |
| `instance_rename` | **08 版本仓库** | 行 [⋯] → 重命名 |
| `instance_duplicate` | **08 版本仓库** | 行 [⋯] → 复制 |
| `instance_remove` | **08 版本仓库** / 04 详情 | 行删除；详情「删除并清理版本文件」 |
| `instance_check_completion` | 04 概览/维护 | [检查完整性]（Hash 级，按钮注明耗时） |
| `instance_repair` | 04 维护 | [修复全部缺失]（返回值 N 展示） |
| `instance_paths` | 04 概览/维护/Mod管理 | 路径卡、隔离预览、浏览菜单路径源 |
| `instance_resolved_settings` | 04 设置 | 继承模型生效值副标题 |

### Modloader（modloader.rs · 2 条）

| 命令 | 页面 | 触发点 |
|---|---|---|
| `modloader_list` | 04 加载器 tab / 05 向导② | loader 版本列表弹窗 |
| `modloader_install` | 04 加载器 tab / 05 向导④ | 确认安装（事件跟踪阶段进度） |

### 模组（mods.rs · 9 条）

| 命令 | 页面 | 触发点 |
|---|---|---|
| `mod_search` | 05 Mod 仓库 | 搜索卡（query/版本/loader/分页；防抖+防竞态） |
| `mod_get_versions` | 05 Mod 仓库 | 详情页版本分组 |
| `mod_get_download_url` | **05 Mod 仓库** | 版本行「复制下载链接」（首次接通） |
| `mod_list` | 04 Mod 管理 / 05 仓库 | 实例 mod 列表；详情页「已安装」标记 |
| `mod_toggle` | 04 Mod 管理 | 行 checkbox 启停 |
| `mod_remove` | 04 Mod 管理 | 行 [⋯] → 删除（确认） |
| `mod_install` | 05 Mod 仓库 | 「安装到实例」（SHA1 后端校验） |
| `mod_install_local` | **04 Mod 管理** | 拖拽 jar 到列表（首次接通） |
| `mod_pick_and_install` | 04 Mod 管理 | [+ 添加本地文件] 原生选择器 |

### 设置/Java（settings.rs · 6 条）

| 命令 | 页面 | 触发点 |
|---|---|---|
| `settings_get` | 08 全部分区 | 进页加载 AppConfig |
| `settings_set` | 08 游戏默认/下载 | 变更即存（防抖由后端保证） |
| `settings_get_user` | 08 通用 / 00 Shell | 语言/主题/更新偏好（i18n、theme 模块现有接线） |
| `settings_set_user` | 08 通用 | 语言、`check_updates` 开关 |
| `settings_supported_locales` | 08 通用 | 语言下拉（i18n 现有接线保留） |
| `java_detect` | 08 游戏默认 / 04 设置 | Java 选择器（含版本/厂商/64bit 副标题） |

### Dashboard/主题（dashboard.rs · 9 条）

| 命令 | 页面 | 触发点 |
|---|---|---|
| `dashboard_get_layout` | 02 主页 | 进页加载（后端归一化） |
| `dashboard_save_layout` | 02 主页 | 退出编辑模式全量提交 |
| `dashboard_reset_layout` | 02 主页 | 重置布局（确认） |
| `dashboard_list_widgets` | 02 主页 | 编辑模式「+ 添加卡片」抽屉 |
| `dashboard_list_themes` | 08 通用-外观 | 主题卡列表 |
| `dashboard_get_theme_tokens` | 08 外观 / 00 theme 模块 | 色板预览；全局换肤（现有接线） |
| `dashboard_set_theme_preference` | 08 外观 | 模式三选 + 主题卡选中 |
| `dashboard_get_theme_preference` | 08 外观 / 00 theme 模块 | 进页回显（现有接线） |
| `dashboard_get_theme_json` | **08 外观** | 「查看主题 JSON」调试折叠入口（首次接通） |

### 插件（plugin.rs 16 + plugin_ui.rs 1 + plugin_logic.rs 2 = 19 条）

| 命令 | 页面 | 触发点 |
|---|---|---|
| `plugin_list` | 09 / 08 插件分区 / 00 Rail | 列表+诊断（Rail 黄点徽标） |
| `plugin_inspect` | 09 详情 | 选中插件加载详情 |
| `plugin_install` | **09** | 拖拽 .rmcpkg 安装（path 版首次接通） |
| `plugin_pick_and_install` | 09 | [+ 安装] 原生选择器 |
| `plugin_enable` / `plugin_disable` | 09 概要 | [启用/禁用] |
| `plugin_remove` | 09 概要 | 卸载（delete_data 可选） |
| `plugin_rollback` | 09 概要 | 回滚上一版本 |
| `plugin_grant` / `plugin_revoke` | 09 权限 | 待审授予 / 已授予撤销 |
| `plugin_config_get` | **09 配置** | 独立刷新（现状借用 inspect，首次独立接通） |
| `plugin_config_set` | 09 配置 | SchemaForm 保存 |
| `plugin_config_reset` | 09 配置 | 重置默认 |
| `plugin_capabilities` | 09 权限 | 17 项能力目录折叠区 |
| `plugin_contributions` | **09 贡献** | 卡片/设置页/主题列表 + 跳转（首次接通） |
| `plugin_ui_invoke` | 02 卡片 / 08 插件设置页 | iframe 桥唯一通道（现有 bridge.ts 复用） |
| `plugin_logic_invoke` | **09 逻辑 tab** | compat.check / mods.source.search 调试台（首次接通） |
| `plugin_logic_cancel` | **09 逻辑 tab** | [取消] 运行中调用（首次接通） |

### Agent（agent.rs · 9 条）——✅ 已替换完成（2026-10-23，ADR-013 S7）

内建 Agent 平台退役（见 `docs/AI_AND_MCP_ARCHITECTURE.md` §7 迁移表），
9 条命令的替换关系（迁移记录）：

| 命令 | 处置 |
|---|---|
| `agent_config_get/set` | ✅ → `ai_config_get/set` + `ai_models_probe`（配置迁入全局 AI 服务，设置-#ai；vault service=`llm` 无缝继承） |
| `agent_run` / `agent_cancel` | ✅ **已移除**（内建编排循环退役；编排交外部 AI 工具经 MCP） |
| `agent_session_list` / `agent_session_read` / `agent_clear_history` | ✅ **已移除**（会话存储退役，ADR-010 修订） |
| `agent_tasks_pending` / `agent_task_resolve` | ✅ → `mcp_confirmations_list` / `mcp_confirmation_resolve`（后端已落地，确认卡 UI 随前端重构） |

> ✅ 迁移已完成：旧命令已从注册表移除，`/agent` 路由重定向设置页，
> 前端无 `agent_*` 引用。

### 日志/外部/诊断（logs.rs + external.rs + ui.rs · 4 条）

| 命令 | 页面 | 触发点 |
|---|---|---|
| `log_get` | 08 通用-日志 | 日志查看器（行数可调 ≤500，级别着色） |
| `log_clear` | 08 通用-日志 | 清空（确认） |
| `open_external_url` | 06/05/08/09 | 设备码链接、Mod 浏览器下载、关于/帮助链接、插件 manifest 外链 |
| `frontend_log` | 00 Shell | App.tsx 引导守卫现有接线保留 |

## 1.5 设计新增：AI 与外部控制（16 条，ADR-013）

**状态（2026-10-23）**：AI 服务 5 条 ✅、Mod 纯逻辑 2 条 ✅、MCP 9 条 ✅ 全部后端已实现；
新命令返回值统一 camelCase（ts-rs `rename_all`，与 dashboard/plugin 域一致）。
外部面另有 **MCP 工具 31 个**（不走 Tauri IPC，目录见架构文档 §5.2）。

| 组 | 命令 | 页面 / 触发点 |
|---|---|---|
| AI 服务（5） | `ai_config_get` / `ai_config_set` | 设置-#ai：回显/保存（key 入 vault，service=`llm`） |
| | `ai_models_probe` | 设置-#ai：输入地址自动检测模型（防抖） |
| | `ai_analyze_log` / `ai_analyze_cancel` | 日志抽屉：AI 分析（流式事件 `ai://delta/done/error` 回传） |
| MCP（9） | `mcp_status` / `mcp_set_enabled` | 设置-#ai：总开关、端口、会话状态 |
| | `mcp_regenerate_token` | 设置-#ai：令牌重生成（踢掉全部会话） |
| | `mcp_tools_list` / `mcp_tools_set` | 设置-#ai：31 工具权限矩阵（allow/confirm/deny） |
| | `mcp_sessions_disconnect` | 设置-#ai：断开外部会话 |
| | `mcp_confirmations_list` / `mcp_confirmation_resolve` | 任务中心：确认卡（07 §3.3） |
| | `mcp_recent_calls` | 设置-#ai：最近 Agent 操作（审计脱敏视图） |
| Mod 纯逻辑（2） | `mods_dependency_scan` / `mods_apply_plan` | 实例详情-Mod 管理：检查缺失前置 / 一键补装（依赖扫描去 Agent 化，P14） |

## 2. 事件矩阵（10 个）

| 事件 | 消费 Store | UI 落点 |
|---|---|---|
| `download://progress` | downloadStore | Rail 环、任务中心、02 下载卡、05 向导④ |
| `download://complete` | downloadStore→profilesStore | 任务中心完成态、toast、向导④推进 loader 步 |
| `download://error` | downloadStore | 任务中心失败详情、toast、向导④失败态 |
| `auth://device-code` | authStore | 06 设备码弹窗 |
| `launch://started` | launchStore | 运行徽标（02/03/04/Rail）、日志抽屉开始 |
| `launch://output` | launchStore（环形缓冲） | 日志抽屉 |
| `launch://exited` | launchStore | 徽标消失、异常退出自动弹抽屉 |
| `modloader://progress` | modloaderStore | 任务中心 loader 区、04 加载器卡 loading |
| `instances://refreshed` | profilesStore/版本仓库视图 | 实例列表、版本仓库表刷新 |
| `download://paused` / `download://resumed` | （预留） | 后端当前不发射；任务中心预留状态位（00 §9-P3） |
| `ai://delta` / `ai://done` / `ai://error`（设计新增） | aiStore | 日志抽屉 AI 分析流式面板（07 §4） |
| `mcp://session` / `mcp://confirmation`（设计新增） | mcpStore | 设置会话列表、任务中心确认卡（07 §3） |
| 既有任务事件 payload 增加 `origin` 字段（设计新增） | 各任务 store | Agent/插件发起操作的归因徽标与 toast（07 §3） |

## 3. api.ts 实施修正清单

| 项 | 问题 | 修正 |
|---|---|---|
| `AgentRunOutput` | 前端手写镜像 `session_id/tool_calls`，后端实发 camelCase | ✅ 已随 `agent_*` 移除而消解（Agent.tsx 删除，types.ts 再生成） |
| `instances.refresh` | 声明返回 `number`，实际 `Vec<InstanceInfo>` | 修正返回类型 |
| `instances.repair` | 声明返回 `void`，实际 `usize` | 修正（展示「已修复 N 个文件」） |
| 新增 wrapper | `mod_install_local`、`instance_list/rename/duplicate/remove`、（可选）`plugin_install(path)` 直用 | 补 wrapper + ts-rs 类型 |

## 4. 大小写/命名陷阱速查（写代码必读）

- **入参**：Rust 命令参数一律 snake_case，api.ts 传 camelCase（Tauri 自动转换）——现有约定不变。
- **返回值**按域分两派：

| 域 | 返回值命名 | 例 |
|---|---|---|
| auth / profile / version / download / launch / instance / modloader / mods / settings / logs | snake_case（Rust 原生 serde） | `profile.game_version`、`mod_list.mods_dir` |
| dashboard / plugin / plugin_logic / plugin_ui | camelCase（serde rename_all） | `widget.instanceId`、`plugin.state` |
| ai / mcp / mods 新增命令（设计） | camelCase（ts-rs `rename_all`，§1.5 新面统一约定） | `aiConfig.baseUrl`、`mcpStatus.port` |

- **id 双语义**：`instance_paths/check_completion/repair` 接 **profile id**（后端回落版本 id）；
  `instance_rename/duplicate/remove` 接**版本 id**（版本仓库页专用）。两页文档已按此约定。
- Modloader IPC 的 loader_type 用无下划线形式：`neoforge/liteloader/optifine/legacyfabric`
  （Rust 枚举是 `neo_forge` 等，但 IPC 层接受无下划线形式——沿用现状映射表）。

## 5. 覆盖率结论

- 除已退役的 `agent_*` 9 条外，既有命令与事件全部有唯一 UI 归属；其中 **15 条命令在新设计中首次获得入口**
  （上表加粗：download_pause/resume、instance_list/refresh/rename/duplicate/remove、
  mod_get_download_url、mod_install_local、dashboard_get_theme_json、plugin_install(path)、
  plugin_config_get、plugin_contributions、plugin_logic_invoke/cancel）。
- **ADR-013 迁移已完成（S4–S7）**：`agent_*` 9 条移除，`ai_*` 5 + mods 2 +
  `mcp_*` 9 全部落地——目标命令面 **93 条**达成（`ipc_tests` 计数断言 93）。
  外部另有 MCP 工具 31 个（非 Tauri IPC，`src-tauri/src/mcp/`）。
- 依赖后端配套（00 §9）才完整生效的 UI：浏览目录菜单（P1 open_path）、loader 移除（P2）、
  下载暂停回执（P3）、主页下载卡（P4）、选中实例持久化（P5）、loader 分析（P6）、
  Mod 元数据/依赖（P7）、CurseForge（P8）、更新流程（P9）、整合包（P10）、
  AI 服务（P12）、MCP 服务器（P13）、依赖扫描 IPC（P14）、事件 origin（P15）。
  以上均有降级形态（灰置 + tooltip / 占位卡 / S 切片前置），不阻塞整体交付。
