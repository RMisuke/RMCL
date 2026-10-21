# 打包与发布

## 版本号

| 版本 | 位置 | 规则 |
|------|------|------|
| 插件版本 | `manifest.version` | SemVer；升级不得静默扩张权限 |
| 插件 API 版本 | `compatibility.pluginApi` | 当前只接受 `1.x` |
| 宿主版本范围 | `compatibility.core` | 例如 `>=0.1.0 <0.2.0` |
| UI API 版本 | 运行时 `UI_API_VERSION` | 宿主与插件在 `init`/请求里自动校验 |

## 打包

```bash
node plugin-sdk/scripts/cli.mjs validate --dir my-card
node plugin-sdk/scripts/cli.mjs pack --dir my-card --out dist/my-card-1.0.0.rmcpkg --rmcli rmcli
```

`pack` 调用 `rmcli plugin pack`，产出无加密 ZIP（ADR-003）：
`manifest.json` 在根，`ui/` 只含白名单类型；包级限额与路径规则由宿主强制。

## 升级

- 宿主以 `plugins/<id>/<version>/` 存放每个版本，升级保留 `previousVersion`；
- 新版本**新增的权限进入 pending**，插件回到 `installed_disabled`，用户审查后才启用；
- 回滚用 `rmcli plugin rollback --id <id>` 或 `/plugins` 页面。

## 分发（当前阶段）

Phase 4 只支持**本地安装**（文件选择器或 `rmcli plugin install`）。
官方 registry、签名、自动更新在 Phase 7；在那之前：

- 不要发布"请关闭校验后安装"的包；
- 包内不要包含用户数据或凭据；
- 提供 `README` 说明所需能力与用途（安装页会展示 `reason` 字段）。

## 兼容矩阵

| 宿主 | UI API v1 | 说明 |
|------|-----------|------|
| macOS ≥ 13（WebKit） | ✅ 已验证 | 开发机实测 |
| Windows 10/11（WebView2） | ⛔ deferred | 未实测，不得宣称支持 |
| Linux（WebKitGTK） | ⛔ deferred | 未实测 |
