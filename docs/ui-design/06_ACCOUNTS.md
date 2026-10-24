# 06 · 账号页

> 路由 `/accounts`。合并现有 `/login`（登录页不再是独立路由，未登录时账号页自身承担引导态）。

## 1. 设计目标

1. 账号的添加、切换、移除、刷新一页完成；
2. Microsoft 设备码流程可视、可复制、可取消；
3. 安全感：让用户知道凭据被加密保存（ADR-009），但**永不展示**任何 token。

## 2. 布局

```
┌──────────────────────────────────────────────────────┐
│ PageHeader：账号                                       │
├──────────────────────────────────────────────────────┤
│ ┌─ 当前会话（RmCard，仅登录时）───────────────────────┐│
│ │ (S) Steve   Microsoft 账号        [刷新登录] [⋯]    ││
│ │             UUID: xxxx-…-xxxx（点击复制）            ││
│ └──────────────────────────────────────────────────┘│
│ ┌─ 账号列表 ─────────────────────────────────────────┐│
│ │ ● (S) Steve    Microsoft    当前  [⋯]              ││
│ │ ○ (A) Alex     离线          [设为当前] [⋯]         ││
│ └──────────────────────────────────────────────────┘│
│ ┌─ 添加账号 ─────────────────────────────────────────┐│
│ │ [+ Microsoft 账号]  [+ 离线账号]                    ││
│ └──────────────────────────────────────────────────┘│
│ 🔒 安全说明（折叠脚注）：登录凭据以 AES-256-GCM 加密保存于本机…│
└──────────────────────────────────────────────────────┘
```

- 列表 = `auth_list_accounts`（AccountMetadata：type/account_id/profile_name/profile_id）；
  当前会话 = `auth_get_session`（取 username/uuid，**忽略 access_token/refresh_token 字段，
  禁止渲染**）。
- 行 [⋯]：设为当前（`auth_switch_account`）/ 刷新（`auth_refresh_token`，仅 Microsoft）/
  移除（`auth_remove_account`，确认文案含「将删除本机保存的该账号登录凭据」）。

## 3. Microsoft 登录流程（设备码）

点击 [+ Microsoft 账号] → `RmDialog`（宽 480px）：

```
┌─ 登录 Microsoft 账号 ────────────────────────┐
│  1. 点击下方按钮打开浏览器                      │
│     [打开 microsoft.com/link ↗]               │  ← open_external_url(verification_uri)
│  2. 输入代码                                   │
│     ┌───────────────────────┐                 │
│     │   X K M J - 4 8 2 9   │  [复制]          │  ← user_code 大字 20px
│     └───────────────────────┘                 │
│  ⏳ 等待你在浏览器中完成登录…                    │
│  [取消]                                       │
└──────────────────────────────────────────────┘
```

- `auth_microsoft_login` 发起后监听 `auth://device-code` → 显示 user_code/verification_uri
  （后端会自动开浏览器，前端按钮作为补救入口）；轮询由后端完成，成功返回 AuthSession
  → 关闭弹窗 + toast「已登录为 Steve」；失败（超时/拒绝）→ 弹窗内错误态 + 重试。
- 取消：直接关弹窗（后端轮询自然超时；后续可加显式中断，非必须）。

## 4. 离线账号

`RmDialog`：用户名输入（校验 ≤32 字符、非空）→ `auth_offline_login` → 自动设为当前。

## 5. 首启引导态（替代旧 /login 的强制跳转）

零账号时进入本页：

```
┌──────────────────────────────────────────────┐
│      欢迎使用 RMCCC                            │
│   ┌───────────────┐  ┌───────────────┐       │
│   │ 🔵 Microsoft   │  │ 👤 离线账号     │       │
│   │   登录         │  │   快速开始      │       │
│   └───────────────┘  └───────────────┘       │
│      「离线模式也能启动游戏，之后可随时添加微软账号」│
└──────────────────────────────────────────────┘
```

**不强制**：主页快速启动卡提供离线启动路径（见 02 §3.1），首次启动不自动跳转本页——
只有用户主动来此页或点击启动引导时才出现引导态。

## 6. 与启动的联动

- 快速启动卡「添加账号后启动」→ 本页 → 登录成功 → 自动返回并直接 `launch_start`
  （authStore 暴露 `pendingLaunchProfile`，登录成功回放）。
- 会话过期：`auth_get_session` 内部自动刷新；若刷新失败且用户点启动 →
  toast「登录已过期」+ 引导到本页重新登录。

## 7. 接口与事件映射

`auth_microsoft_login` / `auth_offline_login` / `auth_logout`（账号卡共享）/
`auth_get_session` / `auth_refresh_token` / `auth_list_accounts` / `auth_switch_account` /
`auth_remove_account`、`open_external_url`；事件 `auth://device-code`。

## 8. 渐进披露自查

一级：添加两种账号、切换当前、移除；
二级：刷新令牌、UUID 复制；
三级：安全说明折叠脚注（内容固定，引用 ADR-009 要点，不放技术细节吓人）。

## 9. i18n key 提案（节选）

`accounts.title` / `accounts.current` / `accounts.list` /
`accounts.add.microsoft` / `accounts.add.offline` /
`accounts.ms.step1` `accounts.ms.step2` `accounts.ms.waiting` `accounts.ms.codeCopied` /
`accounts.offline.name` `accounts.offline.invalid` /
`accounts.setActive` `accounts.refresh` `accounts.remove` `accounts.remove.body` /
`accounts.welcome.title` `accounts.welcome.offlineHint` / `accounts.securityNote`

## 10. 验收要点

- 设备码全流程：发起 → 显示 code → 浏览器完成 → toast 成功（真机验收项）。
- token 字段零渲染：对本页做 DOM 扫描断言（无 `access_token` 值出现）。
- 移除当前账号后自动切到下一个账号或未登录态；主页账号卡同步。
