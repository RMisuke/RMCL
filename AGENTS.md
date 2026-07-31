# RMCCC — AI Agent 协作规范

> 本文件是 AI 助手在本项目中工作时的顶层指引。详细的编码规范见 `docs/AI_EDITING_RULES.md`。

## 项目概述

RMCCC（Rust Minecraft Core Client）是一个纯 Rust 实现的 Minecraft 启动器，使用 Tauri 2.x + SolidJS 构建前端。目标：跨平台、< 10MB、可扩展。

## 核心约束

1. **设计文档优先**：所有架构决策记录在 `docs/ARCHITECTURE.md`，开发顺序见 `docs/DEVELOPMENT_PLAN.md`。代码实现必须与设计文档一致。如有冲突，以设计文档为准；若需变更设计，先更新文档再改代码。
2. **Phase 顺序执行**：按 `DEVELOPMENT_PLAN.md` 中的 Phase 1→5 顺序开发。每个 Phase 完成后需通过验收标准才能进入下一个 Phase。
3. **体积红线**：最终二进制 < 10MB。添加新依赖前必须评估体积影响（`cargo bloat`）。
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
│   ├── ARCHITECTURE.md    # 完整架构设计
│   ├── AI_EDITING_RULES.md # AI 编码规范
│   └── DEVELOPMENT_PLAN.md # 开发路线图
├── crates/                # Rust workspace crates
├── src-tauri/             # Tauri 应用层（IPC 命令）
├── src/                   # SolidJS 前端
└── assets/                # 静态资源
```

## Session Startup

每个新 session 开始时：
1. 读取 `docs/DEVELOPMENT_PLAN.md` 确认当前 Phase 和进度
2. 读取 `docs/ARCHITECTURE.md` 中与当前任务相关的章节
3. 遵循 `docs/AI_EDITING_RULES.md` 中的所有规范
