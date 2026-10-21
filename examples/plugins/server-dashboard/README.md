# Server Dashboard

示例声明式插件（Phase 2）：注册一个 Dashboard 卡片与一个设置页，
并请求对 `status.example.net` 的网络能力（高风险，安装后必须显式授予）。

```bash
rmcli plugin pack --dir examples/plugins/server-dashboard --out /tmp/server-dashboard.rmcpkg
rmcli plugin install --path /tmp/server-dashboard.rmcpkg
# 安装后为 installed_disabled + pending network.request
rmcli plugin grant --id dev.rmccc.server-dashboard --capability network.request
rmcli plugin enable --id dev.rmccc.server-dashboard
rmcli plugin config set --id dev.rmccc.server-dashboard --values '{"host":"status.example.net","port":25565,"showMotd":true}'
```

Phase 2 只登记贡献项（`plugin capabilities` / `plugin inspect` 可查），
真正的卡片渲染在 Phase 3 的 Dashboard 布局引擎中接入。
