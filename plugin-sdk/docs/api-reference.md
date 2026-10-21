# Host Bridge API 参考（UI API v1）

线上契约由三处共同定义，并有跨工件测试保证一致：

- `schemas/plugin-ui-bridge.schema.json` —— 消息 JSON Schema
- `crates/rmccc-plugin/src/ui.rs` —— 宿主侧强制实现（方法表/限额/错误码）
- `packages/plugin-api/src/index.ts` —— SDK 类型

## 消息

| 方向 | 形状 | 说明 |
|------|------|------|
| 插件 → 宿主 | `{ v: 1, id, method, params? }` | 单条请求；`id` 为 `[A-Za-z0-9_-]{1,64}` |
| 插件 → 宿主 | `{ v: 1, type: "ready" }` | 文档已就绪，请求 `init` |
| 宿主 → 插件 | `{ v: 1, id, ok: true, result }` | 成功 |
| 宿主 → 插件 | `{ v: 1, id, ok: false, error: { code, message, retryable } }` | 失败 |
| 宿主 → 插件 | `{ v: 1, type: "init", pluginId, cardId?, theme, limits }` | 容器初始化（一次） |
| 宿主 → 插件 | `{ v: 1, type: "theme", theme }` | 主题切换 |
| 宿主 → 插件 | `{ v: 1, type: "revoke", capability }` | 依赖的能力被撤销 |

所有消息必须是 **JSON 字符串**（`postMessage(JSON.stringify(...), "*")`）。
宿主对 `event.source`、`origin`、长度（≤64 KiB）做校验，非字符串消息直接丢弃。

## 常量

| 名称 | 值 | 含义 |
|------|----|------|
| `UI_API_VERSION` | 1 | 协议主版本 |
| `maxRequestBytes` | 65 536 | 单请求上限 |
| `maxResponseBytes` | 524 288 | 单响应上限 |
| `maxInFlight` | 8 | 同一插件并发请求上限 |
| `defaultTimeoutMs` / `maxTimeoutMs` | 10 000 / 30 000 | 超时 |

## 方法

### `host.info` → `{ uiApiVersion, appVersion, pluginId, cardId?, platform }`
无能力要求（插件必须已启用且有 UI 入口）。`cardId` 由前端宿主补上。

### `theme.get` → `ThemeTokens`
无能力要求。返回**当前生效**的 8 个令牌：
`background / surface / surfaceElevated / text / textMuted / accent / accentContrast / border`。
宿主 shell 直接应答（它才知道"跟随系统"的最终结果）。

### `settings.get` → `{ locale, theme, defaultMinMemoryMb, defaultMaxMemoryMb }`
需要 `settings.read`。只返回公开子集：不含路径、token、账号信息。

### `instances.list` → `{ instances: [{ id, name, gameVersion, loader, memoryMb, lastPlayed }] }`
需要 `instance.read`。`loader` 为 `{ type, version } | null`。

### `network.request` → `{ status, body }`
需要 `network.request`，且**目标域名**必须在该能力的声明白名单里（宿主按 host 授权）。

```js
await host.request("network.request", {
  url: "https://status.example.net/status.json",
  method: "GET",          // 只允许 GET/POST
});
```

限制：请求体 ≤64 KiB、响应 ≤2 MiB、30 s 超时、不跟随重定向、不带 cookie。
插件**不能**自己 `fetch`（CSP `connect-src 'none'`）。

## 错误码

| code | 含义 | 可重试 |
|------|------|--------|
| `validation` | 消息形状/参数非法 | 否 |
| `unsupported_method` | 方法不在 v1 表内 | 否 |
| `protocol_version` | `v` 与宿主不一致 | 否 |
| `too_large` | 请求/响应超限 | 否 |
| `timeout` | 宿主超时未完成 | 是 |
| `cancelled` | 卡片卸载/客户端 dispose | 否 |
| `not_installed` | 插件不存在 | 否 |
| `not_enabled` | 插件未启用 | 否 |
| `permission_denied` | 能力未授予或域名未声明 | 否 |
| `permission_pending` | 能力待用户审查 | 否 |
| `host_not_allowed` | 域名不在声明白名单 | 否 |
| `internal` | 宿主内部错误 | 是 |
