# 14 · 主界面卡片系统 v3

> 状态：2026-10-31 设计并实施。本文定义主页卡片的**唯一尺寸、布局、渲染与插件扩展契约**；
> 02_HOME_DASHBOARD.md 的旧网格/固定宽度方案由本文取代，外观令牌仍遵循 01_STYLE_GUIDE.md。

## 1. 目标与边界

1. **一种卡片**：内置卡片与插件卡片共用同一套尺寸档位、卡片外壳、标题栏、编辑操作和错误隔离。
2. **可预测的布局**：卡片只能落在固定宽度与高度档位上，不允许 1px 级自由拖拽导致参差。
3. **插件可新增卡片，但不能破坏宿主**：插件通过 manifest 声明卡片，宿主统一分配尺寸、权限与生命周期。
4. **默认卡片做实事**：服务器信息查询走真实 SLP 协议；启动状态由真实启动阶段/日志驱动；
   下载卡直接使用统一任务中心的数据与操作。
5. **尺寸档位是协议的一部分**：前端、manifest schema、WidgetDescriptor 和文档使用同一组常量。

不在本轮范围：自由拖拽网格、卡片嵌套、插件自定义卡片外框、Windows 原生窗口探测（deferred）。

## 2. 尺寸系统

### 2.1 宽度档位（12 列）

| 档位 | 列宽 | 用途 | 1280 窗口约宽 |
|---|---:|---|---:|
| S | 3 | 指标、开关、单行摘要 | 293px |
| M | 4 | 紧凑列表、单个状态 | 382px |
| L | 6 | 默认卡片（服务器、下载） | 559px |
| XL | 8 | 需要横向信息的卡片 | 736px |
| Full | 12 | 版本速装、时间线、宽表 | 1216px |

只允许 `3 / 4 / 6 / 8 / 12`。`1/2/5/7/9/10/11` 不接受；插件旧尺寸会被吸附到最近的合法档位。

### 2.2 高度档位（4 档）

CSS 使用 `grid-auto-rows: 92px`、`gap: 14px`，卡片跨行：

| 档位 | 行数 | 实际高度 | 用途 |
|---|---:|---:|---|
| H1 Compact | 1 | 92px | 单行状态、快捷入口 |
| H2 Standard | 2 | 198px | 默认卡片（现有卡片高度） |
| H3 Tall | 3 | 304px | 列表、时间线、公告 |
| H4 Hero | 4 | 410px | 主视觉卡片、复杂插件面板 |

实际高度 = `h × 92 + (h−1) × 14`，保证同一行卡片底边严格对齐。

### 2.3 尺寸约束与吸附

- 内置卡片有 `default / min / max` 三组尺寸。
- 插件在 `contributes.dashboardCards[]` 中声明 `defaultSize`、可选 `minSize`、可选 `maxSize`。
- 宿主把所有尺寸吸附到合法档位：
  - 宽度：在该卡片 `[minW,maxW]` 允许的档位中取与请求值最接近的一档；
  - 高度：在该卡片 `[minH,maxH]` 允许的档位中取与请求值最接近的一档；
  - 无解时回退到包含该值的最小合法档位，再不行回退 3×2。
- 用户在编辑态只能按档位循环调整，不能输入任意像素。

## 3. 布局与持久化

### 3.1 布局模型

```ts
type CardWidth = 3 | 4 | 6 | 8 | 12;
type CardHeight = 1 | 2 | 3 | 4;

interface CardPlacement {
  id: string;          // builtin: server …；插件: plugin:<pluginId>/<cardId>
  w: CardWidth;
  h: CardHeight;
}

interface HomeLayoutV3 {
  schemaVersion: 3;
  cards: CardPlacement[];
  hidden: string[];
}
```

- 存储键：`rmccc.softDashboard.v3`。
- 渲染顺序 = `cards` 数组顺序；CSS Grid 自动换行。
- 删除/隐藏卡片保留在 `hidden`，插件禁用/卸载时保留占位和尺寸，恢复插件后原位回来。
- 从 v2（`rmccc.softDashboard.v2` 的 order/hidden）迁移：
  - 内置卡按新默认尺寸补全；
  - v2 的 `plugin:<pluginId>` 在插件目录加载后映射为该插件第一张卡；
  - 无法识别的 id 放入 `hidden`，不静默丢失。

### 3.2 默认布局

| 卡片 | 默认尺寸 | 默认可见 |
|---|---|---|
| 服务器信息 `server` | L×H2 (6×2) | 是 |
| 下载与任务 `downloads` | L×H2 (6×2) | 是 |
| 版本速装 `versions` | Full×H2 (12×2) | 是 |
| 实例 `instances` | L×H2 (6×2) | 否 |
| 公告 `announcements` | L×H3 (6×3) | 否 |
| 版本对比 `compare` | M×H2 (4×2) | 否 |
| 运行中游戏 `games` | L×H2 (6×2) | 否 |

### 3.3 卡片外壳

所有卡片使用 `.rm-dashboard-card`（视觉上复用 `.soft-card`）：

```
┌────────────────────────────────────────────┐
│ 图标 标题                    [编辑操作]     │  header 40px
├────────────────────────────────────────────┤
│ body（唯一可滚动区域，min-height:0）        │
│                                            │
└────────────────────────────────────────────┘
```

- 卡片本身不滚动；body 在内容超出时 `overflow:auto`，避免撑破网格。
- H1 只显示标题 + 一行摘要；H2 显示默认内容；H3/H4 允许列表/详情展开。
- 编辑态才显示标题栏操作：上移、下移、宽度循环、高度循环、隐藏。
- 插件卡片错误只污染自己的 body（ErrorBoundary + PluginFrame 状态机）。

## 4. 默认卡片功能

### 4.1 服务器信息卡 `server`

**真实查询**，不得再使用 mock：

- 用户输入 `host`、`host:port`、`[IPv6]:port`；端口缺省 25565。
- 点击查询调用 Rust IPC `server_status({ address })`；后端执行 Minecraft Server List Ping：
  1. 解析地址，DNS 解析（不做 SRV，文档明示）；
  2. TCP 连接（4s 超时）；
  3. handshake(nextState=1) + status request；
  4. 解析 VarInt 长度、packet id、JSON；
  5. 统计连接→状态响应耗时作为 latency。
- 返回：MOTD（聊天组件递归展平为纯文本）、online/max、样本玩家（上限 12）、版本名、协议号、
  favicon（仅接受 `data:image/png;base64,...`，上限 128 KiB）、查询时间。
- 卡片行为：
  - 输入地址持久化到 `rmccc.serverQuery.v1`，下次打开自动查询一次；
  - 在线时每 90s 自动刷新（页面隐藏时暂停）；
  - 手动刷新按钮；离线展示后端错误文案 + 重试；
  - H1 只显示在线人数/延迟；H2 显示 MOTD + 元信息；H3+ 显示样本玩家。
- 安全：地址只允许主机名/IP + 端口；拒绝 URL scheme、空白、超长 host；不跟随重定向、不执行服务器返回的 HTML。
- 测试：地址解析、MOTD 展平、本地 mock TCP 服务器端到端 SLP、非法地址。

### 4.2 启动状态（Hero 状态卡）

**唯一进度源：后端 `launch://progress` 事件。**

| percent | stage | 含义 |
|---:|---|---|
| 0–4 | checking | 准备启动，检查实例 |
| 5–34 | repairing | 补齐缺失文件（真实下载进度） |
| 35–54 | checking-java | 检查 Java、解析启动参数 |
| 55–84 | spawning | 生成命令行并拉起进程 |
| 85–99 | waiting-window | 进程已启动，等待游戏窗口 |
| 100 | window-open | **游戏窗口已打开**（唯一 100% 条件） |

- 100% 由游戏 stdout 的窗口就绪标记触发（`Backend library: LWJGL`、`OpenAL initialized`、
  `Sound engine started` 等），不是由 `LAUNCH_STARTED`（进程 spawn）触发。
- 失败：状态卡显示失败 stage + 错误，不显示 100%。
- 进程启动但未检测到窗口：停在 85–99%，文案「等待游戏窗口」；用户可查看日志/停止。
- Hero 状态卡显示环形百分比 + 当前阶段 + 一行消息；启动气泡显示同源百分比和阶段。
- 事件类型：
  ```ts
  interface LaunchProgress {
    profileId: string;
    percent: number;      // 0..100，单调不减（失败除外）
    stage: string;
    message: string;
    windowOpened: boolean;
  }
  ```

### 4.3 下载与任务卡 `downloads`

- 数据源只允许 `tasksStore`（`task://updated` / `task_list`），不得再维护第二套下载状态。
- 卡片头部/底部提供「打开任务中心」按钮，点击 `uiStore.setTaskCenterOpen(true)`。
- 每行显示：标题、当前步骤、总进度、文件/字节、速度、ETA；点击行打开任务中心。
- 操作复用 `tasksStore.pause/resume/cancel`，取消必须二次确认。
- 无活动任务时显示最近完成任务（最多 2 条）和「打开任务中心」；没有历史才显示空态。
- 与任务中心共享同一 TaskSnapshot，不做 batch id 重映射、不合成进度。

### 4.4 其他默认卡

- `versions`：版本速装，Full×H2；保留粘贴版本号入口。
- `instances`：最近实例列表；`games`：运行中游戏；`compare`/`announcements` 保留但默认隐藏。
- 所有默认卡都必须适配 H1–H4；数据不足时优先隐藏次要行，不出现内部滚动条套娃。

## 5. 插件卡片契约

### 5.1 Manifest

```json
{
  "permissions": [
    { "capability": "dashboard.cards" },
    { "capability": "instance.read" },
    { "capability": "tasks.read" },
    { "capability": "launch.read" },
    { "capability": "network.request", "hosts": ["mc.example.com"] }
  ],
  "contributes": {
    "dashboardCards": [{
      "id": "server-status",
      "title": "Server Status",
      "description": "…",
      "defaultSize": { "w": 6, "h": 2 },
      "minSize": { "w": 4, "h": 1 },
      "maxSize": { "w": 8, "h": 3 }
    }]
  }
}
```

- `id` 在插件内唯一；全局卡片实例 id 为 `plugin:<pluginId>/<cardId>`。
- 同插件可以贡献多张卡；旧版 `plugin:<pluginId>` 只作为迁移输入。
- 尺寸必须能吸附到合法档位；`minSize` 不得大于 `defaultSize`，`maxSize` 不得小于 `defaultSize`。
- 没有 `entry.ui` 时：卡片可见但显示「该插件未提供可视化卡片」占位，不伪装成内置数据。

### 5.2 宿主注入的运行时上下文

`init` 消息在现有字段上增加：

```ts
{
  v: 1, type: "init", pluginId, cardId,
  theme,
  card: { title: string; width: CardWidth; height: CardHeight },
  limits
}
```

- 插件不得依赖宿主 DOM、localStorage、Tauri API；只能经 Host Bridge。
- 每个 cardId 独立 bridge；卡片卸载自动 dispose；iframe 未 ready 8s 报错。

### 5.3 Host Bridge 能力表（UI API v1，追加不破坏）

| 方法 | 所需能力 | 返回/能力 |
|---|---|---|
| `host.info` | — | 版本/平台/卡 id |
| `theme.get` | — | 当前主题令牌 |
| `settings.get` | `settings.read` | 公开设置子集 |
| `instances.list` | `instance.read` | 实例概要（不含路径/凭据） |
| `tasks.list` | `tasks.read`（低风险） | 活动 + 最近任务快照（脱敏后的公开字段） |
| `launch.status` | `launch.read`（低风险） | 运行中实例 id + 最近启动阶段 |
| `server.status` | `network.request` + 已声明 host | 真实 SLP 状态（只允许声明域名） |
| `network.request` | `network.request` | 宿主 HTTP 代理，白名单域名 |
| `llm.chat` | `ai.chat` | 宿主 AI 服务 |

- 所有调用每次经 Rust 重新校验插件状态与能力；前端 bridge 的超时只是 UI 保护。
- `tasks.list` 不返回凭据、路径；`currentFile` 截断为文件名。`server.status` 的 host 必须匹配
  `network.request` 声明的 host，且解析到公网地址（与 network.request 同一套 SSRF 过滤）。
- 新增方法必须同步更新：`ui.rs` 方法表、bridge schema、SDK 类型、契约测试、本文档。

### 5.4 插件卡可以“提供”的能力

1. **卡片表面**：隔离 iframe UI；卡片只负责展示与发起请求。
2. **只读上下文**：主题、公开设置、实例概要、任务、启动状态。
3. **受限网络**：通过宿主代理访问声明域名，或通过 `server.status` 查询声明域名的 Minecraft 服务器。
4. **受控动作**：通过既有任务/实例/下载命令执行；高风险能力仍需显式授权。
5. **配置页/主题**：继续使用 `settingsPages` 与 `themes` 贡献项。

插件**不能**：直接访问文件系统、读取账号凭据、注入宿主 CSS/JS、伪造内置卡片、
在卡片内自行启动游戏、绕过任务中心创建不可见下载。

### 5.5 生命周期与失败隔离

- 目录加载 → `dashboard_list_widgets` → 宿主吸附尺寸 → 布局迁移 → 渲染。
- 插件禁用/卸载：卡片保留占位，显示原因；不删除布局。
- iframe 错误、未 ready、bridge 拒绝：只影响该卡 body，可重试。
- 插件升级改变 `defaultSize`：不覆盖用户已保存尺寸，只重新钳制到新的 min/max。

## 6. 验收

- 1280×760 下所有宽度档位同行底边对齐，无横向溢出。
- H1/H2/H3/H4 四档各至少一张卡截图；编辑态宽度/高度循环可达所有合法档位。
- 服务器卡对至少一个真实公网服务器返回 MOTD/人数/延迟；断网/错误地址显示错误而非 mock。
- 启动真实游戏：进度从 0 递增，100% 只在窗口出现后；截图/日志可复核。
- 下载卡与任务中心同一任务 id、同一百分比；暂停/恢复/取消双向一致。
- 插件贡献两张卡时两张都能添加、调整尺寸、独立初始化；禁用插件后占位不崩。
- `pnpm typecheck`、`pnpm test`、`cargo fmt/clippy/test`、`pnpm tauri build --bundles app`。
