# Phase 4 — 隔离 UI 扩展与 SDK

> **对应指南**：§6.2 阶段 B、§9.6、§13，§17 Phase 4。**状态**：pending。
> **前置**：Phase 2 协议 + Phase 3 卡片宿主。**ADR-006**（隔离容器 + 消息桥）须在实现前 accepted。

## 1. 目标

允许第三方开发自己的卡片和页面，但运行在**隔离 UI 容器**（iframe/独立 WebView），通过版本化 Host Bridge 通信。外部样例可独立构建/打包/安装；无法直接调用未授权 Tauri IPC 或宿主文件 API。

## 2. 与当前状态的对应

- 当前所有 UI 在可信主窗口 SolidJS 环境内（审计 §10）；无隔离容器、无 SDK、无打包工具。

## 3. 任务切片分解

### 4.1 ADR-006 隔离容器选型
- iframe vs 独立 WebView 窗口；消息协议（postMessage 或等价）。
- CSP：禁止默认执行远程脚本；插件只用包内静态资源。
- 消息格式：请求 ID/超时/取消/错误/权限拒绝/大小限制。

### 4.2 Host Bridge 协议
- 版本化 `UIExtensionApiVersion`；消息 schema 校验（来源/窗口引用/大小/协议版本）。
- 宿主每次处理请求重新检查 pluginId/权限/作用域（不只信任前端按钮）。
- 插件输出按不可信文本渲染；HTML 须安全处理。

### 4.3 TypeScript SDK
- `plugin-sdk/packages/@rmccc/plugin-api` + `@rmccc/plugin-ui` + `@rmccc/plugin-devtools`。
- 模板：`dashboard-card`/`declarative-theme`/`agent-tool`/`mod-source`。
- `pnpm plugin:dev/validate/pack` 工具（指南 §13.4，注意与 clap 命令树冲突评估）。
- 文档：getting-started/api-reference/permissions/lifecycle/security/cross-platform/publishing。

### 4.4 卡片 Host API 与平台约束
- 插件 UI 不注入通用 Tauri IPC；只通过 Host Bridge。
- 资源白名单；UI API 版本约束兼容性。
- 同一插件 Windows/macOS UI 一致性测试（Windows = deferred）。

### 4.5 示例插件
- `examples/plugins/server-dashboard/`：隔离 UI 卡片，经 Host Bridge 取数据（`network.request` 受控）。
- 端到端：独立构建 → 打包 → 安装 → 显示卡片 → 禁用 → 卡片不再执行。

## 4. 验收标准

- [ ] 外部样例可独立构建/打包/安装；无法越权调 Tauri IPC 或文件 API。
- [ ] Host Bridge 消息 schema/超时/取消/错误映射有测试。
- [ ] 卡片 error boundary + 权限拒绝态 + unsupported 态齐备。
- [ ] CSP 收紧；远程脚本被拒。
- [ ] ADR-006 accepted；`pnpm typecheck/build` 通过；Windows UI 一致性 = deferred。

## 5. 平台策略

- 隔离容器在 macOS WebKit 与 Windows WebView2 行为不同；本机只测 macOS，Windows 标 deferred。
- 不在本机宣称 Windows 隔离 UI 安全模型可用。

## 6. 风险与回滚

- iframe 沙箱逃逸 → CSP + 来源校验 + 不注入通用 IPC；PoC 后 ADR。
- 消息协议破坏性变更 → UI API major 版本提升 + 兼容矩阵公开。

## 7. 文档同步

- `ARCHITECTURE.md` 增"隔离 UI + SDK"章节；`plugin-sdk/` 目录建立。
- `AUDIT` §1.3"UI 扩展能力"更新；`PROJECT_STATUS` 回填。
