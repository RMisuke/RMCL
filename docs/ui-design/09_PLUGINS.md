# 09 · 插件管理页

> 路由 `/plugins`。声明式插件平台的管理中枢：安装、启停、权限、配置、贡献、逻辑模块、
> 诊断——19 条插件域 IPC 的全部宿主。

## 1. 设计目标

1. master-detail：左列表 + 右详情，一眼看状态（11 态状态机 + 诊断）。
2. 权限模型透明：低风险自动授予、中高风险必须人工批——待审醒目、可撤销、撤了会怎样说清楚。
3. 配置从「手写 JSON textarea」升级为 **JSON Schema 驱动的表单**（数据链路已具备，
  `plugin_config_get` 返回 schema）。

## 2. 布局

```
┌──────────────────┬──────────────────────────────────────┐
│ 工具条：          │  详情区（RmTabs 顶置 tab）             │
│ [+ 安装 .rmcpkg]  │  ┌────────────────────────────────┐  │
│ [⟳ 刷新]          │  │ server-dashboard  v1.1.0  [启用] │  │
│──────────────────│  │ 概要 | 权限 | 配置 | 贡献 | 逻辑   │  │
│ ⚠ 诊断横幅        │  │  …tab 内容…                      │  │
│──────────────────│  └────────────────────────────────┘  │
│ ● server-dashboard│                                      │
│ ○ theme-midnight  │                                      │
│ ○ （禁用态灰）     │                                      │
└──────────────────┴──────────────────────────────────────┘
（空态：安装引导 + 指向 examples/plugins 的说明）
```

- 左列表行：name + version + 状态 chip + trust tag（official/development）+ 贡献计数
  （卡片 N · 主题 N · 设置页 N）+ lastError 红点。
- 诊断横幅：`plugin_list` 返回 diagnostics（warning/error）非空时置顶，点击展开明细。

## 3. 安装

- 主按钮 `plugin_pick_and_install`（选择器过滤 .rmcpkg）；
- **拖拽安装**：拖 .rmcpkg 文件到本页 → Tauri onDragDrop 拿路径 → `plugin_install(path)`
  （这是 path 版命令的 UI 入口）；
- InstallReport 处理：`requires_review=true`（有 pending 权限）→ 安装 toast「已安装，
  需要授予权限」+ 直接跳转该插件**权限 tab**（待审区高亮）；`config_error` →
  跳配置 tab 显示错误。

## 4. 详情 tab

### 4.1 概要

manifest 信息（发布者、描述、版本与 previousVersion）、安装路径、安装/更新时间、
状态机可视化（11 态 chip 配色：enabled=success / disabled=neutral / failed=danger…）。
动作区：[启用/禁用]（`plugin_enable/disable`）、[回滚到 v{previous}]（`plugin_rollback`，
仅 previousVersion 存在时）、[卸载]（`plugin_remove`，确认弹窗含「同时删除插件数据」
checkbox → delete_data）。

### 4.2 权限

```
待审批（中/高风险，需你批准）           ← 红框置顶（仅 pending 非空时）
  ⚠ network.request  高风险  [查看说明]  [授予] [拒绝*]
已授予
  ✓ dashboard.cards   低风险  授予于 …   [撤销]
  ✓ instance.read     低风险  …
已声明未申请
  – themes.apply      …
─── 能力目录（plugin_capabilities 17 项，折叠）───
能力名 | 风险(low绿/medium黄/high红) | 说明 | 域名作用域 | 实例作用域 | 可执行
```

- `plugin_grant/revoke` 即时生效；撤销 network.request 时确认弹窗明说
  「该插件的卡片将无法再访问网络」。
- 拒绝（*）：拒绝 ≠ revoke——设计为「忽略待审」（不授予、不卸载，插件功能受损但不弹）；
  后端无 deny 状态，实现为本地忽略列表（localStorage），说明文案「已忽略，可在插件重新安装时重审」。
- 能力目录随 ADR-013 扩充 **`ai.chat`**（插件调用宿主 AI 端口，风险 medium，需显式授予）：
  桥方法 `llm.chat` 支持流式（`llm.delta/done/error` 推送），宿主代持密钥、插件拿不到
  key 本体，限流 6 次/分钟/插件（07 §6）。

### 4.3 配置

- `plugin_config_get` → config.values + schema（JSON Schema）；
- **Schema 表单渲染器**（新组件 `RmSchemaForm`）类型映射：

| JSON Schema | 控件 |
|---|---|
| string | RmInput（format: uri 加链接图标） |
| string + enum | RmSelect |
| number/integer | 数字输入（min/max 步进） |
| boolean | RmSwitch |
| array | 列表编辑（增删行，items 递归） |
| object | 分组递归 |

- 无 schema 的插件 → 回退 textarea + 保存前 `JSON.parse` 校验（现状行为保留）；
- 保存 `plugin_config_set`（后端 schema 校验，错误展示在对应字段下）、重置 `plugin_config_reset`。

### 4.4 贡献

`plugin_contributions`（DashboardCardView[]/settingsPages/themes）三组列表；
每项尾随跳转：卡片 →「前往主页」（该卡片在 Dashboard 的位置闪烁高亮一次）、
主题 →「前往外观设置」、设置页 →「前往插件设置」（/settings#plugins）。

### 4.5 逻辑（仅 manifest 有 entry.logic 的插件显示此 tab）

```
逻辑模块（WASM，独立沙箱进程执行）
├─ ⚙ compat.check
│    输入: [JSON___________]  超时: [30s▾]
│    [运行] [⏹ 取消]（plugin_logic_cancel）
│    输出: { … } / 错误（超时/载荷超限 人话化）
└─ ⚙ mods.source.search
     同上
```

- 面向插件作者/高级用户的调试台（call 白名单 v1 仅这 2 个）；
  运行前提示「将以独立进程执行，超时自动终止」。

## 5. 依赖与卸载保护

- 依赖不满足（DependencyStatus.satisfied=false）：概要 tab 显示黄条
  「缺少依赖 xxx vN（可选）」；被依赖（dependents 非空）的插件卸载时确认弹窗列出
  「以下插件依赖它：…，卸载后它们将不可用」。

## 6. 状态与错误

- 列表加载失败：整页 RmErrorState 重试。
- 单插件 lastError：左列表红点 + 概要 tab 顶部 danger 条（原文 + 复制）。
- 启用/安装失败：toast + 详情区刷新（state=failed 时概要显示原因）。

## 7. 接口映射（插件域 19 条全覆盖）

`plugin_list/inspect/install/pick_and_install/enable/disable/remove/rollback/grant/revoke/
config_get/config_set/config_reset/capabilities/contributions`、`plugin_ui_invoke`
（设置页 iframe 在 08，卡片在 02；本页不直接用桥）、`plugin_logic_invoke/cancel`。

## 8. 渐进披露自查

一级：列表、安装、启停、待审权限；
二级：配置表单、贡献跳转、回滚；
三级：能力目录全文、逻辑调试台、manifest 原始 JSON（概要 tab 折叠）。

## 9. i18n key 提案（节选）

`plugins.title` / `plugins.install` `plugins.install.dropHint` `plugins.install.requiresReview` /
`plugins.list.state.enabled|disabled|failed|…` / `plugins.diagnostics {n}` /
`plugins.detail.tab.overview|permissions|config|contributions|logic` /
`plugins.perms.pending` `plugins.perms.granted` `plugins.perms.revoke` /
`plugins.perms.revoke.networkNote` `plugins.perms.ignore` /
`plugins.config.saved` `plugins.config.invalid` `plugins.config.reset` /
`plugins.contrib.cards` `plugins.contrib.themes` `plugins.contrib.pages` /
`plugins.logic.run` `plugins.logic.cancel` `plugins.logic.timeout` /
`plugins.remove.title` `plugins.remove.deleteData` `plugins.remove.dependents {list}` /
`plugins.rollback` `plugins.empty.*`

## 10. 验收要点

- 安装 server-dashboard → 待审权限流（network.request 需授）→ 授予 → 主页卡片可用 →
  撤销 → 卡片 403 占位。
- RmSchemaForm 对 `examples/plugins` 两个样例的 config.schema.json 渲染正确、
  校验错误回显、reset 恢复默认。
- 拖拽 .rmcpkg 安装路径可用（plugin_install(path) 首次接通）。
- theme-midnight 安装 → 贡献 tab 跳外观设置 → 主题可选用。
- 逻辑 tab：compat.check 带 demo 输入运行成功 + 超时取消路径。
