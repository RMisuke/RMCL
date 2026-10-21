# ADR-006: 插件隔离 UI 容器与版本化 Host Bridge

- **状态**：accepted
- **日期**：2026-10-21
- **阶段**：Phase 4
- **关联**：指南 §6.2（阶段 B）、§9.6、§13、§14.2；ADR-003（包布局）；ADR-004（Manifest）；ADR-005（卡片宿主）

## 背景

Phase 2 只允许**数据型插件**：`ui/`、`logic/` 目录在包校验阶段被直接拒绝
（`crates/rmccc-plugin/src/package.rs::validate_package_file`），`entry.ui` 也被 Manifest 校验拒绝
（`manifest.rs` 中的 "Phase 2 不支持 entry.ui"）。Phase 3 的插件卡片只是宿主绘制的占位卡。

Phase 4 要开放"插件自己的卡片/页面"。指南 §6.2 明确禁止把第三方 UI 脚本以 ES module 导入主窗口：
同一 JS 环境里的代码能拿到主窗口的全部 API，包括 Tauri IPC、`localStorage`、宿主 DOM。

因此必须回答三件事，并且落成可执行的边界而不是约定：

1. 第三方 UI 跑在什么容器里（与主窗口的隔离强度）；
2. 容器里的代码怎么拿到数据（消息协议、版本、限额、拒绝语义）；
3. 静态资源怎么送到容器里（协议、CSP、路径白名单）。

## 目标

- 插件 UI 只在**独立源**中执行：拿不到主窗口 DOM、拿不到 `__TAURI_INTERNALS__`、不能直接调任何 Tauri 命令。
- 数据只能经**版本化消息桥**获取；宿主对每条消息重新做权限判定（不信任前端按钮/缓存）。
- 插件 UI 故障（异常、超时、越权、协议版本不符）只影响自己那张卡片。
- 协议有 schema、有测试、有错误码映射，且能独立于应用版本演进。

**不做**（明确排除）：逻辑插件运行时（Phase 5）、签名/可信目录（Phase 7）、
Windows WebView2 实测（本机 macOS-only，见指南 §1.4）、插件页面的完整路由化（本阶段只做卡片容器，
页面容器复用同一套桥与协议，UI 入口在后续切片接入）。

## 选项

### A. `<iframe>` + 自定义 URI 方案（选中）

- 容器：主窗口内 `<iframe sandbox src="rmcccplugin://<plugin-id>/<entry>">`。
- 资源：Rust 侧注册 `rmcccplugin://` 方案处理器，只服务该插件安装目录下 `ui/` 内的白名单文件，
  响应带严格 CSP。
- 通信：`postMessage`，宿主校验 `event.source === iframe.contentWindow`、来源、
  消息大小、协议版本、方法-权限映射。

优点：单窗口、随卡片网格布局/裁剪；WebKit 对自定义方案的源与 `tauri://localhost` 天然不同源；
宿主能逐条消息复核权限；无额外进程/窗口管理成本。
缺点：同一 WebView 进程（渲染进程隔离不等于进程隔离）；WKWebView 与 WebView2 的
自定义方案/CSP 行为有差异，需要按平台标记 deferred。

### B. 独立 WebView 窗口（Tauri `WebviewWindow`）

优点：独立进程级隔离，可单独开/关。
缺点：无法嵌进 Dashboard 网格（只能做独立窗口）；窗口生命周期、z-order、DPI、
焦点/快捷键与主窗口协调成本高；体积与包体增加。与"卡片"这一核心形态不匹配。

### C. `srcdoc` / `blob:` 内联文档

优点：零协议改动。
缺点：`srcdoc` 继承父文档 CSP 与部分同源语义，隔离强度不足；`blob:` 源不稳定；
相对资源引用需要改写；无法做按插件目录的资源白名单。等于把安全边界交给字符串拼接。

## 决策

采用 **A**，并把它拆成五个可验证的硬约束：

1. **独立源**：插件 UI 只从 `rmcccplugin://<plugin-id>/...` 加载。
   与 `tauri://localhost` 不同 scheme+host，无法读写宿主 DOM/存储，也拿不到注入的 Tauri API。
   iframe 仍带 `sandbox="allow-scripts allow-same-origin"`：同源只对**它自己的** `rmcccplugin://`
   生效（模块脚本、样式、字体才能加载），对宿主源仍是跨源。
2. **资源白名单**：处理器只接受 `ui/` 前缀、无 `..`/绝对路径/反斜杠、扩展名在白名单内
   （`html/js/mjs/css/json/md/txt/svg/png/jpg/jpeg/gif/webp/ttf/otf/woff/woff2/map`）的请求；
   插件必须处于 `enabled` 且持有 `dashboard.cards`（卡片）或 `ui.pages`（页面）能力，
   否则一律 403。不做目录列举，不服务 `ui/` 之外的任何文件。
3. **严格 CSP**（响应头，非 meta）：
   `default-src 'none'; script-src 'self'; style-src 'self' 'unsafe-inline'; img-src 'self' data:;
   font-src 'self'; connect-src 'none'; form-action 'none'; base-uri 'none'; object-src 'none'`。
   `connect-src 'none'` 是关键：插件**不能**自己发网络请求，只能经桥的 `network.request`
   走宿主 HTTP client（受 `network.request` 能力与声明域名约束）。
   远程脚本/样式被 `script-src 'self'` 与 `style-src 'self'` 直接拒绝。
4. **版本化消息桥**：`UI_API_VERSION = 1`，单向请求/响应 + 宿主事件。
   - 插件 → 宿主：`{ v, id, method, params }`，`id` 为 1–64 字符的 `[A-Za-z0-9_-]`；
     宿主回 `{ v, id, ok: true, result }` 或 `{ v, id, ok: false, error: { code, message, retryable } }`。
   - 宿主 → 插件：`{ v, type: "init", pluginId, cardId, theme, limits }`（容器就绪后一次）、
     `{ v, type: "theme", tokens }`（主题切换时）、`{ v, type: "revoke", capability }`（权限被撤销）。
   - 上限：单条请求 ≤ 64 KiB、响应 ≤ 512 KiB（超限直接拒绝，不截断）；
     默认超时 10 s、上限 30 s；同一插件最多 8 个 in-flight。
   - 线上错误码是桥自己的封闭枚举 `BridgeErrorCode`
     （`validation` / `unsupported_method` / `protocol_version` / `too_large` / `timeout` /
     `cancelled` / `not_enabled` / `permission_denied` / `permission_pending` /
     `host_not_allowed` / `internal`），由宿主从 `IpcError.code` 映射而来。
     不复用 `ErrorCode` 作为线上格式：那是应用内错误分类，桥需要稳定的字符串契约。
5. **每条消息重新授权**：`plugin_ui_invoke` 在 Rust 侧解析 method → 所需 capability，
   调 `PluginManager::authorize(plugin_id, capability, host, instance)`；
   撤销能力后下一条消息立即 `PermissionDenied`，并在卡片上显示"权限已被撤销"态。

### v1 方法表

| method | 能力 | 说明 |
|--------|------|------|
| `host.info` | —（需 enabled + UI 入口） | 返回 `{ uiApiVersion, appVersion, pluginId, cardId, platform }` |
| `theme.get` | — | 当前生效令牌（与宿主同源，只读颜色） |
| `settings.get` | `settings.read` | 公开设置子集（语言、主题偏好、默认内存/Java 等，绝不含 token/路径） |
| `instances.list` | `instance.read` | 实例概要（id/name/版本/loader），不含磁盘路径 |
| `network.request` | `network.request` | 宿主 HTTP client；必须命中声明域名白名单；GET/POST；2 MiB 响应上限；不跟随跨域重定向 |

新增方法 = 新 `UI_API_VERSION`（minor 兼容）/ major 破坏性升级；方法表在
`crates/rmccc-plugin/src/ui.rs` 中集中定义，SDK 从同一张表生成类型。

## 后果

**正面**

- 第三方 UI 的安全边界由"源 + CSP + 每消息授权"三重保证，且都能写测试。
- 卡片容器与 Phase 3 的网格/布局/错误边界天然对齐；容器故障不影响其它卡片。
- 协议集中在 Rust，前端只是搬运；后续页面容器复用同一套代码。

**负面/代价**

- WebKit 与 WebView2 在自定义方案、CSP 细节、`sandbox` 行为上不完全一致：
  Windows 一致性测试 **deferred**，不得在本机宣称 Windows 隔离 UI 安全模型可用。
- 插件 UI 不能直接用 npm 生态的运行时特性（无 `fetch`、无 `localStorage` 跨会话），
  需要在 SDK 里提供 `host.request()` 与内存态 store。
- 协议演进需要版本矩阵与兼容测试（SDK 侧做版本检查）。

## 迁移影响

- Phase 2 的"拒绝 `ui/`/`entry.ui`"规则被本 ADR 取代：包校验允许 `ui/`，Manifest 允许 `entry.ui`；
  `logic/` 仍拒绝（Phase 5）。
- 已安装插件不受影响（它们的包里没有 `ui/`）；无需迁移数据。
- 回滚：删除 `ui/` 支持与容器即可回到 Phase 3 形态；协议版本从 1 回退到"无桥"。

## 跨平台验证方式

- macOS（本机）：真实 `RMCCC.app` 中安装样例插件 → 卡片渲染 → 桥调用成功/被拒 → 禁用后 iframe 不再加载。
- 单测：协议校验、路径白名单、CSP 文本、方法-权限映射、超时/取消/限额、错误码映射（Rust）。
- 前端：容器对 `postMessage` 来源/大小/版本的三类拒绝路径（Vitest 或纯函数测试）。
- Windows/Linux：**deferred**（release-blocked），在 `PLATFORM_ADAPTATION.md` 记入待验证清单。

## 状态变更记录

- 2026-10-21：proposed → accepted（Phase 4 开工，按本 ADR 实现 4.1–4.5）
