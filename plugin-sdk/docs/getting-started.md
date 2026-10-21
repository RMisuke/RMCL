# 入门：写一个隔离 UI 卡片

## 1. 包结构

```text
my-card/
├── manifest.json          # 必需
├── config.schema.json     # 可选：声明用户配置
├── themes/*.json          # 可选：声明主题
└── ui/                    # 隔离 UI 静态资源（Phase 4）
    ├── index.html         # entry.ui 指向这里
    ├── app.js
    └── style.css
```

`ui/` 之外不允许放 `js`/`html`；`logic/`、脚本、可执行文件仍然拒绝（Phase 5 才开放逻辑运行时）。

## 2. manifest 关键字段

```json
{
  "entry": { "ui": "ui/index.html" },
  "permissions": [
    { "capability": "dashboard.cards" },
    { "capability": "instance.read" }
  ],
  "contributes": {
    "dashboardCards": [
      { "id": "hello", "title": "Hello", "defaultSize": { "w": 4, "h": 3 } }
    ]
  }
}
```

规则（宿主会拒绝不满足的包）：

- `entry.ui` 必须位于 `ui/` 下、以 `.html` 结尾，且文件存在；
- 声明了 `entry.ui` 就必须有至少一个 `dashboardCards` / `settingsPages`；
- 反过来，包里有 `ui/` 文件却没声明 `entry.ui` 也会被拒。

## 3. 页面里拿数据

```html
<script type="module">
  import { connect, applyTheme, announce } from "./vendor/rmccc-plugin-ui.js";
  const host = connect();
  host.onInit(async ({ theme }) => {
    applyTheme(theme);
    const { instances } = await host.request("instances.list");
    document.body.textContent = `${instances.length} instance(s)`;
  });
  announce();
</script>
```

`connect()` 会：
1. 监听 `message`，只接受 `v === 1` 的消息；
2. `announce()` 发送 `{ "type": "ready" }`；
3. 宿主回 `init`（含主题与限额），之后每次 `request()` 都有超时与错误码。

> 不要在 `init` 之前渲染主题相关内容：宿主在 `init` 里才给出当前配色。

## 4. 校验与安装

```bash
node plugin-sdk/scripts/cli.mjs validate --dir my-card
node plugin-sdk/scripts/cli.mjs pack --dir my-card --out /tmp/my-card.rmcpkg --rmcli ./target/release/rmcli
./target/release/rmcli plugin install --path /tmp/my-card.rmcpkg
./target/release/rmcli plugin enable --id dev.example.my-card
```

安装后 Dashboard 里 **添加插件卡片** 即可看到；禁用/卸载后卡片显示占位，位置保留。

## 5. 调试

- 宿主 `logs/rmccc.<date>.log` 里 `target=rmccc::webview` 是前端错误；
- 卡片上的错误面板会显示线上错误码（`permission_denied` / `timeout` / …）；
- 插件未授予所需能力时，请求返回 `permission_pending` 或 `permission_denied`，
  在 `/plugins` 页面授权后重新加载卡片即可。
