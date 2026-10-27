# 只保留主题 + 主页卡片：性能/内存/体积收益评估（2026-10-28）

- **问题**：如果插件系统只保留「主题」和「主页卡片定制」，性能、内存、体积、运行速度能提升多少？
- **方法**：做了一次**不 strip 的 LTO release 构建**（`/tmp/rmccc-bloat`），用 Mach-O 符号表按
  crate/模块归因 `__TEXT,__text` 字节数；代码路径按源码核读。**未做**“去插件”对照构建，
  内存与冷启动**未实测**（沙箱禁止读其它进程 RSS，见 §7）。
- **基线**：HEAD `48fdc09`，strip 后主可执行 **9,972,720 B ≈ 9.51 MiB**（阈值 20 MiB）。

---

## 0. 结论（先看这段）

1. **“保留主页卡片”有两种含义，差距很大**：
   - 保留**现状**（插件 iframe 卡片）→ 实际只能删 WASM 逻辑，**二进制 −56 KB（−0.6%）**，
     启动/内存/速度**没有可测变化**。因为卡片定制本身就是那套隔离 UI + Host Bridge。
   - 改成**宿主渲染的声明式卡片**（卡片只声明标题/尺寸/数据源，宿主画）→ 才能删掉
     隔离 UI、桥、权限、逻辑，**二进制 −0.24~0.30 MiB（−2.4~3.0%）**，前端 −20~30 KB，
     攻击面大幅缩小；**启动仍然只省 1~3 ms**。
2. **插件系统不是性能问题**：插件相关代码只占 `__text` 的 **4.4%**，且全部在冷路径
   （打开插件页、主题切换、卡片轮询）。它不参与启动热路径、不参与游戏启动、无后台线程。
3. **主题本身比插件机制更“贵”**：`theme.rs` 单模块 69 KB，比整个隔离 UI 协议（4 KB）大一个量级。
   真正想要“更快更小”，优化主题资产路由和 WebView 基线的收益远大于删插件（§6）。

---

## 1. 实测体积归因（符号级）

构建：`CARGO_TARGET_DIR=/tmp/rmccc-bloat CARGO_PROFILE_RELEASE_STRIP=false cargo build -p rmccc --release`
→ 14,187,896 B（未 strip）。同树 strip 产物 9,972,720 B（strip 去掉 4.02 MiB，主要是符号表）。

`__TEXT` 总计：`__text` 7,442,744 B / `__const` 1,198,408 B。

| 归属 | `__text` 字节 | ≈MiB | 说明 |
|---|---:|---:|---|
| `rmccc-plugin` 全crate | 302,148 | 0.288 | 见下方拆分 |
| ├ `manager.rs` | 53,356 | 0.051 | 生命周期/安装事务/授权 |
| ├ `manifest.rs` | 41,608 | 0.040 | 清单校验 + 贡献项 |
| ├ **`theme.rs`** | **69,264** | **0.066** | 主题 v1/v2 文档与校验（保留项） |
| ├ `permissions.rs` | 25,392 | 0.024 | 授权/审计 |
| ├ `registry.rs` | 17,544 | 0.017 | 注册表 |
| ├ `config.rs` | 10,984 | 0.010 | 插件配置 schema |
| ├ `version.rs` | 8,300 | 0.008 | 版本约束 |
| ├ `package.rs` | 8,032 | 0.008 | `.rmcpkg` 解包校验 |
| ├ `theme_assets.rs` | 4,896 | 0.005 | 资产 magic/SVG 校验（保留项） |
| ├ `ui.rs` | 4,156 | 0.004 | 隔离 UI 路径/桥方法表 |
| ├ `capability.rs` | 3,568 | 0.003 | 能力表 |
| ├ `storage.rs` / `installer.rs` / `logic.rs` | 5,544 | 0.005 | 存储/安装/逻辑入口 |
| `rmccc-logic` | 48,420 | 0.046 | WASM 协议/监督（wasmi 不在主程序） |
| `rmccc-dashboard` | 28,400 | 0.027 | 其中 `skin.rs` 21,984（保留项） |
| app `commands::plugin_ui` | 31,008 | 0.030 | 资源路由 + Host Bridge |
| app `commands::plugin_logic` | 7,736 | 0.007 | 逻辑调用 |
| app `commands::dashboard` | 14,144 | 0.013 | 主题 skin 载荷/布局（多在保留侧） |

**口径**：LTO 会把小函数内联进调用方，符号归因是**下界**；字符串/常量表（能力表、错误文案）
没有按符号拆分，估计还有 10–20% 未计入。因此“插件相关”合计取 **0.42–0.50 MiB**。

---

## 2. 两种裁剪口径分别能删什么

### C-2：保留现状（主题 + iframe 卡片），只删 WASM 逻辑

- 删除：`rmccc-logic`、`commands::plugin_logic`、`LogicSlot`/helper 拉起、`plugin_logic_invoke/cancel`。
- 保留：包安装、注册表、权限、隔离 UI、Host Bridge、主题、卡片。
- **收益：`__text` −56 KB ≈ −0.06 MiB（−0.6%）**；helper（1.1 MiB）本来就没打进 `.app`，
  删掉不改变安装包体积。启动、内存、速度：**无可测差异**。

### C-1：主题 + 宿主声明式卡片（不用 iframe）

- 删除：`ui.rs`、`commands::plugin_ui`（资源路由 + 桥）、`PluginFrame`/`bridge.ts`、
  `capability.rs`/`permissions.rs`（卡片改为数据声明后不需要运行期能力）、`rmccc-logic`、
  `Plugins` 页的权限审查 UI；`dashboard.cards`/`ui.pages` 能力与相关审计。
- 保留：`theme.rs`、`theme_assets.rs`、`skin.rs`、`manifest.rs`（卡片声明）、`package.rs`、
  `installer.rs`、`registry.rs`（主题包仍可导入）、`dashboard` 布局。
- **收益：`__text` −240~300 KB ≈ −0.24~0.29 MiB（−2.4~3.0%）**；前端 −20~30 KB。
- 代价：需要新写卡片 DSL（`DashboardCard` 目前只有 id/title/description/尺寸，没有任何内容字段，
  见 `crates/rmccc-plugin/src/manifest.rs:79`），第三方“任意 HTML 卡片”能力消失。

### 参考：主题也改成内置（不再支持第三方导入）

再删 `package/installer/registry/version` 与大部分 `manifest` 校验，合计 **−0.4 MiB 左右（−4%）**，
但主题生态与 `.rmcpkg` 导入同时消失——与最近的 v2 方向冲突。

---

## 3. 启动速度

启动路径（`src-tauri/src/main.rs:199` 起）：窗口先 `show()`，再做 `AppState::new`。

| 阶段 | 成本 | 依据 |
|---|---|---|
| `PluginManager::load` | **1~2 ms（估）** | 读 `registry.json`(2.2 KB) + `permissions.json`(1.9 KB) + 每插件 manifest + 一次 `read_dir`；`PermissionStore::load` 不读审计日志 |
| 主题应用 | **0.5~1.5 ms（估）** | 读 `theme.json` + `validate_theme_assets` 逐资产 magic 校验（现有 4 个插件共 252 KB） |
| WebView 创建 + 首帧 | 数百 ms | 与插件无关 |
| 配置/实例/版本扫描 | 数十~数百 ms | 与插件无关（日志里 `Game repository refreshed` 属此项） |

**结论：全删也就省 ≤3 ms，冷启动占比 <1%，在噪声范围内。** 若把注册表加载挪到首帧后（idle），
收益同样只有 1~2 ms，但风险为零，可作为无害优化。

---

## 4. 内存

| 场景 | 估算 | 说明 |
|---|---|---|
| 常驻（不开插件页） | **< 100 KB** | 4 个 manifest + 注册表 + 授权表；无后台线程/定时器 |
| 打开 Home + N 张插件卡 | **+2~8 MB/卡（估）** | iframe 与主页面共享同一个 WebContent 进程，不新起渲染进程；成本是 DOM/JS 堆 |
| 逻辑插件 | **0** | helper 未打包；若打包，ADR-007 记录 +2.25 MiB 安装体积、首调用 2–3.5 ms 握手 |
| 主题 | 0 | 只注入 CSS 变量与 URL，图片/视频由 WebView 自己缓存（当前 `no-store`，见 §6） |

对照：Tauri/WebKit 应用的空载 RSS 通常在 **100–200 MB** 量级，插件常驻开销是它的 **0.05% 以下**。
**删插件对“内存占用”这个指标基本没有意义**；真正的内存大头是 WebView 与前端状态。

---

## 5. 运行速度（IPC / 主题渲染 / 游戏）

- **Host Bridge**：每条消息一次 IPC + 形状/授权校验，估计 0.5~2 ms；只发生在插件页或卡片轮询，
  不在主界面热路径。
- **主题资产路由**（`src-tauri/src/commands/plugin_ui.rs:42`）：每次请求都
  `state.core.read().await` + `std::fs::read` 整文件 + `Cache-Control: no-store`。
  对 3–16 MB 的背景视频，每次 seek/重载都会重读磁盘；这是**主题渲染**的真实开销，
  与“插件系统是否存在”无关，改缓存策略即可（§6）。
- **游戏启动/游戏内帧率**：0——插件代码不进入启动器→JVM 的参数生成与进程管理路径。

---

## 6. 如果目标是“更快更小”，值得做的（按收益排序）

1. **主题资产：安装期校验一次 + 运行期只读**（去掉每次 `validate_theme_assets`）；
   响应改 `Cache-Control: immutable` + 流式 `Range`（现在整文件读入）。对视频主题收益最大。
2. **插件注册表惰性化**：首帧后再 `scan()`，省 1~2 ms，零风险。
3. **WebView 基线**：前端首包 215.84 kB（gzip 71.94 kB）已按路由拆分；进一步优化收益在
   首屏 JS 执行与字体加载，而非插件。
4. **实例/版本扫描**：启动日志显示 `Game repository refreshed` 是毫秒级到百毫秒级的大头，值得单独看。

---

## 7. 没测什么 / 局限

- **RSS 未实测**：沙箱内 `ps`/`top` 被拒，`proc_pidinfo` 返回 0，无法采样运行中进程；
  §4 是结构推算，不是读数。
- **冷启动未实测**：应用没有启动耗时埋点；§3 是按 I/O 量推算的上界。
- **未做去插件对照构建**：`cargo bloat` 未安装，改用“不 strip 构建 + 符号归因”，
  LTO 内联使结果偏低；如需精确值，应在 `git worktree` 里删 crate 后重建对比。
- 符号归因只覆盖 `__text`；`__const`（1.2 MB，含前端嵌入资源、能力表、文案）未按 crate 拆分。

---

## 8. 建议

- **不要以“性能/内存”为理由裁剪插件系统**：能省的是 0.6%（保留 iframe 卡片）到 3%（改声明式卡片）
  的体积，启动与内存收益在测量噪声内。
- 若动机是**攻击面与维护成本**，C-1（声明式卡片）是合理方向，但要接受“卡片不再是任意网页”，
  并需要一套卡片 DSL；这是产品决策，不是性能优化。
- 若只想少一个未打包、不可用的执行面，**C-2（只删 WASM 逻辑）**成本最低：−0.6% 体积、
  去掉 `plugin_logic_invoke` / `plugin_logic_cancel` 两条 IPC 与 helper 拉起逻辑，主题/卡片体验完全不变。
