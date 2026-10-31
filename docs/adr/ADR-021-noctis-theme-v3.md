# ADR-021: Noctis 主题系统 v3 —— 氛围背景、强调色角色与代表性取色

- **状态**：accepted
- **日期**：2026-10-31
- **阶段**：Phase 3+（外观 / 主题）
- **关联**：[ADR-005](ADR-005-dashboard-layout-and-theme.md)（8-token 主题契约）、
  [ADR-014](ADR-014-theme-v2-appearance-skin.md)（v2 皮肤 / 用户覆盖）、
  [ADR-020](ADR-020-native-resolution-wallpaper-engine.md)（本地视频/图片壁纸）、
  `docs/ui-design/01_STYLE_GUIDE.md`、`08_SETTINGS.md`、`13_NOTES.md`

## 背景

“Chromatic Voxel” 视觉层在 2026-10 叠加到旧软玻璃界面上：三色渐变、轨道光效、
大面积高光与粒子同时出现，导致三个问题：

1. **强调色无处不在**：主按钮、激活项、卡片描边、输入焦点、进度条、图标和光斑
   都使用 accent；一屏出现多个同权重焦点，层级反而变弱。
2. **主题没有默认氛围**：没有视频/图片壁纸时只剩一层底图渐变；内置主题没有
   宿主背景和粒子，主题差异主要靠换 accent 色。
3. **自适应取色不可靠**：只采样一张 64×64 缩略图，视频只取当前一帧；聚类权重
   过度偏爱高饱和像素，容易选到画面中小面积的高饱和噪点；结果强制浅色模式，
   深色主题用户也会被改成浅色，且没有代表性色 / 置信度信息。

公开主题契约仍是 8 个 `ThemeTokens`，不能为了视觉效果扩展插件 schema。因此
需要一次宿主侧重构，而不是继续叠加覆盖层。

## 目标

1. 建立一套与旧版明显不同的视觉语言：深色 chrome、墨色玻璃、圆角卡片、
   accent 只代表“可操作/当前状态”。
2. 让每个内置主题在没有用户壁纸时都拥有**符合主题的动态氛围背景**和粒子意象。
3. 自适应主题从“缩略图主色”升级为**多区域/多帧 + 代表性色选择**，并跟随当前
   明暗模式；采样失败时有清晰、可复现的降级。
4. 保持主题插件 v1/v2 兼容、安全色边界、20 MiB 二进制预算和视频壁纸的零额外
   解码路径。

## 选项

### A. 继续在旧玻璃界面上叠加新的渐变/粒子

成本最低，但层级问题与“主题只是换色”不会解决；还会继续增加 CSS 覆盖和 GPU
模糊层。拒绝。

### B. 扩展主题 JSON，让主题自带背景/粒子参数

表达力强，但需要改公共 schema、权限、插件检查、资源路由和跨版本兼容；且主题
作者会被要求设计动效，违背“主题只是数据、宿主拥有 CSS/JS”的边界。拒绝。

### C. 宿主视觉 v3 + 主题派生场景 + 自适应 v2（采纳）

公开 schema 不动。宿主为已知主题维护 `ThemeScene`（atmosphere / 三色 / 粒子
形状），未知插件主题从 accent 派生确定性的场景；所有背景/粒子由宿主组件渲染，
主题只提供颜色令牌。自适应只生成宿主角色色，不改变插件主题格式。

## 决策

### 1. Noctis 视觉语言

- **主色调**：深色主题使用近黑中性底 + 微色温面板；亮色主题使用暖纸/雾白底，
  不再使用大面积高亮白玻璃。
- **Chrome**：底部导航、账号/启动 dock 和窗口控制始终是深墨色板，为界面提供
  稳定锚点；亮色主题也不把 dock 变成白色。
- **卡片**：统一 14px 四角圆角、细描边、低对比阴影；只有启动
  面板使用深墨底 + accent 主按钮作为唯一大焦点。主面板/卡片透明度与卡片模糊度
  由设置页的 `--rmccc-panel-alpha` / `--rmccc-card-alpha` / `--rmccc-card-blur`
  控制；壁纸激活时主页卡片模糊度自动取设置值的约 55%。
- **强调色角色**：
  - `accent`：主操作、当前导航、键盘焦点、单个关键数字；
  - `secondary`：氛围光、启动面板的极少量渐变；
  - `tertiary`：粒子/装饰微光，不进入正文；
  - danger/success/warning 仍由宿主固定，不受主题控制。
- **动效**：页面 220ms 淡入上移；卡片只改变边框/阴影，不做大幅悬浮；所有动画
  受 `data-motion` 与 `prefers-reduced-motion` 双重闸门。

实现集中在 `src/styles/theme-core.css`、`ambient-stage.css`、`home-stage.css`；
旧的 `visual-system.css` / `soft-dashboard.css` 删除，不再维护两套覆盖层。

### 2. 主题氛围场景（宿主派生）

`src/theme/presets.ts` 为 8 个内置主题提供：

- `atmosphere`：noctis / verdant / abyss / ember / rose / parchment / daybreak；
- `primary / secondary / tertiary`（primary 以主题 `accent` 为准）；
- `particle`：mote / leaf / bubble / spark / petal / dust / shard；
- 粒子数量、速度和透明度。

未知插件主题不报错：从 `accent` 的 HSL 色相派生 secondary/tertiary，使用 `custom`
氛围和 mote 粒子。所有值都是有界的颜色/数字，不执行主题提供的内容。

`ThemeScene` 通过 `<html data-atmosphere>`、`data-theme-key` 和 `--scene-*`
变量发布；`AmbientStage` 只消费这些已校验变量。

### 3. 默认动态背景

`AmbientStage` 在以下情况渲染：

- 用户未启用图片壁纸；且
- 用户未启用视频壁纸（或动态效果关闭，视频层被隐藏）；且
- 当前主题没有 app/both 级 v2 背景媒体。

它由六层组成：主题色 wash、三团缓动 veil、地平线圆弧、透视网格、主题粒子、
暗角/颗粒。没有图片资源、canvas 或网络请求；隐藏时不创建 DOM。用户壁纸存在时
不渲染；v2 主题背景媒体存在时让位给主题媒体，不重复解码。

### 4. 自适应主题 v2

**采样范围**

- 图片：整图 + 中心主体裁切 + 左上/右下细节裁切，共 4 个 112×112 区域；
- 视频：隐藏的静音 clone 在时间轴 8% / 30% / 52% / 74% / 90% 取 5 帧，再加可见
  元素的当前帧；seek 有 1.8s 超时和 `requestVideoFrameCallback` 硬超时兜底；
- 主题图片/视频：同一策略；
- 安全边界：所有媒体读取在 WebView canvas 内完成，失败/跨域污染时退回当前帧，
  不把像素发送到后端或网络。

**代表性选择**

- 多帧按 frame weight 合并后做 OKLab 加权 k-means（k=6）；
- 权重 = 覆盖度 × 色度 × 中心偏置 × 明度中间偏好；
- 角色选择按 `coverage^0.72 × (0.42 + chroma) × 明度平衡` 评分；
- primary 尽可能选有色度的代表色，secondary/tertiary 要求与已选角色色相相隔
  ≥30°；不足时按固定角度派生，保证三色角色始终存在；
- accent 色度在有界范围内轻微提升，避免“缩略图平均值”变成灰浊色。

**可读性与模式**

- 生成 dark 或 light 两套 surface stack，跟随当前主题明暗；
- 所有关键文本/背景对在 OKLCH 中调整 L，直到 ≥ WCAG 4.5:1；
- `checks` 覆盖正文、muted、accent、dock、launch、active nav；
- 版本化存储键 `rmccc.adaptivePalette.v2`；v1 结果视为无效，重新采样。

**UI 反馈**

Appearance 的自适应卡展示代表色、来源、采样帧/像素数、明暗模式和置信度；
采样失败显示 fallback 并允许重新采样。恢复主题时重新应用 8-token 主题并清空
所有 `--adaptive-*` 变量。

## 后果

### 正面

- 没有壁纸时也有主题化、可关闭的动态背景与粒子，不再是一张静态底图。
- 一屏只有明确的 accent 焦点；层级靠表面、边框和字号，不靠满屏发光。
- 自适应在深色/浅色主题下都可读，视频取色覆盖整段而非单帧。
- 公共主题 schema、插件 v2 皮肤、安全色和视频壁纸链路不变。

### 代价 / 限制

- 每个内置主题增加 20–40 个纯 CSS 粒子节点；关闭动态效果后只保留静态渐变。
- 自适应视频取样会短暂创建一个隐藏 video 并 seek 5 次；无法读取时退回当前帧。
- 代表色是审美选择而非数学唯一解；置信度只反映采样覆盖，不保证用户主观喜欢。
- Windows/Linux 的色彩与 WebView `requestVideoFrameCallback` 行为仍需平台实测；
  本机只验证 macOS。

## 迁移影响

- 删除 `src/styles/visual-system.css` 与 `src/styles/soft-dashboard.css`；新文件在
  `main.tsx` 中按 `app → theme-core → theme-skin → ambient → home → instance`
  顺序加载。
- 主题键、`UserSettings.theme`、插件主题 JSON、v2 皮肤变量不变。
- 内置主题的 8 个颜色令牌更新为 Noctis 调色板；键名不变，已有选择继续有效。
- 自适应偏好键不变，调色板键升级到 v2；旧 v1 数据自动失效并重采。

## 验证方式

- `pnpm typecheck`、`pnpm test`：`presets.test.ts` 覆盖 8 个预设、插件主题派生和
  system fallback；`adaptivePalette.test.ts` 覆盖多帧代表色、深浅两套对比度、
  accent fallback 与 panel/ink/dock 一致性。
- 实机：macOS 1280×760，分别验证内置 dark / light 无壁纸氛围、视频壁纸、图片壁纸、
  自适应启用/关闭、动态效果关闭、全设置页/实例页/下载页。
- 视频日志应出现 `frames=6 samples=... mode=... confidence=...`，且
  `readable=true`。
- 体积门禁和 `cargo test --workspace` 按 AGENTS.md 执行。

## 状态变更记录

- 2026-10-31：accepted；Noctis 视觉层、AmbientStage、主题场景和自适应 v2 落地。
