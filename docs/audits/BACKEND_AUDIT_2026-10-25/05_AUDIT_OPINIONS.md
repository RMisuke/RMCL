# 05 · 审计意见与整改路线图

> 汇总 01–04 文档的结论，给出可执行的整改顺序。问题定位（文件:行号）见各分文档，此处只列处置决策。
> 汇总问题量：后端 crates 0 P0/14 P1/36 P2/49 P3；src-tauri 0 P0/2 P1/14 P2/12 P3；前端 2 P0/14 P1/37 P2/33 P3。

---

## 一、总体审计意见

1. **架构层面是健康的**：rmccc-error 唯一叶子无环、依赖方向无反向、平台差异集中在适配层、IPC 契约三向测试、ts-rs 类型桥——这些决策都正确且已兑现。不需要推翻性重构，需要的是**收敛**（合并器/规则/锁纪律）与**补全**（加载器/差距功能）。
2. **当前最大的风险不在安全，在"核心路径正确性"**：三套 inheritsFrom 合并与三套 rules 语义互斥、Forge processors classpath 全平台必坏、OptiFine/LiteLoader 产物不可用、自定义 Java 版本伪造、src-tauri 嵌套读锁死锁——这批问题意味着**一个普通用户装 Forge 1.13+ 或选错 Java 时，得到的不是错误提示而是错误的启动结果**。建议在任何新功能（含 HMCL 差距补齐）之前先清这批账。
3. **安全面整体收敛良好但有六个明确缺口**（见 §三），其中三个属于"防护声明与实现不符"型（safe_join 零调用、opener:default 绕过自建闸、ui_http_client 兜底丢 no-redirect），修复成本低、优先做。
4. **工程质量分层极其明显**：rmccc-plugin（106 测试、zip 防护教科书级）/ 契约测试体系 / plugin-ui 桥安全模型 是标杆；rmccc-version / rmccc-modloader / Settings.tsx 是债务集中区。建议把 plugin crate 的"危险输入全量枚举 + 表驱动测试"做法推广到 version（hash 校验）与 modloader（installer 路径测试）。
5. **与 HMCL 的差距是"广度差"而非"深度差"**：启动/下载/版本管理的核心深度已对齐甚至更细（字段级继承锁），缺的是整合包、CurseForge、外置登录、Java 自动下载、mod 多格式元数据、跨平台。按 04 文档 T1→T3 推进即可，无架构障碍（ModSource trait、AuthProvider 工厂、DownloadProvider 规则表都已预留扩展点）。

---

## 二、A 批：立即修（核心路径正确性 + 死锁，建议本迭代内完成）

| # | 问题 | 位置 | 处置 | 验证 |
|---|---|---|---|---|
| A1 | 嵌套读锁死锁路径（auth_get_session/refresh 持外层读锁再进 try_refresh_if_needed 二次 read） | commands/auth.rs:229/248 + :36 | try_refresh_if_needed 改为借用传入 core，或先 drop 外层守卫 | 构造"读锁持有中并发 settings_set + auth_get_session"的并发测试 |
| A2 | 整个 install_game 持 core 读锁（下载期间几乎所有 IPC 卡死） | commands/version.rs:147-150 | CoreContext 改 Arc 句柄浅克隆，尽早释放锁；launch_start / auth_microsoft_login / plugin 写锁误用（plugin_inspect/config_get）同批处理 | 长下载进行中 settings_get 可用的集成测试 |
| A3 | asset_object 短 hash 切片 panic（镜像可控输入） | rmccc-version/file_layout.rs:119-124 + completion.rs:401 | 入口校验 40 位 hex 或返回 Option | 畸形 hash 单测 |
| A4 | sha1_to_path 无校验（缓存读写可定向任意路径 + 多字节 panic） | rmccc-download/lib.rs:173-176 | 校验 `^[0-9a-fA-F]{40}$` | 注入型 sha1 单测 |
| A5 | Forge 1.13+ processors classpath 分隔符全平台错误 | rmccc-modloader/forge/new_install.rs:198 | 改用 descriptor.classpath_separator()；补内嵌库提取与 MINECRAFT_JAR 指向 jar 文件（对照 HMCL ForgeNewInstallTask） | 真实 Forge 1.20.x 安装→启动冒烟 |
| A6 | OptiFine / LiteLoader 安装产物不可用；Cleanroom stub 但 UI 可选 | modloader/optifine/install_task.rs:22-91、liteloader/install_task.rs:52-60、cleanroom/install_task.rs:15 | 短期：三种加载器从 UI 可选项隐藏（list 过滤）；中期：OptiFine 运行官方 Patcher、LiteLoader 接 dl.liteloader.com/BMCLAPI 源 | 安装后实例可启动 |
| A7 | 三套 inheritsFrom 合并 + 三套 rules 语义互斥 | version/repository.rs:21-81、core/pipeline.rs:143-196、version/arguments.rs:219-252 | 合并收敛到 default_game_repository 一套；rules 统一到 compatibility.rs（last-match-wins，按规则类别定义默认值）；删除 rules_match 与 repository 合并器 | 用同一份 Forge 子版本 JSON 断言三条链路解析结果一致 |
| A8 | 自定义 java_path 伪造版本号（按期望版本生成 JVM 参数、跳过版本校验） | rmccc-core/pipeline.rs:545-565 | 对自定义路径跑 probe_java_binary，探测失败显式报 JavaVersionMismatch | 自定义路径指向错误 JDK 的测试 |
| A9 | legacy minecraftArguments 参数全量翻倍（≤1.12 全部版本） | rmccc-launch/default_launcher.rs:562-589 | default_game_arguments 仅在 arguments 与 minecraftArguments 皆无时使用 | 1.12.2 启动命令行快照测试 |
| A10 | replace_templates 字节级 as char（模板文本非 ASCII 乱码） | rmccc-version/arguments.rs:164 | 按 char_indices 迭代 | 含中文的自定义参数往返测试 |

## 三、B 批：安全专项（下一迭代，全部为小改动）

| # | 问题 | 位置 | 处置 |
|---|---|---|---|
| B1 | vault key 文件先写后收紧权限的窗口 | rmccc-auth/vault.rs:50-61 | Unix 用 OpenOptions mode(0o600) 一步创建 |
| B2 | profile/instance/layout id 零校验直接进路径拼接（save/get/delete 可越界） | rmccc-profile/lib.rs:474、rmccc-version/default_game_repository.rs:569-577、rmccc-dashboard/store.rs:45 | 统一 id 校验工具（复用 agent/confirm.rs:103 思路），三个入口接线；顺手把 safe_join 真正接线或删除（rmccc-config/paths.rs:150-186） |
| B3 | installer 下载无 SHA 校验（镜像劫持=任意代码执行面） | rmccc-modloader/installer.rs:134-145 | 从 maven 元数据取 sha1 校验 + 三份重复下载实现合并为一份（顺带消除 /tmp 固定名泄漏与 180MB 双倍内存） |
| B4 | capabilities opener:default 过宽（绕过 open_external_url 的 http/https 闸 + reveal 任意路径） | capabilities/default.json:21 | 收窄为带 scope 的 opener:allow-open-url 或移除 |
| B5 | bindings.rs release 运行时向构建机路径写 types.ts | bindings.rs:143-153 / main.rs:121-123 | 移到 #[cfg(debug_assertions)] 或 test |
| B6 | ui_http_client 构建失败兜底丢 no-redirect 保证 | state.rs:58-63 | 兜底改为直接 panic/abort（builder 实际不会失败） |
| B7 | microsoft.rs 错误响应体裸打日志（3 处未过 redact_body） | rmccc-auth/microsoft.rs:366-367/194-197/232-234 | 统一过 redact |
| B8 | offline 用户名零校验（进 JVM 参数） | rmccc-auth/offline.rs:93-102 | `[A-Za-z0-9_]{1,16}` 校验 |
| B9 | MCP ping/tools/list 不验会话真实性、会话无空闲回收、连接无超时 | mcp/protocol.rs:116-123、mod.rs:251-272、http.rs:64-68 | 会话注册校验 + 空闲超时回收 + hyper 读写超时（三者均 P3 级残余，一并清） |
| B10 | profile delete cleanup 信任 JSON 内 game_dir 可删任意目录 | rmccc-profile/lib.rs:437-462 | 校验 game_dir 位于 resolved_game_root() 之下 |

## 四、C 批：架构收敛与去冗余（穿插进行，无行为变化的可随时合入）

1. **死代码清理群（约 400+ 行 + 前端 ~800 行）**：launch/process.rs:459-643（LaunchArgBuilder/build_game_env/spawn_managed）、download/provider.rs:271-286（create_provider）、config/paths.rs:99-119（agent_* 四函数）、version/compatibility.rs:81-104（rules_allow）、error/lib.rs:46-48（DeviceCodePending）、events.rs 三死常量（双侧）、前端 HomeDashboard.tsx 整页（回捞 5 项实现后删）、rail/dash 死 CSS、6 个 i18n 死 key、RmTextarea/RmBadge/BackLink 死导出。
2. **锁与阻塞纪律**：plugin_logic cancel 独立 kill 通道（不再与 LogicRunner 互斥）；LogicRunner per-plugin 锁；java_detect/mods 扫描/cache 写盘/natives 解压统一 spawn_blocking；ManagedProcess::kill 内部改 terminate_tree 或删除（与 IPC 层对齐）。
3. **镜像贯通**（与 04 文档 T1-5 同一件事）：规则表补 resources.download.minecraft.net 与各 loader meta 域；version_manifest_url 接线；inject_url_with_candidates 保留原官方 URL 作回退；mirror_url 拼写容错 + 未知值告警；运行时切镜像热生效。
4. **文档同步**（AGENTS.md 要求"设计文档优先"）：ARCHITECTURE.md 依赖图补边、删 ModSource.get_mod/CurseForgeSource 声明或标注 planned、ModLoaderProvider→DependencyManager、provider.rs/cache.rs/patch.rs 三处注释与实现相反的对齐、main.rs 10MB→20MiB 注释、tauri.conf.json $schema 改官方。

## 五、D 批：测试补强（防回归，随对应修复合入）

- **installer 执行路径**（当前 0 测试）：InstallerJarRunner / ForgeNewInstallTask processors 用本地 fixture installer jar 跑冒烟；modloader 40 个测试全是纯函数的现状必须打破。
- **events.rs ↔ events.ts 同步测试**（命令有、事件没有）——补齐后顺手处理三死常量。
- error 映射表驱动单测（0 测试却承载全部前端错误处理）；cache_icon_* 与 decode_icon_uri_path 契约；mcp http 路由级（401/403/413/会话）；auth 刷新窗口。
- 前端引入测试框架（当前 0 测试）：优先锁 P0/P1 修复点（hash 查询、Settings 覆盖编辑、防抖）。

## 六、前端专项（引用 03 文档，两周一迭代）

1. **诚信优先**：假数据卡（服务器/比对/公告）加"演示"标识或下线（P0-2）；hash 查询参数链路修复（P0-1，6 个跳转源 + useSearchParams）。
2. **两件事启动**：UI 实验残骸清理（HomeDashboard 回捞后删除 + 死 CSS）；两套保存心智统一（Settings 每键写盘 → dirty 标记 + 显式保存，与 InstanceDetail 对齐）。
3. Settings.tsx 三 P1（覆盖用户编辑/裸 iframe/每键写盘）+ App 级 ErrorBoundary + TitleBar onCleanup + Download 防抖（一行修复）本迭代清零。

## 七、HMCL 差距路线图（承接 04 文档 T1/T2/T3，粗估工作量档）

| 阶段 | 内容 | 粗档 | 备注 |
|---|---|---|---|
| T1-补核心 | A5/A6（Forge/OptiFine/LiteLoader/Cleanroom） | M | 先修后藏 |
| T1-CurseForge | CurseForgeSource（search/version/指纹）+ 商店源切换解禁 | M | ModSource trait 已预留 |
| T1-整合包 | Modrinth .mrpack 导入 → Curse manifest 导入 → 导出 | L | 前端占位卡已就位 |
| T1-镜像贯通 | 见 §四.3 | S | 与 A 批穿插 |
| T1-外置登录 | authlib-injector provider + -javaagent 注入 + 服务器元数据 | M | 登录工厂已预留 |
| T1-Java 下载 | Mojang runtime 清单下载（macOS 先行） | M | A8 修复后做，版本匹配才可信 |
| T2-体验 | mod 多格式元数据 → 更新检查 → QuickPlay → 代理 → self-update 产线 → 版本号比较 → 崩溃规则引擎 → modloader 移除命令 | 各 S/M | 崩溃规则引擎可与 AI 分析并存（离线规则 + 在线增强） |
| T3-长尾 | 世界/资源包/数据包 → SLP ping → 皮肤上传/规范化 → 离线伪 Yggdrasil → i18n → 硬件检测 | 各 S/M | SLP 或演示卡下线二选一 |
| 结构性 | Windows/Linux 平台后端（unsupported.rs 15 处 TODO） | L | 按 PLATFORM_ADAPTATION.md 路线，之前所有跨平台声称 release-blocked |

## 八、保持项（不要在整改中丢掉的优势）

- rmccc-plugin 的 zip 防护与权限模型、plugin-ui 桥安全模型、MCP 确认流——新增功能（整合包导入、CurseForge 下载）应复用这些既有原语（哈希校验、白名单、两阶段确认），而不是另起一套。
- 三向 IPC 契约测试与 ts-rs 新鲜度测试——新命令必须同步进 BACKEND_ONLY 清单与 bindings types 数组。
- 字段级继承锁（override_properties）与 SetupReport 显式失败计数——CurseForge/整合包安装器应挂进 DependencyManager 分流而不是绕过。

## 九、验收门禁建议

1. 每项 A 批修复合入前：对应单测 + `cargo test --workspace` + `pnpm typecheck` 全绿（当前基线：cargo check 零错误、tsc 零错误、静态测试数 ≈632）。
2. A5/A6 完成后追加真机冒烟矩阵：原版 26.2 / 1.12.2+Forge / 1.20.x+Fabric / 1.20.x+NeoForge 安装并启动成功（记录到 PROJECT_STATUS.md）。
3. B 批完成后跑一次 `cargo bloat` 确认体积无回归（ADR-011 20 MiB 红线，strip 后口径）。
4. 体积/平台红线遵守 AGENTS.md：非 macOS 验证项记 deferred / release-blocked，不得宣称通过。
