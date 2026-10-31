# RMCCC 前端 UI 设计文档（推倒重来版）

> 版本：v1.0（2026-10-23）
> 状态：**设计稿，待实施**。本目录是前端界面重构的唯一权威设计来源。

## 本目录是什么

对 RMCCC 现有 GUI（Tauri 2 + SolidJS）的**完全重设计**。现有界面是各 Phase 顺手堆出来的临时品
（主页拥挤、无全局导航、Mod 浏览器半成品、15 条 IPC 命令无 UI 入口），本设计推倒重来，
以「干净简洁、主页卡片化、一级只放高频必要项、高级参数下沉」为核心原则，
参考 HMCL 的成熟设计层级，并把后端全部 86 条 IPC 命令与 10 个事件安排到明确的界面位置。

设计输入：

- RMCCC 后端全量能力盘点（86 IPC 命令、10 事件、Dashboard 布局引擎、插件 UI 桥、主题系统、
  logic 插件，以及后端缺口清单）；
- AI 方向按 **ADR-013** 重构为「轻干预 · 轻 AI」：不内建 Agent 平台，
  内部保留轻量 OpenAI 兼容 AI 服务、外部 AI 工具经 **MCP** 控制启动器
  （`docs/AI_AND_MCP_ARCHITECTURE.md`），本目录 `07_AI_AND_MCP.md` 为其 UI 设计；
- HMCL-main 代码库的设计架构通读（页面树、导航模型、渐进披露实践、视觉 token 体系）；
- 既有权威文档约束：`docs/ARCHITECTURE.md` §9/§10、
  `docs/RMCCC_PLUGIN_PLATFORM_ARCHITECTURE_GUIDE.md` §9（Dashboard 是布局引擎）、ADR-005/006/007/009/010/013。

## 文档索引与阅读顺序

| 顺序 | 文件 | 内容 | 读者 |
|---|---|---|---|
| 1 | [00_OVERVIEW.md](00_OVERVIEW.md) | 总设计架构：目标原则、信息架构总图、Shell、导航模型、路由表、状态层、后端配套清单 | 所有人先读 |
| 2 | [01_STYLE_GUIDE.md](01_STYLE_GUIDE.md) | 基本设计风格：设计令牌、主题、布局栅格、组件库、图标、动效、i18n、CSS 架构 | 所有人 |
| 3 | [02_HOME_DASHBOARD.md](02_HOME_DASHBOARD.md) | 主页 Dashboard：卡片网格、内置卡片 v2、编辑模式、首启引导 | 实施者 |
| 4 | [03_INSTANCE_LIST.md](03_INSTANCE_LIST.md) | 实例列表页 | 实施者 |
| 5 | [04_INSTANCE_DETAIL.md](04_INSTANCE_DETAIL.md) | 实例详情页（概览 / Mod 管理 / Mod 加载器 / 设置 / 维护） | 实施者 |
| 6 | [05_DOWNLOAD_CENTER.md](05_DOWNLOAD_CENTER.md) | 下载中心：安装游戏向导（本体 + Modloader）、Mod 仓库、整合包占位 | 实施者 |
| 7 | [06_ACCOUNTS.md](06_ACCOUNTS.md) | 账号页（Microsoft 设备码 / 离线 / 凭据保险库） | 实施者 |
| 8 | [07_AI_AND_MCP.md](07_AI_AND_MCP.md) | AI 服务与外部 Agent 控制：设置分区（AI 端口/MCP）、任务归因与确认卡、日志一键 AI 分析 | 实施者 |
| 9 | [08_SETTINGS.md](08_SETTINGS.md) | 全局设置页（通用 / 游戏默认 / 下载 / 插件 / 高级 / 关于） | 实施者 |
| 10 | [09_PLUGINS.md](09_PLUGINS.md) | 插件管理页（安装、权限、配置、贡献、逻辑模块） | 实施者 |
| 11 | [10_TASKS_AND_FEEDBACK.md](10_TASKS_AND_FEEDBACK.md) | 任务中心与全局反馈（下载/启动/加载器任务、Toast、三态内容） | 实施者 |
| 12 | [11_IPC_COVERAGE.md](11_IPC_COVERAGE.md) | 接口覆盖矩阵：86 命令 + 10 事件 → 页面/组件/触发点 | 验收对照 |
| 13 | [12_SUMMARY.md](12_SUMMARY.md) | 总结与实施路线（阶段划分、迁移策略、验收标准、风险） | 所有人 |
| 14 | [13_NOTES.md](13_NOTES.md) | 注意事项：红线、类型陷阱、性能、平台、可访问性、QA 清单 | 实施者必读 |
| 15 | [14_CARD_SYSTEM.md](14_CARD_SYSTEM.md) | **主页卡片系统 v3**：尺寸档位、布局 v3、默认卡片功能、插件卡片/Host Bridge 契约、验收 | 实施者必读 |

## 使用规则（与项目「设计文档优先」原则一致）

1. **实施前先读**：写任何页面的代码之前，先读 `00_OVERVIEW` 与对应页面文档；
   页面文档没覆盖的细节，先补文档再写代码。
2. **冲突裁决**：本目录与 `docs/ARCHITECTURE.md` §9（前端架构）冲突时，以**更新较新者为准并回写另一方**；
   与代码现状冲突时，以本设计为准（本设计就是用来替换现状的）。
3. **变更流程**：UI 结构性变更（新增/删除页面、导航调整、交互模型变化）必须先改本目录对应文档，
   提交说明中注明「docs(ui-design): …」。
4. **接口对齐**：任何页面设计引用的 IPC 命令名、字段名以 `11_IPC_COVERAGE.md` 与
   `src-tauri/src/handlers.rs` 为准；后端新增命令后须同步更新覆盖矩阵。

## 一图速览

```
RMCCC 窗口
├─ TitleBar（自定义标题栏，保留）
├─ 左侧全局导航 Rail（200px，可收起 64px）
│    主页 │ 实例 │ 下载 │ 账号 │ 插件 │ 设置
│    底部：任务指示器（聚合下载/运行中游戏/MCP 待确认）
└─ 内容区
     /                主页 Dashboard（卡片网格：快速启动/账号/版本速装/下载任务 + 插件卡片）
     /instances       实例列表
     /instances/:id   实例详情（概览│Mod 管理│Mod 加载器│设置│维护）
     /download        下载中心（安装游戏向导│Mod 仓库│整合包·规划中）
     /accounts        账号
     /plugins         插件管理
     /settings        设置（游戏默认│通用│下载│AI 与控制│插件│高级│关于）
```
