# 插件系统安全审计 + 主题能力评估（2026-10-27）

- **范围**：`crates/rmccc-plugin`、`crates/rmccc-dashboard/src/theme.rs`、`src-tauri/src/commands/{plugin_ui,dashboard}.rs`、
  `src/plugin-ui/*`、`src-tauri/tauri.conf.json`、`src-tauri/src/state.rs`、`plugin-sdk/`、`examples/plugins/`、
  `schemas/*`；并核对了依赖源码 `wry 0.55.1` / `tauri 2.11.5` 的脚本注入与 ACL 行为。
- **方法**：逐层读实现 + 单测/负向用例 + 依赖源码核对；**本机只做 macOS 实测**，Windows/Linux 行为标记
  deferred（指南 §1.4）。没有做动态渗透测试；结论以代码与测试为据。
- **结论先行**：
  1. 主题可以扩展，且本次已把"主题插件契约"补成可执行规则（格式/颜色/大小/明暗/校验点）。
  2. macOS 上插件**不能**执行宿主代码、读写宿主 DOM、调用任意 IPC；数据面只有 Host Bridge 白名单方法。
  3. **Windows 存在真实的子帧 IPC 逃逸面**（依赖行为所致），已用"不生成 `uiUrl`"门禁降级；这是当前唯一的
     release-blocking 安全项。
  4. 最大**制度性**缺口仍是签名/来源认证（ADR-008 proposed）：安装 = 用户信任本地包。
  5. 本次修复：主题 CSS 注入面、`network.request` SSRF、主题选择静默失败、能力目录 `executable` 漂移、
     桥 origin 校验过宽。

---

## 1. 主题能力评估

### 1.1 现有主题链路（已实现）

| 环节 | 实现 | 状态 |
|---|---|---|
| 声明 | `manifest.contributes.themes[]`（id/name/path，≤8 套） | ✅ |
| 包校验 | 路径存在、`themes/*.json`、≤64 KiB、`ThemeDocument::parse` | ✅ 本次补齐内容校验 |
| 主题文档 | `schemaVersion=1`、8 个颜色令牌、可选 `mode`、严格拒绝未知顶层字段 | ✅ |
| 颜色语法 | 仅 `#rgb`/`#rrggbb`/`#rrggbbaa` | ✅ 本次新增 |
| 列表/明暗 | `dashboard_list_themes` 返回插件主题，`mode` 由声明或亮度推断 | ✅ 本次修复（原恒为 `custom`） |
| 选择 | `dashboard_set_theme_preference` 先校验后落盘 | ✅ 本次修复（原静默保存） |
| 应用 | `dashboard_get_theme_tokens` → 前端写 `--rmccc-*` + 旧变量 | ✅ |
| 安全色 | `danger/success/warning` 宿主拥有，主题中出现即忽略 | ✅ |
| 隔离 UI 读取 | `theme.get` / `init.theme` / `theme` 推送 | ✅ |
| 内置对照 | `dashboard_get_theme_json('builtin:*')` 合成同 schema JSON | ✅ 本次修复（原返回 null） |
| 偏好失效 | 插件禁用/卸载后回退内置主题（列表不再出现该 key） | ✅（UI 无显式提示，见 F-06） |
| 装饰三色 | 宿主按明暗派生，插件主题只贡献 `accent` 作 primary | ⚠️ 设计限制 |
| 圆角/字体/间距/背景图 | 不开放，归宿主视觉系统 | ⛔ 有意不开放 |
| 独立预览页 | 无；设置页用色卡预览 | ⚠️ 可后续增强 |

### 1.2 能不能加新主题风格？

**能。** 数据型主题插件不需要任何代码，新增一套风格 = 新增一个 `themes/*.json` +
在 manifest 的 `contributes.themes` 登记。可以表达：背景/表面层级、主次文字、强调色与强调色上的文字、
边框，以及明暗声明。不能表达：字体、圆角、间距、阴影、背景图、组件级覆盖、布局。

这个边界是有意的（ADR-005 决策 7）：主题只影响颜色，不破坏可读性与可访问性，也不会成为
"第二个 UI 框架"的入口。若未来要开放字体/背景图，必须新增 ADR，并解决远程资源与 CSP 问题。

### 1.3 本次接口补齐

| 接口 | 之前 | 现在 |
|---|---|---|
| 主题内容校验 | 只检查文件存在 | 打包/安装即解析 + 颜色语法 + 大小 |
| `ThemeInfo.mode` | 恒为 `custom` | `dark`/`light`（声明优先，缺省亮度推断） |
| 选择损坏/不存在主题 | 保存成功、界面静默回退 | 后端拒绝并返回错误，前端 toast |
| 内置主题 JSON 对照 | `null` | 同 schema 合成 JSON |
| 运行期主题文件大小 | 无上限（可读 16 MiB） | 64 KiB |
| 能力目录 `executable` | `settings.read`/`instance.read`/`network.request` 标成不可调用 | 与桥方法表一致 |

---

## 2. 安全架构复核（逐层）

### 2.1 包与安装

- 包只允许 `.rmcpkg`（ZIP），16 MiB 上限、512 条目、64 MiB 解压、单文件 16 MiB、压缩比 200:1；
- 条目路径统一 `validate_relative_path`：拒绝绝对路径、`..`、盘符、UNC、反斜杠、控制字符、
  Windows 保留名、大小写不敏感重名；
- 拒绝符号链接/设备文件（`validate_entry_mode`），解压时 `take` 硬限制单文件写入；
- 顶层只允许 `manifest.json`/README/LICENSE/NOTICE/CHANGELOG/config.schema.json + `assets|locales|themes|ui|logic`；
  `logic/` 只允许 `.wasm`，脚本/原生库一律拒绝；
- 安装走 staging → 校验 → 同卷 `rename` 原子提交；目标已存在拒绝覆盖；失败清理 staging；
- 解压后重新解析 manifest 并比对 id/version，避免"检查的字节"与"解压的字节"不一致。

结论：路径穿越、zip bomb、覆盖攻击面已封堵；未发现可写包外文件的路径。

### 2.2 权限模型

- 能力清单是**封闭枚举**，未知能力在 Manifest 解析期拒绝（`filesystem.write` 之类直接失败）；
- 风险分级：low 安装时自动授予（cards/pages/themes/settings.read/instance.read/mods.read），
  medium/high 必须显式授予；
- 升级**不自动扩张**权限：新能力进入 pending，插件回到 `installed_disabled`；
- 撤销即时生效：`contributions()` 按 granted 过滤，徽标/入口/桥调用立即消失；
- 授权带作用域：`network.request` 校验 host（精确或 `*.domain`，拒绝裸 `*`/IP/协议/端口/本地/元数据），
  instance 级能力校验实例 id；
- 每次桥调用重新授权（不信任前端缓存/按钮）；
- 审计日志 `plugin-state/audit.jsonl` 记录 allowed/denied/revoked。

结论：模型是"默认拒绝 + 最小权限 + 运行时复核"，实现与文档一致。低风险自动授予是产品取舍，
不是漏洞（读取范围已收窄，见 F-10）。

### 2.3 隔离 UI（macOS/WebKit）

- 独立源：`rmcccplugin://<id>/` 与主窗口 `tauri://localhost` 不同 scheme+host；
- iframe `sandbox="allow-scripts allow-same-origin"`：同源只对自己的 scheme 生效；
- 资源处理器只服务该插件安装目录 `ui/` 下的白名单扩展名：拒绝 `..`、百分号编码、反斜杠、隐藏文件、
  目录列举、未知类型；单文件 4 MiB；要求插件 `enabled` 且持有 `dashboard.cards` 或 `ui.pages`；
- 响应头 CSP：`default-src 'none'; script-src 'self'; style-src 'self' 'unsafe-inline'; img-src 'self' data:;
  font-src 'self'; connect-src 'none'; form-action 'none'; base-uri 'none'; object-src 'none'`；
  插件自己发不出网络请求（只能走桥的 `network.request`，需 High 能力 + 域名白名单）；
- 前端桥校验：`event.source === frame.contentWindow`、origin（WebKit 沙箱为 `null`；能给出具体 origin 时
  **必须是自己的 `rmcccplugin://<id>`**，本次收紧）、字符串消息、≤64 KiB、JSON 形状、`v`、
  方法表、in-flight ≤8、超时；
- 导航封印：iframe 第一次 `load` 之后的任何导航都会丢弃 pending 并封桥（防止返回包投递给别的文档）；
- Rust 侧 `plugin_ui_invoke` **重新解析原始 JSON**（不信前端结构化参数），重新 `ensure_enabled` +
  `authorize`，响应 ≤512 KiB。

结论：macOS 上插件 UI 无法访问宿主 DOM/存储/IPC；能做的只有桥方法表里的方法。

### 2.4 逻辑插件（WASM）

- 只接受 `logic/*.wasm`（≤2 MiB），独立 helper 进程执行，主程序不链接 wasm 引擎；
- 模块 import 白名单只有 `rmccc.log`，无 WASI/文件/网络/时钟；
- fuel 指令计量 + `StoreLimits` 内存上限 + payload/响应大小上限；
- helper 崩溃/死循环由 supervisor 隔离（不影响主进程）。

结论：逻辑插件是"可计算但不可触达系统"的沙箱；`logic.invoke` 为 Medium，需显式授予。

### 2.5 网络出口（`network.request`）

- 只允许 GET/POST，目标必须命中声明 host 白名单；无自定义 header、无 cookie；
- 不跟随重定向（30x 逃不出白名单）；30 s 超时；请求体 ≤64 KiB；响应 ≤2 MiB。

本次加固（F-03）：域名解析后过滤回环/私网/链路本地/CGNAT/组播/保留地址，全部不合格即拒绝；
用 `resolve_to_addrs` 把连接钉在已检查地址上，封 DNS rebinding 时间窗。仍不解析/不代理任意 URL。

### 2.6 配置

- `config.schema.json` 只支持严格子集，未知 keyword 安装期报错；schema ≤256 KiB、config ≤1 MiB；
- 缺失值按 default 生成；升级不兼容时保留原文件并报错；
- 配置写入原子替换；配置值只回给插件自己（不进入宿主 UI 渲染）。

### 2.7 主题（新）

见 §1；安全要点：颜色语法收窄 + 三处校验 + 安全色边界 + 主题不继承权限。
主题值通过 `style.setProperty` 写入 CSS 变量（不是 `innerHTML`），即使校验被绕过也无法闭合声明之外
的 CSS 块；颜色语法是纵深防御。

### 2.8 主窗口与 Tauri ACL

- `withGlobalTauri: false`，capabilities 只授予 `main` 窗口最小集合（window/event/process/dialog/updater）；
- 应用自定义命令默认对主窗口可用（Tauri 2 行为），因此**主窗口 XSS = 全权限**；当前未发现注入点：
  插件数据全部按文本渲染，唯一 `innerHTML` 是经过白名单净化的 AI Markdown；
- 主窗口**没有配置 CSP**（`tauri.conf.json` 无 `app.security.csp`）。这是纵深防御缺口（F-05），
  但当前无同源第三方内容，风险低。建议后续加 CSP（需回归 `asset:`/`rmcccicon:`/图片域名）。

### 2.9 信任与分发

- 所有安装 `trust=development`，无签名验证、无可信目录；GUI 只能选本地 `.rmcpkg`，CLI 同；
- `SHA-256` 仅诊断；ADR-008（root/index 双密钥、分离式包签名、四档信任）仍是 **proposed**；
- 因此：**安装一个包 = 用户对包内容的完全信任**。这是当前最大的制度性风险（F-04），
  在分发/自动更新通道打开前必须落地 ADR-008。

---

## 3. 发现清单

| ID | 严重度 | 状态 | 发现 | 处置 |
|---|---|---|---|---|
| F-01 | High | ✅ 已修复 | 主题颜色接受任意字符串，`url(...)`/`;` 可写入 CSS 变量（外链探针/样式污染） | 颜色语法收窄为 hex；包检查/选择/运行期三处校验；新增注入负向测试 |
| F-02 | Critical（Windows） | ⚠️ 已门禁，deferred | wry 0.55 WebView2 把初始化脚本注入**所有子帧**；Tauri 2.11 把自定义协议判定为 local origin → 插件 iframe 可能拿到 `__TAURI_INTERNALS__` 并调用主窗口命令 | Windows 上不生成 `uiUrl`（声明式降级）+ 诊断；ADR-006 补充解除条件（Tauri 修复或独立 Webview + 空 capability） |
| F-03 | Medium | ✅ 已修复 | `network.request` 只校验域名白名单，域名可解析到 127.0.0.1/169.254.169.254/私网（SSRF 读本地服务） | 本地域名黑名单 + DNS 解析过滤 + `resolve_to_addrs` 钉死地址；单测覆盖 |
| F-04 | High（制度） | ⏳ 接受（ADR-008） | 无签名/来源认证；安装即完全信任 | 文档显著提示；ADR-008 落地前不开放远程目录/静默更新 |
| F-05 | Low | ⏳ 接受 | 主窗口无 CSP | 记录为纵深防御 TODO；当前插件数据不进入 DOM HTML |
| F-06 | Low | ⚠️ 部分修复 | 插件禁用/卸载后，用户主题偏好仍指向失效 key；设置页无提示（但已回退内置主题） | 本次已让"选择"拒绝失效 key；启动回退不弹提示，留待 UI 增强 |
| F-07 | Info | ✅ 符合预期 | `serve_asset` 只要求 `dashboard.cards` 或 `ui.pages`，纯主题插件不能加载 UI | 无 |
| F-08 | Low | ⏳ 接受 | `plugin_ui_invoke` 信任调用方传入的 `plugin_id`（无窗口内身份令牌） | macOS 只有主窗口能调用；Windows 已门禁；主窗口 XSS 已等于全权限 |
| F-09 | Low | ⏳ 接受 | 主题可制造低对比度/观感伪装（不能改安全色/文案/行为） | 规范建议对比度；安全色固定 |
| F-10 | Low | ✅ 接受 | 低风险 `instance.read`/`settings.read`/`mods.read` 安装即授予 | 只暴露收敛后的 DTO（无路径/凭据）；撤销即时 |
| F-11 | Info | ✅ 已修复 | 能力目录把 `settings.read`/`instance.read`/`network.request` 标为"不可调用"，与桥方法表不一致 | 修正 `executable` + 一致性测试 |
| F-12 | Low | ✅ 已修复 | 桥 origin 校验接受任意 `rmcccplugin://` 前缀（可能跨插件混淆） | 收紧为 `rmcccplugin://<自己的 id>`，保留 WebKit 的 `null` 分支 |

**未发现**：路径穿越写包外文件、包内脚本执行、插件读取账户 Token/LLM Key、插件直接访问宿主 DOM/存储、
插件绕过能力检查（macOS）、zip bomb/符号链接逃逸、日志泄露凭据。

---

## 4. 当前插件能做什么（能力矩阵）

| 能力 | 风险 | 执行通道 | 说明 |
|---|---|---|---|
| `dashboard.cards` / `ui.pages` / `themes.apply` | low | 声明式贡献 | 列表/主题/卡片；无代码执行 |
| `settings.read` | low | `settings.get` | 语言、主题偏好、默认内存；无路径/凭据 |
| `instance.read` | low | `instances.list` | id/名称/版本/loader/内存/最后游玩；无磁盘路径 |
| `mods.read` | low | —（未实现） | 仅声明 |
| `mods.search` / `mods.plan_install` / `downloads.*` | medium | —（未实现） | 仅声明 |
| `ai.chat` | medium | `llm.chat` | 宿主代持 Key；每插件 6 次/分钟；非流式/流式 |
| `logic.invoke` | medium | helper 进程 | 只读 WASM，白名单 import + fuel/内存上限 |
| `network.request` | high | `network.request` | 域名白名单 + SSRF 过滤 + 无重定向/无 header |
| `mods.install` / `mods.manage` / `game.launch` / `logs.read` / `agent.*` / `credentials.use:*` | high | —（未实现） | 仅声明；启用前必须显式授权 |

> 说明：`executable=false` 的能力目前**没有执行通道**。这既限制了插件，也意味着攻击面更小；
> 后续开放每一项都必须新增桥方法/命令、错误码、限额、负向测试与 ADR 更新。

---

## 5. 残余风险与后续工作

1. **F-02（Windows）**：在 Tauri/wry 修复或改独立 Webview 容器前，Windows 不开放插件 UI。
   需要在 Windows 原生 CI/真机上验证：iframe 内 `window.__TAURI_INTERNALS__ === undefined`。
2. **F-04（签名）**：ADR-008 定稿并实现前，不接入远程插件目录/自动更新；文档与 UI 继续显示"未签名"。
3. **F-05（CSP）**：给主窗口加 CSP（`default-src 'self'` + 必要的 `asset:`/`rmcccicon:`/图片域名 +
   `style-src 'unsafe-inline'`），并回归全部页面。
4. **桥的窗口内身份**：若未来有多个可调用 `plugin_ui_invoke` 的窗口/Webview，需要按 webview label 或
   一次性 token 绑定调用方，不能只信 `plugin_id` 参数。
5. **主题 UX**：失效偏好提示/一键恢复；对比度 lint（作者工具）可后续加。
6. **动态验证**：建议在 macOS 真机跑一次"恶意样例包"（非法颜色、越权 host、超大文件、
   符号链接、`ui/` 里放脚本）验收拒绝路径；本次以单测覆盖为主。

---

## 6. 验证记录

- `cargo test --workspace`：**681 passed / 0 failed**；clippy（`-D warnings`）与 `cargo fmt --check` 通过。
- `cargo test -p rmccc-plugin --lib`：118 passed（含主题注入、包路径、权限、UI 门禁新增用例）。
- `cargo test -p rmccc-dashboard --lib`：主题解析/令牌/明暗用例。
- 前端：`pnpm typecheck`、`pnpm test`（34）、`pnpm build`；SDK 契约测试 `pnpm sdk:test`（14）。
- **E2E 拒绝路径**：用 `rmcli plugin pack` 打包一个 `background: "url(https://tracker.example/p)"`
  的主题包，进程返回 `ok:false` / `plugin_package_invalid`，错误信息明确指出颜色必须是
  `#rgb / #rrggbb / #rrggbbaa`——证明"能打包 = 能安装 = 能应用"的校验确实生效。
- macOS 真机：未在本轮启动 GUI 做恶意包渗透；隔离 UI 的既有烟测结论沿用 ADR-006 记录。
- Windows/WebView2：**未实测**（无环境），门禁按依赖源码推断实现，标记 release-blocked。
