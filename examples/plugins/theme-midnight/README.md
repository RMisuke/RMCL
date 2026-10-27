# Midnight Theme

示例数据型插件：只声明主题与静态资源，**不包含任何可执行代码**。
主题文件格式、颜色语法与验收见 [`docs/guides/THEME_PLUGIN_AUTHORING.md`](../../../docs/guides/THEME_PLUGIN_AUTHORING.md)。

```bash
rmcli plugin pack --dir examples/plugins/theme-midnight --out /tmp/theme-midnight.rmcpkg
rmcli plugin install --path /tmp/theme-midnight.rmcpkg --enable
rmcli plugin inspect --id dev.rmccc.theme-midnight
```

安装后 `plugin-state/permissions.json` 自动授予低风险能力 `themes.apply`；
配置写入 `plugin-data/dev.rmccc.theme-midnight/config.json`。
在「设置 → 外观」里选择 `Midnight` 即可应用；`danger`/`success`/`warning` 由宿主拥有，
即使写进 `colors` 也会被忽略，因此本示例不再声明它们。
