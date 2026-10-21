# Phase 7 — 插件目录、签名与生产分发

> **对应指南**：§14.4、§16，§17 Phase 7。**状态**：pending。
> **前置**：Phase 2–6 全部稳定 + 原生 CI runner 触达。**ADR-008**（插件目录/签名发布）须 accepted。

## 1. 目标

形成可维护的插件生态与生产分发：官方 registry 元数据/发布者身份/签名验证/哈希/更新回滚；完成 Windows/macOS 安装器/签名/公证/updater/体积/E2E 验收。

## 2. 与当前状态的对应

| 审计条目 | 现状 | Phase 7 处理 |
|----------|------|--------------|
| §13.7 三平台产物 | release.yml 齐备缺证书/Secret | 配真实证书 + Secret；原生 CI 触达后实跑 |
| §13.7 体积审计 | 未做 | cargo bloat + 前端 bundle，对照 ADR-011 < 10MB |
| §13.7 E2E | 配置未实跑 | tauri-driver 真实 E2E（含插件装/禁） |
| §13.7 updater | endpoints 占位 + pubkey 示例 | 真实 endpoints + 签名密钥 |
| §15.5 strip | 失效 | 修复（Phase 0 已尝试），此阶段确认 release 流水线 strip 生效 |

## 3. 任务切片分解

### 7.1 插件目录与签名（ADR-008）
- 官方 registry 元数据 + 发布者身份；SHA-256 + 签名验证。
- hash 只证明文件匹配，发布者身份需签名/可信目录。
- 插件更新与 RMCCC 应用更新分开管理；用户可单独关闭插件自动更新。

### 7.2 平台产物与签名/公证
- macOS ARM64：`.app`/`.dmg`，Developer ID 签名 + notarization。
- macOS Intel：评估 universal binary 体积 vs 单架构（ADR-012）。
- Windows x64：NSIS 或 MSI（先一个主发行格式），Authenticode 签名。
- Linux x64：选定一种主格式。
- Tauri updater 签名密钥私钥不入 Git/日志/artifact。

### 7.3 E2E 与发布验收
- 每平台 E2E：启动/关闭/Profile 持久化/装声明式插件/显示卡片/禁用/恢复/损坏插件不阻断启动/关键游戏安装启动回归（指南 §16.2）。
- 跨平台完成度报告表（指南 §16.5）。

### 7.4 体积门禁
- 分别记录 stripped executable/`.app`/安装包/updater bundle 大小（不混指标）。
- 超 ADR-011 阈值 → release-blocked。

### 7.5 插件审核/撤回流程
- 违规处理/撤回/安全更新流程文档化。

## 4. 验收标准

- [ ] 公开发布件在干净系统安装/升级可用；插件更新失败可恢复。
- [ ] 三平台原生产物 + 签名/公证 + updater 真实 endpoint。
- [ ] 体积对照 ADR-011 全部 < 阈值。
- [ ] E2E 全平台绿；跨平台完成度报告表填实际值（缺证书项标 N/A + 原因）。
- [ ] ADR-008 accepted。

## 5. 平台策略

- 本阶段是**唯一**真正做 Windows/Linux 原生验收的阶段（依赖原生 CI runner）。
- runner image 变化时以官方文档为准（指南 §16.1）。
- 仍服从 §1.4：在原生 runner 触达前，所有非 macOS 项保持 deferred。

## 6. 风险与回滚

- 证书/Secret 不可用 → 标 release-blocked，不降低安全要求。
- updater 签名与 OS 应用签名混淆 → 文档区分，分别配置。
- 插件签名密钥泄露 → 撤回流程 + 轮换预案。

## 7. 文档同步

- `ARCHITECTURE.md` §12/§13/§14 同步发布产物矩阵。
- `AUDIT` §13.7 全部销账；`OPERATIONS.md` 发布步骤完整。
- `PROJECT_STATUS` 回填；ADR-008 入库。
