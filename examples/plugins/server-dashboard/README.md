# Server Dashboard

示例**隔离 UI 插件**（Phase 4，[ADR-006](../../../docs/adr/ADR-006-isolated-ui-container-and-host-bridge.md)）。

- `entry.ui` = `ui/index.html`：卡片在 `rmcccplugin://dev.rmccc.server-dashboard/` 的
  sandbox iframe 里渲染，与主窗口不同源；
- 数据全部经 Host Bridge（`postMessage`）：`instances.list`（需 `instance.read`）、
  `settings.get`（需 `settings.read`）、`network.request`（需 `network.request` +
  声明域名 `status.example.net`）；
- UI 侧零依赖、无构建步骤（直接用线协议，不引 SDK），用来证明协议自足。

```bash
rmcli plugin pack --dir examples/plugins/server-dashboard --out /tmp/server-dashboard.rmcpkg
rmcli plugin install --path /tmp/server-dashboard.rmcpkg
# 安装后为 installed_disabled + pending network.request
rmcli plugin grant --id dev.rmccc.server-dashboard --capability network.request
rmcli plugin enable --id dev.rmccc.server-dashboard
```

装好后在 Dashboard 点 **添加插件卡片** → “Server Status”。

双击卡片里的 **Refresh** 按钮会触发一次 `network.request` 到
`https://status.example.net/status.json`（该域名不存在，用于演示失败态：
卡片会显示 `internal`/`timeout` 错误码而不是白屏）。
