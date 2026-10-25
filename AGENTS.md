# RMCCC — AI Agent 协作规范

> 本文件是 AI 助手在本项目中工作时的顶层指引。详细的编码规范见 `docs/AI_EDITING_RULES.md`。

## 项目概述

RMCCC（Rust Minecraft Core Client）是一个纯 Rust 实现的 Minecraft 启动器，使用 Tauri 2.x + SolidJS 构建前端。目标：跨平台、< 20MB、可扩展。

## 核心约束

1. **设计文档优先**：所有架构决策记录在 `docs/ARCHITECTURE.md`，开发顺序见 `docs/DEVELOPMENT_PLAN.md`。代码实现必须与设计文档一致。如有冲突，以设计文档为准；若需变更设计，先更新文档再改代码。
2. **Phase 顺序执行**：按 `DEVELOPMENT_PLAN.md` 中的 Phase 1→5 顺序开发。每个 Phase 完成后需通过验收标准才能进入下一个 Phase。
3. **体积红线**：最终二进制（**strip 后的主可执行文件**）< **20 MiB**（1 MiB = 1,048,576 B），见 `docs/adr/ADR-011`（2026-10-24 由 10 MiB 放宽）。strip 失效时体积门禁判为 release-blocked，不得用未剥离数字宣称通过。添加新依赖前必须评估体积影响（`cargo bloat`）。
4. **编码规范**：详见 `docs/AI_EDITING_RULES.md`。

## 关键技术决策（已确定）

| 决策 | 选择 | 理由 |
|------|------|------|
| 原生 Shell | Tauri 2.x | 系统 WebView，~4-5MB |
| 前端框架 | SolidJS | ~8KB gzipped，无虚拟 DOM |
| 样式方案 | Tailwind CSS v4 | Utility-first，tree-shaking |
| 类型桥接 | ts-rs | Rust struct → TypeScript interface 自动生成 |
| 认证加密 | AES-256-GCM + 本地 key file | 轻量，无系统密钥链依赖 |
| 日志 | tracing + tracing-appender | Phase 1 即接入，结构化日志 |
| 进程参数 | Response file | 防止 `ps aux` 泄露 token |
| Profile 存储 | JSON 文件 | 简单、可调试，后续可迁移 SQLite |

## 目录结构约定

```
RMCL/
├── AGENTS.md              ← 你正在读的文件
├── docs/
│   ├── ARCHITECTURE.md            # 完整架构设计（最高权威）
│   ├── AI_AND_MCP_ARCHITECTURE.md # AI 服务 + 外部 Agent 控制（ADR-013，S1–S7 切片与进度）
│   ├── AI_EDITING_RULES.md        # AI 编码规范
│   ├── DEVELOPMENT_PLAN.md        # 开发路线图（Phase 1–5）
│   ├── PROJECT_STATUS.md          # 轮次变更记录
│   ├── RMCCC_PLUGIN_PLATFORM_ARCHITECTURE_GUIDE.md  # 插件平台目标架构（含 §1.4 当前 macOS-only 约束、§5.7 平台适配层）
│   ├── README.md                  # docs 分类索引
│   ├── adr/                       # 架构决策记录（ADR-002–007、009–011、013 accepted；001/012 proposed）
│   ├── audits/                    # 全量审计 + 增量台账（AUDIT_2026-10-20 / 10-23，agent 部分已标注 ADR-013 取代）
│   ├── guides/                    # 运维手册 / 插件编写指南
│   ├── roadmap/                   # 细化执行计划（Phase 0–7 + 平台适配）
│   ├── ui-design/                 # 前端重设计（15 份，前端重构依据）
│   └── archive/                   # 历史快照（仅问题溯源）
├── schemas/                # 公共协议 schema（manifest / dashboard-layout / plugin-ui-bridge）
├── crates/                # Rust workspace crates（含 rmccc-platform / rmccc-plugin / rmccc-dashboard / rmccc-logic / rmccc-llm / rmccc-agent[ADR-013 瘦身]）
├── examples/plugins/      # 插件样例（theme-midnight / server-dashboard：隔离 UI 卡片）
├── plugin-sdk/            # 第三方插件 SDK（plugin-api / plugin-ui / 模板 / 校验打包工具）
├── src-tauri/             # Tauri 应用层（IPC 命令）
├── src-logic-host/        # 逻辑插件 helper 进程（唯一链接 wasmi 的二进制，ADR-007）
├── poc/                   # 一次性 PoC（非 workspace 成员，如 ADR-007 的运行时测量）
├── src/                   # SolidJS 前端
└── assets/                # 静态资源
```

## Session Startup

每个新 session 开始时：
1. 读取 `docs/DEVELOPMENT_PLAN.md` 确认当前 Phase 和进度
2. 读取 `docs/ARCHITECTURE.md` 中与当前任务相关的章节
3. 读取 `docs/roadmap/README.md` 与 `00_ROADMAP_OVERVIEW.md`，确认当前细化阶段
4. 若涉及插件/平台适配，读 `docs/RMCCC_PLUGIN_PLATFORM_ARCHITECTURE_GUIDE.md`（注意 §1.4 当前 macOS-only 约束、§5.7 平台适配层归属规则）；若涉及 AI/MCP，读 `docs/AI_AND_MCP_ARCHITECTURE.md`（ADR-013：内部轻量 AI + 外部 Agent 经 MCP，**不做内建 Agent**）
5. 若做前端工作，读 `docs/ui-design/`（前端重构的唯一设计依据）
6. 遵循 `docs/AI_EDITING_RULES.md` 中的所有规范

> **当前阶段约束（服从插件平台指南 §1.4）**：本机仅 macOS Apple Silicon 开发；不在本机做跨平台交叉编译或 Windows/Linux 实测。架构须保留全平台能力：平台差异集中在适配层（`docs/roadmap/PLATFORM_ADAPTATION.md`），领域核心只定义 trait/DTO，不内联 `#[cfg]` 执行分支。非 macOS 的"必须验证"项一律记 deferred / release-blocked。
