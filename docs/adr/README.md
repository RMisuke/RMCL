# RMCCC ADR 索引

> **ADR（Architecture Decision Record）** 记录不可由 Agent 个人偏好静默做出的架构决策。
> 规则见 [`../RMCCC_PLUGIN_PLATFORM_ARCHITECTURE_GUIDE.md`](../RMCCC_PLUGIN_PLATFORM_ARCHITECTURE_GUIDE.md) §20：
> 插件 API、权限、Manifest、持久化 Schema、运行时选择、跨平台发布策略的变更通常需要 ADR。
>
> **状态机**：`proposed` → `accepted` →（可选）`superseded` / `rejected`。
> 只有 `accepted` 的 ADR 才能作为代码变更依据；`proposed` 仅记录方向，不据此改协议。

## 列表

| ADR | 标题 | 状态 | 触发阶段 | 关联 |
|-----|------|------|---------|------|
| [ADR-001](ADR-001-supported-platforms.md) | 支持的 OS 最低版本与架构 | proposed | Phase 0 | 指南 §5.1；ARCHITECTURE §13.1 |
| [ADR-002](ADR-002-profile-instance-semantics.md) | Profile / Instance 语义分界 + Core Service 门面 | **superseded by ADR-016**（2026-10-27） | Phase 1 | 审计 P2-10、§7.4 |
| [ADR-003](ADR-003-plugin-package-and-install-transaction.md) | 插件包归档、目录布局与安装事务 | **accepted**（2026-10-21） | Phase 2 | 指南 §7/§11/§12/§14.4；ADR-004 |
| [ADR-004](ADR-004-plugin-manifest-and-versioning.md) | 插件 Manifest Schema、ID 与版本兼容 | **accepted**（2026-10-21） | Phase 2 | 指南 §6.4/§7/§8.1；ADR-003 |
| [ADR-005](ADR-005-dashboard-layout-and-theme.md) | Dashboard 布局 Schema/归一化/断点/主题令牌 | **accepted**（2026-10-21） | Phase 3 | 指南 §9；ADR-003/004 |
| [ADR-006](ADR-006-isolated-ui-container-and-host-bridge.md) | 插件隔离 UI 容器与版本化 Host Bridge | **accepted**（2026-10-21） | Phase 4 | 指南 §6.2/§9.6/§13/§14.2；ADR-003/004/005 |
| [ADR-007](ADR-007-logic-runtime.md) | 逻辑插件运行时：独立宿主进程 + WASM 解释执行 | **accepted**（2026-10-21，含 PoC 实测） | Phase 5 | 指南 §6.2/§6.3/§17；ADR-003/006/011 |
| [ADR-008](ADR-008-plugin-registry-signing-and-distribution.md) | 插件目录、发布者签名与分发（静态签名索引 + 分离式 Ed25519 包签名） | **proposed**（2026-10-26 草案） | Phase 7 | 指南 §6.4/§14.4/§15.3/§16.3；ADR-003/004/011；`../roadmap/09_phase7_distribution.md` |
| [ADR-009](ADR-009-credential-storage.md) | 凭据存储：AES-256-GCM 文件 vault 为首选，系统密钥链可替换 | **accepted**（2026-10-22） | Phase 6 | 指南 §10.4/§12.1；ADR-011 |
| [ADR-010](ADR-010-agent-session-audit-privacy.md) | Agent 会话、工具审计与隐私边界 | **accepted**（2026-10-22；部分被 ADR-013 修订） | Phase 6 | 指南 §10.1/§10.2/§10.5/§12.1；ADR-009 |
| [ADR-011](ADR-011-binary-size-metric.md) | 主可执行体积门禁：度量对象与阈值（2026-10-24 起 < 20 MiB） | **accepted** | Phase 0 | AGENTS.md 体积红线；审计 §15.5 |
| [ADR-012](ADR-012-macos-intel-build.md) | macOS Intel 的 runner、打包和最低系统版本 | proposed | Phase 0 | 指南 §5.1、§16.3 |
| [ADR-013](ADR-013-lightweight-ai-and-external-mcp.md) | 轻干预 AI 架构：内部 LLM 服务 + 外部 MCP 控制（内建 Agent 退役） | **accepted**（2026-10-23） | Phase 6→AI/MCP | `../AI_AND_MCP_ARCHITECTURE.md`；ADR-009/010（修订）/011；`../ui-design/07_AI_AND_MCP.md` |
| [ADR-014](ADR-014-theme-v2-appearance-skin.md) | 主题 v2：外观皮肤（字体/形状/间距/背景/视频/组件槽/特效）+ 用户覆盖 | **accepted**（2026-10-27 核心切片） | Phase 3+ | ADR-003/004/005/006；`../audits/PLUGIN_SECURITY_AUDIT_2026-10-27.md`；`../guides/THEME_PLUGIN_AUTHORING.md` |
| [ADR-016](ADR-016-instance-management-v2.md) | 实例管理 v2：目录注册表 + 版本即实例 + `.rmccc` 设置覆盖 | **accepted**（2026-10-27） | Phase 2+ | ADR-002（取代）/011；`ARCHITECTURE.md` §3.3/§3.4/§15；`ui-design/03`/`04` |

> ADR-008 已于 2026-10-26 建立草案（proposed）；ADR-014 于 2026-10-27 建立主题 v2 草案；
> accepted 前均不得据此改协议。
> 序号不按字母序排列以保持历史编号稳定。

## 模板

```markdown
# ADR-XXX: 标题

- **状态**：proposed | accepted | superseded by ADR-YYY | rejected
- **日期**：YYYY-MM-DD
- **阶段**：Phase N
- **关联**：审计条目 / 指南章节 / 其它 ADR

## 背景
为什么需要这个决策；当前事实（引用源码/实测）。

## 目标
这个 ADR 要解决什么；明确不解决什么。

## 选项
A / B / C，各自优缺点与代价。

## 决策
选择哪个；理由。

## 后果
正面/负面；对代码、CI、发布、体积、平台的影响。

## 迁移影响
是否需要改现有代码/数据；如何回滚。

## 跨平台验证方式
每平台如何验证；当前本机局限（服从指南 §1.4）。

## 状态变更记录
- YYYY-MM-DD：proposed
```
