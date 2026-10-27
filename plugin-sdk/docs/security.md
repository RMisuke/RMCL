# 安全模型（隔离 UI）

> 完整决策见 [ADR-006](../../docs/adr/ADR-006-isolated-ui-container-and-host-bridge.md)。

## 隔离是什么

| 面 | 保证 |
|----|------|
| 源 | 插件 UI 只从 `rmcccplugin://<pluginId>/` 加载；主窗口是 `tauri://localhost`，两者不同源 |
| DOM/存储 | iframe 无法访问宿主 DOM、`localStorage`、`indexedDB`、cookie |
| IPC | macOS/WebKit 与 Linux/WebKitGTK：不向子帧注入 `__TAURI_INTERNALS__`；**Windows 上宿主直接不生成 `uiUrl`**（见下）；也没有通用 `fs`/`shell` 能力 |
| 网络 | CSP `connect-src 'none'`：插件自己发不出任何请求；桥的 `network.request` 还拒绝本地/私网/云 metadata 地址 |
| 资源 | 只服务该插件安装目录下 `ui/` 内的白名单扩展名文件，禁 `..`、禁百分号编码、禁隐藏文件 |
| 授权 | 每次桥调用重新检查插件状态 + 能力 + 作用域（域名/实例） |
| 输出 | 插件返回值按不可信数据使用；宿主不会把其中的 HTML 注入自己的 DOM |
| 主题 | 纯数据：只校验颜色并写入 CSS 变量，不执行插件代码；`danger/success/warning` 由宿主拥有 |

## 隔离不是什么

- **不是进程隔离**：iframe 与宿主同属一个 WebView 进程（逻辑插件/独立进程是 Phase 5 的议题）。
- **不是签名校验**：Phase 4 的插件仍是 `trust=development`，签名与可信目录在 Phase 7。
- **Windows 上隔离 UI 当前关闭**（2026-10-27 安全审计）：wry 0.55 的 WebView2 后端把
  Tauri 初始化脚本注入所有子帧，而 Tauri 2.11 把已注册自定义协议视为 local origin——
  插件 iframe 可能直接拿到 `__TAURI_INTERNALS__` 绕过 Host Bridge。宿主在
  `platform_id` 以 `windows` 开头时不生成 `uiUrl`，卡片/页面降级为声明式渲染。
  解除条件见 [ADR-006 补充](../../docs/adr/ADR-006-isolated-ui-container-and-host-bridge.md)；
  WebView2 实测通过前不得宣称 Windows 隔离 UI 可用。
- **Linux/WebKitGTK 未实测**：机制上尊重 main-frame-only，但仍标记 deferred（指南 §1.4）。

## 你能依赖什么

- 宿主拒绝未知协议版本、未知方法、超限消息；
- 卡片被卸载时宿主清理监听与超时；插件崩溃只影响自己那张卡；
- 权限撤销后下一次调用返回 `permission_denied`（宿主同时推送 `revoke` 消息）。

## 你不应该做

- 不要试图在 `ui/` 之外放可执行文件（包校验会拒绝）；
- 不要依赖 `window.parent` 的任何属性（跨源访问会抛错）；
- 不要把用户凭据写进 `analytics`/日志——宿主日志会记录桥调用的 method 与错误码，
  但不会记录 `params` 的内容；这并不意味着可以随便记录敏感数据。

## 报告问题

安全相关问题请附上：插件 `manifest.json`、`ui/` 下相关文件、宿主日志
（`logs/rmccc.<date>.log`，可用 `/settings` 的日志查看器导出）。
