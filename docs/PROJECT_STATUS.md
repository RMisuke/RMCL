# RMCCC 项目现状与继续开发交接（2026-10-26 更新）

> 本文档是**变更记录与交接入口**，按轮次记录"这轮改了什么"。
> **完整架构说明 + 功能矩阵 + 技术债台账**见 [`audits/AUDIT_2026-10-20.md`](audits/AUDIT_2026-10-20.md)（横切全景，不按轮次组织）。
> 历史审计与整改台账仍然有效作为**问题溯源**（见 §6），但其"当前状态"结论已过期；
> §6–§9 为早期交接快照，现状一律以上方轮次记录为准。
> 本文所有数字均由本机实际执行命令得到，未采信文档自述。

---

## 0. 更新记录

**2026-10-26（前端审计 2026-10-26 整改：B0–B5 六批次全量执行）**（提交 `79b7965`）：

基线为 [`audits/FRONTEND_AUDIT_2026-10-26/`](audits/FRONTEND_AUDIT_2026-10-26/README.md)：上轮 51 项复核后
FIXED 2 / PARTIAL 9 / STILL 40，另新增 19 项（P1×3 + P2×4 + P3×12），合计 ≈59 项未解决 + 9 项部分残留。
按 `03_FIX_PLAN.md` 的 B0→B5 执行，除两处显式决策外全部落地：

1. **B0 止血**：P0-1 hash 内查询参数断链修复（5 个跳转源改 path query、删 ModDetail 手写 hash 解析、ModStore「退出上下文」响应式修复）。
2. **B1 新 P1**：N-1 版本列表错误通道（`gameVersionsError`）+ 重试 + 恢复「绑定本地版本」；N-2 安装细节折叠状态提升到 TaskCenter 面板层；N-3 `profile_delete` 增加 `cleanup_game_dir`（MCP 保守传 `false`，删除对话框加开关，先删档案再删版本目录）。
3. **B2 旧 P1 集群**：`useHashTab` 统一 Download/InstanceDetail/Settings 三页 tab；Settings 改显式保存 + touched/dirty 守卫（含内存/窗口/JVM/游戏参数/并发/分辨率复合锁）；launch `stop()` 统一确认 + `userKilled`；窗口 1280×760；bindings 漂移测试改只读比较。
4. **B3 P2 清理**：删除 `HomeDashboard.tsx`、死 CSS ~150 行与三条 `dashboard_*_layout` 命令（IPC 100→97，五项能力回捞进 hero 首页）；cache/downloads/profiles/mcp/TaskCenter/SkinViewer/versionCache 一批竞态与缓存修复；三类吞错点补 toast；暗色星空与插件主题氛围层回归；实例设置补字段；MCP/插件/删除对话框确认；VirtualList 键盘可达。
5. **B4 P3 收尾**：注释/错误字面量/`safeInvoke` 归一化/多播主题监听/`duration_ms` 类型/硬编码文案清零/LogDrawer 监听清理等。
6. **B5 测试与守卫**：引入 vitest 5.0.5（独立 `vitest.config.ts`，与应用构建链隔离），5 个测试文件 32 用例（markdown 安全模型优先、format、exitCodes、loaders、versionCache TTL）。

**未执行/偏离（显式记录）**：P0-2 首页三张假数据卡按产品决策保留为占位（D3 取「保留」，待后端 server ping 接入）；D2「停止游戏不再询问」checkbox 未做（仅保留确认）。

验证：`pnpm typecheck`、`pnpm test`（32/32）、`pnpm build`、`cargo clippy --workspace --all-targets -D warnings` 零警告、`cargo test --workspace` 654 passed / 0 failed。

**2026-10-26（视觉刷新 Chromatic Voxel：层叠玻璃 / 三色主题 / 体素几何）**（提交 `46a8e31`）：

- **实现集中化**：新增 `src/styles/visual-system.css`（760 行）在 `app.css` 之后加载，集中覆盖圆角/阴影/动效数值；业务布局与安全语义色不变，未改 8-token 主题 schema。
- **三装饰色由宿主派生**：内置主题各配 `accent / accent-secondary / accent-tertiary` 调色板（`theme/index.ts` 的 `visualPaletteFor`），`applyThemeTokens` 写入 CSS 变量与 `data-visual-theme`；插件主题回退按明暗取自内置基准色。
- **主页装饰层**：hero 空白区新增漂浮体素方块、三色轨道环、透视网格与 SNGL 品牌标记，全部 `aria-hidden` 且不接管指针；粒子参数删除死字段。
- **动效闸门不变**：继续受 `data-motion` 与 `prefers-reduced-motion` 双重控制。
- **文档**：`ui-design/01_STYLE_GUIDE.md` 增视觉规范、`ui-design/02_HOME_DASHBOARD.md` 记录 hero 结构不变与新增装饰。

验证：`pnpm typecheck` / `pnpm build` 通过；视觉验收采用无头 Edge 截图人工复核，未纳入自动化门禁。

**2026-10-26（第二批性能优化：四项延期项落地）**：

1. **M4 · installer ZIP 单句柄复用**：`ForgeNewInstallTask` 新增 `execute_with_archive(&mut InstallerArchive)` 贯穿 install_profile 解析 / 内嵌 maven 提取 / data 条目解压 / version JSON 读取（原各步独立 `ZipArchive::new` 共 4 次打开 + 中央目录解析）；NeoForge 路径的 profile 读取与 NEOFORGE.RSA 探测并入同一句柄后传给 Forge 流程——~180MB 安装器从 5-6 次打开降为 1 次。
2. **V6 · 版本仓库 mtime 增量解析**：`DefaultGameRepository` 增加 `(mtime, len) → Arc<GameVersion>` 解析记忆化；`refresh()`（每次安装/修复/rename 后触发）对 stat 未变的 JSON 直接复用 Arc，免重读重解析；记忆化按本轮实际使用的实例目录剪枝。主路径与 mismatched-filename 回退路径均走 memo。
3. **C5 · 账户未变更跳过保存**：`Account` trait 新增 `needs_save()`（默认 `true` 保守）；Microsoft 账户以 `AtomicBool` 脏标记仅在真正刷新会话时置位（`needs_save` 消费读取），离线账户恒 `false`；`refresh_session` / `log_in_active_account` 两处按标记决定是否落盘——token 在有效窗口内时每次启动不再付出 2 读 2 写 + 2 次 AES-GCM 加解密。
4. **S1 · Modrinth 下载流式化**：`ModSource` 新增 `fetch_stream()`；默认 `install` 改为流式写入 `<dest>.part` + 边下边算 SHA1 + 校验通过后 rename 发布——大体积 mod/shader（50–200MB）不再整份双缓冲驻留内存，哈希不匹配也绝不在 dest 留下损坏文件（P1-8 语义对齐）。

验证：`cargo clippy --workspace -D warnings` 零警告、`cargo test --workspace` 654 通过、`pnpm typecheck` 通过。

**2026-10-26（性能审计与五批次优化：启动 / 日志 / 下载 / IPC 锁纪律 / 内核效率）**：

三路审计（前端 / Tauri 应用层 / 核心 crates）落地五批次整改，全部通过 `cargo clippy --workspace -D warnings`、`cargo test --workspace`（654 通过）、`pnpm typecheck`、`pnpm build` 验证：

1. **批次 1 · 启动路径**：镜像自动探测 `TcpStream::connect` 加 2s `connect_timeout` 且两探测并行（原代码 SYN 黑洞下可卡启动 ~75s）；`window.show()` 提前到重型 `AppState::new` 之前；前端 `initI18n`（内部两 IPC）/`initTheme` 并行化。
2. **批次 2 · 日志链路**：`LAUNCH_OUTPUT` 后端合帧（50 行或 100ms 窗口批量 emit，payload 改 `{profile_id, lines: string[]}`），退出时兜底 flush；modloader 安装器 stdout "detail" 事件 100ms 限流；LogDrawer 关闭时断开 buffers 订阅（memo 提前返回）、autoscroll rAF 节流；AI 流式 delta 前端 150ms 攒批（markdown 渲染 O(n²)→O(n)）。
3. **批次 3 · 下载管线**：下载流式写入同时增量算 SHA1（`stream_to_file`，校验零二次读盘）+ 128KiB BufWriter；dest 已存在时 size 短路（不匹配免全量哈希）；缓存命中优先硬链接（store 时已验证）；`CacheRepository` 全 API 异步化；asset 对象校验 Hash→Size 降级（内容寻址文件名）；`file_ok` 单次 stat；asset index 本地 SHA1 命中免重下。
4. **批次 4 · IPC/锁纪律 + 前端 store**：`mod_sources` Box→Arc，mods 全套命令改 `core.handle()` 快照模式（原 mod_install 持读锁跨整个下载）；`mods_dependency_scan` spawn_blocking；`mods_apply_plan` 弃 `block_in_place` 改 `rmccc_agent::apply_missing_dependencies_async`；logs/AI 日志读取改 spawn_blocking 尾部窗口；natives 解压、Java 探测（launch + settings 两处）spawn_blocking；settings_set 校验/建目录挪出写锁；instance_list/instance_refresh 扫描移出 core 锁并 spawn_blocking；前端 downloads/modloaders store 改 `createStore` 细粒度路径更新；MCP 2s 永久轮询改事件驱动（仅确认卡 pending 时轮询）+ status 无变化跳过 set；profiles 刷新保持对象引用；InstanceDetail 三处 effect 依赖收敛到 profile.id；Home 原地 sort 改拷贝 + memo。
5. **批次 5 · 内核效率**：`raw_json` String→`Arc<str>`（serde rc）；版本仓库 manifests HashMap→`Arc<GameVersion>`，继承链 resolve 消除每级深拷贝；`chain_resolvable` retain O(N²)→HashSet；manifest 缓存 `Arc<Vec<VersionInfo>>` + 读侧免整表克隆；`library_applies` 规则借用转换；本地父版本优先（免网络解析继承链）；`write_version_json` 存在检查前置；manifest ETag+body 磁盘持久化（冷启动条件请求）；parse_version_json 库数组按值解析；Forge processor 依赖下载 8 并发（原串行）+ `fetch_url` 免 Bytes→Vec 拷贝 + 流式 SHA1；Fabric 版本列表惰性物化（原 75k 对象常驻）；patch 型安装任务注入共享 HTTP 客户端（原 `Client::new()`）；下载引擎 mpsc+Mutex 改 JoinSet+Semaphore、暂停改 watch 通知（弃 100ms 忙等）、批次完成即清理 DashMap 条目、`wait_for_batch` 不再跨 await 持 DashMap 守卫；release profile 对 sha1/serde_json/zip/flate2 单独 -O3（保持全局 -Os 护体积红线）。

审计中证实为误报/已修复而未动的项：`cache.ts` inFlight（P2-5 已修）、TaskCenter 确认卡定时器（已 onCleanup）、SkinViewer blob LRU（P2-8 已修）、Settings 数字输入（本就是草稿+保存模式）。延期项：installer ZIP 复用单句柄（M4）、版本仓库 mtime 增量解析（V6）、账户未变更跳过保存（C5）、Modrinth 下载流式化（S1）。

**2026-10-25（正式立项：项目更名 SNGL，内核保留 RMCCC）**：

- **显示层更名**：标题栏品牌、窗口标题、`<title>`、欢迎页、MCP/AI 提示文案、关于页卡题、主页插件卡副标题、`productName`（bundle 名 RMCCC.app → SNGL.app）→ **SNGL**。
- **对游戏的启动器品牌**：`launcher_name` / `-Dminecraft.launcher.brand` 由 `RMCL` 改为 `SNGL`（外部可见标识）。
- **内核与协议保留 RMCCC**：crate 名（rmccc-*）、Rust 标识符、IPC/协议命名空间（`rmcccplugin://` 等）、CSS 变量（`--rmccc-*`）、localStorage 键、bundle 标识符 `com.rmccc.launcher`（改它会孤立 `~/Library/Application Support/rmccc` 用户数据目录）、HTTP UA——全部不动。



**2026-10-25（NeoForge 安装失败诊断与修复：UA 403 / 镜像候选 / data 解压 / 宽松解析）**：

真实环境（MC 26.1 + NeoForge 26.1.0.1-beta）端到端复现修复，四层缺陷逐层暴露、全部修掉：

1. **共享 HTTP 客户端无 User-Agent**：BMCLAPI 的 302 重定向镜像池（cernet/ustc 等）对无 UA 请求直接 403（reqwest 默认 UA 为空；curl 有 UA 全程 200）。`CoreContext` 下载客户端补 `RMCCC/{version}` UA——全部下载路径受益。
2. **安装器候选无重试**：`download_installer_file` 每候选单次尝试，镜像轮询的单次 403 即废掉候选；补每候选 3 次重试（与下载管理器 attempts 对齐）。
3. **processor 依赖库无镜像候选**：`install_profile.libraries` 的 URL 直连 maven.neoforged.net（本机网络超时）；补 BMCLAPI `/maven/` 镜像候选（原 URL 优先，neoforged / minecraftforge 两大 maven 覆盖）。
4. **data 表内嵌文件未解压**：`{BINPATCH}` = `/data/client.lzma` 是安装器 ZIP 内路径，HMCL 会先解压到临时文件再传处理器；此前直接传字符串路径 → `FileNotFoundException`。`ForgeNewInstallTask` 补 data 条目解压（Forge 47.4.5 因走 installer-JAR 整体运行从未暴露）。
5. **安装器 version JSON 严格解析失败**：NeoForge 26.1 的 version.json 缺 `assetIndex`，typed 解析整体报错；改走 `merge_partial_json_into_parent` 宽松解析（patch 只需 mainClass/arguments/libraries）。
6. **base client jar 就位保障**：PROCESS_MINECRAFT_JAR 处理器需要原版 jar；`core::install_loader` 在安装前检查缺失即补一次基础版本修复（GUI 向导本有顺序保证，CLI/MCP 无此约定）。

诊断手段：临时端到端复现测试（真实下载 + 真实 processors 执行），最终 `patch id=neoforge version=26.1.0.1-beta` 全流程通过后移除。

**2026-10-25（NeoForge 安装重做：processors 管线对齐 HMCL）**：

- **架构修正**：NeoForge 此前被划入 installer-JAR 路径（整体 `java -jar` 跑官方安装器）；HMCL 对 NeoForge 是**刻意自处理**（读 `install_profile.json` 后复用 Forge processors 管线）。现对齐 HMCL `download/neoforge/`：
  - `needs_installer_jar` 只保留 Forge；NeoForge 走 patch 管线，经 `NeoForgeInstallTask` 路由到 `ForgeNewInstallTask`（processors 逐条执行 + maven 内嵌提取 + 依赖下载）。此前实现只提取库、从不跑 processors，1.20.2+ 的 universal/binpatched 产物缺失，装出来必然不可启动。
  - **Path A**（1.20.1 世代，forge profile + `NEOFORGE.RSA` / 正文含 neoforge）：跑 Forge 管线后 patch 改名 `forge→neoforge`、版本号剥 MC 前缀（HMCL `modifyNeoForgeOldVersion` 语义）；
  - **Path B**（1.20.2+ 原生 neoforge profile）：同管线，版本号剥 `neoforge` 段（`modifyNeoForgeNewVersion`）；两路径均带 MC 版本一致性校验（HMCL `VersionMismatchException` 对齐）。
  - 已知差异（注释记录）：HMCL 会把 `DOWNLOAD_MOJMAPS` 处理器替换为镜像直下 mappings（仅为加速），我们按原样执行处理器，功能等价。
- **版本列表**：BMCLAPI 镜像为主；失败回退官方元数据（`net/neoforged/forge` + `net/neoforged/neoforge` 两端点，HMCL `NeoForgeOfficialVersionList` 对齐，含 26.x 四段版本号 → MC 版本推算算法）。安装器下载候选 = [BMCLAPI, maven.neoforged.net 官方]，403/失败切官方。

**2026-10-25（实测修复二：Forge 启动模块冲突 `ResolutionException`）**：

- **现象**：Forge 47.4.5 装好、依赖补齐后启动，ModLauncher 模块层构建时抛 `java.lang.module.ResolutionException: Modules minecraft and _1._20._1 export package net.minecraft.client to module forge`（崩溃点在 `ModuleLayerHandler.buildLayer`，游戏未进入初始化）。
- **根因**：模块名 `_1._20._1` 是从文件名 `1.20.1.jar` 推导的自动模块名——即**原版混淆 jar**（`versions/1.20.1/1.20.1.jar`，我们与 HMCL 同样把主 jar 按 jar id 放进 classpath）没有被 BootstrapLauncher 的 `-DignoreList` 排除，被收进模块层后与 srg jar 构成的 `minecraft` 模块导出同包。Forge 预置 ignoreList 里的 `${version_name}.jar` 只匹配"主 jar 按启动器版本 id 命名"的官方布局，不匹配我们的 `1.20.1.jar`。
- **修复**（对齐 HMCL `MaintainTask.maintainGameWithCpwBoostrapLauncher`，bsl ≥ 0.1.17 分支）：`generate_command_line` 中检测 BootstrapLauncher 主类（Forge/NeoForge 1.17+），把 `${primary_jar_name}`（主 jar 文件名）追加进解析后的 `-DignoreList`。回归测试 `test_bootstrap_launcher_ignore_list_appends_primary_jar` 钉死追加与 vanilla 不动两侧行为。

**2026-10-25（实测修复：Forge 启动崩溃根因 + AI 日志分析回退）**：

- **Forge 47.4.5 启动即崩（exit 1, `InaccessibleObjectException`）根因修复**：版本隔离（默认 `modded` 隔离 + Forge 实例）时 `run_dir` 是实例根，`get_configurations` 把它当 `game_dir` 传入 `build_template_vars`，`${library_directory}` 被推导成 `{实例根}/libraries`（不存在）→ Forge `-p ${library_directory}/...` 模块路径全部落空 → JVM `Unknown module: cpw.mods.securejarhandler` 警告 → `--add-opens java.base/java.lang.invoke=...` 被静默忽略 → `BootstrapLauncher` 反射 `IMPL_LOOKUP` 抛 `ExceptionInInitializerError`。修复：`get_configurations` 显式用共享 libraries 根覆盖该变量（HMCL `DefaultLauncher` "library_directory" → `repository.getLibrariesDirectory` 对齐）；回归测试 `test_library_directory_points_at_shared_libraries_root` 钉死模板变量与解析后 `-p` 两层。
- **AI 日志分析拿不到崩溃日志（"未找到游戏日志…先启动一次游戏再分析"误报）**：崩溃发生在 log4j 文件初始化之前时 `latest.log` 不生成，而 AI 分析（`ai_analyze_log`）与 MCP `game_logs` 只读该文件；启动器侧捕获的进程输出在 `launch_start` 拿到 pid 后即被丢弃。修复：`AppState` 新增每实例输出环形缓冲（`game_output`，2000 行，`TauriProcessListener.on_log` 落行、新一轮启动清空）；两处读取在 latest.log 缺失或为空时回退到捕获输出，AI 分析的 context 标注日志来源，MCP 返回增加 `source` 字段。

**2026-10-25（审计整改批次 7 `chore/audit-cleanup`：后端死代码清理 + 文档对齐）**：

- **死代码清理（净删 ~530 行）**：
  - `rmccc-launch`：删除与 DefaultLauncher 平行的第二套 JVM 参数实现（`LaunchArgBuilder` + `build_game_env` + `spawn_managed` + 附属 `VersionGate`/gated 辅助函数与 8 个死测试）——缺 log4j/natives 逻辑，留着必然漂移（审计跨 crate #6）；
  - `rmccc-download`：删除零调用方的 `create_provider`（含 2 个死测试）；
  - `rmccc-config`：删除 ADR-013 瘦身后遗留的 `agent_sessions_dir`/`agent_tasks_dir`；`agent_audit_path` 从死代码转为**接线**（MCP 审计日志改经它取径，单一事实来源）；
  - `rmccc-error`：删除从未构造的 `DeviceCodePending` 变体（含 error/cli 两处 match 臂）；
  - 事件面双侧清理：`DOWNLOAD_PAUSED`/`DOWNLOAD_RESUMED`/`AUTH_STATE_CHANGED` 三死常量从 events.rs 与 src/ipc/events.ts 同步删除（暂停/恢复乐观 UI 无后端事件对账的现状已在审计记录）；删除预留未用的 `ThrottledEmitter::emit_immediate`。
- **文档对齐（ARCHITECTURE.md，设计文档优先约束）**：
  - §3.1 依赖图补齐实测边（auth→platform、version→platform、modloader→platform、launch→platform）；
  - §3.2 `ModLoaderProvider` 章节 as-built 化（改记 `DependencyManager` 能力面，注明原 trait 已收敛删除）；`ModSource` 删除未实现的 `get_mod`，`CurseForgeSource` 标注 planned（指向审计差距 T1）；
  - §12 体积预算 10MB→**20 MiB（ADR-011）**，含 strip 度量口径说明；main.rs 同步 20 MiB 注释；
  - tauri.conf.json `$schema` 从 fork 仓库改回官方 `schema.tauri.app`。
- 前端死代码（HomeDashboard 回捞/死 CSS/i18n 死 key）不在本批：前端工作区有并行未提交改动，避免冲突，留给前端线。
- 门禁：`cargo test --workspace` 42 套件 0 失败、clippy 0 警告、fmt 干净、`pnpm typecheck` 0 错误。

**2026-10-25（审计整改批次 6 `fix/security-hygiene`：B 批安全专项）**：

- **B1 vault key 权限窗口**：`PlatformFs` 新增 `create_owner_only`（Unix `OpenOptions::mode(0o600)` 一步创建，文件从诞生第一刻即 0600）；vault 改用它——此前"先 `fs::write`（0644 可读）后收紧"存在同机其他用户可读的窗口。macos/unsupported/mock 三个后端实现，`MacosFs` 顺手转 pub（测试需要）。
- **B7 日志脱敏**：microsoft.rs 的 XBL/XSTS/refresh 三处错误响应体改经 `redact_body`（此前仅 device code 一处脱敏）。
- **B8 离线用户名校验**：`auth_offline_login` 强制 `[A-Za-z0-9_]{1,16}`（Minecraft 官方约束）——用户名会进 `--username` JVM 参数与 @argfile，含引号/空格的名字会破坏参数结构。
- **B2 id 防穿越三处接线**：`validate_profile_id`（profile get/save/delete 前置，`[A-Za-z0-9_.-]{1,64}` 且拒绝 `..`）；version 仓库的 rename/duplicate/remove 校验实例 id（非空、无路径分隔符、非 `.`/`..`、无控制字符——实例名允许空格故不做字母数字白名单）；dashboard 的 load/save/reset 校验 layout_id。**safe_join 文档纠正**：不再自称"全工程防穿越统一入口"（防护点在各 IPC 边界）。
- **B10 profile delete 守卫**：`cleanup_game_dir` 在共享根检查之外新增 `looks_like_game_dir` 标记校验（assets/saves/mods/resourcepacks/shaderpacks/options.txt 等）——game_dir 被手工改写指向 `~/Documents` 之类时绝不能整树删除（此前唯一防护是 versions/|libraries/ 存在与否）。
- **B4 capabilities 收窄**：移除 `opener:default`（此前授权前端 JS 无 scope 调 open_url + reveal 任意路径，绕过 `open_external_url` 的 http/https 闸）；Rust 侧 `tauri_plugin_opener::open_url` 不经该 capability，不受影响。
- **B5 types.ts 写盘限 debug**：main.rs 的 `write_types_file` 挂 `#[cfg(debug_assertions)]`——release 进程不再可能经构建机路径改写源码树。
- **B6 ui_http_client**：构建失败向上传播（`Client::new()` 兜底会跟随重定向，"30x 不逃出域名白名单"保证失效）。
- **B9 MCP 会话三补**：`require_session` 校验会话**已登记**（此前只验头非空，伪造 id 可无限 tools/list）；会话空闲 30 分钟自动回收（4 槽位不再可被占满，回收发 GUI 事件）；hyper 连接挂 `TokioTimer` 启用 header 读超时。新增 3 个会话测试；`McpHandle::app()` 改 Option 化（无句柄时跳过事件而非 panic，测试可直连）。
- 门禁：`cargo test --workspace` 45 套件 0 失败、clippy 0 警告、fmt 干净、`pnpm typecheck` 0 错误。

**2026-10-25（审计整改批次 5 `fix/launch-args`：启动参数正确性，A8/A9 + kill 统一）**：

- **A9 legacy 参数翻倍**：`generate_command_line` 的三种 game args 来源改为互斥——
  `arguments.game`（1.13+）优先，其次 `minecraftArguments`（≤1.12 legacy，自带完整
  参数集），两者皆无才用内置默认集。此前 legacy 收录后又追加一整套
  `default_game_arguments`（--username/--gameDir 等全量翻倍，OptiFine tweaker 等
  按位置解析的参数会错乱）。三个分支各有回归测试（`generate_command_line` 快照断言）。
- **A8 自定义 Java 真实探测**：`detect_launch_java` 对自定义 `java_path` 跑
  `probe_java_binary`（`java -XshowSettings:properties -version`）——此前直接伪造
  `{major_version: 期望值, is_64bit: true}`，装错 JDK 时所有按版本门控的 JVM 参数
  按"期望版本"生成且 `JavaVersionMismatch` 校验被完全跳过。探测失败显式报错，
  版本不符返回 `JavaVersionMismatch{found, required}`。
- **ManagedProcess::kill 统一进程组语义**：`kill()` 优先走平台层
  `terminate_tree(pid)`（与 IPC 层 `launch_kill` 同一语义，Unix 进程组 SIGKILL），
  平台终止失败时退回 `child.kill()` 兜底——带 wrapper/`nice` 前缀时 child 是外层
  包装进程，直接杀 child 会留下孤儿 java。`ManagedProcess::with_platform` 接收
  平台句柄（spawn 侧 `process_group(0)` 保证组长正确）。
- 门禁：`cargo test --workspace` 43 套件 0 失败（rmccc-launch 新增 3 测试）、
  clippy 0 警告、fmt 干净、`pnpm typecheck` 0 错误。

**2026-10-25（审计整改批次 4 `fix/loader-install`：Forge processors 修复 + Fabric/Quilt 侧别修正 + installer 下载合一）**：

- **installer 下载三副本合一（B3 提前）**：新增 `installer::download_installer_file` 共享实现——流式写盘（~180MB installer 不再整份进内存双倍拷贝）、唯一临时文件名（修 forge 路径 `/tmp/forge-installer-{pid}.jar` 从不删除的每次安装 ~180MB 泄漏）、ZIP 完整性早失败、`{url}.sha1` 可取时强制校验（镜像劫持/损坏即失败，取不到警告放行）。`InstallerJarRunner` 运行加 600s 超时（此前可永久挂起）；新增 `run_local` 供本地安装器文件复用同一条执行/探测路径。neoforge 的第三份副本（固定名 + 失败路径不清理）一并删除。
- **Forge 1.13+ processors 路径修复（A5）**：
  - classpath 分隔符改经平台描述符（`DependencyManager.with_platform` 注入；此前 `MAIN_SEPARATOR` 在 Unix/Windows 都不是 Java classpath 分隔符）；
  - `MINECRAFT_JAR` 指向 client jar 文件（此前是 `versions/` 目录）；
  - `[artifact]` 模板解析为 libraries 下的实际 jar 路径（此前原样返回 Maven 坐标）；`{key}` 的 data 值递归解析（artifact/嵌套 key/字面量，对齐 HMCL 数据表语义）；
  - **补齐两步缺失**：安装器内嵌 `maven/**` 提取到 libraries/（含防穿越）+ `install_profile.libraries` 未内嵌依赖下载落盘（带 SHA1 校验）——此前 processor JAR 根本不在 classpath 路径上。
- **Fabric/Quilt/LegacyFabric 侧别修正**：客户端安装取 `launcherMeta.libraries.{common,client}`（此前误取 common+**server**：server 库进客户端 classpath、client 库缺失）；**Quilt hashed 存在时取代 intermediary**（此前双映射并存冲突，HMCL 语义），测试更新并新增"无 hashed 退回 intermediary"用例。
- **双轨消除**：删除零调用方的 `install_from_file`（审计 P2：它把本地 installer 路由进与 InstallerJarRunner 行为不一致的第二条 Forge 安装路径）；`RemoteVersion::create_install_task` 新增 `InstallTaskContext`（game_dir/java_path）——此前 `install_with_patch` 的这两个参数被整体忽略，Forge 任务静默退到 PATH `java` 与 `.`。
- 门禁：`cargo test --workspace` 42 套件 0 失败、clippy 0 警告、fmt 干净、`pnpm typecheck` 0 错误。
- **待真机验收**（本机未联网跑安装器）：Forge 1.20.x / Fabric / NeoForge 各装一次并启动成功后回填本记录。

**2026-10-25（审计整改批次 3 `refactor/version-merge-unify`：inheritsFrom 合并与规则求值收敛，A7）**：

- **文档先行**：ARCHITECTURE.md §3.3 新增「版本继承合并与规则求值的统一语义」定版小节
  （逐字段合并语义、规则三分支语义、禁止新增第三套实现的约束）。
- **合并收敛**：新增 `rmccc_version::merge` 模块为唯一权威——
  `merge_child_into_parent`（从 default_game_repository 迁入，HMCL 追加语义）+
  `merge_partial_json_into_parent`（部分子 JSON 宽松合并，语义与 typed 完全一致）。
  三条链路全部折叠：`repository::resolve_version`（网络链，重写为 root→child canonical
  折叠——旧实现不继承 arguments/minecraftArguments/downloads/logging/jar）；
  `DefaultGameRepository`（本地链，改薄委托）；`pipeline::resolve_version` 本地分支
  （删除第三套手写合并——旧实现 arguments 整体替换且漏 4 个字段）。
  **顺修两个 bug**：① 网络链全字段继承（Forge/Fabric 子版本不再丢父版 JVM 参数与
  client jar 下载信息）；② 完整可解析的本地子 JSON 声明 `inheritsFrom` 时此前直接
  返回不合并，现必须走合并。
- **规则收敛**：`compatibility::rules_allow` 定为唯一权威（last-match-wins、
  无规则=适用、**有规则未命中=不适用**，对齐 HMCL `appliesToCurrentEnvironment`，
  修正此前 no-match→allow 的错误默认）；`arguments::rules_match`（旧"任意命中+
  disallow 一票否决"违背 Mojang）与 `libraries::library_applies` 全部改为薄封装。
  `applies_to_environment`（清单可见性，any-match 语义）保留不变。
- 新增测试：merge 模块 4 项（typed/partial 语义一致、追加非替换、null/未知字段忽略）、
  网络链全字段继承、`[{disallow osx},{allow}]` macOS 判定（审计跨 crate #2 案例）。
- 门禁：`cargo test --workspace` 42 套件 0 失败、clippy 0 警告、fmt 干净、typecheck 0 错误。

**2026-10-25（审计整改批次 2 `fix/ipc-lock-discipline`：IPC 锁纪律重构）**：

- **CoreHandle 快照（A2 根治）**：`CoreContext` 的 `version_provider`/`download_provider` Box→Arc、`profile_store`→Arc；新增 `CoreHandle`（Arc 快照 + config 克隆）与 `core.handle()`；pipeline 7 个编排函数与 core::modloader 3 个函数签名改 `&CoreHandle`，CLI/Tauri 调用方全部跟进。`version_install`/`launch_start`/`instance_repair`/`instance_check_completion`/`modloader_install`/`version_list` 改为"短锁取快照 → 长操作全程无锁"——整包下载 / Forge installer 运行 / Java 探测期间 `settings_set` 等写命令不再被阻塞（此前 tokio 公平 RwLock 下新读请求也排队，几乎所有 IPC 卡死）。
- **auth 嵌套读锁死锁（A1 根治）**：`refresh_session` 抽为公共函数（仅依赖 `account_repo`）；`auth_get_session`/`auth_refresh_token` 先释放守卫再刷新；Microsoft/离线登录短锁克隆 `Arc<AccountRepository>` 后，设备码流（分钟级等用户授权）全程不持 core 锁。
- **逻辑插件并发（审计 02 P2）**：`state.logic` 由全局 `Mutex<LogicRunner>` 改为 per-plugin `LogicSlot`（runner 独占锁 + helper PID 发布槽）；rmccc-logic 新增 `LogicHost::pid` / `LogicRunner::load` / `host_pid`；`plugin_logic_cancel` 快路径 `try_lock` 干净取消，慢路径按 PID 经平台层杀进程组——不再被在途调用锁住最长 30s。新增槽位独立性契约测试。
- 复核修正：`plugin_inspect`/`plugin_config_get` 的写锁**有意保留**（config_get 成功时清除 `config_error` 粘滞位，`&mut self` 是类型要求而非误用），已加注释说明。
- 顺带 clippy 清零（is_ascii_hexdigit ×2、`json_looks_like_version_manifest` 移到 tests 前）。
- 门禁：`cargo test --workspace` 42 套件 0 失败、`cargo clippy --workspace --all-targets` 0 警告、fmt 干净、`pnpm typecheck` 0 错误。

**2026-10-25（审计整改批次 1 `fix/panic-guards`：输入校验止血 + 未完成加载器下架）**：

- 后端五项修复（对应 [`audits/BACKEND_AUDIT_2026-10-25`](audits/BACKEND_AUDIT_2026-10-25/) 05 文档 A 批的止血子集）：
  - **A3** `file_layout::asset_object` 短/畸形哈希切片 panic → 返回 `Option<PathBuf>`（40 位 hex 门禁）；`check_completion` 对畸形哈希在计数两侧同时跳过（避免"缺失"永远追不上"修复"）；`GameRepository::get_asset_object` trait 签名跟进。
  - **A4** `CacheRepository::sha1_to_path` 缓存键零校验（路径注入/多字节 panic）→ 40 位 hex 校验：`check_by_sha1` 视为 miss、`store` 返回 InvalidInput。
  - **A10** `replace_templates` 字节级 `as char`（非 ASCII 模板文本乱码）→ 按 UTF-8 char 整字拷贝；中文模板/替换值回归测试钉死。
  - manifest.rs 解析失败的 library 由 `Library::default()` 空名占位（混入 classpath/下载列表）改为 **skip + warn**。
  - **A6** OptiFine / LiteLoader / Cleanroom 安装器未完成（产物不可用）→ 新增 `ModLoaderType::installable()` 白名单：`modloader_list` 返回空列表（向导自然无版本可选）、`install_loader` 快速失败并指向审计报告；`all()` 保留全部 8 种供 analyzer 识别已安装实例。
- 门禁：`cargo test --workspace` 42 套件全绿（0 failed，含新增 4 个回归测试）、`cargo fmt` 干净、`pnpm typecheck` 0 错误。前端零改动（另一会话进行中，未触碰其文件）。

**2026-10-25（全量审计轮：后端/前端/HMCL 比对，未改代码）**：

- 完成五路并行审计（crates 16 crate / src-tauri 99 命令 / 前端 / HMCL-main 功能盘点 /
  RMCL 功能矩阵），报告存档于 [`audits/BACKEND_AUDIT_2026-10-25/`](audits/BACKEND_AUDIT_2026-10-25/)
  （README 总览 + 01 crates + 02 src-tauri + 03 前端 + 04 HMCL 比对 + 05 审计意见）。
- 门禁实测：`cargo check --workspace` 零错误、`pnpm typecheck` 零错误；静态测试数 ≈632（10-20 时 300）。
- 问题总量：后端 0 P0 / 16 P1 / 50 P2 / 61 P3；前端 2 P0 / 14 P1 / 37 P2 / 33 P3。
  最优先：auth.rs 嵌套读锁死锁、version_install 长持锁、三套 inheritsFrom/rules 语义互斥、
  Forge processors classpath 全平台错、OptiFine/LiteLoader 产物不可用、asset_object 短 hash panic。
- HMCL 差距结论：RMCL 覆盖约 50% 功能点；缺整合包（全缺）、CurseForge、authlib-injector
  外置登录、Java 自动下载、mod 多格式元数据；RMCL 独有插件平台 / AI-MCP / 字段级继承锁。
  修复顺序见 05 文档 A/B/C/D 批与 T1→T3 路线图。

**2026-10-24（ui/home-hero-dock 轮：3D 皮肤预览根治 + 闪退修复 + 体积红线放宽）**：

- **3D 皮肤预览（skinview3d）渲染架构定版**：canvas/viewer 归"常驻舞台层"
  （`position:fixed`，挂在 `document.body`，应用生命周期一份）所有；组件挂载时
  按占位元素矩形做几何同步（`syncRect`），卸载时隐藏舞台 + `renderPaused`。
  **canvas 节点从不移动/重建**。此前两版（每页重建 / 单例 DOM re-attach）在
  快速切页后空白：WKWebView 对 reparent 的 WebGL canvas 不重建合成层，
  rAF 照跑、上下文活着但画面不上屏（日志 contextLost=false 可证）。
- **切账户闪退修复**：日志三次 `panic in a function that cannot unwind`（abort）
  均发生在 rmcccicon 协议请求风暴期间——快速切页/切账户时头像/皮肤重渲染
  产生大量 scheme task start/stop 竞速，撞 wry 0.55.1 scheme responder 的
  TOCTOU 缺陷。缓解三件套：协议响应 `Cache-Control: no-store → max-age=300`
  （WebKit 内存缓存去重同 URL 重拉）、皮肤 blob URL 模块级记忆化、加载代际
  保护（晚到的旧结果不覆盖新结果）。磁盘缓存仍是新鲜度权威。
- **体积红线放宽（ADR-011 修订）**：strip 后主可执行 < **20 MiB**（原 10 MiB），
  度量口径与门禁机制不变；AGENTS.md、ADR-007/011/012、DEVELOPMENT_PLAN 同步。
- 顺带：账号页/主页滚动容器补 hover 裁剪余量；卡片阴影统一为
  `--rmccc-shadow-card(-hover)` token；Toast 倒计时环改顺时针生长（修复底部
  不显示）并去掉左侧强调条；全局输入框 48px→32px；AI 设置表单统一宽度上限。

**2026-10-23（前端重构轮：ui-design 全面落地）**：按 `docs/ui-design/`（15 份）从零重写前端。

- **架构**：`src/pages/`（7 页，路由级 lazy 分包）+ `src/stores/`（9 个领域 store +
  `init.ts` 唯一事件订阅点）+ `src/components/rm/`（组件库三件套）+ `src/components/`
  （TaskCenter / LogDrawer / PageHeader / 内联 SVG 图标 47 枚）。旧 7 路由全部删除
  （`src/routes/` 清空），`app.css` 从 2253 行手写样式重写为 1100 行 token 化设计系统。
- **路由**：`/` `/instances` `/instances/:id`（hash 五 tab）`/download`（向导/Mod 仓库/
  整合包占位）`/accounts`（合并旧 /login）`/plugins` `/settings`（七分区含 #ai）；
  旧路由重定向（/login→/accounts、/mods→/download#mods、/agent→/settings#ai）。
- **设计支柱落地**：Rail 全局导航 + SideTabs 页内子导航；快速启动卡五态机；字段级继承锁
  （20 个可覆盖字段 + resolved settings 生效值）；实例上下文 Mod 仓库（锁定筛选 + 推荐版本
  置顶 + 安装兼容警告）；任务中心非模态面板（Rail 指示器 + 确认卡置顶 + 双进度 + EMA 速度）；
  日志抽屉（级别过滤 + 异常退出自动弹开 + AI 流式分析 + 外发确认）；MCP 设置面（令牌/接入
  配置复制/会话/逐工具策略/审计视图）；插件 master-detail（拖拽安装/权限/Schema 配置表单/
  逻辑调试台）；插件卡 error boundary + `plugin://push` 中继进 iframe。
- **i18n**：全新 453 key 双语 bundle（en 参考 + zh-CN 同步，typecheck 守护 key 对齐）。
- **门禁**：`pnpm typecheck` 0 错误；`pnpm build` 全绿（主 chunk 145 KB / gzip 47 KB，
  各页独立 chunk，总 gzip ~74 KB，体积预算 +150 KB 内）；`ipc_tests` 契约（registry ↔
  api.ts）47 项通过——后端零改动。
- **已知降级形态**（对应 ui-design 00 §9 后端配套清单）：open_path（P1）未实现——浏览菜单
  显示路径 + 复制；modloader 移除（P2）未开放；下载暂停事件（P3）用乐观 UI 兜底；
  downloads-card 内置卡已接（P4 由 dashboard_list_widgets 提供，若后端未注册则经「添加卡片」）；
  更新检查（P9）灰置「即将支持」；整合包（P10）纯占位。
- **待人工验收**：真机 GUI 走查（ui-design 13 §7 QA 矩阵：分辨率/主题/语言/断点/三态/
  MCP 确认流/AI 分析流式），`pnpm tauri dev` 联调。

**2026-10-23（ADR-013 S4/S5/S6 轮：MCP 服务器 + 插件 AI 桥）**：后端完成外部 Agent 控制面，
未动前端。

- **MCP 服务器**（新模块 `src-tauri/src/mcp/`）：Streamable HTTP（JSON 模式）仅回环 +
  Bearer Token（`<data>/mcp-token` 0600）；默认**关闭**，`mcp_set_enabled` 启停，端口自动挑选
  并落 `<data>/mcp-endpoint`。传输基于 hyper 1.x（reqwest 依赖树已有，启用 server feature，
  未引入 axum/tower——体积红线见下）。会话 ≤4（`Mcp-Session-Id`），每会话 60 次/分钟，
  `instance.check_completion` 全局并发 1；`initialize` 登记客户端名并发 `mcp://session`。
- **31 工具**（15 只读 / 11 操作 / 5 破坏性）：dispatch 直接调用与 GUI 同一份 `commands::*`，
  下载/启动事件天然可见；操作类另发 `mcp://call`（started/finished 归因，GUI 徽标数据源）。
  只读 `settings.get` 递归剔除形似凭据的键；破坏性 `settings.set` 字段白名单（目录/内存/并发/
  镜像等，未知字段拒绝）。工具策略逐条可覆盖（`<data>/mcp-policies.json`；破坏性默认 confirm）。
- **确认流**：破坏性工具复用 `TaskSnapshot` 两阶段（`kind=mcp:<tool>`，120s 超时即拒）；
  `mcp://confirmation` 事件 + `mcp_confirmations_list/resolve` IPC 供确认卡；审计 fail-closed
  （`agent/audit.jsonl`，ADR-010 脱敏规则，破坏性先落审计再执行）。
- **stdio 桥**：`rmccc mcp [--launch]`（src-cli，reqwest blocking）——行分隔 JSON-RPC 转发到
  HTTP 端点，捕获 `Mcp-Session-Id`；未运行返回明确错误「RMCCC 未运行，请先启动启动器」；
  `--launch` 拉起同目录 GUI（或 `RMCCC_APP_PATH`）等待端点就绪 ≤10s。
- **S6 插件桥**：能力 `ai.chat`（medium，需显式授予）+ 桥方法 `llm.chat`——
  非流式回完整文本（30s 上限），流式立即返 `streamId` 并经 `plugin://push` 中继
  `llm.delta/done/error`；每插件 6 次/分钟限流；密钥由宿主代持（`host_llm_credential`），
  插件永不接触 key 本体。桥契约四处同步（ui.rs 方法表 / schema / SDK / 契约测试），
  新增线上错误码 `rate_limited`。
- **IPC 84→93**（设计目标达成）：`mcp_*` 9 条；`mcp_*` 暂列 `ipc_tests::BACKEND_ONLY`
  白名单（设置页分区随前端重构接线）。types.ts 再生成。
- **门禁**：`cargo test --workspace` **631 passed / 0 failed**（+12：MCP 协议/工具/schema、
  桥转发）；fmt / 两条 clippy 门禁 / `pnpm typecheck` / `pnpm sdk:test`（14 项）全绿。
  **体积 8.82 MiB stripped**（hyper server 合入后 +0.64 MiB，ADR-011 < 10 MiB ✅）。
- **未做（随前端重构）**：设置页 MCP 分区与 AI 分区 UI、任务中心确认卡、`mcp://call`/`mcp://session`
  消费的徽标与 toast、`plugin://push` 的 iframe 中继、事件 payload `origin` 字段
  （当前以 `mcp://call` 替代归因数据源）。

**2026-10-23（ADR-013 迁移轮：去 Agent 化 + 文档重组）**：按
[`AI_AND_MCP_ARCHITECTURE.md`](AI_AND_MCP_ARCHITECTURE.md) 完成 S1/S2/S3/S7 后端落地，
并整理 docs 目录。

- **crate 拆分**：新增 `rmccc-llm`（`llm` 流式/重试/取消 + `probe_models` 模型探测 +
  `credentials` vault + `redact` 脱敏，31 项测试；vault 沿用 `private/agent-credentials.json`
  与 service=`llm`，旧配置无缝继承）。`rmccc-agent` 瘦身为 `mods`（依赖扫描/补装纯逻辑）+
  `audit` + `confirm`（`TaskSnapshot` 两阶段原语，供 MCP 确认流复用），删除
  `runtime`/`tools`/`session`（14 项测试）。
- **IPC 换血（86→84）**：移除 `agent_*` 9 条；新增 `ai_config_get/set`（字段可选、
  留空沿用已存值）、`ai_models_probe`、`ai_analyze_log/cancel`（日志尾部 12k 字符 →
  脱敏 → 流式事件 `ai://delta/done/error`，并发 ≤2、整体 180s 超时）与
  `mods_dependency_scan/apply_plan`（纯逻辑，Modrinth 版本匹配 + SHA1 校验）。
  `/agent` 路由重定向 `/settings`；i18n / api.ts / types.ts 同步再生成。
- **门禁**：`cargo test --workspace` **619 passed / 0 failed**；`cargo fmt` /
  两条 clippy 门禁 / `pnpm typecheck` 全绿（`ipc_tests` 计数断言 84）。
- **文档重组**：docs/ 归类为 `adr/ audits/ guides/ roadmap/ ui-design/ archive/`，
  删除 RMCLI 时代测试日志与 `old/` 迁移存根；新增 `AI_AND_MCP_ARCHITECTURE.md`（S1–S7
  实施切片）与 `ui-design/`（15 份前端重设计文档）；`ARCHITECTURE.md` §25 重写为
  ADR-013 形态、§11 IPC 表重建为 84 条。
- **未做（后续切片）**：S4/S5 MCP 服务器、S6 插件桥 `llm.chat`；设置页 AI 分区 /
  实例页「检查缺失前置」/ 日志 AI 分析面板的 UI 随整体前端重构（`ui-design/`）一并落地。

**2026-10-24（Phase 6.5 第二批轮）**：首个写工具 + 上下文范围（6.7）。

- **写工具链**：`AgentTool` 新增两阶段协议（`approval_payload` 产出审阅载荷）；
  `ToolRegistry::invoke_approved` 是写工具唯一执行通道——校验批准快照（状态 = approved、
  kind 指向同一工具、会话一致），防重放（执行后任务推进 applied/failed）、防误用（拿 A 的批准执行 B）。
  `never` 确认策略的写工具在执行循环里直接拒绝（写操作必须经用户）。
- **`mods.apply_plan`**（指南 §10.6 旗舰场景）：模型零参数触发；工具自产 dry-run 审阅载荷 →
  批准 → 执行时重新扫描实例（Core 再校验），把仍缺失的依赖交给 `PlanApplier`。
  `src-tauri` 实现 `ModrinthPlanApplier`（搜索 → 版本匹配游戏版本+加载器 → SHA1 校验安装）；
  约束字符串为 best-effort（不求解 semver，载荷里人工确认）。
- **6.7 上下文**：`run_turn_with_context`——宿主只提供所选 profile 元数据（决策 5），
  进提示词前脱敏；`TurnOutput::context_sent` + `/agent` 页"发送给模型的上下文"面板。
- **门禁**：`cargo test --workspace` **647 passed / 0 failed**（Agent 77 项）；
  fmt / 两条 clippy / typecheck / build 全绿（JS 182.00 kB）。体积 **8.18 MiB**（ADR-011 ✅）。
- **待人工验证**：`ModrinthPlanApplier` 的真实网络解析（需要真实凭据 + 网络联调）。

**2026-10-24（Phase 6.5 第一批轮）**：Agent 执行循环 + 审批链 + IPC/UI 接线（里程碑 M7 达成）。

- **接线前审计债清偿**：P2-17（home 路径脱敏从死代码接上生产调用点；`agent/`/`private/` 目录经
  新增的 `PlatformFs::restrict_owner_only_dir` 收紧到 0700；内置只读工具改为 `AgentRoots` 注入固定根目录，
  输入 Schema 变为空对象——模型不再提供任何路径）；P2-28（无元数据 jar 不再计入已安装集合，
  消除依赖漏报）；P2-29（工具审计失败 fail-closed、JSONL 追加互斥、`clear_all` 写 `agent.clear_history` 审计记录）。
  `rmccc-platform` 新增目录权限 trait 方法（macOS 0700 / unsupported / mock 三后端）。
- **6.5 执行循环**：`rmccc-agent::runtime`——`AgentService`（模型 ↔ 工具迭代、严格 ```rmccc-tool 围栏解析、
  迭代上限、取消、上下文裁剪）+ `ApprovalBroker`（任务快照 `proposed→approved/rejected` + 事件留痕 + 轮询等待）。
  8 项端到端测试（脚本化假 provider）：直接调用回灌 / 审批批准与拒绝 / 取消 / 迭代上限 / resolve 幂等。
- **IPC + UI**：`agent_*` 9 条命令（注册表 77→86）+ `/agent` 页面（模型配置 / 对话运行与取消 /
  审批队列 / 会话历史）+ Dashboard 入口。`src-tauri` 首次依赖 `rmccc-agent`。
- **门禁**：`cargo test --workspace` **643 passed / 0 failed**（Agent 73 项）；
  fmt / 两条 clippy / `pnpm typecheck` / `pnpm build` 全绿（JS 181.61 kB，gzip 52.78 kB）。
- **体积**：主可执行 **8.16 MiB stripped**（+0.25 MiB，ADR-011 < 10 MiB ✅）。
- **未做**：6.4 注册表持久化（Phase 7）、6.7 隐私上下文范围 UI、写工具注册（写操作仍被注册表拒绝）。

**2026-10-22（Phase 6.3 轮）**：Agent 会话事件流与任务快照落地（ADR-010 决策 1/2/6）。

- **6.3 Session Store**：`rmccc-agent::session`——`agent/sessions/<id>.jsonl` 的 9 种白名单事件
  （`session_started` / `user_message` / `model_response` / `tool_call` / `tool_result` /
  `approval_requested` / `approval_resolved` / `error` / `session_ended`），
  提示词与模型输出**只存 `sha256` + 字符数**，工具结果只存 `ok`/耗时/输出哈希/字节数。
- **任务快照**：`agent/tasks/<id>.json`——`TaskSnapshot`（`proposed/approved/rejected/applied/failed`），
  `PlatformFs::atomic_replace` 原子写 + 写盘前脱敏；`read_task` 对损坏文件报错而非静默丢弃计划。
- **路径安全**：`validate_id` 只允许 `[A-Za-z0-9_-]{1,64}`，`../`、`/`、`..`、空串、超长全部拒绝（session/task 同规则）。
- **保留与清除**：`purge_expired`（纯 mtime 裁剪，默认 30 天，不接定时器）、`clear_all`（删会话/任务，保留 `audit.jsonl`）。
- **脱敏补强**：URL 查询参数里的敏感名（`?token=…` / `&api_key=…`）整值打码——Mod 下载链接常带 token。
- **路径统一**：`rmccc-config::paths` 新增 `agent_dir` / `agent_sessions_dir` / `agent_tasks_dir` / `agent_audit_path`。
- **门禁**：`cargo test --workspace` **617 passed / 0 failed**（Agent 43 → **63**，新增 16 项 session 单测 + 2 项脱敏 + 2 项端到端）；
  `fmt` / 两条 clippy / `pnpm typecheck` 全绿。`cargo tree -p rmccc` 中 **0 处** `rmccc-agent`，主可执行体积仍不受影响。
- **未做**：6.4 注册表持久化（描述符/四级风险已在 6.6 就位）、6.5 Planning/Execution Loop + Approval Broker + IPC/UI、6.7 隐私 UI。

**2026-10-22（Phase 6 启动轮）**：ADR-009/010 定稿，Agent 平台 6.1 / 6.2 / 6.6 落地，新增 `rmccc-agent`。

- **ADR**：[ADR-009](adr/ADR-009-credential-storage.md)（AES-256-GCM 文件 vault；Keychain 为可替换后端）与
  [ADR-010](adr/ADR-010-agent-session-audit-privacy.md)（分层事件日志 + 写盘前脱敏）accepted。
- **6.1 LLM adapter**：`LlmProvider` trait + `OpenAiCompatibleProvider`——SSE 增量解析、总超时、指数退避重试
  （仅 429/5xx/网络）、最小间隔限流、`CancelToken`；错误模型 `LlmError` 与 `RmcccError` 分离。
- **6.2 凭据**：`CredentialStore` trait + `FileCredentialStore`；`private/agent.key`（0600，与账号 `.key` 隔离）
  + `private/agent-credentials.json`；公开视图无 `api_key`；远程仅 HTTPS、本地仅 loopback HTTP。
- **6.6 只读工具**：`ToolRegistry`（JSON Schema 校验 + 风险门禁 + 审计）、内置 `versions.installed` /
  `mods.installed` / `mods.dependency_scan`；后者解析 `fabric.mod.json` 的 `depends`/`suggests`/`breaks`，
  产出 `DependencyReport` + `dry_run` 的 `ModInstallPlan`（不写盘）。**审计 P3-2 销账**：`ModDependency` 首次有消费方。
- **审计/脱敏**：`agent/audit.jsonl` 字段白名单；`redact` 覆盖敏感键名、`Bearer`/凭据前缀、home 路径、邮箱。
- **门禁**：`cargo test --workspace` **597 passed / 0 failed**（新增 43 项 Agent 测试）；`fmt`/两条 clippy /
  `pnpm typecheck` 全绿。`rmccc-agent` 尚未被 `src-tauri` 依赖，主可执行体积不变（仍为 stripped 7.96 MiB）。
- **未做**：session store（6.3）、工具注册持久化（6.4）、Planning/Execution Loop + Approval Broker（6.5）、
  IPC/UI 入口与 Forge `mods.toml` 解析。

**2026-10-22（文档同步轮）**：把 `docs/roadmap/` 与审计台账回填到 **Phase 5 完成后的真实现状**，不涉业务代码。

- 本机复测：`cargo test --workspace` **554 passed / 0 failed**；`pnpm typecheck` ✅；`cargo metadata` **17 个 workspace 包**；
  `IPC_COMMANDS` **77 条**（前端引用 72 条）；GUI 手动 strip 后 **7.96 MiB**；`git status` 干净，HEAD `a7a4f57`。
- `roadmap/00_ROADMAP_OVERVIEW.md`：起点描述、状态锚点（成员/测试/IPC）、进度段落、里程碑 M0/M1/M2/M6 全部改为 Phase 0–5 完成口径；
  新增 Agent/发布两行欠账；`roadmap/README.md` 给 Phase 4/5 文档打 ✅ 并补"当前阶段"导语。
- `audits/AUDIT_2026-10-20.md` §15 台账：P2-5/P2-8/P2-9 销账，P2-6（72/77）与 P2-10（语义已定、入口仍缺）转部分修复，
  P2-7/P2-3 标注 2026-10-22 复测仍开；§10.5 与 §11 加"46 命令快照"横幅，指向 `handlers.rs` 单一真源。
- `DEVELOPMENT_PLAN.md`：Phase 3/4/5 状态块改为现状（loader 进度事件未订阅、CurseForge 与依赖解析仍缺、分发缺口移交 roadmap Phase 7）。

**2026-10-20（第 8 轮）**：执行 **Phase 0 基线重建**（`docs/roadmap/01_phase0_baseline.md` 切片 0.1+0.2+0.3），
产出可信基线，未做业务重构。

- **工具链**：Homebrew rustup 1.29.1 下重装 stable **1.99.1** 到 `~/.rustup/toolchains/stable-aarch64-apple-darwin`
  （原 1.98.1 已随环境回归丢失）。`~/.cargo/bin` shim 未生成，调用须显式
  `export PATH="$HOME/.rustup/toolchains/stable-aarch64-apple-darwin/bin:$PATH"`。
- **五门禁复跑全绿**：`cargo fmt` ✅、`clippy --all-targets` ✅、`clippy --lib --bins` panic/unwrap ✅、
  `cargo test --workspace` **300 passed / 0 failed**（含 1 doc-test）、`pnpm typecheck` ✅、`pnpm build` ✅。
- **工具链升级回归**：1.99 新 lint `clippy::double_must_use` 击中 5 处 async_trait trait
  （`DownloadManager`/`VersionProvider`/`AuthProvider`/`ModSource`/`ModLoaderProvider` 等），
  已加 1 处 trait 级 + 4 处 crate 级 `#[allow]`（注释"1.99.1 兼容、非业务逻辑"），零行为变更。
- **体积基线**（本机实测）：GUI `rmccc` 未剥离 10,498,088 B（10.01 MiB）/ 手动 strip 7,170,752 B（6.84 MiB）；
  CLI `rmcli` 未剥离 6,156,904 B（5.87 MiB）/ 手动 strip 4,485,120 B（4.28 MiB）。
  `strip = true` 仍失效（`rust-objcopy` SIGABRT，缺 `libLLVM.dylib`），可系统 `strip` 手动后处理。
  `<10MB` 度量对象待 ADR-011 定稿（GUI 未剥离 10.01 MiB 恰在红线）。
- **Phase 0 状态**：0.1–0.5 全部完成。0.4 平台执行散落清单由 `docs/roadmap/PLATFORM_ADAPTATION.md` 的 grep 映射覆盖；
  0.5 已在 `docs/adr/` 建立索引 + ADR-001（OS 最低版本）、ADR-011（<10MB = stripped 主可执行，**已 accepted**）、
  ADR-012（macOS Intel 策略）、ADR-002 占位。ADR-011 accepted 后 `AGENTS.md` 体积红线已同步为
  "strip 后的主可执行 < 10 MiB"；其余 ADR 仍 `proposed`。
- **Phase 0.5.1 前端契约统一（同日）**：26 处裸 `safeInvoke` 改用 `api.ts` 分域 wrapper；
  `api.ts` wrapper 全部改为 `safeInvoke` 返回 `{ok,error}`（统一错误处理）；`i18n` 3 处适配；
  `events.ts` 补 `INSTANCES_REFRESHED`；`mods` 在实例设置页以 `mods as modsApi` 别名避开局部 signal 遮蔽。
  `pnpm typecheck` ✅、`pnpm build` ✅（JS 118.59KB gzip 34.91KB）。`AUDIT` §10.2 与 P2-11 已回填。
- **Phase 0.5.2 Mod 浏览器（同日）**：新增 `/mods` 页（`src/routes/ModBrowser.tsx`）——实例选择 +
  Modrinth 搜索 + 分页 + 展开版本列表 + 安装；Dashboard 实例面板加入口。复用 `mods.search/getVersions/install`。
  类型桥补 `ModInfo` / `ModSearchResult` 的 ts-rs 导出（`u64` 字段以 `#[ts(type = "number")]` 修正为 number，
  避免生成 `bigint` 与 JSON 实际不符），`types.ts` 重新生成，`bindings.rs` 数组 24→26。
  `pnpm typecheck` ✅、`build` ✅（JS 123.80KB gzip 36.41KB / CSS 32.99KB gzip 7.17KB）；
  `cargo fmt` / `clippy` 通过。UI 可直达命令 **27 → 30**，`AUDIT` §1.1/§1.2/§10.5/P1-1/P2-6 已同步。
- **Phase 0.5.3 / 0.5.5 / 0.5.6（同日）**：Settings 页新增「账户管理」分区（列表 / 切换 / 删除 / 刷新令牌，
  复用 `auth_list/switch/remove/refresh`）与「日志」分区（读取 200 行 / 清空，复用 `log_get/clear`）；
  实例设置页新增「完整性」分区（检查 / 修复，复用 `instance_check_completion/repair`）。
  `pnpm typecheck` / `build` 通过（JS 130.64KB gzip 37.97KB）。UI 可直达 **30 → 38**；
  `AUDIT` §1.1/§1.2/§10.5/P1-2/P2-6 已同步。
- **Phase 0.5.4 本地 mod 安装（同日，方案 A）**：引入 `tauri-plugin-dialog`（`dialog:allow-open` capability），
  新增命令 `mod_pick_and_install`（Rust 侧原生文件选择器 + 复制到 `instance_mods_dir`）与
  `mod_install_local`（按路径复制，备用）；实例设置页「模组」分区加「从本地文件安装…」按钮。
  IPC 命令 **46 → 48**，UI 可直达 **38 → 39**。体积：unstripped 10.30 MiB / **stripped 7.03 MiB**
  （dialog 依赖 +0.19 MiB，仍 < ADR-011 的 10 MiB 红线）。`cargo fmt`/`clippy`、`pnpm typecheck` 通过。
  **Phase 0.5 全部切片（0.5.1–0.5.6）完成。**
- **修复：Mod 下载 `error decoding response body`（同日）**：根因是 `reqwest` 只启用
  `rustls-tls/json/stream`，**缺 `gzip`/`brotli`**，Modrinth/CDN 的压缩响应用 `.text()`/`.bytes()`
  解码时报错。已在 workspace `Cargo.toml` 加 `gzip`/`brotli` feature，并让 `download_bytes` /
  `get_download_url` 的错误保留底层 source（含 HTTP status）便于诊断。体积 strip 后 **7.03 → 7.20 MiB**
  （仍 < ADR-011 的 10 MiB）。
- **Phase 1.5 zip-slip 修复（2026-10-21，审计 P2-4）**：`rmccc-launch/src/natives.rs` 新增
  `safe_relative_entry()`，拒绝绝对路径、`..`、Windows 盘符前缀与空路径；`decompress_natives`
  只接受安全相对路径拼接目标目录，恶意条目记 warn 并跳过。新增正负向单测（`rmccc-launch` 50→51）。
  `cargo fmt`/`clippy` 通过。**Phase 1 开始（1.5 完成）**。
- **修复：Mod 安装 `TimedOut`（同日）**：`error decoding response body` 修好后暴露第二个问题——
  `rmccc-core` 的 reqwest 设了 **总超时 120s**（`.timeout`），慢速 CDN 下载大 jar 时 body 读到一半
  被 `TimedOut` 误杀（`kind: Body`）。改为 **`read_timeout(120s)`**（单次读间隔）+ `connect_timeout(15s)`，
  移除总超时：只要仍在收数据就能续传式读完，真正卡死才失败。

- **Phase 1.2 编排层收口（2026-10-21，审计 P1-3，最高优先级结构债）**：`rmccc-core::pipeline` 成为
  Tauri 与 CLI **唯一**的业务编排入口。
  - `install_game`：三类别下载合并为**单批次**（进度单调，百分比不再回弹）+ 磁盘空间预检（120% 余量）
    + natives 解压；返回 `SetupReport { total_files, failed_files }`。删除旧的 `repair_resources`
    （第三份分叉实现、0 调用方）。
  - 新增 `repair_instance`（版本 id 入口，Tauri `instance_repair` 与 CLI `resource repair` /
    `ensure_launch_ready` 共用）、`ensure_launch_ready` 改为其 profile 视图薄委托。
  - 新增 `commit_install`（重读 profile → 写 `game_version` → 刷索引）与 `refresh_repository`，
    两端共用同一提交路径。
  - `launch_game` 改为返回 `Arc<ManagedProcess>`（**不阻塞**），CLI 与 Tauri 各自只需提供
    `ProcessListener`；`last_played` 在启动成功处统一更新。
  - 顺带修掉的行为差异：GUI 安装原先**不解压 natives**；安装提交原先用几秒前的 profile 快照覆盖用户设置；
    Tauri 自定义 Java 路径不存在时要到 spawn 才失败（现立即 `JavaNotFound`）；
    `wait_for_batch` 通知早于最终进度回调（失败计数读取竞态），已把 `notify_one` 移到回调之后。
  - 代码量：删掉三处复刻（约 400 行），`version.rs` 309→203 行、`launch.rs` 309→168 行。
  - `cargo fmt` ✅、`clippy --workspace --all-targets -D warnings` ✅、`cargo test --workspace`
    **311 passed / 0 failed**（`rmccc-core` 新增 10 个编排/工具单测）、`pnpm typecheck` ✅、`pnpm build` ✅。

- **Phase 1.1a 平台适配层骨架（2026-10-21，指南 §5.7）**：新建 `crates/rmccc-platform`（第 11 个
  workspace crate），把"平台执行"从领域 crate 抽成可注入的 trait：
  - `PlatformDescriptor`：os/arch、`classpath_separator`、`exe_suffix`、`default_encoding`、
    `native_classifier_candidates`、`matches_rule_os/arch`（`osx`/`macos`、`arm64`/`aarch64` 均兼容）。
    领域层日后判断 Mojang `rules[].os` 一律经它，不再裸写 `cfg!`。
  - `PlatformFs`（`restrict_owner_only` / `make_executable` / `symlink` / `remove_symlink`）、
    `PlatformProcessControl`（`terminate_tree` / `set_priority` / `priority_command_prefix` /
    `configure_child_process_group`）、`PlatformShell`（`open_url` 白名单 http(s) + `open_path`）。
  - macOS 后端全部落地（`chmod 600/755`、`killpg`、`setpriority`、`nice` 前缀、`process_group(0)`、`open`）；
    Windows/Linux 后端为 `UnsupportedFs/Process/Shell`，一律返回 `UnsupportedPlatform`（deferred 待原生 CI）。
  - `Platform::current()` 是 crate 内**唯一**的 `#[cfg]` 选择器；`Platform::mock()` + `MockPlatform`
    可断言调用序列、可模拟单次失败，供领域层单测使用。
  - `CoreContext.platform: Arc<Platform>` 在组装层注入一次（指南 §5.7.1 第 5 条）。
  - `rmccc-error` 新增 `Validation` / `UnsupportedPlatform` 变体，`ErrorCode::UnsupportedPlatform`
    （CLI 退出码 11），`types.ts` 已重新生成。
  - **故意未做**：`CredentialStore`（指南要求先评估 Keychain；现有 AES vault 除 chmod 外与平台无关）、
    `JavaDetector`（`rmccc-launch/java.rs` 已是 per-OS 分文件，随 1.1d 提升为 trait）。
  - 门禁：`cargo test --workspace` **335 passed / 0 failed**（平台 crate 23 + doc-test 1）、
    `fmt` ✅、`clippy --all-targets -D warnings` ✅、`clippy --lib --bins -D panic -D unwrap_used` ✅、
    `pnpm typecheck` ✅、`pnpm build` ✅。
  - 下一步：**1.1b 数据筛选切换**（`rmccc-version` 去裸 `cfg!`）→ 1.1c 执行迁移 → 1.1d Java trait → 1.1e 分隔符。

- **Phase 1.1b 数据筛选去裸 `cfg!`（2026-10-21，指南 §5.7.1 第 3 条，审计 P2-12）**：
  `rmccc-version` 现在**零平台 `#[cfg]`**。
  - `libraries` / `arguments` / `compatibility` / `file_layout` / `completion` /
    `default_game_repository` 全部改收 `&PlatformDescriptor`；
    删掉 3 个 `cfg` 函数（`current_os_name`×2 / `current_arch`×2 / `minecraft_arch`）与 per-OS classifier 分支。
  - native classifier 的**唯一事实来源**收归 `PlatformDescriptor::native_classifier_candidates()`
    （含 `natives-osx` / `natives-windows-64` / `natives-unix` 等历史别名），新增
    `accepts_native_classifier` / `minecraft_arch()`。
  - 注入路径：`CoreContext.platform` → `pipeline` / `LaunchOptions.platform`（新增字段）/
    `DefaultGameRepository::with_platform`。
  - 测试改造：所有 `#[cfg(target_os)] assert!` 删除，改为显式描述符**双向断言**
    （同一份 manifest 喂 macOS / Windows 描述符，结果互补）——原先那些断言在非 macOS 上从未执行过。
  - **顺带修掉真实 bug（P2-12）**：`library_applies` 默认值反了。Mojang
    `Library.isRulesSatisfied` / HMCL `appliesToCurrentEnvironment` 是 **default-deny**，
    本项目原为 default-allow → 有 `rules` 但无命中的库也被选中。用真实 1.20.4 manifest 实测：
    修复前 macOS 选中 **88/88** 个 library（含 30 个 `natives-windows*` / `netty…:linux-x86_64`），
    修复后 **58** 个，异平台 natives 正确排除（每次安装少下 30 个无用 jar，classpath 不再被污染）。
    已加回归测试 `test_library_selection_matches_real_1_20_4_semantics`。
  - 门禁：`cargo test --workspace` **339 passed / 0 failed**、`fmt` ✅、
    `clippy --all-targets -D warnings` ✅、`clippy --lib --bins -D panic -D unwrap_used` ✅、
    `pnpm typecheck` ✅、`pnpm build` ✅。
  - 下一步：**1.1c 执行迁移**（`terminate_tree` / `restrict_owner_only` / `make_executable` /
    `symlink` / 进程组）→ 1.1d Java trait → 1.1e 分隔符。

- **Phase 1.1c 执行迁移（2026-10-21，指南 §5.7.2，真机烟测通过）**：把"随 OS 改变的系统调用"
  全部迁入平台适配层，领域 crate 只剩 `java.rs` 4 处 `#[cfg]`（属 1.1d）。
  - 进程终止：Tauri `launch_kill` / CLI `launch_stop` → `platform.process.terminate_tree(pid)`；
    `libc::kill` 与 `taskkill` 分支删除，`src-tauri` + `src-cli` 的 `libc` 依赖一并移除。
  - 进程组：`spawn_game(.., &Platform)` → `configure_child_process_group`。
  - 优先级：`nice -n N` 前缀 → `priority_command_prefix`；`ProcessPriority` 定义收归平台 crate。
  - 文件权限：`TokenVault::init(path, fs)` 与 argfile → `restrict_owner_only`（0600）；
    启动脚本 → `make_executable`（0755）。
  - 符号链接：natives 临时链接 → `symlink` / `remove_symlink`。
  - 平台数据去内联：`-Xdock:name` → `descriptor.extra_jvm_args()`（`{instance}` 模板）、
    `-Duser.home` → `is_windows()`、分隔符 → `classpath_separator()`、
    `-Dfile.encoding` → `default_encoding()`、非 ASCII natives 兜底 → `needs_non_ascii_natives_workaround()`。
  - 顺带：`decompress_natives` 按平台过滤 native jar → 安装日志里 **30 条**
    "Native library JAR not found" 噪音归零。
  - **真机烟测（macOS Apple Silicon）**：`rmcli` 新建实例 → `resource install` 1.20.4
    **3870 文件 / 0 失败 / natives 16 个**；`launch run` **启动到主菜单**
    （OpenAL 初始化「外置耳机」、全部纹理图集创建、正常 `exit_code 0` 退出）；
    `.key` 权限实测 `-rw-------`。
  - 门禁：`cargo test --workspace` **341 passed / 0 failed**（新增真实子进程
    `setpgid`+`killpg` 烟测）、`fmt` ✅、`clippy --all-targets -D warnings` ✅、
    `clippy --lib --bins -D panic -D unwrap_used` ✅。
  - 下一步：**1.1d Java 探测提升为 `JavaDetector` trait**（`rmccc-launch/java.rs` 最后 4 处
    `#[cfg]`）→ 1.1e 分隔符收尾 → 1.3 loader 单体系。

- **Phase 1.1d 平台适配层收尾 + 1.3 loader 单体系 + 1.4 context 拆分（2026-10-21，一口气完成）**
  - **1.1d Java 探测提升为 trait**：`JavaInstallation` + `JavaDetector` 迁入 `rmccc-platform::java`；
    `select_for()` 选版规则是纯函数（精确匹配 > 最近的更高版本 > `JavaVersionMismatch`）。
    macOS 后端 `java_home -V` + `JAVA_HOME` + Homebrew 路径；Windows/Linux 返回 `UnsupportedPlatform`。
    `rmccc-launch/src/java.rs` 265→80 行（只留 re-export），**最后 4 处 `#[cfg(target_os)]` 消失**。
  - **1.1e 分隔符**：随 1.1c 已完成，核对销账。**`crates/*/src` 除 `rmccc-platform` 外零平台 `#[cfg]`。**
  - **1.3 loader 单体系（P1-4）**：删 `ModLoaderProvider` / `create_provider` / 三个 `*Installer`
    与 `CoreContext.modloader_providers`；新增 `rmccc-modloader::installer`（官方安装器运行器）
    与 `DependencyManager::install_loader`（唯一路由）；新增 `rmccc-core::modloader::install_loader`
    （Tauri/CLI 都只调它）。顺带修两处不一致：CLI 原先写合并 JSON（丢 HMCL 回滚/叠加语义）；
    装完 loader 不下载自带库 → `--no-repair` 启动 `ClassNotFoundException: KnotClient`（已修，安装末尾补 8 个库）。
  - **1.4 context.rs 拆分（P2-1）**：1,450 行 → `context/mod.rs` **93 行** + 10 个域文件
    （`auth/config/profile/version/resource/modloader/mods/launch/system/helpers`，最大 253 行）。
    `CliContext` 仍是唯一入口，命令层零改动。
  - **真机验证**：`rmcli` 新建实例 → 安装 1.20.4（3870 文件 / 0 失败）→ 安装 Fabric 0.15.11
    （增量 JSON + 自动补 8 个 loader 库）→ `--no-repair` 启动到主菜单
    （Fabric Loader 0.15.11 / Mixin 0.8.5 / OpenAL 就绪）→ `modloader remove` 恢复正常。
  - 门禁：`cargo test --workspace` **356 passed / 0 failed**、`fmt` ✅、
    `clippy --all-targets -D warnings` ✅、`clippy --lib --bins -D panic -D unwrap_used` ✅、
    `pnpm typecheck` ✅、`pnpm build` ✅。
  - Phase 1 剩余：**1.6 IPC 契约测试**（`src-tauri` 2→≥46）、**1.7 ADR-002**（profile/instance 语义）、
    **1.8 生效值单一来源**。

- **Phase 1.6–1.8 完成（2026-10-21）—— Phase 1 全部切片收尾**
  - **1.6 IPC 契约测试（P2-2）**：新增 `handlers.rs`（`generate_handler!` 单一注册表 +
    `IPC_COMMANDS` 清单）与 `ipc_tests.rs`（37 测试）。覆盖三层契约：注册表一致性
    （含与前端 `src/ipc/api.ts` 交叉校验）、参数校验（mod 文件名 / URL scheme / loader 类型 /
    profile id 解析 / 日志行数 clamp）、错误码（19 个 `RmcccError` 变体映射 + `retryable` 语义 +
    `{code,message,retryable}` 形状）。`src-tauri` 测试 **2 → 39**。
    测试过程中修掉 3 个真实问题：`mod_install_local` 注册但前端从不调用（白名单+理由）；
    `sanitize_filename("../x")` 产出 `.._x`（现剥前导点、空名回落 `mod`）；
    `account_repo.remove(未知 id)` 实为幂等 no-op（按契约修正测试）。
    **已知缺口**：运行级 mock-app 测试需把 49 个命令泛型化到 `Runtime`，记为 deferred；
    前端组件测试 0 个（Phase 3 补）。
  - **1.7 ADR-002 定稿（P2-10）**：profile 为唯一用户实体，`instance_*` 按"是否依赖 profile 字段"
    分两组并逐条写明 id 语义。修掉一个真实功能缺陷：实例设置页用 profile id 调
    `instance_check_completion`/`instance_repair`，而它们按版本 id 查仓库 → 一直 `NotFound`，
    "完整性检查/修复"其实不可用；现在 profile 优先、版本 id 回落。
    `ProfileStore::delete(cleanup=true)` 增加**共享根保护**（否则会删掉所有实例的游戏文件）。
  - **1.8 生效值单一来源（P2-8/P2-9）**：新增 IPC `instance_resolved_settings`
    （`ResolvedInstanceSettings` 加 `Serialize+TS`，`types.ts` 重生成、`bindings.rs` 26→27）；
    实例设置页在字段继承时显示后端算出的「生效值」，TS 侧零继承规则。
    安装目标确认单一取值点（`profile.game_dir` == `resolved_game_root()`）。
  - 门禁：`cargo test --workspace` **394 passed / 0 failed**、`fmt` ✅、
    `clippy --all-targets -D warnings` ✅、`clippy --lib --bins -D panic -D unwrap_used` ✅、
    `pnpm typecheck` ✅、`pnpm build` ✅。CLI 烟测（`system info` / `resource check`）无回归。
  - **Phase 1 验收**：三链路单一编排 ✅、平台适配层 ✅、loader 单体系 ✅、`context.rs` 93 行 ✅、
    zip-slip ✅、IPC 契约 ✅、ADR-002 ✅、生效值单一来源 ✅。
    遗留：前端组件测试 0 个（Phase 3）、Windows/Linux 后端 deferred（release-blocked）。

- **Phase 2 声明式插件系统完成（2026-10-21）—— 2.1–2.6 + 2.7 IPC/UI**
  - **新增 `crates/rmccc-plugin`（第 12 个 crate，14 个 workspace 成员）**：
    `manifest`（严格 v1 + 12 条校验）、`version`（SemVer 2.0 + 受限范围）、`capability`（18 项能力/风险/host 策略）、
    `package`（`.rmcpkg` 包级限额与路径/类型检查）、`installer`（staging）、`registry`（11 状态机）、
    `permissions`（授予/撤销/每次复核/审计）、`config`（schema 子集 + 版本迁移）、`manager`（窄门面）。
  - **ADR-003 / ADR-004 accepted**：包格式 = 无加密 ZIP；`plugin-state/registry.json` 为唯一状态来源
    （**不建 `enabled.json`**）；Phase 2 无签名通道，安装一律 `trust=development` 并显示风险提示；
    `ui/`、`logic/` 与脚本/可执行文件在包校验阶段拒绝。
  - **安装事务**：staging → 校验 → `rename` 原子提交 → 原子写注册表/权限；失败删除新版本目录并恢复快照；
    升级保留 `previousVersion` 与旧目录；`plugin rollback` 交换版本并重新计算待审权限。
    升级**不自动扩张任何权限**，新权限进入 pending，插件保持 `installed_disabled`。
  - **损坏隔离**：损坏的 `registry.json`/插件 manifest 只产生诊断并备份 `.bak-<ts>`，
    `PluginManager::load` 永不返回 Err，其它插件与主应用照常启动。
  - **平台归属**：新增 `PlatformFs::atomic_replace`（Unix rename / Windows 待 ReplaceFile），
    插件 crate 零 `#[cfg]`；`PlatformDescriptor::plugin_platform_id()` 统一平台标识。
  - **CLI**：`rmcli plugin list|inspect|install|enable|disable|remove|rollback|grant|revoke|
    config|capabilities|pack`（13 个子命令）；真实烟测覆盖 pack→install→pending→grant→enable→config。
  - **IPC + UI（额外切片 2.7）**：15 条命令（注册表 49→64）；`/plugins` 页面支持安装/状态、
    权限审查（风险/作用域/授予/撤销）、依赖状态、配置编辑、回滚、卸载与诊断。
  - **样例**：`examples/plugins/theme-midnight`（纯主题）与 `server-dashboard`
    （卡片 + 设置页 + 受限 `network.request`）；`example_plugins_pack_and_install` 锁定真实样例可安装。
  - 门禁：`cargo test --workspace` **468 passed / 0 failed**（新增 74 项插件测试）、`fmt` ✅、
    `clippy --all-targets -D warnings` ✅、`clippy --lib --bins -D panic -D unwrap_used` ✅、
    `pnpm typecheck` ✅、`pnpm build` ✅（JS 150.42 kB / gzip 43.20 kB；CSS 37.47 kB / gzip 8.01 kB）。
  - 遗留（明确 deferred）：签名/可信目录（Phase 7）、隔离 UI（Phase 4）、逻辑运行时（Phase 5）、
    插件仓库与依赖自动下载（Phase 7）、Windows/Linux 安装事务实测（原生 CI）、前端组件测试（Phase 3）。
  - **下一步**：Phase 3（Dashboard 卡片与主题引擎），先建 ADR-005（Layout Schema / 冲突算法）。

- **Phase 3 Dashboard 布局与主题引擎（2026-10-21）—— 3.1–3.5 完成**
  - **新增 `crates/rmccc-dashboard`**：布局 Schema + 确定性归一化算法（去重/尺寸钳制/
    换行/排序/push-down/压缩）、Widget Registry（内置 `org.rmccc.core-ui` + 插件卡片）、
    布局持久化（损坏备份 + 默认恢复）、主题令牌与安全语义色边界。17 项单测。
  - **ADR-005 accepted** + `schemas/dashboard-layout.schema.json`：
    12 列网格、`(x,y,w,h)` 逻辑坐标、断点 wide/medium/compact（12/8/4 列）。
  - **9 条 IPC**：`dashboard_get_layout/save_layout/reset_layout/list_widgets/list_themes/
    get_theme_tokens/set_theme_preference/get_theme_preference/get_theme_json`；
    `UserSettings` 新增 `theme` 偏好（light/dark/跟随系统/插件主题 key）。
  - **前端迁移**：Dashboard 从固定 240px/1fr/260px 三列改为注册表驱动 CSS Grid；
    三块内置面板成为 `org.rmccc.core-ui` Widget；支持拖放、宽度循环、隐藏/恢复/重置、
    添加插件卡片；`ErrorBoundary` 隔离单卡片失败，卸载插件卡片显示保位占位。
  - **主题令牌**：`--rmccc-*` 新令牌 + 桥接旧 `--surface-*`/`--text-*`；
    内置 light/dark + 插件 `contributes.themes`；安全色由宿主控制，主题不可覆盖。
  - 门禁：`cargo test --workspace` **485 passed / 0 failed**（新增 17 项 Dashboard 测试）、
    `fmt`/`clippy` ✅、`pnpm typecheck` ✅、`pnpm build` ✅（JS 163.64 kB / gzip 47.35 kB；
    CSS 41.12 kB / gzip 8.55 kB）。
  - 遗留：插件卡片真实渲染在 Phase 4（当前占位）；高 DPI/Windows WebView2 差异 deferred；
    前端组件测试仍为 0。

- **修复：启动后"透明窗口"（前端完全不渲染，2026-10-21）**
  - **现象**：打开 `RMCCC.app` 后窗口是透明矩形——无标题栏、无内容，`logs/` 里也没有任何前端 IPC。
  - **根因（实证）**：`.app` 内的可执行文件**没有嵌入任何前端资源**（`strings` 查不到
    `/assets/index-*.js` 键），WebView 拿到空 asset map，`index.html` 与模块脚本全部 404。
    成因是 `tauri-build` 只对 `tauri.conf.json` / capabilities / 资源发 `rerun-if-changed`，
    **不监听 `frontendDist`**：早先一次 `dist/` 尚未生成时的 `cargo build` 把"空 asset map"
    编译进了 `rmccc`，之后的 `tauri build` 因无 Rust 源码变更不再重编译，直接复用了那个二进制。
  - **结构修复**：`src-tauri/build.rs` 增加 `cargo:rerun-if-changed=../dist`（+ `dist/index.html`），
    dist 变化必然触发 `rmccc` 重编译。
  - **失败可见化**（同类问题不再表现为"透明窗口"）：
    - 新增 `src-tauri/src/bundle.rs`：启动时用 `asset_resolver()` 校验 `index.html` 及文档引用的
      `src=`/`href=` 资源是否都在 asset map 中；缺失则写 `ERROR` 日志，并把窗口导航到内联
      `data:text/html` 错误页（列出缺失清单 + `pnpm tauri build` 提示）。用导航而非 `eval`：
      `index.html` 缺失时 WebView 没有可求值的脚本上下文。
    - `index.html` 内联 boot guard（在 bundle 之前、不依赖模块加载）：捕获资源加载失败、
      未捕获异常、未处理 Promise、4s 未挂载，绘制可读错误面板，并通过新增 IPC
      `frontend_log`（注册表 73→74）写入 `logs/rmccc.<date>.log`。
    - `src/main.tsx` 在 `render()` 后派发 `rmccc:shell-mounted`，guard 收到后撤下错误面板。
  - **验证**：移走 `dist/` 后 release 重编译 → 窗口显示"内置前端资源不完整"错误页，
    日志有 `Embedded frontend bundle is incomplete`；恢复 `dist/` 重编译 → UI 正常渲染、
    `version_list` 等 IPC 正常。前端 guard 另在 Safari 用"故意改错 JS 文件名"的 dist 验证
    （面板显示具体 404 URL）。
  - 门禁：`cargo test --workspace` **497 passed / 0 failed**（新增 6 项 bundle 校验测试 +
    3 项 `frontend_log` 契约测试）、`fmt` ✅、`clippy --all-targets -D warnings` ✅、
    严格 clippy ✅、`pnpm typecheck` ✅、`pnpm build` ✅。
  - 体积：GUI 未剥离 11.39 MiB / 手动 strip **7.79 MiB**（ADR-011 仍通过）。

- **Phase 5 逻辑运行时（2026-10-21）—— 5.1–5.5 完成**
  - **5.4 包与 Manifest 开放逻辑入口**：`entry.logic` 从"拒绝"改为校验后允许
    （必须 `logic/` 下的单个 `.wasm`、≤2 MiB，禁止原生库/脚本）；安装器对
    `entry.logic` ↔ `logic/` 目录做双向印证（缺文件、孤儿资源都拒绝）。
  - **5.5 第一个只读扩展 + 权限门控**：新增能力 `logic.invoke`（medium，必须显式授予）；
    `plugin_logic_invoke` / `plugin_logic_cancel`（IPC 75→77）每次调用依次检查
    启用状态 → 能力 → 调用白名单（v1 只有 `compat.check`、`mods.source.search`）→ 入口/路径；
    调用在阻塞线程上跑，超时由 helper 进程 kill 实现；取消 = 杀掉该插件的 helper。
  - **测试**：插件 106 项（新增逻辑包校验/升级/撤销）、Tauri 57 项（新增调用白名单、
    错误映射、能力目录、**真实 helper 的安装→授权→执行→取消 E2E**）、逻辑运行时 17 项。
  - 门禁：`cargo test --workspace` **554 passed / 0 failed**、`fmt`/`clippy --all-targets -D warnings`/
    严格 clippy ✅、`pnpm typecheck` ✅、`pnpm build` ✅（JS 171.71 kB / gzip 50.01 kB）。
  - 体积：主可执行 **7.91 MiB stripped**（+0.06 MiB；`strings` 确认 wasmi crate 未进主程序（唯一命中是 `.wasm` 扩展名字符串）），
    helper（wasmi）作为独立产物计 2.25 MiB，两者合计仍低于旧版 ADR-011 单二进制预算。
  - 遗留：helper 打包/签名/三平台 runner 在 Phase 7；逻辑插件暂无官方样例与 UI 入口。

- **Phase 5 逻辑运行时（2026-10-21）—— 5.1–5.3 完成**
  - **ADR-007 accepted（基于实测）**：PoC（`poc/logic-runtime/`，独立 crate，不进 workspace）
    在 macOS ARM64 release/strip 后测得：baseline 0.29 MiB、wasmi **+1.96 MiB**、
    wasmtime/Cranelift **+5.89 MiB**；wasmi 调用 0.047 µs / RSS 2.9 MiB、
    wasmtime 调用 0.055 µs / RSS 6.4 MiB；独立进程 spawn 0.3–0.9 ms、stdin 往返 8.5 µs。
    主程序只剩 2.15 MiB 余量（ADR-011）→ 嵌入式 wasmtime 直接出局，嵌入式 wasmi 只剩 0.2 MiB
    且没有崩溃隔离 → **决策：独立 helper 进程 + wasmi 解释 core WASM 模块**。
  - **新增 `crates/rmccc-logic`**（第 14 个 crate）：ABI 常量（只允许导入 `rmccc.log`，
    导出 `rmccc_alloc`/`rmccc_invoke`）、行分隔 JSON-RPC、`LogicLimits`、
    `LogicHost` 监管者（spawn/握手/每调用墙钟超时→kill/restart/崩溃分类）。
  - **新增 `src-logic-host`**（第 15 个 workspace 成员，唯一链接 wasmi 的二进制）：
    实例化前拒绝非白名单导入、`StoreLimits` 限制线性内存、`consume_fuel(true)` 计量 fuel、
    每次载荷拷贝做边界检查、插件日志转发到 stderr。
  - **测试**：8 项协议单测 + 9 项**真实 spawn** 集成测试（握手、echo 往返、未知插件、
    禁用导入、fuel 耗尽后 helper 存活、64 MiB 内存被拒、超时 kill + 重启恢复、协议版本）。
  - 主可执行文件体积不变（wasmi 只被 helper 链接）；helper 分发/签名留待 Phase 7，
    当前通过 `RMCCC_LOGIC_HOST` 或同目录解析。
  - 门禁：`cargo test --workspace` **542 passed / 0 failed**、`fmt`/`clippy --all-targets -D warnings`/
    严格 clippy ✅。
  - 待做：`entry.logic` 包校验 + `logic.invoke` 能力接线 + 第一个只读扩展（5.4/5.5）。

- **修复：Dashboard 一直处于"编辑态"，卡片点不动（2026-10-21）**
  - **现象**：鼠标移到卡片上就出现一条白色卡片头，整页无法点击/无法交互插件卡片。
  - **根因**：卡片头 `position:absolute; z-index:5; opacity:0` 仍然参与命中测试——
    透明不等于不可点；它又只在 `:hover` 时才可见，于是看起来"一放上去就变白"。
  - **修法**：改成**显式编辑模式**。视图态根本不渲染卡片头、不接受 drag/drop；编辑态才渲染
    `⠿`/`↔`/`×` 与「添加/恢复/重置」，并给槽位加虚线框与 `pointer-events:none`（否则
    iframe 会吞掉拖放）。工具栏新增「编辑布局 / 完成」开关（`aria-pressed`），
    两态各有提示文案。
  - 门禁：`pnpm typecheck` ✅、`pnpm build` ✅（JS 171.71 kB / gzip 50.01 kB；
    CSS 42.47 kB / gzip 8.76 kB）；真实 `.app` 中验证视图态可点击、编辑态可拖动/调整/隐藏。

- **Phase 4 隔离 UI 扩展与 SDK（2026-10-21）—— 4.1–4.5 完成**
  - **ADR-006 accepted**：容器 = 主窗口内 `<iframe sandbox src="rmcccplugin://<id>/…">`（独立源，
    不是独立 WebView：卡片要进网格）；协议 = 版本化 postMessage（`UI_API_VERSION = 1`）；
    资源 = 自定义 URI 方案只服务 `ui/` 白名单；授权 = 每条消息重新判定。
  - **新增 `crates/rmccc-plugin/src/ui.rs`**：方法表（`host.info` / `theme.get` / `settings.get` /
    `instances.list` / `network.request`，各自能力+超时）、12 个线上错误码、请求解析
    （≤64 KiB/形状/版本/方法/ID）、资源白名单、严格 CSP（`connect-src 'none'`）。16 项单测。
  - **包与 Manifest 开放 UI**：`ui/` 目录与 `entry.ui` 从"拒绝"改为"校验后允许"（必须位于 `ui/`、
    `.html`、文件存在、且与 `dashboardCards`/`settingsPages` 互相印证）；`logic/` 仍拒绝。
  - **宿主桥 `plugin_ui_invoke`（注册表 74→75）**：接收 iframe 的**原始** JSON 重新解析，
    逐条 `ensure_enabled` + `authorize`（`network.request` 以目标 host 为作用域），
    返回带线上错误码的 `BridgeReply`；`network.request` 用独立 HTTP client（禁重定向、30 s、2 MiB）。
  - **`rmcccplugin://` 方案处理器**：只服务该插件 `<install>/ui/` 内白名单文件，
    HTML 带 CSP 与 `nosniff`；未启用/无 `dashboard.cards`·`ui.pages` 一律 403。
  - **前端**：`src/plugin-ui/bridge.ts`（source/origin/大小/并发/超时/错误转发）+ `PluginFrame.tsx`
    （加载/错误/就绪三态、8 s 未 ready 判失败、主题变更推送）；`WidgetDescriptor.ui_url` 驱动
    Dashboard 自动选择 iframe 或声明式卡片。
  - **SDK `plugin-sdk/`**：`@rmccc/plugin-api`（线协议类型 + 方法表）、`@rmccc/plugin-ui`
    （运行时 client，纯 ESM/JSDoc）、`templates/dashboard-card`、`scripts/cli.mjs`
    （`validate`/`pack`，与安装器同规则）、`docs/`（入门/API/安全/发布）、
    **跨工件契约测试**（schema ↔ Rust ↔ SDK 的方法表与错误码必须一致）。
  - **样例**：`examples/plugins/server-dashboard` 升级为隔离 UI 卡片并通过桥读取实例/设置；
    同时在真实 `.app` 中验证了"未授权 → 拒绝错误态 → 授权 → 正常数据"的完整链路。
  - **顺手修掉 Phase 2 的一个权限漏洞**：升级新增低风险能力时既不在 granted 也不在 pending
    （插件能启用但永远拿不到），`pending` 已改为"所有已声明未授予"，升级仍不自动授权。
  - **4.6 收尾（同日）**：桥新增插件**主动取消**（`{type:"cancel", id}`，宿主丢弃 pending、
    迟到结果丢弃；SDK 用 `AbortSignal` 暴露），补 9 项 SDK 传输测试（ready/请求配对/错误映射/
    版本忽略/超时/取消/cancel(id)/init/revoke），契约测试增为 5 项。
  - 门禁：`cargo test --workspace` **525 passed / 0 failed**、`fmt`/`clippy --all-targets -D warnings`/
    严格 clippy ✅、`pnpm typecheck` ✅、`pnpm build` ✅（JS 170.21 kB / gzip 49.59 kB；
    CSS 42.15 kB / gzip 8.71 kB）、`pnpm sdk:test` **14 项** ✅。
  - 体积：GUI 未剥离 11.53 MiB / 手动 strip **7.85 MiB**（ADR-011 仍通过）。
  - 遗留（deferred）：Windows/Linux 隔离 UI 行为（release-blocked）、插件页面（settingsPages）的
    iframe 路由、`plugin:dev` 热更新（与"不信任任意路径"冲突）、前端组件测试仍为 0。

**2026-10-20（第 7 轮）**：产出**全量架构与审计文档** [`audits/AUDIT_2026-10-20.md`](audits/AUDIT_2026-10-20.md)，
并修正本文档与 `ARCHITECTURE.md`、`DEVELOPMENT_PLAN.md` 中过期的数字。

审计新增的关键发现（此前未记录）：

1. **编排层被绕过**：`pipeline` 的 `resolve_version` / `repair_resources` / `launch_game` 均为**零调用方**；
   Tauri 的 `version_install`、`instance_repair`、`launch_start` 各自复刻了一遍流程（只有
   `ensure_launch_ready` 被两端共用）。
2. **两套"实例"概念并存**：`profile`（UUID，前端实例卡片）与 `instance`（版本 id，`versions/{id}/`）。
   `instance_rename/duplicate/remove` 操作的是后者，这解释了它们为何在前端零调用。
3. **环境回归**：Homebrew 升级 rustup 后 `/opt/homebrew/opt/rustup/toolchains/` 失效
   （实体在 `Cellar/rustup/1.29.1/toolchains/…`），且 `~/.cargo` 已不存在
   → §7.3 记的工具链路径已过期；当前 `cargo test` 需联网重建依赖缓存。
4. 数字校正：IPC **46**（非 42/45/48）、UI 直达 **27**、前端 **17 文件 4,279 行**、
   测试 **299 个 `#[test]`**（上次全量 300 passed）。

**2026-10-10（第 6 轮）**：实现**每实例独立设置**（参考 HMCL），含实例卡片入口与全局/实例分层。

**调研结论（修正了一个常见误解）**：HMCL 的"全局游戏设置"**不在 `LauncherSettings` 里**，而是可多套的
"游戏设置预设"（`config/game-settings.json`），实例通过 `parent` 引用其一；40 个游戏字段全部是
`InheritableProperty`（默认继承、可显式覆盖）。"继承 vs 覆盖"的唯一真相是
`Instance.overrideProperties` **字段名集合**——值与该标志分开存，因此来回切换不会丢值。

**落地设计**（三点，与 HMCL 一致或有据可循）：

| 层 | 类型 | 存储 |
|----|------|------|
| 全局默认值 | `AppConfig`（新增 jvm/game args、isolation、窗口、优先级、auto_memory） | `config/launcher-settings.json` |
| 实例值 + 覆盖名单 | `Profile`（新增 `override_properties`、`isolation`） | `profiles/{id}.json` |
| 生效值 | `ResolvedInstanceSettings`（`Profile::resolve`） | 不落盘 |

- **值不丢**：`override_properties` 不含某字段时该字段继承全局，但实例里已填的值仍留在磁盘上；
  已有 profile JSON 无此字段 → 全部继承，且原值保留（新增测试覆盖）。
- **隔离两层**：`isolation`（never/modded/always，默认 modded）决定**运行目录**；
  共享游戏根（versions/libraries/assets）始终不变。`rmccc_launch::instance_run_dir` 是唯一推导入口，
  启动与模组命令共用，杜绝"安装目录 ≠ 启动目录"重现。模组目录随运行目录走，隔离实例因此拥有独立 mods。
- **与 HMCL 的差异（有意）**：只有一套全局默认值（不做多预设）；实例这半存在 `profiles/{id}.json`
  而非 `{instance_root}/.hmcl/config/`——RMCCC 的实例注册表就是 ProfileStore，不扫描 `versions/`。

**后端新增**：`instance_paths`（报告生效的游戏根/运行目录/模组目录）、`ModList{enabled,disabled,mods_dir}`
（此前只返回启用的模组，禁用的一半看不见）、`ProfileUpdateOpts` 覆盖全部字段、
`SettingsUpdate` 覆盖全部全局默认值、`LoaderVersion` 导出为 TS 类型。

**前端**：实例卡片新增齿轮按钮 → 路由 `/instances/:id`；设置页新增"全局游戏默认值"分区；
`api.ts` 去掉手写的 `SettingsUpdate`/`ProfileUpdateOpts`（它们此前**遮蔽**了 ts-rs 生成的同名类型，
字段一变就报错——本次正是被它绊到），改为 `Partial<生成类型>`。

| commit | 内容 |
|--------|------|
| `5ef603b` | feat(config,profile)：全局默认值 + 实例覆盖模型 |
| `dfea619` | feat(launch,...)：消费生效值 + 共享根/运行目录分离 |
| `79a0054` | feat(...)：每实例模组目录随运行目录 |
| `d3e9a3c` | feat(tauri)：实例与全局设置的完整接口面 |
| `883f985` | feat(tauri,ui)：实例设置入口接线 |
| `4f92579` | test(launch)：隔离运行目录与生效值测试 + 文档 |

**2026-10-10（第 5 轮）**：修复"能下载但打不开游戏"，并完成创建实例的版本选择闭环。

用户实测症状：前端能创建实例、能下载，但打不开游戏。根因是**三个独立 bug 叠加**，全部修复并实机验证。

1. **安装目录 ≠ 仓库根目录（阻断）**：`profile_create` 把 `game_dir` 设为
   `{game_root}/{profile_id}`，安装写进那里；而 `DefaultGameRepository` 索引的是共享
   `{game_root}`，启动链路经 `get_resolved()` 解析版本。于是日志里装完 5262 个文件仍然
   `instances=0`，启动在第一个阶段就失败。现统一为共享游戏根（`IsolationType` 只影响运行目录），
   并新增旧布局迁移把 `instances/{id}/versions|libraries|assets` 并入共享根。
2. **response file 竞态（阻断）**：文件在 `spawn()` 后**立即删除**，而 JVM 是在解析命令行时才读
   `@argfile`，两者竞争 → 游戏以 `could not open .../.launch-args-*` 退出（exit 1）。
   现改为交给后台任务在 JVM 读完参数后删除，并在下次启动清扫陈旧文件。
3. **argfile 混入 java 可执行文件（阻断）**：`Command::jvm_args` 的第一项是 java 路径（其文档只说是
   "JVM arguments"），于是 response file 首行成了 java 路径，被 JVM 当成主类 →
   `could not find or load main class /Library/.../bin/java`。现由两个消费方各自提供可执行文件
   （`spawn_game()` 作为程序、`make_launch_script()` 前置）。

**实机验证**（临时数据目录 + 软链真实 SHA1 缓存）：

```
Game repository refreshed game_dir=.../instances instances=1        ← 修复 1
[Render thread/INFO]: Setting user: E2ETester                       ← 修复 2+3
[Render thread/INFO]: Backend library: LWJGL version 3.4.3+4
[Render thread/INFO]: Using graphics device: Apple M1 (Apple)
[Render thread/INFO]: Created window using SDL video driver: cocoa
```

（离线账号的 `Failed to fetch user properties ... 401` 属预期，不影响启动。）

**前端**：创建实例的弹窗新增**必选**游戏版本下拉（默认正式版，可勾选包含快照），
名称或版本为空时 Create 按钮禁用；创建成功后立即为该实例下载所选版本。

**$schema 命名空间**改为 `https://rmcl.snowebsite.top/json/schema/...`，旧 HMCL 命名空间仍可读，
并在**加载时**即被改写（不再等用户改设置）。已确认本机 `launcher-settings.json` /
`user-settings.json` 均已迁移且设置值完整保留。

| commit | 内容 |
|--------|------|
| `049cc26` | feat(config)：schema 命名空间迁移 |
| `15f1cf3` | fix(core,tauri,cli)：安装到共享游戏根 + 旧布局迁移 |
| `4046bfa` | fix(launch)：response file 交付修复 |
| `36c2234` | feat(ui)：创建实例必须选版本并立即下载 |
| `2270d8c` | feat(config)：加载时改写旧 `$schema` + 文档同步 |

**2026-10-10（第 4 轮）**：修复窗口外壳与交互缺陷，并新增语言设置（English / 简体中文）。

**窗口外壳缺陷**（用户实测反馈：无法拖动、启动在角落、四周有缝隙）根因三条，均已修复：

1. **无法拖动/没有窗口按钮**：`TitleBar.tsx` 与 `.app-root` 的 CSS 早已写好，但 `main.tsx` 只渲染了
   `<Router>`，**从未挂载过它们**。改为 `Router root={App}`，shell（标题栏 + 圆角容器 + toast 层）常驻。
2. **启动在角落 + 尺寸异常**：`tauri-plugin-window-state` 恢复了一份过期几何
   （**2200×1426 @ (-760, 892)**，而显示器只有 **1440×900**）——用户只看到一个巨大窗口的一角。
   已把该状态文件备份为 `.window-state.json.stale-20261010`，并在 `App.tsx` 增加启动时的
   显示器范围校验（不可达则 `center()`）。
3. **四周缝隙**：路由根元素的 `flex: 1` 找不到 flex 父容器（`.app-root` 未挂载）→ 内容塌缩、
   透明窗口露出桌面。挂载 shell 后即恢复设计意图。

顺带修复：窗口 `visible: false` 却**无人调用 `show()`**——此前仅靠 window-state 里存的
`visible: true` 侥幸可见，删掉那份状态后窗口将永不出现。现由 `App.tsx` 在 `initI18n()` 后显式
`show()`（ready-to-show）。另外 `ToastContainer` 也从未挂载，所有 `showToast` 都不可见，一并补上。
新增 capability：`core:window:allow-center`、`core:window:allow-available-monitors`。

**语言设置**：见 §9.5（`ARCHITECTURE.md`）。`UserSettings.locale` 此前从未被 IPC 读写，
现补齐 `settings_get_user` / `settings_set_user` / `settings_supported_locales` 三个命令。
默认跟随系统语言，系统语言不在支持列表内（如 `fr`/`ja`）回退英语。

> 注：窗口外壳修复与语言功能落在同两个 commit（`6609fa4` 含 capability，`371b0e1` 含 shell），
> 未单独成一个 commit。

| commit | 内容 |
|--------|------|
| `6609fa4` | feat(config,tauri)：用户级设置与 UI 语言后端 |
| `371b0e1` | feat(ui)：i18n（English / 简体中文）+ 窗口外壳修复 |
| `b7f7150` | docs：i18n 管线与新命令 |

**2026-10-10（第 3 轮）**：**第 0 步地基全部完成**——CI 的 lint 三门禁已本地跑通并修复，文档漂移已同步。

- 补齐本地工具链：`rustfmt` / `clippy` 组件（清华镜像缺这两个组件，改用 `static.rust-lang.org`
  的 1.98.1 组件包装入现有工具链）。
- CI lint job 原本**三项全红**，现已全绿：
  - `cargo fmt --all -- --check`：27 个文件未格式化 → 已 `cargo fmt --all`。
  - `cargo clippy --workspace --all-targets -- -D warnings`：15 处 lint（`derivable_impls`、
    `unnecessary_to_owned`、`needless_borrows_for_generic_args`、`question_mark`、
    `field_reassign_with_default`）→ 已修。
  - `cargo clippy --workspace --lib --bins -- -D warnings -D clippy::panic -D clippy::unwrap_used`：
    **9 处生产代码 `unwrap()`**（`MicrosoftAccount` 的 session mutex）→ 改为
    `session_lock()` / `session_snapshot()`，从锁中毒恢复而非 panic。
- 文档漂移已修：`ARCHITECTURE.md` 的 8 类漂移（§2 目录树、§4.1 加密、§7 路径、§8.3 下载粒度、
  §9.1 路由、§10.3 认证流、§11 命令表 31→42、§15.1 数据目录）已对齐实现；
  `DEVELOPMENT_PLAN.md` 已为 5 个 Phase 回填状态块；4 份历史文档已加「已被取代 / 历史快照」横幅。

| commit | 内容 |
|--------|------|
| `9ed6cf0` | fix(lint)：通过 fmt / clippy / 生产 unwrap 三门禁 |
| （本轮 docs） | 同步 ARCHITECTURE.md、DEVELOPMENT_PLAN.md 与历史文档横幅 |

**2026-10-10（第 2 轮）**：§3、§4 记录的 CLI 问题**已全部修复**，工作区**已按主题提交为 10 个 commit**。

| commit | 内容 |
|--------|------|
| `9d3d496` | chore(build)：工具链、CI/release 工作流、依赖清单 |
| `3f9c21e` | feat(auth,config)：账户与配置持久化对齐 HMCL |
| `59b86a7` | feat(version,download)：版本管理本地磁盘中心化 |
| `b4bcb09` | feat(launch,profile)：启动模块按 HMCL 结构重写 |
| `050de4d` | feat(modloader,mods,error)：加载器 provider 与 Modrinth |
| `7618ee0` | refactor(core)：编排抽取为 pipeline + 真 DI |
| `27b2447` | feat(tauri)：IPC 命令面补齐 + ts-rs 绑定 |
| `3840337` | feat(cli)：命令面重建 + 统一输出信封（含下列修复） |
| `9f59d23` | feat(ui)：SolidJS 前端接线 |
| `6d93d52` | docs：审计、整改台账、运维手册与现状交接 |

> 说明：§3 / §4 保留为**问题溯源记录**，其中"现象/根因/修法"仍有价值；"未修复"的措辞已不代表当前状态。

---

## 1. 结论速览

| 维度 | 实测结论 |
|------|----------|
| 编译 | `cargo check --workspace --all-targets` **通过**；clippy 无 warning |
| 前端类型 | `pnpm typecheck` **通过** |
| CI lint 三门禁 | `cargo fmt --check` / `clippy --all-targets -D warnings` / `clippy --lib --bins -D clippy::panic -D clippy::unwrap_used` **全部本地通过** |
| 测试 | **617 passed / 0 failed**（2026-10-22 全量复跑；插件 106 项 + Dashboard 17 项 + 逻辑运行时 8 单测与 9 项 helper spawn 集成 + Agent 63 项 + bundle 6 项）；`pnpm sdk:test` **14 项**全过 |
| 版本控制 | HEAD = `a7a4f57`；工作树干净（Phase 0–5 均已按主题提交） |
| 发布体积 | GUI 未剥离 12,139,832 B（11.58 MiB）/ 手动 strip **8,346,816 B（7.96 MiB）**；CLI 未剥离 6,523,016 B / 手动 strip 4,701,256 B（4.48 MiB）；逻辑 helper（独立）**2.25 MiB**（ADR-011：主可执行 stripped < 10 MiB ✅；`strip = true` 仍失效，P2-7） |
| 代码规模 | Rust **200 文件 / 50,872 行**（含 `rmccc-logic`、`src-logic-host` 与 `rmccc-agent`）；前端 22 文件 / 7,171 行；SDK 19 文件 / 1,306 行（详见 [`audits/AUDIT_2026-10-20.md`](audits/AUDIT_2026-10-20.md)） |
| 文档同步 | `ARCHITECTURE.md` 新增 §25（Agent 平台）；ADR-002–007/009/010/011 accepted，ADR-001/012 仍 proposed |
| 阶段判断 | **Phase 0–5 完成；Phase 6 进行中**（6.1 LLM adapter / 6.2 凭据 / 6.3 会话事件流 / 6.6 只读工具完成；6.5 loop+审批 与 6.7 隐私待做）；Phase 7 未开始 |

**最重要的三个判断**：

1. **结构债已清偿**：pipeline 单一编排、平台适配层、loader 单体系、`context.rs` 93 行、IPC 契约测试、ADR-002 生效值单一来源——Phase 1 全部验收项完成。
2. **插件平台第一阶段已交付**：Manifest v1 + `.rmcpkg` 事务 + 生命周期 + 权限/审计 + 配置 schema + CLI + `/plugins` 页面；损坏插件隔离、安装失败回滚、升级不扩张权限均有测试。
3. **剩余缺口是"生态与分发"而非地基**：Agent 平台（Phase 6）、签名/目录/公证（Phase 7），以及前端组件测试（当前 0）。Dashboard 布局/主题（Phase 3）、隔离 UI（Phase 4）、逻辑运行时（Phase 5）均已交付。

---

## 2. 本次实测验证结果

执行环境（本机 macOS / Apple Silicon，Homebrew rustup，工具链 rustc 1.98.1）：

```bash
export PATH=/opt/homebrew/opt/rustup/toolchains/stable-aarch64-apple-darwin/bin:$PATH
export CARGO_HOME=$HOME/.cargo
```

| 命令 | 结果 |
|------|------|
| `cargo check --workspace --all-targets` | ✅ 通过，1 warning：`src-cli/tests/cli_integration.rs:68` 未使用变量 `h` |
| `cargo test --workspace` | ❌ 277 passed / 7 failed（详见 §3） |
| `pnpm typecheck` | ✅ 通过 |
| `cargo build --release -p rmccc-cli` | ✅ 通过，1 warning（`strip` 失败，环境问题） |
| `cargo fmt --check` / `cargo clippy` | ⛔ 无法执行——工具链未安装 `rustfmt` / `clippy` 组件（见 §7.2） |

crate 级单测（全部通过）：`auth 31`、`config 32`、`launch 48`、`modloader 36`、`version 58+2`、`download 18`、`mods 4`、`profile 3`、`core 3`、`cli lib 16`、`src-tauri 1(+1 E2E 默认跳过)`、`rmccc-error 0`。

---

## 3. 当前唯一的硬阻断：CLI 每次调用都 panic（已修）

**现象**：`rmcli` 任何参数都以 exit code 101 panic：

```
Command rmcli: command name `help` is duplicated
  at clap_builder-4.6.2/src/builder/debug_asserts.rs:353
```

**根因**：`src-cli/src/app.rs` 的顶层 `Commands` 枚举定义了自定义 `Help` 变体（`app.rs:87-91`），与 clap 内建的 `help` 子命令重名；clap 的 debug 断言在 debug 构建下直接 panic。

**已做修改（1 行 + 注释）**：在 `Cli` 上追加 `#[command(disable_help_subcommand = true)]`
（[`src-cli/src/app.rs:12-17`](../src-cli/src/app.rs#L12-L17)），使自定义 `help` 子命令取代内建命令。

**效果**：测试失败数 **28 → 7**。

> ⚠️ 该 bug 让此前所有 CLI 测试全部失败，因此它**掩盖了另外 7 个真实失败**。这正是"文档自述全绿但实际为红"的原因。
>
> ✅ **状态：已修复并提交**（`3840337`）。

---

## 4. 其余 7 个失败的分类与定性（均已修复）

> ✅ 本节 7 个失败在 `3840337` 中全部处理完毕，`cargo test --workspace` 现为 **284 passed / 0 failed**。以下保留现象与根因分析，供后续回归参照。

| # | 测试 | 现象 | 定性 |
|---|------|------|------|
| 1 | `e2e_version_and_help` | `rmcli --help` 输出的是 JSON 错误信封，不是帮助文本 | **生产代码缺陷** |
| 2 | `completions_generate_for_each_shell` | `completions --shell tcsh`（不支持的值）被 clap 拒绝，exit 2；测试期望"优雅处理、exit 0" | 契约分歧 |
| 3 | `local_mod_install_list_toggle_remove` | `mod toggle --enabled false` 是用法错误（exit 2） | **生产代码缺陷** |
| 4–7 | `e2e_error_exit_codes`、`json_error_emits_error_envelope_with_exit_code`、`profile_create_rejects_invalid_input`、`local_mod_install_rejects_missing_file` | 测试期望 `"ProfileNotFound"` / `"ValidationError"` / `"NotFound"`，实际输出 `"profile_not_found"` / `"validation_error"` / `"not_found"` | **测试过期** |

### 4.1 缺陷 1：`--help` / `--version` 走错分支（生产代码）—— ✅ 已修

`src-cli/src/lib.rs:45-53` 把 `Cli::try_parse_from` 的所有 `Err` 一律当作用法错误渲染：

```rust
Err(e) => {
    let out = Output::new(OutputMode::Auto, false, stdout_is_terminal); // 注意：硬编码 Auto，忽略 -o
    out.render_clap_error(&e);
    return e.exit_code();
}
```

而 clap 对 `--help` / `--version` 返回的是 `DisplayHelp` / `DisplayVersion` 这两种**非错误** kind（`exit_code()` 为 0）。结果：

```console
$ rmcli --version
{"ok":false,"error":{"code":"cli_usage_error","exit_code":2,"message":"rmcli 0.2.0",...}}
$ rmcli -o plain --version      # -o 也被忽略
{"ok":false,"error":{...同一 JSON...}}
```

**影响**：脚本/CI 无法用 `rmcli --version` 取版本；`--help` 不可读；`-o plain` 在错误路径不生效。
**修法**：在 `Err` 分支先匹配 `e.kind()`，`DisplayHelp` / `DisplayVersion` 直接 `print!("{e}")` 并返回 0；其余再走错误信封；同时把 `-o` 已解析的值传给输出层。同一问题存在于 `execute_line`（`lib.rs:140-153`）。

### 4.2 缺陷 2：`--enabled` 是 flag 而非取值参数（生产代码）—— ✅ 已修

`app.rs:405` 定义为 `enabled: bool`，clap derive 对 `bool` 生成 **SetTrue flag**，因此：

- `mod toggle --profile P --file F` → 禁用（enabled=false）
- `mod toggle --profile P --file F --enabled` → 启用（enabled=true）
- `mod toggle ... --enabled false` → **用法错误**（`false` 成为多余位置参数）

用户无法用 `--enabled false` 显式表达禁用（只能靠省略），与测试及常规 CLI 直觉不符。
**修法**：改为 `#[arg(long, action = clap::ArgAction::Set)] enabled: bool`（或 `Option<bool>` + `num_args(1)`），使取值语义生效。

### 4.3 测试过期的 4 项：错误码大小写 —— ✅ 已修（改测试断言）

`ErrorCode` 的权威定义是 `#[serde(rename_all = "snake_case")]`（[`crates/rmccc-error/src/lib.rs:113`](../crates/rmccc-error/src/lib.rs#L113)），ts-rs 生成的 IPC 契约同样是 snake_case
（[`src/ipc/types.ts:196`](../src/ipc/types.ts#L196) 起：`"profile_not_found" | ... | "validation_error" | "not_found"`）。

因此**测试期望的 PascalCase 才是错的**（很可能是 `rename_all` 加上之前写下的断言）。
**修法**：把这 4 个断言改成 snake_case，与 `types.ts` 契约对齐。

### 4.4 契约分歧：不支持 shell 的处理 —— ✅ 已修（去掉 value_parser）

`app.rs:92-96` 用 `value_parser = ["bash","zsh","fish","powershell","elvish"]` 限制了取值，而 `registry::render_completions` 本可对未知 shell 友好提示。测试期望后者。
**建议**：去掉 `value_parser`，改由 `render_completions` 处理未知值（与测试一致，也更符合"优雅降级"）。

---

## 5. 架构现状（as-built）

### 5.1 crate 分层（严格 DAG，无环）

```
rmccc-core（编排 + DI 容器 CoreContext）
├── rmccc-error        0.2k 行  统一错误 + ErrorCode（snake_case，ts-rs 导出）
├── rmccc-config       2.3k 行  $schema 版本化 / AppConfig·UserSettings / FileSaver 防抖 / paths
├── rmccc-auth         2.9k 行  AccountID + Account/AccountFactory + AccountRepository + TokenVault
├── rmccc-download     1.2k 行  并发下载 / Range 续传 / SHA1 / CacheRepository / DownloadProvider
├── rmccc-version      4.4k 行  GameRepository（本地索引 + resolve 双视图）+ manifest/assets/libraries
├── rmccc-launch       4.0k 行  DefaultLauncher（HMCL 同构命令行）+ natives/log4j/ProcessListener
├── rmccc-modloader    5.6k 行  8 种 loader（Fabric/Forge/NeoForge/Quilt/LiteLoader/OptiFine/Cleanroom/LegacyFabric）
├── rmccc-mods         0.5k 行  ModSource trait + Modrinth 实现
└── rmccc-profile      0.6k 行  Profile JSON 存储（原子写）+ 继承/覆盖模型
src-tauri  2.7k 行  46 个 IPC 命令 + AppState + 事件常量 + ts-rs 绑定导出（24 类型）
src-cli    5.3k 行  clap 命令面 + CliContext 门面 + registry（帮助/补全）+ 统一输出信封
```

### 5.2 关键设计（与 ARCHITECTURE.md 一致且已落地）

- **单一编排层**：`CoreContext` 持有全部 `dyn` provider（download / version / auth / modloader / mod source / account repo / file saver），命令层只做参数适配与输出。
  ⚠ 但 `pipeline` 的 `launch_game` / `repair_resources` / `resolve_version` **零调用方**，Tauri 侧自行复刻了安装/修复/启动——详见审计文档 §5.3、P1-3。
- **本地磁盘为中心**：`DefaultGameRepository` 扫描 `versions/` 建索引，离线 `resolve` 出 `launch` / `standalone` 双视图；网络 `MojangVersionProvider` 只在安装时取元数据。
- **启动链路 HMCL 同构**：`DefaultLauncher` 负责命令行生成、`natives-{platform}` 解压、log4j 提取、`.abnormal` 崩溃标记、环境变量注入、启动脚本生成。
- **持久化收口两条通路**：账户走 `AccountRepository`（明文 metadata + 加密 private 分离），配置/状态走 `FileSaver`（200ms 防抖）。
- **安全**：AES-256-GCM + 本地 `.key`（Windows `icacls`）；token 经 response file 传递且**延迟**删除（JVM 解析完参数后）；capability 禁通用 `fs`/`shell`。
- **类型桥**：ts-rs 自动生成 24 个类型到 `src/ipc/types.ts`（439 行），CI 用 `git diff` 断言其最新。

### 5.3 IPC 面实测

- 定义 46 个 `#[tauri::command]`，注册 46 个（`src-tauri/src/main.rs`），**差集为空**。
- **UI 可直达 27 个**（Dashboard 11 / InstanceSettings 8 / Login 3 / Settings 5）。
- 15 个仅有 `api.ts` wrapper、无页面调用：`mod_search/get_versions/get_download_url/install`、
  `download_pause/resume`、`auth_list_accounts/switch_account/remove_account/refresh_token`、
  `log_get/clear`、`instance_check_completion/repair/refresh`。
- 4 个连 wrapper 都没有：`instance_list/duplicate/rename/remove`——它们操作的是"版本 id"
  而非 profile，属概念未收敛（审计文档 §7.4）。
- 事件常量 Rust 侧 12 个、前端 11 个，缺 `INSTANCES_REFRESHED = "instances://refreshed"`（`src-tauri/src/events.rs:34`）。
- `src/ipc/api.ts` 已不再手写 `SettingsUpdate` / `ProfileUpdateOpts`，改为 `Partial<ts-rs 生成类型>`（第 6 轮修复）。

---

## 6. 进度对照 DEVELOPMENT_PLAN

| Phase | 验收标准达成情况 | 判定 |
|-------|------------------|------|
| **1 Foundation** | 编译通过、下载引擎、认证（MS device flow + 离线 + 加密 vault）、日志、IPC 框架、SolidJS 布局与登录页均就绪 | ✅ 完成 |
| **2 Core Game** | Mojang API、版本列表缓存、assets/libraries、Java 检测、response file 启动参数、进程管理、Profile CRUD 均就绪；CLI 曾端到端启动原版 26.2 | ✅ 完成（GUI 侧启动面板已在 Dashboard 中） |
| **3 Mod Loaders** | 后端 8 种 loader 安装任务齐备；**实例设置页已有 loader 选择 + 版本列表 + 安装**（Cleanroom 仍为 stub） | ✅ 后端完成，前端已接（未逐项实机点击） |
| **4 Mod Management** | Modrinth 搜索/版本/下载/安装/启停/移除 + 本地 mod 管理后端齐备；**实例页已有已装模组列表（启停/删除），仍缺 Mod 浏览/详情页** | ⚠️ 后端完成，前端缺浏览器 |
| **5 Polish & Ship** | CI（lint + 三平台 test + e2e job）、release.yml（三平台打包 + macOS 公证 + updater 签名）、运维手册、updater 配置齐备 | ⚠️ 配置齐备，但 CI 实为红、证书/Secret 未配、无体积审计记录 |

**结构性欠账**（跨 Phase）：

- `src-cli/src/context.rs` **1,511 行** God Class（历史审计 760 行，规模已翻倍）。
- `crates/rmccc-modloader` 并存两套体系：旧 `ModLoaderProvider` + `create_provider`（仅 `core` 使用）与新 `DependencyManager`（Tauri 使用）。
- `crates/rmccc-core/src/pipeline.rs:583` 的 `launch_game` **无任何调用方**——CLI（`context.rs:987` 起）与 Tauri 各自直接构造 `DefaultLauncher`，编排出现两条分叉。
- `crates/rmccc-modloader/src/cleanroom/` 整个子模块是 stub（`execute()` 直接返回 "not yet implemented"）。
- 死代码：`src/App.tsx`（33 行占位，无人 import）、`src/components/TitleBar.tsx`（92 行，0 引用）、`ToastContainer`（从未挂载）、`src/routes/index.ts`（3 行空操作）。

**已解除（第 5 轮）**：曾经的三处启动阻断——`instances=0`（安装目录与仓库根不一致）、
response file 竞态、argfile 首行混入 java 路径——全部修复；"安装 → 仓库索引 → 启动游戏"全链路
已实机跑通（Minecraft 26.3 进入渲染线程并创建窗口），创建实例的版本选择闭环也已完成。

**测试覆盖薄弱处**：`src-tauri/src/commands/` **46 个命令 0 个单测**；前端 0 测试；crate 侧 `rmccc-mods` 4、`rmccc-core` 5、`rmccc-error` 0。

---

## 7. 文档体系现状与漂移

### 7.1 现有文档（2026-10-23 重组后）

| 文档 | 性质 | 时效 |
|------|------|------|
| `ARCHITECTURE.md` | 设计基线，AGENTS.md 指定的最高权威 | ✅ 2026-10-23 已同步（IPC 表 84 条、§25 重写为 ADR-013 形态） |
| `AI_AND_MCP_ARCHITECTURE.md` | AI 服务 + 外部 Agent 控制 | ✅ 2026-10-23 权威设计（ADR-013，S1–S7 切片） |
| `DEVELOPMENT_PLAN.md` | Phase 1–5 路线图 | ✅ 各 Phase 状态块已回填 |
| `AI_EDITING_RULES.md` | 编码规范 | 有效 |
| `audits/AUDIT_2026-10-20.md` / `audits/AUDIT_2026-10-23.md` | 全量审计 + 增量台账 | 有效（agent 相关条目已标注被 ADR-013 取代） |
| `guides/OPERATIONS.md` / `guides/PLUGIN_AUTHORING.md` | 运维 / 插件编写指南 | 有效 |
| `adr/` / `roadmap/` / `ui-design/` | 决策记录 / 细化执行计划 / 前端重设计 | 有效 |
| `archive/`（07-31 与 08-04 审计、整改台账、问题索引、HMCL 架构分析等 6 份） | 历史快照 | 仅问题溯源（2026-10-23 移入） |
| ~~`test_log.md`、`test-flow.md`、`old/`~~ | RMCLI 时代测试日志 / 迁移存根 | ❌ 已删除（2026-10-23，git 历史可查） |

### 7.2 ARCHITECTURE.md 的漂移点 —— ✅ 已全部修正（2026-10-10）

> 下列 8 类漂移已在本轮同步到实现，此处保留为记录。

1. `:41` 称离线认证为 **UUID v5** —— 实际已改为官方算法 **UUID v3 = MD5("OfflinePlayer:"+name)**（`crates/rmccc-auth/src/offline.rs`）。
2. `:100-109` 称 `src/`（前端）为 placeholder —— 实际已是真实 Router + 3 个页面（1,134 行）。
3. `:59-64` 称 modloader 为 stubs —— 实际 8 种 loader 已实现（5.6k 行）。
4. `:65-66` 称 `rmccc-mods` 仅类型 —— 实际 Modrinth API 已实现。
5. `:1246-1276` IPC 命令表 31 条 —— 实际 42 条。
6. `:437` 引用 `rmccc-auth/src/crypto.rs`（不存在，实为 `vault.rs`）；`:785`/`:804` 引用 `rmccc-download/src/retry.rs`、`resume.rs`（不存在，该 crate 只有 `lib.rs`/`provider.rs`）。
7. `:963`/`:1268` 把 `download_cancel` 定义为"单文件取消" —— 实际按 `batch_id` 批量取消。
8. §15.1 数据目录仍写 `config.json`/`auth.json` —— 实际已拆为 `config/launcher-settings.json`、`state/launcher-state.json`、`user-settings.json`、`accounts.json` + `private/account-private-data.json`。

> AGENTS.md 规定"设计文档优先，冲突以文档为准"。当前漂移方向是**代码领先于文档**，因此必须先更新文档，否则后续开发会被错误的基线误导。

### 7.3 环境问题（非代码缺陷）

推荐的本地构建环境变量（本机实测可用）：

```bash
export PATH=/opt/homebrew/opt/rustup/toolchains/stable-aarch64-apple-darwin/bin:$PATH
export CARGO_HOME=$HOME/.cargo
```

- **`cargo` 不在 PATH**：Homebrew rustup 未安装任何工具链到 `~/.rustup`，实际工具链在
  `/opt/homebrew/opt/rustup/toolchains/stable-aarch64-apple-darwin`。直接用该路径调用即可正常工作
  （`cargo` / `rustc` / `rustfmt` / `cargo-fmt` / `clippy-driver` / `cargo-clippy` 都在该 bin 目录下）。
- **`rustfmt` / `clippy` 组件**：✅ **已安装**（2026-10-10）。注意配置的清华镜像
  （`mirrors.tuna.tsinghua.edu.cn`）**缺这两个组件的 1.98.1 包（404）**，且已安装工具链的
  `multirust-channel-manifest.toml` 写死了该镜像的绝对 URL，`RUSTUP_DIST_SERVER` 无法覆盖。
  解决方式：从 `https://static.rust-lang.org/dist/2026-09-03/` 取
  `rustfmt-1.98.1-aarch64-apple-darwin.tar.xz` 与 `clippy-1.98.1-aarch64-apple-darwin.tar.xz`，
  解包后用包内 `install.sh --prefix=<toolchain dir>` 装入。
- **`strip = true` 失效**：`rust-objcopy` 因缺 `libLLVM.dylib` SIGABRT，release 二进制未剥离符号
  → 5.74MB；手工 `strip` 后 **4.09MB**。真实体积应报 4.09MB。
  （安装 `llvm-tools` 组件可修此问题，但该组件同样在清华镜像缺失；尚未处理，不影响链接结果。）

---

## 8. 继续开发的建议顺序

### 第 0 步：地基 —— ✅ 全部完成

1. ✅ **提交现有工作**。已按主题拆为 10 个 commit（见 §0），CI 可运行、历史可追溯。
2. ✅ **修掉 §3/§4 的 CLI 问题**，`cargo test --workspace` 已 **292 passed / 0 failed**。
3. ✅ **补齐本地 lint 能力**并跑通 CI 的 lint job：安装 rustfmt/clippy 组件后，发现三门禁原本全红
   （27 文件未格式化 + 15 处 clippy lint + 9 处生产 `unwrap()`），均已修复。
4. ✅ **同步文档漂移**：`ARCHITECTURE.md` 8 类漂移已修（§7.2）、`DEVELOPMENT_PLAN.md` 已回填
   5 个 Phase 状态、4 份历史文档已加取代/快照横幅。

### 第 1 步：补齐前端（Phase 3/4 的缺口，投入产出比最高）

后端两条链路已就绪却无 UI，是当前最明显的"做完一半"：

1. `instance_*` 7 个命令：实例列表/刷新/重命名/复制/删除/完整性检查/修复 → Dashboard 的实例面板 + 修复入口。
2. `modloader_*` 2 个命令：loader 列表 + 安装进度（事件 `modloader://install-progress` 已在 Rust 侧就绪）。
3. Mod 浏览器（Phase 4 验收项）：搜索列表 + 详情 + 安装；以及已装 mod 的启停/删除面板。
4. 清理死代码：删除 `src/App.tsx`、`TitleBar.tsx` 或将其接线；挂载 `ToastContainer`；让 `api.ts` 直接 re-export ts-rs 生成的 `SettingsUpdate`/`ProfileUpdateOpts`/`InstanceInfo`，消除手工重复定义。
5. 前端事件常量补 `INSTANCES_REFRESHED`。

### 第 2 步：结构性偿债

1. 拆分 `src-cli/src/context.rs`（1,511 行）为按域的门面（auth/instance/version/mod/modloader/launch/system）。
2. 统一编排：让 CLI 与 Tauri 都经由 `pipeline::launch_game`（当前无调用方），消除两条分叉。
3. 收敛 modloader 双体系：删除旧 `ModLoaderProvider` + `create_provider`，或在 `core` 侧也改用 `DependencyManager`。
4. 补 `src-tauri/src/commands/` 的单元测试（当前 46 个命令 0 测试）。

### 第 3 步：发布工程

1. 跑一次真实 E2E 验收：登录 → 建实例 → 安装原版 → 安装 Fabric/Forge → 离线启动 → 异常退出检出 `.abnormal`。
2. `cargo bloat` 体积审计：重构后 CLI 已从文档记载的 3.1MB 增至 **4.09MB（剥离后）**，需确认 Tauri 最终产物仍 < 10MB。
3. 配置 `TAURI_SIGNING_PRIVATE_KEY_PATH` Secret 与 updater 的真实 `endpoints`；申请 Apple 证书跑通公证。

---

## 9. 未验证 / 待确认

- `cargo fmt` 与 `cargo clippy` 未能在本机执行（组件缺失），CI lint job 的真实结论未知。
- 未运行 `pnpm tauri dev` / `pnpm build`，GUI 端到端表现未验证。
- 未做任何联网行为验证（版本下载、Modrinth、MS 登录）。
- `failing` 的 7 项测试中，判定"测试过期"的 4 项依据是 `ErrorCode` 的 `rename_all = "snake_case"` 与生成的 `types.ts` 契约；若产品口径要求 PascalCase，则应改生产代码而非测试。
