# RMCCC 插件编写指南

> 本指南覆盖**声明式插件**（主题、翻译、Dashboard 卡片/设置页声明、配置 schema、打包安装）——
> 这仍是绝大多数插件的形态。Phase 4 起另有两种**受控执行**形态：
> 隔离 UI 容器（`entry.ui`，ADR-006）与逻辑插件（`entry.logic`，WASM 只读白名单，ADR-007），
> 详见 `plugin-sdk/` 与对应 ADR；插件调用宿主 AI 服务（`ai.chat`，桥方法 `llm.chat`，
> ADR-013 S6）已实现。
> 协议细节见 [ADR-003](../adr/ADR-003-plugin-package-and-install-transaction.md) 与
> [ADR-004](../adr/ADR-004-plugin-manifest-and-versioning.md)。
>
> **只做主题？** 直接读 [THEME_PLUGIN_AUTHORING.md](THEME_PLUGIN_AUTHORING.md)：
> 主题是纯数据插件，有独立的字段/颜色/校验规则与验收清单。

## 1. 目录结构

```text
my-plugin/
  manifest.json          # 必需
  config.schema.json     # 可选：用户配置 schema（严格子集）
  README.md / CHANGELOG.md / LICENSE / NOTICE
  assets/                # 图片、字体等静态资源
  locales/               # 翻译 JSON
  themes/                # 主题 JSON
```

可以并存，也可以只有一个。完整示例见 [`examples/plugins/`](../../examples/plugins)。

## 2. Manifest

```json
{
  "manifestVersion": 1,
  "id": "dev.example.my-plugin",
  "name": "My Plugin",
  "version": "1.0.0",
  "description": "…",
  "publisher": { "id": "dev.example", "name": "Example" },
  "compatibility": {
    "pluginApi": "1.x",
    "core": ">=0.1.0 <0.2.0",
    "platforms": []
  },
  "permissions": [
    { "capability": "dashboard.cards" },
    { "capability": "network.request", "hosts": ["api.example.com"] }
  ],
  "contributes": {
    "dashboardCards": [
      {
        "id": "status",
        "title": "Status",
        "defaultSize": { "w": 6, "h": 2 },
        "minSize": { "w": 4, "h": 2 },
        "maxSize": { "w": 8, "h": 3 }
      }
    ]
  }
}
```

规则要点：

- `id` 必须是反向域名（小写字母/数字/`-`/`.`，至少两段）。`org.rmccc.*` 保留。
- `version` 是严格 SemVer（`1.2.3`，不接受 `v1.2`）。
- `compatibility.core` 用空格分隔的比较器（`>=0.1.0 <0.2.0`）；
  `pluginApi` 用 `MAJOR.x`（当前 `1.x`）。
- `platforms` 为空 = 全平台；否则用
  `darwin-aarch64` / `darwin-x86_64` / `windows-x86_64` / `windows-aarch64` /
  `linux-x86_64` / `linux-aarch64`。
- **未知字段会被拒绝**（v1 严格模式），拼错不会静默忽略。

### 2.1 卡片尺寸与 Host Bridge

卡片宽度只能取 `3 / 4 / 6 / 8 / 12`（12 列网格），高度只能取 `1 / 2 / 3 / 4`
（92px 行高 + 14px 间隔）。manifest 的 `defaultSize` 必填，`minSize` / `maxSize`
可选；宿主会把旧插件声明的任意 1..24 尺寸吸附到最近的合法档位。

隔离 UI 卡片的 `init` 消息包含 `cardId` 与 `card: { title, width, height }`。
v1 可调用的宿主方法：

| 方法 | 能力 | 用途 |
|---|---|---|
| `host.info` / `theme.get` | — | 版本、卡 id、当前主题 |
| `settings.get` | `settings.read` | 公开设置子集 |
| `instances.list` | `instance.read` | 实例概要 |
| `tasks.list` | `tasks.read` | 统一任务中心快照（已脱敏，无日志/路径） |
| `launch.status` | `launch.read` | 运行中的实例 key |
| `server.status` | `network.request` + 声明 host | 真实 Minecraft SLP 查询 |
| `network.request` | `network.request` | 声明域名的 HTTP 代理 |
| `llm.chat` | `ai.chat` | 宿主 AI 服务 |

新增低风险能力 `tasks.read` / `launch.read` 安装时自动授予；`network.request`
仍需显式授权。插件不能直接访问文件系统、账号凭据或宿主 DOM。

## 3. 权限

| 风险 | 行为 |
|---|---|
| low（`dashboard.cards`/`ui.pages`/`themes.apply`/`settings.read`/`instance.read`/`mods.read`） | 安装时自动授予 |
| medium/high | 安装后保持 `installed_disabled`，必须显式授予才能启用 |

完整清单：`rmcli plugin capabilities`。`network.request` 的 `hosts` 只接受精确域名或
`*.example.com`；拒绝裸 `*`、协议、端口、IP、`localhost`、私网/metadata 地址。

升级不会自动扩张权限：新版本请求的新能力全部进入待审查列表。

## 4. 配置 schema（可选）

`config.schema.json` 使用严格子集：
`type` / `properties` / `required` / `additionalProperties`（仅 bool） / `enum` /
`minimum` / `maximum` / `minLength` / `maxLength` / `items` / `default` /
`x-rmccc-schemaVersion`。**其它 keyword 会在安装时报错**。

```json
{
  "x-rmccc-schemaVersion": 1,
  "type": "object",
  "additionalProperties": false,
  "required": ["server"],
  "properties": {
    "server": { "type": "string", "minLength": 1, "default": "play.example.net" },
    "port": { "type": "integer", "minimum": 1, "maximum": 65535, "default": 25565 }
  }
}
```

- 缺失的配置按 `default` 生成到 `plugin-data/<id>/config.json`。
- 版本升级不兼容旧配置时：保留原文件、报错、不覆盖默认值。
- 卸载默认保留配置；`--delete-data` 才删除。

## 5. 打包与安装

```bash
# 开发：把目录打成 .rmcpkg（内部会再跑一遍完整校验）
rmcli plugin pack --dir examples/plugins/theme-midnight \
  --out /tmp/theme-midnight.rmcpkg

rmcli plugin install --path /tmp/theme-midnight.rmcpkg
rmcli plugin inspect --id dev.rmccc.theme-midnight
rmcli plugin enable  --id dev.rmccc.theme-midnight
```

GUI：Dashboard →「插件」→「安装插件…」（选择 `.rmcpkg`）。
安装后若存在待审权限，插件保持禁用；在详情里授予后再启用。

包限制：16 MiB 包 / 64 MiB 解压 / 16 MiB 单文件 / 512 条目 / 180 字节路径 /
12 层目录 / 200:1 压缩比；拒绝符号链接、大小写重名、Windows 保留名。

## 6. 信任与签名

Phase 2 **不做签名验证**，所有安装记为 `development` 并显示“未签名”提示。
`SHA-256` 只用于诊断，不是来源认证。签名/可信目录在 Phase 7 引入；
在此之前只安装你信任的包。

## 7. 各阶段能力对照

- Dashboard 布局/主题渲染：✅ Phase 3（ADR-005）。
- 隔离 UI（`entry.ui` + 宿主桥 `plugin_ui_invoke`）：✅ Phase 4（ADR-006）。
- 逻辑运行时（`entry.logic`，只读调用白名单 + `logic.invoke` 能力）：✅ Phase 5（ADR-007）。
- 宿主 AI 服务能力（`ai.chat`，经桥 `llm.chat`，每插件 6 次/分钟限流）：✅ ADR-013 S6。
- 主题插件（`contributes.themes` + 8 个颜色令牌，无 JS）：✅ Phase 3，规范见
  [THEME_PLUGIN_AUTHORING.md](THEME_PLUGIN_AUTHORING.md)。
- 插件目录与签名分发：Phase 7（ADR-008 草案，proposed）；当前所有安装记为 `development`。
- **Windows 隔离 UI**：⛔ release-blocked（wry 子帧 IPC 风险，见 ADR-006 补充）——
  Windows 上 `uiUrl` 不生成，卡片/页面降级为声明式；macOS 不受该机制影响，Linux 未实测。
