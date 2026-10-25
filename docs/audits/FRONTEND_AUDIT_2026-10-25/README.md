# 前端 GUI 全面审计报告（2026-10-25）

> 审计对象：RMCL 前端（Tauri 2.x + SolidJS），分支 `ui/home-hero-dock`
> 审计方法：四路并行深读（架构横切层 / 页面组件层 / HMCL 功能全景 / HMCL 设置项枚举）+ 主审对关键结论逐条抽查实证（假数据、hash 参数失效、分辨率缺陷、死代码、API_Key 状态等均经人工复核）。
> 参照库：`/Volumes/APPLE 1TB/Code/HMCL-main`（HMCL 2026 重构版）。

## 报告索引

| 文档 | 内容 |
|---|---|
| [01_ARCHITECTURE.md](01_ARCHITECTURE.md) | 架构与横切层：分层/IPC/状态管理/生命周期/i18n/样式/构建，风险与冗余 |
| [02_PAGES_AND_COMPONENTS.md](02_PAGES_AND_COMPONENTS.md) | 8 页面 + 9 全局组件逐页审计；Home/HomeDashboard 并存判定；布局形态与选项分布专项 |
| [03_HMCL_COMPARISON.md](03_HMCL_COMPARISON.md) | 与 HMCL 的功能对比：领域矩阵、A/B/C 级缺失清单、显示不完整清单、设置项数量对比 |
| [04_ISSUES.md](04_ISSUES.md) | 问题台账全量去重（P0×2 / P1×15 / P2×22 / P3×12，共 51 项，均带 `文件:行号`） |
| [05_RECOMMENDATIONS.md](05_RECOMMENDATIONS.md) | 审计意见：S0-S5 修复路线、布局与选项分布整改、保持项 |

## 规模与基线

- 前端 49 个文件 / 15,175 行（8 页面、12 stores、rm 组件库、i18n、2015 行 CSS）
- i18n 双语完全一致（zh/en 各 558 key，编译期强制，引用零缺失）
- CSS 死代码约 135 行（6.7%）；运行时依赖 6 个，大件已懒加载分包

## 总体结论

**架构纪律优秀，功能面中等，打磨度不足，实验残骸未清理。**

| 维度 | 评价 |
|---|---|
| 分层与 IPC 纪律 | ★★★★★ safeInvoke 统一封装 + stores 唯一 listen 点 + ts-rs 类型桥，同类项目优秀实践 |
| 功能完整性 | ★★★☆☆ 骨架齐全；对照 HMCL 缺 A 级功能 10 项（整合包/世界/资源包/CurseForge/Java 管理等） |
| 打磨度 | ★★☆☆☆ 51 项问题：核心旅程断裂 ×2、功能失效 ×15 |
| 一致性 | ★★☆☆☆ 两种保存模式并存、假数据无标识、i18n 尚有硬编码尾巴 |
| 测试与守卫 | ★☆☆☆☆ 前端零测试；bindings 漂移守卫恒真 |

## Top 10 必修（详见 04/05）

1. **P0** hash 内查询参数全链路失效——"实例→Mod 商店上下文"与"版本预选"两条核心链路实际不可用（`Download.tsx:411` 等 5 个跳转源）
2. **P0** 首页默认三卡片（服务器状态/版本比对/公告）全为假数据且无标识（`Home.tsx:423-541`）
3. **P1** Settings 保存任意项会覆盖用户正在编辑的输入（`Settings.tsx:80`）
4. **P1** InstanceDetail Overview 快捷按钮跨 tab 跳转失效 + Download Step5 同款（tab 状态不随 hash 变化）
5. **P1** 分辨率"高"编辑被静默丢弃（`InstanceDetail.tsx:888`）
6. **P1** 所有重命名对话框不回填当前名（`overlays.tsx:126`，一行修复）
7. **P1** 手动停止游戏被当成"崩溃"弹日志抽屉（`init.ts:58`，exit_type 未消费）
8. **P1** 停止游戏 5 处无确认 + 关窗无运行提示
9. **P1** `HomeDashboard.tsx` 645 行死代码——且死版本有 5 项优于在用版的实现待回捞
10. **P1** 在用页面零响应式断点 + 窗口最小尺寸与设计文档冲突（800×500 vs 1280×720）

## 审计更正记录

- 初审曾判"仓库根 `API_Key.txt` 已入库为 P0 泄露"；经主审复核：该文件未被 git 跟踪且 `.gitignore:5` 已正确忽略，**不构成泄露**，降级为 P3 运维备注。
