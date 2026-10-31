# 13 · 注意事项（红线、陷阱与 QA）

> 实施者必读。违反任何一条「红线」= 不可合并。

## 1. 红线

### 体积红线（ADR-011：strip 后主可执行 < 20 MiB）

- 前端影响的是 **JS bundle**（现状 182 KB），不直接进二进制，但 WebView 内存与启动时间
  受影响。本设计预算：**增量 ≤ +150 KB**：
  - 图标：内联 SVG 子集 ~90 枚（≤ 90 KB），**禁止**图标字体/图标 CDN/整包 Material Symbols；
  - 组件库 + stores：预计 ~40 KB；
  - 页面代码：路由级 lazy import（`lazy(() => import(...))`），首屏只载 Shell + 主页。
- 每阶段 `pnpm build` 记录 bundle 数字写入 PR 描述（项目惯例，参照 Phase 5 记录法）。

### 安全红线

1. **token/key 永不渲染**：`AuthSession.access_token/refresh_token`、Agent api_key
   不进 DOM/日志/toast；账号页验收含 DOM 扫描断言（06 §10）。
2. **插件 iframe 不可信**：只经 `plugin_ui_invoke` 桥；宿主安全色
   （danger/success/warning）不被主题覆盖；插件卡片 error boundary 必须生效（02 §3.5）。
3. **外链只走 `open_external_url`**（后端限 http/https）；「打开本地目录」在 P1 落地前
   不得用 shell 逃逸的替代方案，只显示路径+复制。
4. **外部操作确认不可绕过**：破坏性 MCP 工具调用必须走 GUI 确认卡
   （`mcp_confirmation_resolve`）；MCP 服务默认关闭、仅回环监听、Bearer 令牌（ADR-013）。
5. **生产代码禁 unwrap/panic**（后端红线，前端对应：禁止裸 `as` 类型断言处理 IPC 数据、
   禁止 `!` 非空断言——一律走 `RmContentState` 错误态）。
6. **AI 数据外发**：AI 密钥只入 vault（service=`llm`）且 UI 永不回显；把用户数据
   （游戏日志等）发送至 AI 端点前必须 redact + 首次外发确认并标注目标 host。

### 文档红线

- UI 结构性变更先改本目录文档再改代码（AGENTS.md「设计文档优先」）；
  新增 IPC 后同步 `11_IPC_COVERAGE`。

## 2. 类型与命名陷阱（踩过的坑）

1. **返回值大小写按域分裂**（11 §4 速查表）：auth/profile/… 返回 snake_case，
   dashboard/plugin（及迁移期 agent、设计新增 ai/mcp，11 §1.5）返回 camelCase。
   禁止凭感觉写 `.instance_id` vs `.instanceId`——
   一律以 `src/ipc/types.ts`（ts-rs 生成）为准。
2. 已知错误（实施时修）：`instances.refresh/repair` 返回类型（11 §3）；
   `AgentRunOutput` 镜像错误随 `agent_*` 命令移除而消解（迁移期若旧页面仍运行需先修）。
3. **id 双语义**：`instance_paths/check_completion/repair` 收 profile id；
   `instance_rename/duplicate/remove` 收版本 id。版本仓库页（08）与实例页（04）各自锁定语义。
4. modloader loader_type 走无下划线 id（`neoforge`…），与 Rust 枚举名不同。
5. Tauri 入参 camelCase→snake_case 自动转换仅对**命名参数**生效；`plugin_ui_invoke` 的
   `request` 是原始 JSON 字符串（≤64 KiB），内部字段名以桥 schema 为准。

## 2.5 主题 v3 / Noctis（2026-10-31，ADR-021）

- **唯一视觉层**：`theme-core.css`（token/控件/页面）、`ambient-stage.css`（默认动态
  背景）、`home-stage.css`（主页/dock）替换旧 `visual-system.css` 与
  `soft-dashboard.css`；不要再往 `app.css` 追加视觉覆盖。
- **强调色角色**：accent 只做“主操作/当前导航/焦点/一个关键数字”；secondary 只进
  启动面板氛围与背景；tertiary 只做粒子。新增页面禁止把 accent 同时用于卡片描边、
  普通 hover、图标和进度条。
- **默认动态背景**：`AmbientStage` 只在没有用户图片/视频壁纸且没有 v2 app 背景时
  创建；它是纯 CSS、aria-hidden、pointer-events:none，动画受 `data-motion` 与
  `prefers-reduced-motion` 双闸门。关闭动效后保留静态渐变和少量粒子剪影。
- **场景派生**：`src/theme/presets.ts` 为内置主题提供 atmosphere/三色/粒子；
  插件主题按 accent HSL 派生，不新增主题 schema。未知主题不得抛错。
- **自适应 v2**：图片 4 区域 / 视频 5 帧 + 当前帧，k=6 后按覆盖度×色度×中心
  偏置选代表色；dark/light 跟随当前主题。调色板版本 2，旧 v1 数据失效。
- **验收陷阱**：检查“无壁纸 + 深色/浅色 + 动效关”三态；检查视频性能模式只降低
  模糊、不隐藏内容；检查关闭自适应后 `--adaptive-*` 变量与 `data-adaptive` 均清除。

## 3. 事件与性能

- `listen()` 只在 stores 注册一次；页面销毁不留订阅（SolidJS `onCleanup`）。
- `task://updated` 已按 100ms/任务节流，前端按 task id 合并渲染；
  任务中心列表行数多时对 `current_file` 文本做 `memo`。
- 虚拟滚动用于：版本列表（700+）、Mod 搜索结果（分页 20–100/页其实不需要，
  但保留组件位）、Modrinth 详情版本分组、版本仓库表。
- iframe 卡片懒加载（IntersectionObserver）；编辑模式给 iframe 加遮罩避免拖拽误触。
- 日志环形缓冲 1000 行/实例；退出后清理（内存红线意识）。
- 图片（Mod 图标）：`loading="lazy"` + 缺省字母占位（不请求占位服务，离线可用）。

### Local image wallpaper (2026-10-31, ADR-020 addendum)

- **Placement**: the still-image card sits directly above the dynamic-video card
  in Settings → Appearance. The image layer is rendered after the video layer, so
  it is the top wallpaper layer (below every foreground surface).
- **Upload contract**: the native picker accepts PNG / JPEG / WebP / GIF / BMP /
  AVIF up to 32 MiB. The backend canonicalizes the file, checks it is a regular
  file, and grants asset-protocol access to exactly that path. The file is read in
  place; it is never copied, re-encoded or uploaded.
- **Lifecycle**: one `background_image_prepare` call per path (bootstrap and
  selection) grants access; the WebView owns decode and `object-fit: cover`
  scaling. No ImageBitmap/JPEG cache is kept in the frontend.
- **Layering and exclusivity**: image and video are mutually exclusive in the UI.
  Enabling the image turns the video off (and pauses its layer); enabling or
  picking a video turns the image off. The image layer still paints above the
  video layer so a stale preference can never hide the selected image.
- **Adaptive palette**: user image is sampled before user video and theme media,
  so the shell palette follows the visible top layer.
- **Static permanence**: a still image ignores the motion-effects switch and does
  not trigger `data-video-wallpaper` performance mode; normal glass remains.

### Local video wallpaper (2026-10-31, ADR-020)

- **GPU path**: the system WebView owns decode and composition. Keep the video in
  its own composited layer (`translate3d(0,0,0)` / `will-change`), with no
  per-frame scale or filter. macOS uses VideoToolbox; Windows uses WebView2 and
  Media Foundation/D3D11 when the codec, GPU and driver support it.
- **Native resolution**: H.264 within 4096×2304 and HEVC play directly. Oversized
  H.264 is converted once to native-resolution HEVC, never reduced to 1080p.
  Codec, exact dimensions, exact rational frame rate and frame count are checked;
  duration drift is limited to 200 ms for container rounding.
- **Platform boundary**: shared probing/decision/cache/validation lives in
  `rmccc-platform::video`. macOS uses `avconvert PresetHEVCHighestQuality`;
  Windows uses hardware-accelerated WinRT `MediaTranscoder`. Neither UI nor IPC
  contains platform branches.
- **Failure contract**: preparation or decode failure is visible in Settings.
  Never silently fall back to an oversized H.264 source that may use software
  decoding. Windows HEVC codec/GPU availability is a real prerequisite, not an
  assumed guarantee.
- **Simple lifecycle**: `main.tsx` starts one prepare-and-grant IPC. One persistent
  muted/looping/inline video uses the returned URL. Route changes never touch it.
  Only `ended` and visibility recovery remain; no watchdog, route nudge, forced
  `load()`, proxy-upgrade swap or poster/IndexedDB frame cache.
- **Glass fallback**: `<html data-video-wallpaper="on">` removes the full-page
  backdrop filter and caps card/titlebar/dock blur at 8–12 px. Normal glass is
  restored when wallpaper or motion is disabled.

## 4. 平台注意（当前 macOS-only，保留跨平台能力）

- TitleBar 自定义实现保留（红绿灯偏移已在）；新 Rail/PageHeader 不引入平台 `#[cfg]`
  式的前端分支（用 CSS 环境变量 `env(titlebar-area-*)` 预留）。
- 字体栈含中文回退（PingFang SC / Microsoft YaHei / Noto Sans CJK），
  12px 以下字号避免中文（可读性），最小 12px。
- 路径显示统一用后端返回的原样字符串（macOS `/`；未来 Windows `\` 无需前端处理），
  长路径中段省略（`/Volumes/…/mods`）+ 完整路径进 tooltip/复制。
- 非 macOS 平台的验证项全部记 deferred / release-blocked（项目惯例）。

## 5. i18n 规范

- key 扁平点号 + 页面命名空间（各页「i18n key 提案」是种子清单，实施可增不可乱命名）；
- en.ts 为参考语言包（`as const`），zh-CN 缺键/多键必须 typecheck 报错——提 PR 前跑
  `pnpm typecheck`；
- 插值 `{name}`；不裸拼字符串；错误文案含「原因 + 下一步」（01 §7）。

## 6. 可访问性 checklist

- [ ] 全部交互件可 Tab；焦点环可见（01 §9）；
- [ ] 图标按钮/行操作按钮 `aria-label`（i18n）；
- [ ] Toast `aria-live="polite"`；危险确认 `role="alertdialog"`；
- [ ] 对比度抽查：muted 文字、accent 上的文字、danger 按钮文字 ≥ 4.5:1；
- [ ] `prefers-reduced-motion` 全局降级（01 §8）。

## 7. 视觉/交互 QA 清单（阶段 F 全量走查）

| 维度 | 取值 |
|---|---|
| 分辨率 | 1280×720（最小）、1440×900、2560×1440；窗口缩放 100%/150% |
| 主题 | builtin:dark、builtin:light、跟随系统切换、theme-midnight 插件主题 |
| 语言 | zh-CN、en（抽查长文案溢出：德语类长词不做，但英文长错误文案要截断合理） |
| Dashboard 断点 | wide/medium/compact 三档 + 编辑模式操作 |
| 状态 | 每列表页三态（骨架/错误/空）截图；运行中/下载中徽标 |
| 外部控制 | MCP 开/关两态、Agent 归因徽标、确认卡倒计时、日志 AI 分析流式（07 §11） |
| 键盘 | 全程 Tab 能走通：Rail→页面→弹窗→关闭 |

## 8. 测试策略

- 单测（vitest，可选但推荐）：stores 的事件归约逻辑（tasksStore 的终态入 recent、
  launchStore 缓冲清理）抽纯函数测；
- 契约：`ipc_tests.rs` 已保证命令注册；前端对 api.ts 的 casing 断言可以做一个
  「全 wrapper 调用 mock invoke 参数快照」测试（防止再犯 AgentRunOutput 类错误）；
- 人工验收脚本：12 §5 的五个用户旅程 + 各页「验收要点」小节。

## 9. 本目录文档维护责任

| 变更类型 | 必须同步 |
|---|---|
| 新增/删除 IPC | `11_IPC_COVERAGE` + `src-tauri/src/handlers.rs`（后端 PR） |
| 页面结构/tab 增删 | 对应页面文档 + `00_OVERVIEW` §2 页面树 + README 索引 |
| token/组件规范变化 | `01_STYLE_GUIDE` |
| 后端配套项状态变化（P1–P11） | `00_OVERVIEW` §9 表打勾 + 引用页降级形态说明更新 |
