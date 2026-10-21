# RMCCC Plugin SDK

第三方插件的公开开发接口。当前覆盖 **Phase 4：隔离 UI 扩展**
（[ADR-006](../docs/adr/ADR-006-isolated-ui-container-and-host-bridge.md)）。

| 目录 | 内容 |
|------|------|
| `packages/plugin-api/` | 桥的线协议类型、方法表、限额（`@rmccc/plugin-api`） |
| `packages/plugin-ui/` | 运行时客户端 `connect()` / `applyTheme()` / `announce()`（`@rmccc/plugin-ui`），纯 ESM、无构建步骤 |
| `templates/dashboard-card/` | 最小隔离 UI 卡片模板（自带 vendored 运行时） |
| `scripts/cli.mjs` | `validate`（本地校验）/ `pack`（调用 `rmcli plugin pack`） |
| `test/` | 跨工件契约测试：schema ↔ Rust ↔ SDK 三处方法表/错误码必须一致 |
| `docs/` | 入门、API 参考、安全模型、发布流程 |

## 安全边界（先读这个）

插件 UI **运行在独立源** `rmcccplugin://<pluginId>/` 的 sandbox iframe 里：

- 拿不到宿主 DOM、`localStorage`、也拿不到注入的 Tauri API；
- 不能自己发网络请求（响应头 `connect-src 'none'`），只能经桥的 `network.request`
  走宿主 HTTP client，且目标域名必须在 `manifest.permissions` 里声明；
- 数据请求逐条授权：宿主每次处理都重新检查插件状态与能力，撤销后立即失败；
- 输出按不可信文本处理：宿主不会把插件返回的 HTML 注入自己的 DOM。

## 快速开始

```bash
cp -R plugin-sdk/templates/dashboard-card my-card
cd my-card
node ../plugin-sdk/scripts/cli.mjs validate --dir .
node ../plugin-sdk/scripts/cli.mjs pack --dir . --out /tmp/my-card.rmcpkg \
  --rmcli ../target/release/rmcli
../target/release/rmcli plugin install --path /tmp/my-card.rmcpkg
```

详见 [`docs/getting-started.md`](docs/getting-started.md)。

## 版本策略

- `UI_API_VERSION`（当前 1）只在与宿主**破坏性**变更时提升；新增方法走 minor。
- 插件包版本（SemVer）与 UI API 版本互相独立，见
  [`docs/publishing.md`](docs/publishing.md)。
- 宿主拒绝未知 `v`，并回 `protocol_version` 错误码。

## 本地开发

宿主只加载**已安装**的插件（不读任意路径）。开发循环：

1. `cli.mjs validate --dir <dir>` —— 与安装器同一套规则，先排掉结构性错误；
2. `cli.mjs pack` + `rmcli plugin install`；
3. 宿主中禁用/启用插件即可重新加载 iframe；改完 UI 重新 pack+install 会走升级事务
   （权限不扩张，新增权限进入 pending）。

不提供 `plugin:dev` 热更新：它需要宿主信任任意本地路径，与"生产构建不降低校验"冲突
（指南 §13.4 明确要求开发模式与生产分离，Phase 4 先不做）。
