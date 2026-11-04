# 17. 当前外壳：左侧浮动 Dock + 空白首页

> 状态：**已实施**（2026-11-04，分支 `ui/home-dock-concept`）
> 适用代码：`src/App.tsx`、`src/components/LeftDock.tsx`、`src/components/WindowControls.tsx`、
> `src/components/LaunchDock.tsx`、`src/pages/Home.tsx`、`src/styles/left-dock.css`、`src/styles/home-stage.css`
>
> 本文补记 card 首页下线后的实际外壳。`00_OVERVIEW.md`–`14_CARD_SYSTEM.md` 描述的是上一代
> 信息架构；与本文冲突时以本文与代码为准。

## 1. 壳层结构

```
.app-root (18px 圆角，隐藏窗口边框)
├─ BackdropLayer        唯一背景层：user image > user video > theme > ambient
├─ DecorLayer           主题装饰（effects）
├─ .window-drag-strip   顶部 16px 拖动区（data-tauri-drag-region="deep"）
├─ LeftDock             左侧 64px 浮动 Dock：账户 / 5 个导航 / 任务中心
├─ LaunchDock           仅首页：右下角启动胶囊（实例切换 + 启动/停止）
├─ WindowControls       右上角：最小化 / 最大化 / 关闭（30×30）
├─ .app-body            右侧内容区（除首页外）
└─ 全局浮层              任务中心、日志抽屉、启动气泡、Toast、确认/输入框
```

- 首页（`/`）是**空白页**：背景铺满，主内容区不绘制卡片；启动入口只有右下角胶囊。
- 其余页面占满 Dock 右侧区域；底部导航不存在，导航只在左 Dock。
- 窗口 `decorations: false` + `transparent: true`，圆角、边框、阴影全部由 CSS 绘制；
  最大化时通过 `html[data-window-maximized]` 去掉圆角。

## 2. 路由

| 路径 | 页面 |
|---|---|
| `/` | 空白首页 + 启动胶囊 |
| `/instances` | 实例列表 |
| `/instances/:id` | 实例详情（hash tabs） |
| `/download` | 下载中心 |
| `/accounts` | 账户 |
| `/plugins` | 插件管理 |
| `/settings` | 设置（hash 分区，含 `#ai`） |
| `/login` → `/accounts`、`/mods` → `/download#mods`、`/agent` → `/settings#ai` | 兼容重定向 |

## 3. 渲染档（RenderTier）

`src/render/tier-decision.ts` 的单一决策：

| 条件 | 档位 |
|---|---|
| 用户关动效 / 系统 `prefers-reduced-motion` / GPU=low | `static`（无动效、不透明表面） |
| 视频背景 | `acrylic`（不实时采样背景） |
| macOS / 未知平台 | `acrylic` |
| Windows + **独显**（GPU=high）+ ambient/image | `full`（真实 `backdrop-filter`） |
| 其余 | `acrylic` |

GPU 探测把 SwiftShader/llvmpipe/software 判为 `low`，NVIDIA/AMD 独显/Intel Arc 判为 `high`，
集显保持 `standard`。**集显 Windows 不默认进入 `full`**：全仓 70 处 `backdrop-filter` 在
核显上尚未实测，先以 `acrylic` 保守兜底。

## 4. Windows 约束（必须满足）

1. **WebView2 ≥ Chromium 111**：CSS 依赖 `color-mix()`（111+）与 `:has()`（105+）。
   `public/boot-guard.js` 在 bundle 之前检查 UA，版本过低时显示升级面板并阻止 UI 被误判为正常。
2. **CSP 已启用**：主窗口只允许 self + `ipc:`/`http://ipc.localhost` + `asset:`/`http://asset.localhost`
   + `rmcccplugin:`/`http://rmcccplugin.localhost`；`script-src 'self'`，因此 boot guard 必须是外部文件。
3. **插件 iframe URL 分平台**：WebView2 用 `http://rmcccplugin.localhost/<id>/ui/...`，
   macOS 用 `rmcccplugin://<id>/ui/...`（`src/plugin-ui/PluginFrame.tsx`）。
4. **单实例**：`tauri-plugin-single-instance` 在第一个插件位注册，二次启动只聚焦已有窗口。
5. **无边框窗口**：Windows 11 由 DWM 关闭系统圆角/1px 边框（`main.rs::apply_windows_window_chrome`）；
   Windows 10 调用失败仅记录 debug。

## 5. 已知缺口

- 自定义最大化按钮不提供 Windows 11 Snap Layouts；拖动区仅顶部 16px，无双击最大化。
- 窗口位置/尺寸不持久化，每次启动居中（仅做多屏可达性纠偏）。
- 插件 UI 目前是**最小骨架**（sandbox iframe + 平台 URL），host 桥协议留待新架构设计。
- 视觉/性能仍待 macOS 实机与 Windows 实机验收；Windows 项按 `AUDIT_2026-11-01_WINDOWS.md` 挂账。
