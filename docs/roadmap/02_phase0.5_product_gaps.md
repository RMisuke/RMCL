# Phase 0.5 — 既有产品缺口补齐（主线闭环补全）

> **对应指南**：不属指南 Phase 0–7，是审计 §18 Sprint A 的细化。**状态**：0.5.1–0.5.6 全部完成（2026-10-20）。
> **当前事实来源**：审计 §10.5（UI 可直达 27/46）、§13.1–13.4、P1-1/P1-2/P1-5/P2-5/P2-6。

## 1. 目标

把"后端已就绪却无 UI"的命令接到前端，使 **UI 可直达 ≥ 40/46 命令**，主线产品闭环（Mod 浏览、多账号、日志、暂停恢复、本地 mod 安装）。**只接线既有后端，不改业务行为**——结构性偿债留给 Phase 1。

## 2. 前置条件

- Phase 0 完成（5 门禁全绿复跑，确认基线）。
- `api.ts` 分域 wrapper 与事件常量现状已清点（审计 §10.2、P2-5/P2-11）。

## 3. 与当前状态的对应

| 审计条目 | 现状 | Phase 0.5 处理 |
|----------|------|----------------|
| P1-1 Mod 浏览器缺失 | `mod_search/get_versions/get_download_url/install` 有 wrapper 无页面 | 新增 `/mods` 或实例页内嵌搜索面板，复用 4 命令 |
| P1-2 多账号无 UI | `auth_list_accounts/switch_account/remove_account/refresh_token` 无页面 | 设置页或账户下拉加"管理账户" |
| P1-5 本地 mod 安装未实现 | 缺 `tauri-plugin-dialog` | 引入 dialog 插件（评估体积），`mod_install` 接本地文件 |
| P2-5 `INSTANCES_REFRESHED` 前端常量缺失 | events.ts 11 个，后端 12 个 | 补常量并接线 |
| P2-6 15 命令有 wrapper 无页面 | 见上 | 同批接线 |
| P2-11 api.ts 死代码 + 裸字符串调用 | 路由 26 处 `safeInvoke("字符串")` | 路由改用 `api.ts` 分域 wrapper；删未使用 import |
| §13.6 日志页缺失 | `log_get/clear` 无 UI | 新增日志查看页 |
| §13.2 暂停/恢复无 UI | `download_pause/resume` 无入口 | Dashboard 下载面板加按钮 |

## 4. 任务切片分解

### 0.5.1 前端调用契约统一（先做，是其它切片的基础）
- 让所有路由页改用 `src/ipc/api.ts` 的分域 wrapper，**不再裸 `safeInvoke("命令名")`**。
- 删除 Dashboard 残留的 `auth,profiles,versions,launch` 未使用 import（审计 §10.2）。
- 前端事件常量补 `INSTANCES_REFRESHED = "instances://refreshed"`（P2-5）。
- 验收：`grep -rn 'safeInvoke(' src/routes` 仅出现在 `api.ts`/wrapper 内部，不在路由页。

**✅ 完成（2026-10-20）**：26 处裸 `safeInvoke` → `api.ts` 分域 wrapper；`api.ts` wrapper 全部改
`safeInvoke` 返回 `{ok,error}`；`i18n` 3 处适配；`events.ts` 补 `INSTANCES_REFRESHED`；
`mods` 以 `modsApi` 别名避开 signal 遮蔽。`grep -rn 'safeInvoke' src/routes` 为空；
`pnpm typecheck` / `build` 通过。Dashboard 的 4 个 import 从"未使用"变为真正使用（+`downloads`）。

### 0.5.2 Mod 浏览器（P1-1）
- 新增 `/mods` 或实例页内嵌搜索：搜索框 + 结果列表（虚拟滚动，审计 P3-5 当前硬截断 100）+ 版本选择 + 安装按钮。
- 复用 `mod_search/mod_get_versions/mod_get_download_url/mod_install`。
- 安装目标实例随 `instance_mods_dir()`（已随 run_dir，审计 §6.6）。
- 验收：能在 GUI 搜索 Modrinth mod 并装到指定实例。

**✅ 完成（2026-10-20）**：独立 `/mods` 页（`ModBrowser.tsx`）= 实例下拉 + Modrinth 搜索 + 分页（20/页）+
展开版本 + 安装；Dashboard 实例面板加入口。未做虚拟滚动（用分页替代，P3-5 仍待后续）。
类型桥补 `ModInfo`/`ModSearchResult` 导出（`u64` → `number` 修正）。UI 可直达 27→30。

### 0.5.3 多账号管理 UI（P1-2）
- 设置页或账户卡新增"管理账户"：列表 + 切换 + 删除 + 刷新。
- 复用 `auth_list_accounts/auth_switch_account/auth_remove_account/auth_refresh_token`。
- 验收：多账户可切换/删除，UI 直达 +4 命令。

### 0.5.4 本地 mod 安装（P1-5）
- 引入 `tauri-plugin-dialog`（先评估体积，记入 ADR-011 度量）；capability 只开文件选择。
- `mod_install` 接受本地文件路径（已有 `sanitize_filename`+`validate_mod_filename`，审计 §6.8）。
- 验收：能从本地选择 jar 装到实例。

### 0.5.5 日志查看页 + 下载暂停恢复（§13.6、§13.2）
- 新增日志页：`log_get` 读尾部 + `log_clear`。
- Dashboard 下载面板：`download_pause/resume/cancel` 按钮（按 `batch_id`，审计 §8.3）。
- 验收：UI 直达 `log_get/clear/download_pause/resume`。

### 0.5.6 实例完整性检查/修复入口
- 实例设置页加"检查完整性 / 修复"按钮（`instance_check_completion/instance_repair`）。
- 注意：`instance_repair` 当前**绕过 pipeline**（审计 P1-3），本切片**不修**，只接线；修复留 Phase 1。
- 验收：UI 直达 +2 命令；修复行为暂以 toast 反馈。

## 5. 验收标准

- [ ] UI 可直达命令数 ≥ 40/46（审计 §10.5 更新数字）。
- [ ] `PROJECT_STATUS.md` §5.3 数字同步。
- [ ] 路由页无裸 `safeInvoke` 字符串命令名（除 wrapper 内部）。
- [ ] `events.ts` 常量与后端 12 个对齐。
- [ ] `pnpm typecheck` + `pnpm build` 通过；`pnpm tauri dev` 手测 5 个新入口可走通。

## 6. 验证命令

```bash
pnpm typecheck && pnpm build
cargo fmt --all -- --check && cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
# 前端契约检查
grep -rn 'safeInvoke(' src/routes   # 应为空（wrapper 内部除外）
```

## 7. 平台策略

- `tauri-plugin-dialog` 在 macOS/WebKit 与 Windows/WebView2 行为不同（审计 §5.6）；本机只测 macOS，Windows 标 deferred。
- 文件选择器、拖拽、输入法等 UI 差异按两平台分别留 TODO，不在本机假装验证 Windows。

## 8. 风险与回滚

- 引入 dialog 插件后体积变化 → Phase 0 基线对照，超阈值回退或提 ADR。
- `instance_repair` 接线后用户依赖其行为，Phase 1 重构时须保证语义不变（先校验后执行）。
- 虚拟滚动实现复杂 → 可先分页，再迭代。

## 9. 文档同步

- 更新 `AUDIT` §10.5、§13 功能矩阵、§16 漂移表。
- `DEVELOPMENT_PLAN.md` Phase 4 状态从"缺浏览器"改为"已完成"。
- `PROJECT_STATUS` 回填一轮。
