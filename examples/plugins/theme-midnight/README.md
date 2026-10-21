# Midnight Theme

示例数据型插件（Phase 2）：只声明主题与静态资源，**不包含任何可执行代码**。

```bash
rmcli plugin pack --dir examples/plugins/theme-midnight --out /tmp/theme-midnight.rmcpkg
rmcli plugin install --path /tmp/theme-midnight.rmcpkg --enable
rmcli plugin inspect --id dev.rmccc.theme-midnight
```

安装后 `plugin-state/permissions.json` 自动授予低风险能力 `themes.apply`；
配置写入 `plugin-data/dev.rmccc.theme-midnight/config.json`。
