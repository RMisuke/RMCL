# ADR-014: 主题 v2 —— 外观皮肤（字体 / 形状 / 间距 / 背景图 / 组件槽）与用户覆盖

- **状态**：proposed（2026-10-27 草案）
- **日期**：2026-10-27
- **阶段**：Phase 3+（主题系统扩展）
- **关联**：ADR-003（包格式）、ADR-004（Manifest/权限）、ADR-005（主题令牌/安全色）、
  ADR-006（隔离 UI/资源）、[`audit 2026-10-27`](../audits/PLUGIN_SECURITY_AUDIT_2026-10-27.md)（F-01/F-02）、
  指南 §9.5、[`THEME_PLUGIN_AUTHORING.md`](../guides/THEME_PLUGIN_AUTHORING.md)

## 背景

主题 v1（ADR-005 决策 7）只开放 8 个颜色令牌，已能表达"换一套配色"，但用户明确希望
"装了插件之后获得全新的体验"：字体、圆角、间距、背景图、组件级外观。

当前前端样式事实（2026-10-27 核对）：

- `src/styles/app.css`（2,436 行）+ `src/styles/visual-system.css`（777 行）是唯一视觉来源；
- 已有可复用令牌：`--rmccc-radius-{tag,control,card,modal}`、`--rmccc-space-1..6`、
  `--font-stack` / `--font-mono`、`--rmccc-shadow-*`、`--rmccc-accent-secondary/tertiary`；
- 但大量组件仍写死数值：约 125 处 `border-radius`、成百处 padding/gap/font-size；
- `.hero::before` 已经消费 `--hero-image`（"未来可替换为壁纸"的预留钩子）；
- 组件类名稳定（`.rm-card` / `.rm-btn` / `.rm-input` / `.rm-menu` / `.hero-card` /
  `.instance-card` / `.titlebar`），可以作为一个受控的"皮肤槽"集合。

因此**不需要**重写全部 CSS，也不应该把任意 CSS 交给插件。真正要决策的是：
开放哪些维度、用什么类型系统表达、如何在不放弃安全边界的前提下让主题和用户都能覆盖。

### 可行性验证（2026-10-27，demo-only）

在 `demo/`（浏览器演示，不纳入版本库）里用**单层 CSS 变量 + 受控选择器**把一套
"Aurora Glass"皮肤叠加到真实前端上：字体栈（UI/标题/等宽）、圆角 22–26px、
间距 5/10/15/20/30/40、背景图 + 暗化叠层、卡片/按钮/输入/菜单/对话框的玻璃质感。
Home 与 Settings 均正常渲染，未改任何组件 JSX，也未出现布局塌陷。验证结论：

1. 组件级外观可以通过"宿主 CSS + 主题变量"实现，主题本身仍然只是数据；
2. 必须**连同颜色**一起换（只改暗背景会把浅色主题的文字变成不可读），所以 v2 主题
   仍然包含 `colors`；
3. 背景图必须加暗化/模糊叠层并限制在内容层之后，否则影响可读性；
4. 字体、圆角、间距、组件槽可以用同一套变量通道，不需要每个组件单独开 API。

## 目标

- 主题可以声明：**字体**（系统字体栈或包内字体文件）、**形状**（圆角）、
  **间距/密度**、**背景图**、**组件槽外观**（卡片/按钮/输入/菜单/对话框/标签/Hero/标题栏）。
- 主题仍是**纯数据**：没有 CSS 文本、没有 JS、没有远程资源；所有 CSS 由宿主拥有。
- 用户拥有最终决定权：可逐项接受/拒绝主题的字体、背景、组件外观，并可覆盖密度、
  圆角缩放、背景图。
- v1 主题完全兼容；v2 主题在旧版本上 fail-closed（拒绝应用，而不是部分生效）。
- 新增能力有明确风险等级与撤销语义，不扩大现有 `themes.apply` 的权限。

**不解决**：任意 CSS/组件选择器、布局重排、动画关键帧、远程字体/图片、
改变文案/行为/安全语义色、插件 iframe 内部的任意换肤（iframe 只拿令牌）。

## 选项

### A. 允许主题携带 CSS 文件（受控 sanitizer）

优点：表现力最大，作者几乎可以做任何事。
缺点：CSS 本身就能隐藏/伪装关键操作（`display:none` 危险按钮、伪元素覆盖点击区）、
用 `url()` 外链做追踪、破坏可读性；要写一个安全的 CSS 子集解析器成本极高，
且每加一个浏览器特性都要重新审计。**否决**。

### B. 类型化令牌白名单 + 宿主皮肤层（选中）

主题声明一组**有类型、有范围**的设计值；宿主把它们编译成 CSS 变量，由宿主的
`theme-skin.css` 消费。主题不能写选择器，也不能写任意 CSS 值。
优点：表现力覆盖用户要的五个维度；安全边界可测试；CSS 演进不影响协议。
缺点：组件槽是白名单，作者不能"想改哪里改哪里"；需要维护皮肤层与视觉系统同步。

### C. 完整 Design Token/组件 API（每个组件独立 schema）

优点：最干净、最可扩展。
缺点：等于重做设计系统，工作量和回归面都远超本轮需求；先把 B 做稳，再按需升级。

## 决策

采用 **B**，并补充三条产品约束：**能力分级、用户覆盖优先、包内资源**。

### 1. 主题文档 v2

`themes/<id>.json` 的 `schemaVersion` 从 `1` 升到 `2`。v2 包含 v1 的 `colors`，
并可追加以下**全部可选**段：

```json
{
  "schemaVersion": 2,
  "name": "Aurora Glass",
  "mode": "dark",
  "colors": {
    "background": "#06111b",
    "surface": "#0c1b28",
    "surfaceElevated": "#132635",
    "text": "#e7f4f2",
    "textMuted": "#93abaa",
    "accent": "#6fe7d2",
    "accentContrast": "#04130f",
    "border": "#22394c"
  },
  "typography": {
    "ui": {
      "family": "Aurora Sans",
      "fallback": "sans-serif",
      "files": [
        {
          "src": "assets/fonts/aurora-ui.woff2",
          "weight": "400 700",
          "style": "normal",
          "display": "swap"
        }
      ]
    },
    "display": { "family": "Iowan Old Style", "fallback": "serif" },
    "mono": { "family": "SF Mono", "fallback": "monospace" }
  },
  "shape": {
    "tag": 9,
    "control": 14,
    "card": 22,
    "modal": 26,
    "pill": true
  },
  "spacing": {
    "unit": 5,
    "density": "comfortable"
  },
  "background": {
    "image": "assets/bg/aurora.webp",
    "fit": "cover",
    "position": "center",
    "dim": 0.55,
    "blur": 0,
    "scope": "both"
  },
  "components": {
    "card": {
      "background": "#0a18268c",
      "border": "#96f0e624",
      "radius": 22,
      "shadow": "soft",
      "blur": 20,
      "padding": 20
    },
    "button": { "radius": 999, "primaryBackground": "#6fe7d2", "primaryText": "#04130f" },
    "input": { "background": "#040e1880", "border": "#96f0e624", "radius": 14 },
    "menu": { "background": "#081420dc", "radius": 18, "blur": 26 },
    "dialog": { "radius": 26, "blur": 26 },
    "tag": { "radius": 9 },
    "hero": { "background": "#0a18268c", "radius": 24, "blur": 26 },
    "titlebar": { "background": "#06111b6b", "blur": 24 }
  }
}
```

字段规则：

| 段 | 字段 | 类型/范围 |
|---|---|---|
| `typography.*` | `family` | 字体族名（字母/数字/空格/`-`/`_`，≤64）；宿主加引号并追加 fallback |
| | `fallback` | 枚举 `sans-serif`/`serif`/`monospace`/`system-ui` |
| | `files[]` | `assets/…` 下 `.woff2/.woff/.ttf/.otf`，≤2 MiB/个，≤4 个/主题 |
| | `files[].weight` | `"100".."900"` 或 `"100 900"` 区间 |
| | `files[].style` | `normal`/`italic`；`display`：`auto`/`swap`/`block` |
| `shape` | `tag/control/card/modal` | 0–32（px）；`pill` 为 bool（按钮是否全圆角） |
| `spacing` | `unit` | 3–6（px）；宿主派生 `space-1..6 = unit,2u,3u,4u,6u,8u` |
| | `density` | `compact`/`comfortable`/`spacious`（组件 padding 的预设倍率） |
| `background` | `image` | `assets/…` 下 `.png/.jpg/.jpeg/.webp`，≤8 MiB；禁 SVG/远程 |
| | `fit` | `cover`/`contain`；`position`：`center/top/bottom/left/right` |
| | `dim` | 0–0.9；`blur`：0–40（px） |
| | `scope` | `app`/`hero`/`both` |
| `components.*` | `background`/`border`/`text` | 颜色 `#rgb/#rrggbb/#rrggbbaa` 或 `null`（继承） |
| | `radius` | 0–32；`padding`：0–32；`blur`：0–40 |
| | `shadow` | 枚举 `none`/`soft`/`strong` |
| | `primaryBackground`/`primaryText`（仅 button） | 颜色/null |

- 未知字段拒绝（与 v1 一致）；`components` 只允许上述 8 个槽，未知槽拒绝；
- 任何非 `colors` 段出现即 `requires_skin = true`；
- 数值越界、颜色非法、字体文件名非法都在**安装期**拒绝，而不是应用时才失败。

### 2. 能力分级：新增 `themes.skin`（medium）

- `themes.apply`（low，自动授予）继续只覆盖 `colors`；
- `themes.skin`（medium，必须显式授予）覆盖字体/形状/间距/背景/组件；
- 包检查时解析每个主题文档：`requires_skin == true` 而 Manifest 未声明
  `themes.skin` → **拒绝安装**（fail closed）；
- 声明了 `themes.skin` 的插件与其它 medium 能力一致：安装后 `installed_disabled`，
  用户在插件详情里审查授予后才可启用；
- 运行期撤销 `themes.skin`：下一次应用只使用 `colors`（`skin` 返回 `null`），
  主题本身仍在列表里，不抛错、不残留字体/背景。

理由：字体可以重映射字形、背景图可以伪装界面元素，属于"视觉欺骗"面，与纯配色不同级。
这条分级同时满足"安全优先"和"给用户调整空间"——用户明确知道自己在接受什么。

### 3. 宿主管线

```text
dashboard_get_theme_skin(key)
  ├─ 校验主题文档（大小/schema/类型/范围）
  ├─ 校验资产：存在、扩展名、≤上限、magic bytes（字体/图片）
  ├─ 生成 rmcccplugin://<id>/assets/... 的只读 URL（宿主拼装，主题不提供 URL）
  └─ 返回 { skin, assets, requiresSkin, capabilityGranted }
        ↓
applyThemeSkin(skin, appearance)     ← 用户覆盖在这一步合并，用户优先
  ├─ 写 CSS 变量（--rmccc-* / --rmccc-skin-*）
  ├─ 注入 <style id="rmccc-theme-fonts"> 的 @font-face（仅已校验 URL）
  └─ 切换 <html data-skin-font/background/components>
        ↓
theme-skin.css（宿主所有，视觉系统的一部分）
```

- **资产服务**：`rmcccplugin://` 增加 `assets/` 命名空间，仅 image/font 扩展名；
  要求插件 `enabled` 且同时持有 `themes.apply` + `themes.skin`；
  字体响应加 `Access-Control-Allow-Origin: *`（`@font-face` 需要 CORS），
  图片/字体均 `Cache-Control: no-store`；不服务 JSON/HTML/JS；不服务 `themes/`。
- **URL 拼装**：由 Rust 用 `url` 类型构造并做百分号编码，主题只提供相对路径；
  主题永远不能提供 `http(s)://`、`data:`、`file:` 或 `javascript:`。
- **应用顺序**：内置视觉系统 → `theme-skin.css`（变量默认值 = 当前外观） →
  主题变量 → 用户覆盖变量。用户覆盖永远最后写入。
- **插件 UI**：`theme.get` 继续返回 `colors`；v2 额外返回 `typography/shape/spacing`
  的**令牌**（不含资产 URL），让插件卡片与宿主观感一致；iframe 仍不能加载主题字体/背景
  （CSP `font-src 'self'`），避免扩大隔离面。

### 4. 组件槽（白名单）

| 槽 | 选择器 | 可覆盖 |
|---|---|---|
| `app` | `.app-root` | 背景图/暗化（`background` 段） |
| `titlebar` | `.titlebar`, `.titlebar-dock`, `.dock-item` | 背景/圆角/模糊 |
| `card` | `.rm-card`, `.instance-card` | 背景/边框/文字/圆角/阴影/模糊/padding |
| `hero` | `.hero-card`, `.hero-module`, `.hero::before` | 背景/边框/圆角/模糊 + `--hero-image` |
| `button` | `.rm-btn`, `.rm-btn-*` | 圆角/主色/文字色/padding（尺寸仍由 `sm/md/lg` 决定） |
| `input` | `.rm-input`, `.rm-select`, `.rm-textarea` | 背景/边框/圆角 |
| `menu` | `.rm-menu`, `.rm-menu-item`, `.toast` | 背景/边框/圆角/模糊/阴影 |
| `dialog` | `.rm-dialog` | 背景/边框/圆角/模糊/阴影 |
| `tag` | `.rm-tag`, `.rm-chip` | 圆角/背景/边框/文字 |

`theme-skin.css` 的默认值**等于当前 Visual Voxel 外观**，因此不装 v2 主题时零视觉变化；
每个槽只允许上表字段，不开放宽高、定位、display、字体大小等布局属性。

### 5. 用户覆盖（"给用户调整的空间"）

新增 `<data_dir>/appearance.json`（独立文件，schemaVersion 1），由外观页写入：

```json
{
  "schemaVersion": 1,
  "acceptThemeFont": true,
  "acceptThemeBackground": true,
  "acceptThemeComponents": true,
  "density": "theme",
  "radiusScale": 1.0,
  "fontFamily": "theme",
  "background": { "mode": "theme", "path": null, "dim": 0.55, "blur": 0 }
}
```

- `density`：`theme`/`compact`/`comfortable`/`spacious`（覆盖主题 unit 倍率）；
- `radiusScale`：0.5–1.5（在主题圆角上乘算，钳到 0–32）；
- `fontFamily`：`theme`/`system`/`serif`/`rounded`/`mono`（内置预设，用户不必选字体文件）；
- `background.mode`：`theme`/`none`/`image`；`image` 使用用户选择的本地图片
  （由宿主复制到 app data 并校验，仅登记相对文件名，不存绝对路径）；
- 三项 `acceptTheme*` 默认 `true`（既然用户已显式授予 `themes.skin`），
  但可一键关闭；关闭后主题对应维度不生效，其余照常；
- 外观页提供"恢复默认外观"（保留主题配色，清空 `acceptTheme*`/覆盖项）。

### 6. 安全与隐私

| 威胁 | 处置 |
|---|---|
| 主题注入任意 CSS/选择器 | 不接受 CSS 文本；所有 CSS 宿主所有；值类型化 + 范围 |
| 通过 `url()` 外链追踪 | 主题只给相对路径；URL 宿主拼装；只允许包内 image/font |
| 背景图伪装界面/诱导点击 | 背景层 `pointer-events:none` 且在内容之后；用户可拒绝/关闭；应用前预览 |
| 字体重映射字形进行视觉欺骗 | `themes.skin` 显式授权 + 预览 + 用户可拒绝字体；只允许包内字体 |
| 字体/图片炸弹 | 单文件上限（字体 2 MiB / 图片 8 MiB）、数量上限、包总上限；magic bytes 校验 |
| 通过主题资产读本地文件 | 资产只从插件安装目录 `assets/` 读取；`validate_relative_path` 禁 `..`/绝对路径/符号链接 |
| 主题偷偷改变危险操作外观 | 安全语义色仍由宿主拥有；组件槽不允许改 `danger/success/warning` 的语义 |
| 主题导致界面不可用 | 用户覆盖与"恢复默认"；数值钳制；主题文件损坏时回退 v1 颜色/内置主题 |

残余风险：任何允许换字体/背景的系统都无法完全消除"视觉钓鱼"；本 ADR 通过
显式授权、预览、逐项开关和"主题不能改文案/行为"把风险限制在可接受范围。

## 后果

**正面**

- 主题表达力覆盖用户要求的五个维度，且不需要拆分/重写组件。
- 协议仍是数据，CSS 归宿主；新增维度可独立审计、独立撤销。
- 用户覆盖让"不喜欢主题字体/背景"不再等于"卸载主题"。
- 现有 8 套内置主题与 v1 插件零改动。

**负面/代价**

- 需要维护 `theme-skin.css` 与视觉系统的同步（新增组件若要可换肤，要登记槽）；
- 组件槽是白名单，作者不能任意改布局；
- 安装期需要解析主题文档并检查资产，包检查时间略增；
- 用户覆盖增加一个设置文件与一轮 UI 工作。

## 迁移影响

- v1 主题：不变，`schemaVersion: 1` 继续通过。
- v2 主题：旧版本（不支持 v2）在解析时拒绝并回退内置主题——这是有意的 fail-closed。
- 用户设置：`user_settings.theme` 不变；新增 `appearance.json`，缺失时用默认值。
- 回滚：删除 `theme-skin.css` 与 v2 应用逻辑即可回到 v1；`appearance.json` 被忽略。

## 实施切片

| 切片 | 内容 | 验收 |
|---|---|---|
| **S1** | v2 schema/校验/DTO；`dashboard_get_theme_skin`；`theme-skin.css` 默认层；shape/spacing/typography（系统字体）；用户覆盖（密度/圆角）；`appearance.json` 读写 | v1/v2 主题均能加载；不装 v2 主题时截图与当前完全一致；非法值安装被拒；单测覆盖 |
| **S2** | 包内背景图 + 字体：资产校验（magic/大小/数量）、`assets/` 服务 + CORS、`@font-face` 注入、预览/确认 | 真实图片/字体在 Home+Settings 生效；`url()`/SVG/超大文件被拒；字体加载失败回退 |
| **S3** | 组件槽（8 个）变量化；`theme-skin.css` 全面接线；对比度/可读性 lint | 槽位全覆盖；明暗两套主题下按钮/文字可读；无布局位移 |
| **S4** | 外观页"视觉定制"、主题预览、用户自定义背景（本地图片复制）、示例主题 Aurora | 用户可逐项开关/重置；示例主题打包可安装；文档与截图 |

S1 完成前不改任何主题协议；S1–S3 每步加负向测试并更新安全审计的威胁表。

## 验证方式

- Rust：v2 解析/范围/未知字段/资产引用/能力门槛（安装期拒绝）单测；
- 前端：`applyThemeSkin` 纯函数单测（默认/主题/用户覆盖三层优先级的变量快照）；
- 视觉：demo harness 在 Home/Settings/Instances 三页做"默认 vs v2"截图对比；
- 安全：恶意包（`background: "https://…"`、`url()` 字体、SVG、超大文件、越界圆角）
  全部安装失败；撤销 `themes.skin` 后下一次应用无字体/背景残留；
- 可访问性：`text` 对 `background/surface` 对比度 ≥4.5:1（警告，不强制拒绝）；
- Windows/Linux：沿用 ADR-006 的 deferred 结论，不因本 ADR 改变。

## 待拍板问题

1. `themes.skin` 采用 **medium（显式授权）**，还是 low + 应用前二次确认？本文推荐 medium。
2. 用户覆盖是否需要"独立于主题保存多套外观预设"？本文先做单套 + 重置。
3. 组件槽是否包含 `sidebar`/`table`/`tooltip`？本文先做 8 个高频槽，按反馈扩展。
4. 是否允许主题替换**插件 iframe** 内的字体？本文只传令牌，不加载资产（避免扩大 iframe 攻击面）。

## 状态变更记录

- 2026-10-27：proposed。基于 demo-only 可视化验证（Aurora Glass：字体/圆角/间距/
  背景图/组件玻璃质感），未改生产代码；accepted 前不得据此改协议。
