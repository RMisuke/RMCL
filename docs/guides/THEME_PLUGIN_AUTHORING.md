# RMCCC 主题插件开发规范

> 适用对象：想为 RMCCC / SNGL 增加一套配色风格的主题作者。
> 主题是**纯数据插件**：只有 JSON 与静态资源，**不执行任何代码**。
> 相关决策：[ADR-005](../adr/ADR-005-dashboard-layout-and-theme.md)（令牌与安全语义色）、
> [ADR-004](../adr/ADR-004-plugin-manifest-and-versioning.md)（Manifest / 权限）、
> [ADR-003](../adr/ADR-003-plugin-package-and-install-transaction.md)（包格式）。
> 通用插件流程见 [PLUGIN_AUTHORING.md](PLUGIN_AUTHORING.md)；本文只讲"主题"这一形态。

> **v2 已实现（ADR-014 accepted）**：`schemaVersion: 2` 支持字体/圆角/间距/背景图/
> 循环视频/组件贴图/主界面特效/Tab 装饰，需要额外能力 `themes.skin`（medium，安装后
> 由用户显式授予）。字段与示例见 §10；`schemaVersion: 1` 仍完全兼容。

## 0. 先明确边界

| 主题可以做 | 主题不能做 |
|---|---|
| 定义 8 个设计令牌（背景/表面/文字/强调色/边框等） | 执行 JS、WASM 或任何脚本 |
| 声明明暗（`dark` / `light`） | 覆盖 `danger` / `success` / `warning` 三个安全语义色 |
| 提供多套主题（同一插件最多 8 套） | 改变布局、文案、按钮行为、权限或数据 |
| 附带 `assets/`、`locales/` 静态资源 | 加载远程字体/图片/CSS（包内资源才可以） |
| 在隔离 UI 里通过 `theme.get` 读取当前令牌 | 主动"推送"主题、改别的插件或宿主设置 |

宿主只把主题值写进 CSS 自定义属性；**主题不继承业务权限**，也不参与任何数据请求。
因此"恶意主题"能造成的最坏效果是"界面难看"，而不是"读到账号/删掉实例"。
即便如此，颜色值仍然按不可信输入校验（见 §3.3），避免 CSS 注入或外链探针。

## 1. 主题插件包含什么

```text
my-theme/
  manifest.json          # 必需
  themes/
    midnight.json        # 必需：每套主题一个文件，路径必须以 themes/ 开头、.json 结尾
  README.md              # 建议
  LICENSE                # 建议
  CHANGELOG.md           # 可选
  config.schema.json     # 可选：只有需要用户配置（如"强调色变体"）时才要
  assets/                # 可选：截图、图标等静态资源
```

最小可用插件 = `manifest.json` + 一个通过校验的主题 JSON。参考实现：
[`examples/plugins/theme-midnight/`](../../examples/plugins/theme-midnight)。

包级限制（与所有插件相同）：16 MiB 包 / 64 MiB 解压 / 512 条目 / 单文件 16 MiB；
**主题文件本身另有 64 KiB 上限**。

## 2. Manifest

```json
{
  "manifestVersion": 1,
  "id": "dev.example.my-theme",
  "name": "My Theme",
  "version": "1.0.0",
  "description": "一套深色主题，无任何可执行代码。",
  "publisher": { "id": "dev.example", "name": "Example" },
  "compatibility": {
    "pluginApi": "1.x",
    "core": ">=0.1.0 <0.2.0",
    "platforms": []
  },
  "permissions": [
    { "capability": "themes.apply", "reason": "提供一套深色主题" }
  ],
  "contributes": {
    "themes": [
      { "id": "midnight", "name": "Midnight", "path": "themes/midnight.json" }
    ]
  }
}
```

主题相关规则：

- `permissions` 只需声明 `themes.apply`（低风险，安装时自动授予）。
  不要为了主题去申请 `network.request` / `game.launch` 等能力——主题不需要它们，
  申请了反而会让插件进入"待审查"、无法自动启用。
- `contributes.themes` 最多 **8** 项；`id` 必须匹配 `^[a-z][a-z0-9-]*$`（≤64），
  在**整个插件内唯一**（与卡片/设置页共享命名空间）。
- `path` 必须同时满足：包内相对路径、`themes/` 前缀、`.json` 后缀、文件真实存在。
- `name` 非空、≤80 字符；主题列表优先显示它（主题 JSON 里的 `name` 只作说明）。
- 同一插件的主题 key 是 `<pluginId>/<themeId>`，例如
  `dev.example.my-theme/midnight`。**主题 id 一旦发布不要复用**（用户偏好按 key 保存）。

## 3. 主题 JSON

### 3.1 完整示例

```json
{
  "schemaVersion": 1,
  "name": "Midnight",
  "mode": "dark",
  "colors": {
    "background": "#0b1020",
    "surface": "#141a2e",
    "surfaceElevated": "#1c2440",
    "text": "#e6e9f2",
    "textMuted": "#8b93a8",
    "accent": "#7c6cff",
    "accentContrast": "#ffffff",
    "border": "#2a3352"
  }
}
```

JSON Schema：[`schemas/theme.schema.json`](../../schemas/theme.schema.json)（编辑器可挂 `$schema`）。

### 3.2 字段

| 字段 | 必需 | 规则 |
|---|---|---|
| `schemaVersion` | ✅ | 必须是 `1`；未知版本安装即失败（不会"尽量解析"） |
| `name` | ❌ | 非空、≤80 字符；仅说明用途 |
| `mode` | ❌ | `"dark"` 或 `"light"`；缺省时宿主按 `background` 亮度推断 |
| `colors` | ✅ | 颜色令牌表，见 §3.3 |

**未知顶层字段会被拒绝**（v1 严格模式，防拼写错误静默生效）。`colors` 里不认识的键
会被忽略——它们不会进入任何 CSS 变量，也不会报错。

### 3.3 颜色令牌

| 令牌 | 必需 | 缺省回退 | 作用 |
|---|---|---|---|
| `background` | ✅ | — | 应用背景 / 页面底色 |
| `surface` | ✅ | — | 卡片、面板、输入框底色 |
| `surfaceElevated` | ❌ | 取 `surface` | 浮层、悬停、二级面板 |
| `text` | ✅ | — | 主文字 |
| `textMuted` | ❌ | 取 `text` | 次要文字、说明 |
| `accent` | ✅ | — | 主强调色（按钮、选中态、焦点） |
| `accentContrast` | ❌ | `#ffffff` | 强调色上的文字色 |
| `border` | ✅ | — | 边框、分隔线 |

兼容别名：`surface_elevated` / `text_muted` / `accent_contrast` 也接受，
但**同时存在时 camelCase 优先**；新主题请只用 camelCase。

**颜色语法只允许三种**：

```text
#rgb   #rrggbb   #rrggbbaa
```

`url(...)`、`var(...)`、`rgb()/hsl()`、颜色名、分号、换行、空串一律拒绝。
这条规则同时用于打包、安装与应用运行期——即使安装后有人手改文件，宿主也不会把
非十六进制字符串写进 CSS 变量。

> `danger` / `success` / `warning` 是**宿主拥有的安全语义色**。
> 写进 `colors` 不会报错，但会被忽略；它们按当前明暗由宿主固定注入。
> 这是为了确保"删除/警告/成功"的颜色不会被主题伪装。

### 3.4 文件大小与编码

- 单个主题文件 ≤ **64 KiB**（UTF-8，无 BOM）。
- 只允许 JSON 对象；不允许注释、尾部逗号。建议 2 空格缩进。

## 4. 明暗、对比度与观感

- **明暗判定顺序**：`mode` 声明 → 缺省按 `background` 相对亮度（< 0.5 视为暗色）。
  宿主据此决定安全语义色、`color-scheme` 与装饰层。
- 建议对比度：`text` 对 `background`/`surface` ≥ 4.5:1；`textMuted` ≥ 3:1；
  `accentContrast` 对 `accent` 也应按 4.5:1 检查。
- 亮色主题需同时确认：输入框/卡片与背景不要完全相同（否则边界消失）；
  `border` 建议比 `surface` 略深。
- 主题只控制颜色，**不控制**圆角、间距、字体、阴影与背景图——这些由宿主视觉系统统一
  （`src/styles/theme-core.css` + `theme-skin.css`），以保证所有主题下的可读性与可访问性。

## 5. 宿主如何对接（数据流）

```text
安装 .rmcpkg
  └─ 包检查：主题文件存在 + ≤64 KiB + ThemeDocument::parse 通过
       └─ 注册表登记（无权限扩张）
            └─ 设置 → 外观：dashboard_list_themes 返回 {key, name, mode}
                 └─ 用户点选 → dashboard_set_theme_preference
                      └─ 后端再次读取/校验主题文件（失败则拒绝保存）
                           └─ dashboard_get_theme_tokens → 8 个令牌
                                └─ 前端写入 CSS 变量（--rmccc-* + legacy 变量）
```

要点：

1. **令牌解析是宿主唯一真相**。前端不读主题文件，只消费 Rust 返回的令牌；
   插件 UI 通过桥的 `theme.get`（或 `init` / `theme` 消息）拿同一份令牌。
2. **选择即校验**。损坏/缺失的主题无法被保存为偏好；界面会显示错误而不是静默回退。
3. **运行期第二道闸**。已保存的偏好每次应用都重新读文件、重新校验；文件被删/被改坏时
   回退到内置安全主题并记录日志。
4. **偏好失效**。插件被禁用/卸载后，它的主题从列表消失，当前偏好回退到内置主题；
   重新启用插件即可再次选择。

### 5.1 CSS 变量映射

| 令牌 | 新变量（`--rmccc-*`） | 兼容变量（宿主内部） |
|---|---|---|
| `background` | `--rmccc-background` | `--surface-bg` |
| `surface` | `--rmccc-surface` | `--surface-card` |
| `surfaceElevated` | `--rmccc-surface-elevated` | `--surface-raised` / `--surface-input` / `--surface-hover` |
| `text` | `--rmccc-text` | `--text-primary` / `--text-on-input` |
| `textMuted` | `--rmccc-text-muted` | `--text-secondary` |
| `accent` | `--rmccc-accent` | `--color-blue` |
| `accentContrast` | `--rmccc-accent-contrast` | — |
| `border` | `--rmccc-border` | `--border-default` / `--border-subtle` |

宿主还会派生（**主题不可直接设置**）：

- `--rmccc-accent-secondary` / `--rmccc-accent-tertiary`：装饰三色；
  插件主题取当前明暗的内置基准调色板，`primary` 使用你的 `accent`；
- `--rmccc-accent-gradient`：由 `accent` 与装饰色生成的渐变；
- `--color-red/green/yellow` 及其浅色/边框变体：安全语义色，按 `mode` 固定。

## 6. 本地开发与验收

```bash
# 1) 结构校验（与安装器同一套规则，包含主题内容校验）
rmcli plugin pack --dir my-theme --out /tmp/my-theme.rmcpkg

# 2) 安装（主题能力会自动授予）
rmcli plugin install --path /tmp/my-theme.rmcpkg --enable
rmcli plugin inspect --id dev.example.my-theme

# 3) 在 GUI：设置 → 外观 → 选择你的主题
#    切换后立即生效；"查看主题 JSON"可对照内置主题的合成 JSON
```

验收清单：

- [ ] `themes.apply` 是唯一能力；安装后插件为 `enabled` 且无需审查。
- [ ] 主题出现在「设置 → 外观」，明暗标记与声明/推断一致。
- [ ] 切换后背景、卡片、文字、边框、强调色、悬停态都正确，无"某块仍是旧主题"。
- [ ] 亮色主题下安全语义色（错误红/成功绿/警告黄）仍清晰可辨。
- [ ] 输入框、禁用态、选中态、焦点环在明暗两种模式下都可读。
- [ ] 删除/禁用插件后，主题从列表消失且界面回退正常（不崩、不残留）。

## 7. 版本与发布

- 插件用严格 SemVer（`1.2.3`）；`compatibility.pluginApi` 目前是 `1.x`。
- **升级不自动扩张权限**：新版本新增能力会进入待审查列表，插件回到禁用状态。
- 主题文件可以增加可选令牌（向后兼容）；删除/改名令牌属于破坏性变更，应升 major。
- 主题 key（`<pluginId>/<themeId>`）是用户偏好的持久标识，发布后不要改 `id`。
- 当前**没有签名校验**（ADR-008 仍为 proposed）：所有安装记为 `development`，
  只安装你信任的包；`SHA-256` 仅用于诊断，不是来源认证。

## 8. 安全红线（会被直接拒绝或忽略）

1. 不要尝试在颜色里写 `url()`、`var()`、`;`、`}`、换行或超长字符串——校验会拒绝整个包。
2. 不要试图覆盖 `danger`/`success`/`warning`——宿主忽略它们。
3. 不要在主题包里放 `.js`/`.html`/`.wasm`/原生库——包白名单会拒绝（主题不需要代码）。
4. 不要依赖"主题可以改布局/文案/权限"——令牌只影响颜色。
5. 不要在主题文件里放用户数据或远程地址；主题会被原样打包分发。

## 9. 常见问题

| 现象 | 原因 / 处理 |
|---|---|
| 安装报"主题…校验失败：颜色必须…" | 颜色不是 `#rgb`/`#rrggbb`/`#rrggbbaa`；改成十六进制 |
| 安装报"主题…过大" | 文件 > 64 KiB；删掉无用元数据 |
| 安装报"主题…缺少必需颜色令牌" | 补 `background`/`surface`/`text`/`accent`/`border` |
| 主题列表里没有我的主题 | 插件未启用、`themes.apply` 未授予，或 `contributes.themes` 未声明 |
| 点选主题报"主题不存在，或所属插件未启用" | 插件被禁用/卸载；重新启用后重选 |
| 明暗判断不对 | 显式写 `"mode": "dark"` 或 `"light"` |
| 界面某处仍是旧配色 | 宿主 CSS 变量没覆盖到的组件属于视觉系统 bug，请附主题 JSON + 截图反馈 |

## 10. v2 外观皮肤（ADR-014，已实现）

```json
{
  "schemaVersion": 2,
  "mode": "dark",
  "colors": { "background": "#06111b", "surface": "#0c1b28", "text": "#e7f4f2", "accent": "#6fe7d2", "border": "#22394c" },
  "typography": { "ui": { "family": "Avenir Next", "fallback": "sans-serif" } },
  "shape": { "tag": 9, "control": 14, "card": 22, "modal": 26, "pill": true },
  "spacing": { "unit": 5, "density": "comfortable" },
  "background": { "video": "assets/bg/loop.mp4", "poster": "assets/bg/poster.jpg", "fit": "cover", "position": "center", "dim": 0.42, "scope": "both" },
  "components": {
    "card": { "background": "#0a18268c", "border": "#96f0e624", "radius": 22, "shadow": "soft", "blur": 18, "padding": 20 },
    "button": { "radius": 24, "primaryBackground": "#6fe7d2", "primaryText": "#04130f" }
  },
  "effects": [
    { "type": "aurora", "intensity": 0.6, "speed": 26, "area": "app" },
    { "type": "rotating-blocks", "texture": "assets/tex/cube.png", "count": 7, "size": 54, "speed": 20, "area": "hero" },
    { "type": "particles", "kind": "star", "color": "#9fe8ff", "count": 18, "area": "hero" }
  ],
  "navigation": {
    "icons": { "home": "assets/icons/home.svg" },
    "ornaments": [{ "image": "assets/ui/ornament.png", "side": "right", "width": 120 }]
  }
}
```

- **能力**：v2 任一非 `colors` 段出现 → Manifest 必须声明 `themes.skin`（medium）。
  未授予时只应用配色；授权后可启用。`themes.apply` 仍是主题列表的门槛。
- **字体**：`typography.ui/display/mono`，可只给系统字体族，或用 `files` 指向包内
  `.woff2/.woff/.ttf/.otf`（≤2 MiB/个、≤4 个/槽）。宿主注入 `@font-face`，中文缺字回退。
- **形状/间距**：`shape` 0–32px；`spacing.unit` 3–6px（派生 6 档间距），`density` 仅作语义标记。
- **背景**：`image` 或 `video`（mp4/webm），可选 `poster`。只允许包内 `assets/`；
  文件 ≤8 MiB（图片）/16 MiB（视频）；`dim` 0–0.9、`blur` 0–40、`scope` app/hero/both。
- **组件槽**：`card/button/input/menu/dialog/tag/hero/titlebar`，可设 background/border/text/
  radius/shadow/blur/padding，以及 `image` + `imageSlice` + `imageMode`
  （`background` 拉伸 / `border` 九宫格）。按钮九宫格用伪元素绘制，不会挤压文字。
- **特效**（宿主预设，主题只给参数，最多 3 条）：
  `rotating-blocks`（包内贴图、count 1–24、size 24–160、speed 4–60、area）、
  `aurora`（intensity 0–1、speed）、`particles`（dot/star/square、hex 颜色、count 1–80）。
  动效受宿主“减少动态效果”开关约束；视频在关闭动效时只显示 poster。
- **导航**：`icons` 覆盖六个 Dock 图标（home/gamepad/download/person/extension/settings），
  `ornaments` 最多 2 个（left/right、width 24–240、opacity 0–1），纯装饰、不接管点击。
- **安全**：资产只从包内 `assets/` 读取；SVG 拒绝 script/DOCTYPE/ENTITY 与外部 href；
  图片/字体/视频做魔数校验。资源路由 `rmcccplugin://<id>/assets/...` 需要
  `themes.apply` + `themes.skin`，字体带 CORS、视频支持 Range。
- **示例**：`examples/plugins/theme-aurora`（视频 + 玻璃 + 极光/方块/粒子）与
  `examples/plugins/theme-grass`（KenPixel 字体 + MC 九宫格 + 草方块 + 全景图）。

验收：`rmcli plugin pack/install` → 授予 `themes.skin` → 启用 → 设置→外观选择；
切换主题后字体/背景/组件/特效应完整生效，切回内置主题应完全恢复。
