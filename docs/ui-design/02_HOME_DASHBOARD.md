# 02 · 主页 Dashboard

> 路由 `/`。主页 = 「打开即玩」的启动台 + 可定制的卡片总览。
> 沿用后端 Dashboard 布局引擎（ADR-005：注册表 + 布局配置 + 受控渲染），重写宿主与内置卡片。

## 1. 设计目标

1. 首屏第一焦点是**快速启动卡**：当前实例名 + 大启动按钮，1 次点击进入游戏。
2. 卡片网格承载总览：账号、下载任务、版本速装、插件卡片，全部可增删拖拽。
3. 未完成引导（无账号/无实例）时，快速启动卡变身为引导卡，把新用户送进正确流程。
4. 任何卡片坏了（插件卡片崩溃/被禁用）不影响主页可用。

## 2. 布局

```
┌──────────────────────────────────────────────────────────────┐
│ PageHeader：主页                            [编辑布局] [⋯]    │
├──────────────────────────────────────────────────────────────┤
│  12 列网格（gap 16 / 行高 120px，后端归一化，断点同现状）        │
│                                                              │
│ ┌────────────────────────────┬──────────────┐ ┌──────────┐ │
│ │  快速启动卡 instance-launcher│  账号卡       │ │ 版本速装卡 │ │
│ │  （默认 8×4）                │ account-card │ │ (12×3)   │ │
│ └────────────────────────────┴──────────────┘ └──────────┘ │
│ ┌────────────────────────────┬────────────────────────────┐ │
│ │  下载任务卡 downloads-card   │  插件卡片（rmcccplugin://） │ │
│ │  （默认 8×3，无任务时 8×1）   │  dashboard-card:*        │ │
│ └────────────────────────────┴────────────────────────────┘ │
└──────────────────────────────────────────────────────────────┘
```

- 网格引擎保持现状能力：`dashboard_get_layout`（进入时拉，后端已归一化）→ 渲染 →
  编辑后 `dashboard_save_layout` 全量提交，`NormalizationResult.corrections` 非空时
  toast「布局已自动修正 N 处」。
- 断点：wide ≥1280（12 列）/ medium ≥768（≤8 列）/ compact（≤4 列），沿用现有实现。

## 3. 内置卡片设计（org.rmccc.core-ui 命名空间）

### 3.1 快速启动卡 `instance-launcher`（默认 8×4，min 4×2）

主页的绝对主角，状态机驱动：

```
[零实例引导态]      无任何 profile：
   大按钮「+ 安装游戏」→ /download#install
   副文案「选择一个 Minecraft 版本，几分钟就好」
[未登录态]          有实例、无会话（auth_get_session = null）：
   主按钮变「添加账号后启动」→ /accounts
   次按钮「以离线身份启动」（离线用户名输入 dialog → auth_offline_login → launch）
[就绪态]（默认）    ┌────────────────────────────────┐
                   │  ▶  启 动            [实例 ▾]   │  ← 40px 大按钮 + 实例切换
                   │  1.20.4 · Fabric 0.15.x         │
                   │────────────────────────────────│
                   │  ‹sodium 实验存档›  3 小时前玩过  │  ← last_played 最近
                   │  ‹纯净生存›        2 天前        │     3 行迷你列表（点击切换）
                   └────────────────────────────────┘
[启动中]           按钮转 progress +「正在准备…（校验文件/补缺）」
                   ensure_launch_ready 自动修复由 launch_start 内部完成，此处只显示进度
[运行中]           按钮变「■ 停止」(launch_kill) + 「查看日志」（打开日志抽屉，见 10）
                   + 运行时长计时（launch://started 起算）
```

- 实例切换 `[实例 ▾]`：菜单列出全部 profile（icon + name + 版本/loader 副标题），
  按 `last_played` 倒序；选中态写 profilesStore（会话内共享，持久化待后端 P5）。
- 迷你列表同时是「最近游玩」信息；点击行 = 切换当前实例。
- 空间不足（卡片被缩到 min 尺寸）时自动省略迷你列表，只留按钮 + 实例名。

### 3.2 账号卡 `account-card`（默认 4×4，min 4×2）

```
[已登录]   ┌────────────────┐
           │  (头像首字母)    │  ← uuid 无法拿皮肤头像（无 MineSkin/材质接口），用
           │  Steve          │     首字母圆形 + provider tag（microsoft/offline）
           │  Microsoft 账号 │
           │  [切换 ▾]       │  ← 菜单：其他账号（auth_switch_account）/
           └────────────────┘     管理账号（→ /accounts）/ 退出登录（auth_logout）
[未登录]   「尚未登录」+ [添加账号] → /accounts
```

### 3.3 版本速装卡 `version-browser` v2（默认 12×3，min 6×2）

从「版本浏览大列表」瘦身成「速装入口」——完整版本列表移入下载中心向导，主页只做捷径：

```
┌───────────────────────────────────────────────────────┐
│ 安装新实例   [1.21.x] [1.21.x-1] [1.20.x] … [全部版本›] │  ← 最新 release 快捷
│ 或粘贴版本号 [________________] [创建→]                 │     芯片点击即进入向导
└───────────────────────────────────────────────────────┘
```

- 快捷芯片取 `version_list` 前若干 release（去重主版本），点击 → `/download#install?version=<id>` 预填。
- 「全部版本›」→ `/download#install`。本卡不再承担安装进度展示（归下载任务卡）。

### 3.4 下载任务卡 `downloads-card`（新增内置，默认 8×3，min 6×2；需后端 P4 注册）

```
[无任务]   「没有进行中的下载」+「安装游戏›」链接（卡片自动塌缩为 1 行高）
[有任务]   每批一行：
           ┌────────────────────────────────────────────┐
           │ 1.20.4 · sodium 实例      ▓▓▓▓▓▓░░░░  62%   │
           │ 1,204/4,023 个文件 · 512.3 MiB/1.8 GiB      │
           │ 正在下载: libraries/…  [⏸][✕]               │
           └────────────────────────────────────────────┘
```

- 数据源：downloadStore（事件驱动，无轮询）；操作 = `download_pause/resume/cancel`。
- 行点击 → 展开任务中心面板（见 10_TASKS_AND_FEEDBACK）。
- 后端 P4 未就绪前的过渡：功能由 PageHeader 临时按钮承担，卡片注册后迁移。

### 3.5 插件卡片（`dashboard-card:*`）

- 渲染走现有 `PluginFrame` iframe 桥（`rmcccplugin://` scheme + postMessage 协议），
  宿主补齐：**error boundary**（卡片组件级，崩溃显示 RmErrorState +「禁用该卡片」快捷键，
  不连坐整个主页）；**权限态**（插件被禁用/能力被撤销 → 显示占位卡「卡片不可用：插件已禁用」，
  保留位置与配置，与插件卸载后保留占位的既有行为一致）。
- iframe 懒加载：不在视口内的插件卡片延迟到 IntersectionObserver 触发再加载（性能）。

## 4. 编辑模式

PageHeader「编辑布局」进入，与现状能力对齐并按 01 规范化：

- 每卡显示拖拽把手 + 宽度循环按钮（1/2/3/4/6/8/12 列）+ 隐藏按钮；
- 网格顶部出现「+ 添加卡片」抽屉：`dashboard_list_widgets` 列表（含插件贡献的 widget，
  显示 displayName + 来源插件 + 默认尺寸），点击追加到网格末尾；
- 工具条：恢复隐藏卡片（列出 hidden 的 widget）/ 补齐缺失 widget / 重置布局（`dashboard_reset_layout`，二次确认）；
- 退出编辑即 `dashboard_save_layout`；corrections 非空 → toast 汇报。
- 编辑模式下插件 iframe 卡片加半透明遮罩（拦截其内部交互，避免拖拽误触）。

## 5. 首启与空态总策略（全局唯一引导逻辑）

```
首次启动（无账号无实例）:
  快速启动卡 = 安装引导（§3.1 零实例态）
  版本速装卡 = 高亮（accent 边框呼吸 1 次）
  账号卡     = 「可稍后添加，离线也能玩」降低压力
装好第一个实例后:
  快速启动卡切就绪态；若无账号，主按钮引导加账号（不阻断离线启动）
```

不弹全屏向导、不弹更新检查弹窗（更新入口在设置页，见 08）。

## 6. 接口与事件映射

| 动作 | 接口/事件 |
|---|---|
| 布局读写 | `dashboard_get_layout` / `save_layout` / `reset_layout` / `list_widgets` |
| 启动/停止 | `launch_start` / `launch_kill`；`launch://started/exited` 更新态 |
| 会话恢复 | `auth_get_session`（App 启动时 authStore） |
| 账号操作 | `auth_switch_account` / `auth_logout` / `auth_offline_login` |
| 实例列表 | `profiles.list`（profilesStore） |
| 版本捷径 | `versions.list` |
| 下载卡 | downloadStore + `downloads.pause/resume/cancel` |

## 7. 渐进披露自查

一级：启动/停止、实例切换、添加账号、快捷版本；
二级：编辑布局（PageHeader 按钮）；
三级：布局自动修正详情（toast → 查看详情 dialog）、重置布局。
主页**不出现**任何 JVM 参数/镜像/高级设置入口。

## 8. i18n key 提案（节选）

`home.launch` / `home.stop` / `home.preparing` / `home.runningFor {min}` /
`home.noInstance.title` / `home.noInstance.cta` / `home.noAccount.cta` /
`home.recent.subtitle` / `home.downloads.title` / `home.downloads.empty` /
`home.editLayout` / `home.addWidget` / `home.layoutCorrected {n}` /
`home.card.disabled {plugin}` …

## 9. 验收要点

- 零实例/未登录/就绪/启动中/运行中 五态截图齐备（1280×720 与 2560×1440）。
- 拖拽 + 宽度调整 + 隐藏 + 恢复 + 重置在 medium/compact 断点下无布局破坏（后端归一化兜底）。
- 插件卡片注入异常脚本（手动测试用 server-dashboard 改坏）时主页不崩溃。
- 快速启动卡从点击到 `launch_start` 发出的路径无多余确认弹窗（除非缺账号）。
