# Phase 3 — Dashboard 卡片与主题引擎

> **对应指南**：§9，§17 Phase 3。**状态**：pending。
> **当前事实来源**：前端为固定 JSX 卡片排列（审计 §10.1，Dashboard 812 行）；暗色唯一主题（§10.4）；`app.css` 1494 行手写设计系统。

## 1. 目标

把固定 JSX 主界面迁移为"注册表 + 布局配置 + 受控渲染"，并落地主题令牌系统。用户可自定义布局并重启后保留；插件卡片失败或卸载不损坏其它卡片/主程序。

## 2. 前置条件

- Phase 2 完成（插件注册表/状态机/声明式插件可用）。
- ADR-005（Dashboard Layout Schema/冲突算法/断点规则）accepted。

## 3. 与当前状态的对应

| 指南要求 | 当前 | Phase 3 产出 |
|----------|------|--------------|
| Widget Registry | 固定卡片 | `WidgetDescriptor` 注册表 + 受控渲染 |
| Layout Schema | 无 | `dashboard/layouts/<id>.json` + 规范化/迁移 |
| 拖放/调整尺寸/隐藏 | 无 | CSS Grid + 拖放 + 尺寸约束 |
| 主题令牌 | `app.css` 变量 | 设计令牌层 + 浅/深/跟随系统 |
| 卡片生命周期/错误边界 | 无 | loading/empty/error/permission-denied/unsupported |
| 内置功能迁移为 Widget | Dashboard 固定 | 启动/实例/账户等迁为 `org.rmccc.core-ui` Widget |

## 4. 任务切片分解

### 3.1 Layout Schema + 规范化算法（ADR-005）
- `schemas/dashboard-layout.schema.json`：`schemaVersion`/`grid{columns,gap}`/`widgets[]`。
- 冲突/越界/非法尺寸在写入前归一化（确定性算法 + 单测，不依赖 DOM 顺序）。
- 多布局模板；损坏布局备份并恢复默认，不阻断启动。
- 保留 Shell（标题栏/导航/插件管理/全局 Toast/恢复入口）。

### 3.2 Widget Registry + 受控渲染
- `WidgetDescriptor`（pluginId/widgetId/size/breakpoints/settingsSchemaRef/requiredCapabilities/lifecyclePolicy）。
- `WidgetInstance`（instanceId/position/size/configuration）。
- 内置功能迁为 `org.rmccc.core-ui` Widget（启动面板/实例列表/账户卡/日志）。
- 卡片 error boundary：单卡片失败不崩 Dashboard。

### 3.3 拖放与响应式
- CSS Grid + 拖放（有效放置区域提示）+ 调整尺寸 + 隐藏/添加/恢复默认。
- 测试 1280×720/小窗口/宽屏/高 DPI/系统缩放；所有卡片有 loading/empty/error 态。

### 3.4 主题令牌系统
- 设计令牌第一层（背景/surface/文字/强调/危险/边框/圆角/间距/字体/阴影）→ CSS variables。
- 主题包默认只 JSON + 静态资源（无 JS）；浅/深/跟随系统。
- 主题无效回退内置安全主题 + 日志记录。
- 主题不继承业务权限。

### 3.5 插件卸载后保留占位
- 插件卸载后保留不可用占位 + 原设置，防再装时布局丢失。

## 5. 验收标准

- [ ] 用户可自定义布局并重启保留；恢复默认可用。
- [ ] 内置功能全部表现为 `org.rmccc.core-ui` Widget。
- [ ] 插件卡片失败/卸载不损坏其它卡片或主程序。
- [ ] 主题令牌 + 浅/深/跟随系统；无效主题回退。
- [ ] 布局算法单测覆盖冲突/越界/迁移。
- [ ] ADR-005 accepted；`pnpm typecheck/build` 通过；Windows UI 差异 = deferred。

## 6. 验证命令

```bash
pnpm typecheck && pnpm build
cargo fmt --all -- --check && cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace   # 布局算法/迁移测试
```

## 7. 平台策略

- WebView 差异（macOS WebKit vs Windows WebView2）按指南 §5.6 分平台验证；本机只测 macOS，Windows 标 deferred。
- 高 DPI/字体回退/输入法等差异留 TODO。

## 8. 风险与回滚

- 迁移内置功能为 Widget 期间主线不可用 → 保留旧 Dashboard 路由直至新 Registry 烟测通过再切换。
- 主题覆盖安全语义色 → 令牌层禁止插件覆盖宿主安全提示语义色。

## 9. 文档同步

- `ARCHITECTURE.md` §9 同步 Widget Registry/Layout/主题。
- `AUDIT` §10.1/§10.4 更新；`PROJECT_STATUS` 回填。
