# 01 · crates/ 全量代码审计（16 crate，约 3.9 万行）

> 审计对象：`crates/rmccc-{error,auth,config,download,version,launch,modloader,plugin,logic,llm,agent,core,profile,mods,platform,dashboard}`。
> 对照文档：`docs/ARCHITECTURE.md`、`docs/adr/`。所有行号为 2026-10-25 工作区实测。
> 定级：P0 = 安全漏洞/启动级缺陷；P1 = 高风险/功能不完整；P2 = 缺陷/冗余/明显优化点；P3 = 小问题。
> **修正说明**：两处初判经人工复核修正——`replace_templates`（P0→P1，值经 push_str 不受损，仅模板文本乱码）、`ManagedProcess::kill`（P1→P2，GUI 停止游戏走 IPC 层 terminate_tree 是正确的）。修正后总计：**0 P0 / 14 P1 / 36 P2 / 49 P3**。

---

## 1. rmccc-error（279 行，0 测试）

统一错误类型叶子 crate，`RmcccError` + `PluginErrorCode` + IPC 映射，设计干净，是全图唯一无环叶子。

- [P3] crates/rmccc-error/src/lib.rs:46-48 — `DeviceCodePending` 变体带 `#[allow(dead_code)]`，全工程只在 src-cli/src/error.rs:54 被模式匹配、从未被构造 — 残留死代码；建议实现 device-code 轮询中断时再启用。
- [P3] crates/rmccc-error/src/lib.rs:217-263 — `IpcError`/`ErrorCode` 映射（含 11 个 Plugin 子码映射）零测试 — 映射错位会直接破坏前端错误处理，建议补表驱动单测。
- [P3] crates/rmccc-error/src/lib.rs:152-158 — `is_retryable()` 对 `DownloadFailed`/`HashMismatch` 返回 false，而下载引擎（download/lib.rs:637）恰好依赖它决定"是否换下一个候选 URL"——语义上"哈希不匹配"应触发候选切换，目前靠 download 侧 `continue` 兜住，耦合脆弱。

## 2. rmccc-auth（2,924 行，31 测试）

Microsoft OAuth2 device flow + 离线账户 + HMCL 式 `AccountRepository`（明文元数据 + AES-256-GCM 加密私有数据）。token 生命周期（提前刷新、WrongAccount、CredentialExpired）实现完整且有 mock 测试。

- [P1] crates/rmccc-auth/src/vault.rs:50-61 — key 文件先用 `std::fs::write` 写入（默认 umask 下 0644，同机其他用户可读）**之后**才调用 `restrict_owner_only` 收紧到 0600 — 存在权限窗口，密钥内容短暂全局可读。建议用 `OpenOptions::new().mode(0o600)`（Unix）一步创建。
- [P2] crates/rmccc-auth/src/microsoft.rs:366-367 — token 刷新失败时 `tracing::error!(... body = %body ...)` 将完整响应体写入日志，未经 `redact_body` 脱敏（同函数 85 行处已有脱敏工具却未复用）；microsoft.rs:194-197、232-234（XBL/XSTS 失败）同样裸打 body — 错误响应体可能携带敏感字段。
- [P2] crates/rmccc-auth/src/microsoft.rs:681-718 — `MicrosoftAccount::log_in` 先 `session_snapshot()` 释锁，再在锁外 `refresh`，最后 `*self.session_lock() = refreshed` — 两个并发 `log_in` 会用同一 refresh token 各刷一次；MS refresh token 轮换策略下第二次可能失败甚至使旧 token 失效，且后写覆盖先写。建议把"检查过期→刷新→写回"整段放锁内。
- [P2] crates/rmccc-auth/src/offline.rs:93-102 — `OfflineAccount::new` / `create_offline_session` 对用户名零校验（空串、含空格/引号/超长均可）— 用户名最终进入 `--username ${auth_player_name}` JVM 参数，恶意或含特殊字符名字会破坏参数结构。建议加 `[A-Za-z0-9_]{1,16}` 类校验。
- [P2] crates/rmccc-auth/src/repository.rs:200-234 — `save()` 先写 `accounts.json` 再写 `private/account-private-data.json`，两段非事务 — 中途崩溃会产生"元数据存在但凭据缺失"的账户。建议先写私有文件成功后再写元数据。
- [P3] crates/rmccc-auth/src/repository.rs:469-483 — `atomic_write_str` 的 tmp+rename 没有 fsync — 掉电可能丢账户数据。
- [P3] crates/rmccc-auth/src/microsoft.rs:151-153 — device code 轮询收到 `slow_down` 仅 `continue`，未按 RFC 8628 增大 interval — 可能被服务端限流。
- [P3] crates/rmccc-auth/src/microsoft.rs:26 — `CLIENT_ID` 硬编码为 HMCL 的 Azure 应用 ID（d51b460a-…）— 法律/伦理上应注册自己的应用 ID。

## 3. rmccc-config（2,414 行，39 测试）

设置体系（`$schema` 版本状态机 + `SettingFile` + FileSaver 防抖 + 备份恢复）复刻 HMCL，质量高：损坏文件备份重建、未知字段透传、legacy 命名空间升级均有测试锁定。

- [P2] crates/rmccc-config/src/paths.rs:150-186 — `safe_join`（目录穿越校验）声称是路径防穿越的统一入口，但**全工程零调用**（module doc 名不副实）— 真正的路径拼接（`instance_root`、`profile_path`、`asset_object` 等）都没走它。建议要么接线要么删除声明。
- [P2] crates/rmccc-config/src/paths.rs:99-119 — `agent_dir`/`agent_sessions_dir`/`agent_tasks_dir`/`agent_audit_path` 在 ADR-013 去 Agent 化后全工程无调用 — 死代码残留。
- [P3] crates/rmccc-config/src/lib.rs:391-393 — `profile_game_dir` 直接 `join(profile_id)`，profile id 未校验（与 rmccc-profile P1 同根）。
- [P3] crates/rmccc-config/src/file_saver.rs:53-57 — `save()` 中 `let _ = self.tx.send(...)` — 后台线程已死时保存静默丢失。
- [P3] crates/rmccc-config/src/setting_file.rs:254-263、file_saver.rs:169-177 — 原子写均无 fsync；且 `write_atomic` 用 `with_extension("json.tmp")`，对非 `.json` 路径会产生意外名字。
- [P3] crates/rmccc-config/src/logging.rs:34 — `LOG_GUARD.set(guard).ok()` 二次 init 会静默丢 guard。

## 4. rmccc-download（1,514 行，24 测试）

并发下载引擎（worker 池 + 重试 + Range 续传 + `.part` 暂存发布 + SHA1 缓存仓库）。P1-8 系列（哈希失败不得污染目标、暂存-校验-发布）修复质量高，mock HTTP 回归测试完善。

- [P1] crates/rmccc-download/src/lib.rs:173-176 — `sha1_to_path` 对 `sha1` 无任何校验：`&sha1[..2.min(len)]` 在多字节字符边界 panic；含 `/`、`..` 的 sha1 可把缓存读写定向到 `objects/` 之外任意路径（`check_by_sha1` 读、`store` 写）。sha1 来源于版本 JSON / 镜像响应，属外部可控输入。建议先校验 `^[0-9a-fA-F]{40}$`。
- [P2] crates/rmccc-download/src/lib.rs:434-438 — `wait_for_batch` 用 `Notify::notified().await`：同一批次第二个等待者永远挂死（单 permit）；且 `batches` DashMap 只 insert 从不 remove — GUI 长会话中每个批次泄漏一个 `BatchState`，同时是死锁源。建议改为 `tokio::sync::watch` 或在批次完成回调里 `remove`。
- [P2] crates/rmccc-download/src/lib.rs:564、626 — `CacheRepository::check_by_sha1`/`store` 在 async 上下文用 `std::fs::copy`/`metadata`（Forge installer ~180MB 时阻塞 tokio worker 数秒）。
- [P2] crates/rmccc-download/src/lib.rs:693-697 — 206 响应未校验 `Content-Range` 起点即直接 append 到 `.part` — 异常/恶意镜像返回"完整内容但标 206"会拼出损坏文件；虽有 SHA1 兜底，但结果是每个候选 URL 重试到穷尽。
- [P2] crates/rmccc-download/src/lib.rs:700-728 — `x-bmclapi-hash` 命中即信任镜像头并直接发布（仅无 `expected_sha1` 时启用）— 无完整性锚点的镜像内容可被永久缓存。
- [P2] crates/rmccc-download/src/provider.rs:239-254 — `inject_url_with_candidates`：主规则命中时返回 `[injected]` 单元素（原 Mojang URL 被丢弃，不作为回退）；只有 fallback 规则才保留原 URL — 镜像故障时失去官方源兜底，与"candidates"语义相悖。
- [P3] crates/rmccc-download/src/lib.rs:595 — `2u64.pow(attempt - 1)` 在 `max_retries ≥ 64` 时 debug 构建直接 panic；203 行 `2u32.pow(attempt)` 同理。
- [P3] crates/rmccc-download/src/provider.rs:96-107 vs 127-191 — 文档注释声称含 `resources.download.minecraft.net → {apiRoot}/assets` 规则，规则表实际没有（测试 304-313 自己确认"no rule for resources"）— 文档与实现冲突，且导致 assets 永远不走镜像（见跨 crate #4）。
- [P3] crates/rmccc-download/src/provider.rs:271-286 — `create_provider` 全工程无调用（死代码）；且 `"BMCLAPI@..."` 大写前缀因先 `to_lowercase` 再用原串 `strip_prefix("bmclapi@")` 而永不匹配。

## 5. rmccc-version（4,832 行，67 测试）

版本元数据模型（完整 Mojang 字段 + HMCL `inheritsFrom`/`patches` 双视图）、兼容规则、资源/库解析、完整性检查。是全项目问题密度最高的核心 crate。

- [P1·初判 P0 修正] crates/rmccc-version/src/arguments.rs:164 — `replace_templates` 逐**字节** `result.push(arg.as_bytes()[i] as char)`：**模板文本**中的多字节 UTF-8 被拆成 Latin-1 单字符再编码。经人工复核：替换值走 `push_str` 完好无损，因此"中文用户名/中文 game_dir 作为变量值"不受影响；实际触发面是**模板参数串本身含非 ASCII**——典型为 Profile 自定义 JVM/游戏参数（用户在设置里输入的中文注释、中文路径直写）。建议按 `char_indices` 迭代或仅在非模板片段按字节拷贝原始 UTF-8。该函数是新旧两代参数模板替换的唯一实现（default_launcher.rs:566/575 均经它），属编码级缺陷必须修，但触发条件比初判窄，定级 P1。
- [P1] crates/rmccc-version/src/arguments.rs:219-252 — `rules_match` 实现"任意匹配 + disallow 一票否决"，违背 Mojang 官方"最后一条匹配规则胜出"语义（例：`[{disallow osx},{allow}]` 在 macOS 上官方判定 allow、本实现判定 disallow）— 影响 1.13+ JVM/游戏参数选择。更糟的是同 crate 有三套规则语义并存：libraries.rs:24-53（last-match-wins、有规则无匹配默认 deny，正确）、compatibility.rs:81-104（last-match-wins、默认 **allow**，注释还自称"matching Mojang's library rules"，与 libraries.rs 直接矛盾）且 `rules_allow` 全工程零调用。建议统一到 compatibility.rs 并按三种规则类别分别定义默认值。
- [P1] crates/rmccc-version/src/file_layout.rs:119-124（`&hash[..2]`）+ completion.rs:153、401 — `asset_object` 对短于 2 字节的 hash 直接 panic；completion.rs:401 在 405 行 `hash.get(..2)` 守卫**之前**就调用了它 — 注释宣称"malformed hash skipped instead of slicing into a panic"实际防不住。hash 来自下载的 asset index JSON（镜像可控）。修法：`asset_object` 返回 Option 或入口校验 40 位 hex。
- [P1] crates/rmccc-version/src/repository.rs:21-81 — `resolve_version`（被 core/modloader.rs:44、core/pipeline.rs:155/172/175 用于安装链）只合并 libraries/mainClass/assetIndex/javaVersion，**不合并** `arguments`、`minecraftArguments`、`downloads`、`logging`、`jar` — 子版本 JSON 未重复声明时（Forge/Fabric 的常见形态）父版本的 JVM 参数与 client jar 下载信息全部丢失。与 default_game_repository.rs:85-171 的完整合并实现形成同一语义的两套代码（见跨 crate #1）。
- [P2] crates/rmccc-version/src/assets.rs:29-99 — 资源 URL 硬编码 `https://resources.download.minecraft.net/`（84-87 行）绕过了 DownloadProvider 镜像注入；且每次调用重新下载 index JSON（无解析缓存/磁盘复用）；41-49 行发请求后未检查 HTTP status 就 `bytes()`（404 会以"sha1 mismatch"形式报错，难排查）。
- [P2] crates/rmccc-version/src/manifest.rs:186-189 — `"old_beta" | "old_alpha" => ReleaseType::OldBeta` — OldAlpha 类型信息丢失（types.rs:20 明明有该变体）。
- [P2] crates/rmccc-version/src/manifest.rs:204-222 — 单条 library 解析失败时 push `Library::default()`（空 name）并仅 warn — 空名库会进 classpath 与下载列表。建议 skip 而非占位。
- [P2] crates/rmccc-version/src/default_game_repository.rs:507-537 — `rename_instance` 不更新其他实例 `inheritsFrom` 指向旧 id 的引用 — 重命名基础版本后其子实例在下次 refresh 时被 chain_resolvable 静默从索引剔除（无任何日志）。
- [P2] crates/rmccc-version/src/default_game_repository.rs:569-577 — `remove_instance` 同样不检查反向依赖，且 `instance_root(base, id)` 对 id 无穿越校验（`remove_instance("../x")` 可删任意目录）。CRUD 入口均来自 IPC。
- [P2] crates/rmccc-version/src/cache.rs:14-18 vs 44-52 — 文档称"Serves stale data when offline"，实际 `read()` 过期返回 `None`、损坏文件返回 `Err` — 文档/实现相反。
- [P3] crates/rmccc-version/src/default_game_repository.rs:409-418 — `resolvable.contains(id)` 在 Vec 上线性查找，O(n²)。
- [P3] crates/rmccc-version/src/manifest.rs:126-146 — `fetch_version` 缓存未命中时全量拉 manifest（有 ETag 条件请求缓解，尚可）。

## 6. rmccc-launch（4,173 行，60 测试）

HMCL `DefaultLauncher` 20 步命令行管线的 Rust 移植：@argfile 防进程列表泄 token、natives 解压（zip-slip 防护完备，natives.rs:109-128）、log4j2 配置注入、ManagedProcess 双管道泵 + 退出等待。整体结构清晰、测试密度高。

- [P1] crates/rmccc-launch/src/default_launcher.rs:562-589 — 当版本只有 legacy `minecraftArguments`（全部 ≤1.12 版本）时：563-568 行已把 tokenized 的 minecraftArguments 推入 game_args，582-589 行**又**追加一整份 `default_game_arguments`（--username/--version/--gameDir/… 全套重复）— 命令行参数全量翻倍。MC 多数参数"后者生效"侥幸能跑，但任何按出现次数/位置解析的参数（如 OptiFine tweaker 顺序）会错乱。`default_game_arguments` 应仅在两者皆无时使用。
- [P2·初判 P1 修正] crates/rmccc-launch/src/managed.rs:202-210 — `kill()` 只对直接 child 调 `kill().await`。经人工复核：GUI「停止游戏」实际走 IPC `launch_kill`（src-tauri/src/commands/launch.rs:128-154）→ `core.platform.process.terminate_tree(pid)`（Unix 进程组 SIGKILL），**该路径正确**；且 spawn 时 `process_group(0)` 使 wrapper/nice 前缀进程为组长，terminate_tree 可整组收割。`ManagedProcess::kill` 当前无游戏链路调用方（仅 rmccc-logic supervisor 有自己的 kill）——问题是双层 kill 语义不一致 + 该弱实现 API 留存易被未来误用，定级 P2。建议 `ManagedProcess::kill` 内部改走 `terminate_tree` 或直接删除。
- [P2] crates/rmccc-launch/src/managed.rs:69-73 — `add_listener` 用 `try_lock`，锁竞争时**静默丢弃 listener** — Tauri 日志转发/退出事件监听可能无声丢失。应 `lock().await`。
- [P2] crates/rmccc-launch/src/default_launcher.rs:1187-1189 — `batch_quote` 只转义 `"`→`""`，不处理 `%`（变量展开）、`&`/`^`/`|`（cmd 命令分隔符）— 含这些字符的 game_dir/用户名注入生成的 `.bat` 脚本可执行任意命令（用户主动导出脚本场景）。建议至少拒绝/警告这些字符。
- [P2] crates/rmccc-launch/src/default_launcher.rs:613-709 + natives.rs:130-259 — `launch()` 是 async fn，但内部 natives 解压（全同步 zip + std::fs）、log4j 写盘、run_dir 创建、stale 清扫全部阻塞 tokio worker；大 natives 集合可阻塞数百 ms~秒级。建议 `spawn_blocking` 包裹解压。
- [P2] crates/rmccc-launch/src/process.rs:459-643 — `LaunchArgBuilder`（jvm_args/game_args 全套第二实现，约 185 行）与 `build_game_env`、`spawn_managed` 全工程零调用 — 与 DefaultLauncher 管线平行的死代码，且缺少 log4j/natives/temp-natives 等逻辑，留着必然漂移。建议删除。
- [P3] crates/rmccc-launch/src/managed.rs:167-186 — `wait_exit` 50ms 忙轮询。
- [P3] crates/rmccc-launch/src/default_launcher.rs:1101-1103 — `replace_templates_owned` 是对 `rmccc_version::replace_templates` 的无谓包装。
- [P3] crates/rmccc-launch/src/natives.rs:229-235 — 解压前已 `clean_directory_quietly` 清空目录，"已存在且同 size 跳过"分支在同一次调用内永远不命中（死分支）。
- [P3] crates/rmccc-launch/src/natives.rs:243 — `Vec::with_capacity(entry.size() as usize)` 信任 zip 头声明大小，恶意 zip 可借虚假 size 触发大预分配。
- 亮点：process.rs:66-73 `java_argfile_escape` 正确实现 Java @argfile 的 `""` 转义；default_launcher.rs:499-509 natives 子目录拼接有 `starts_with(natives_dir)` 穿越防护；.abnormal 崩溃标记与 HMCL 对齐。

## 7. rmccc-modloader（5,384 行，40 测试）

8 种加载器的版本发现 + 安装（Forge/NeoForge 走官方 installer JAR，其余 6 种打 `GameInstancePatch`），DependencyManager 统一分流是正确决策，但**多个加载器的安装产物不可用**（流水线存在 ≠ 安装成功后能启动）。

- [P1] crates/rmccc-modloader/src/optifine/install_task.rs:22-91 — 下载的 OptiFine 安装器仅用于嗅探 `launchwrapper-2.0.jar`（63-71 行），**既不运行安装器也不从中提取产物**；patch 引用 `optifine:OptiFine:{mc}_{ver}`（50-54 行）的 Maven 坐标在任何仓库都不存在 → 安装"成功"后启动必然 `ClassNotFoundException`。HMCL 的做法是运行 installer 内置 patcher（java -cp installer.jar optifine.Patcher）解出真实库。注：功能矩阵盘点曾将 OptiFine 记为"完整"，指流水线形态完整；按产物正确性以本条深审结论为准。
- [P1] crates/rmccc-modloader/src/cleanroom/install_task.rs:14-20 — `execute` 永远返回 "Cleanroom install not yet implemented"，但 `ModLoaderType::all()`（lib.rs:89-100）向 UI 暴露 8 个可选项（前端 Download.tsx:301 下拉含 cleanroom）— 用户可选一个必然失败的加载器。至少应在 list 阶段过滤。
- [P1] crates/rmccc-modloader/src/forge/new_install.rs:196-199 — 处理器 classpath 用 `classpath.join(std::path::MAIN_SEPARATOR_STR)` 拼接。经人工复核确认：该写法在**所有平台**都错——Unix 上 MAIN_SEPARATOR 是 `/` 而 Java classpath 分隔符是 `:`；Windows 上是 `\` 而应为 `;`。Forge 1.13+ 处理器（DEOBFUSCATE_SINGLE/MERGE_MAPPING 等）classpath 全坏。同文件 27-36 行文档声称"extracts embedded libraries, downloads processor dependencies"，实现里没有这两步（编号直接从 1 跳到 3）；278-294 行 `MINECRAFT_JAR` 变量指向 `versions/` 目录而非 client jar 文件。应改用 platform descriptor 的 `classpath_separator()`（descriptor.rs:119 已有）。
- [P1] crates/rmccc-modloader/src/liteloader/install_task.rs:52-60 — patch 引用 `com.mumfrey:liteloader:{v}`，但库下载只会走 rmccc-version/libraries.rs:162-186 的 `maven_urls_from_name`（仅特判 fabricmc/quiltmc，其余一律 Maven Central + libraries.minecraft.net）— LiteLoader jar 从未发布到这两个仓库 → 必然下载失败。BMCLAPI 规则表里 `dl.liteloader.com` 的改写规则（provider.rs:156-160）因从未生成该 URL 而形同虚设。（与 OptiFine 同理，功能矩阵的"完整"指流水线形态，产物不可用。）
- [P2] crates/rmccc-modloader/src/lib.rs:364-437 — `install_from_file`（本地安装器入口）对 forge/neoforge/cleanroom 走 `rv.create_install_task`（397-413 行）→ 路由进上述不完整的 ForgeNewInstallTask 处理器路径，与 `install_loader` 的 InstallerJarRunner 形成双轨（两条 Forge 安装路径并存、行为不一致）。
- [P2] crates/rmccc-modloader/src/installer.rs:55-58 — installer 落盘到固定路径 `{game_dir}/{loader}-installer.jar`，两个并发安装互踩；134-145 行 `resp.bytes()` 把 ~180MB installer 全量读入内存再 `to_vec()`（双倍内存），且**无任何 SHA 校验**（对比 HMCL 从 maven 元数据取 sha1）— 镜像劫持即任意代码执行面。运行 installer（65-79 行）也没有超时。
- [P2] crates/rmccc-modloader/src/forge/install_task.rs:111-133 — 第三份 installer 下载实现：写到固定名 `/tmp/forge-installer-{pid}.jar`（并发冲突）且**从不删除**——每次 Forge 安装泄漏 ~180MB 临时文件。
- [P2] crates/rmccc-modloader/src/fabric/install_task.rs:133-148（quilt/install_task.rs:118-131、legacyfabric/install_task.rs:60-74 同）— launcherMeta 库取 `["common","server"]` 两侧 — 客户端安装应取 `common + client`；server 侧库被塞进客户端 classpath、client 侧库缺失。
- [P2] crates/rmccc-modloader/src/quilt/install_task.rs:133-147 — `intermediary` 与 `hashed` 同时加入 libraries，注释自己写"hashed may replace intermediary"（HMCL 语义为 hashed 存在时取代 intermediary）— 双映射并存会产生冲突库。
- [P2] crates/rmccc-modloader/src/fabric/version_list.rs:88-113 — refresh 时对全部游戏版本 × 全部 loader 版本做笛卡尔积（数百 × 上百个堆分配），只为查询一个组合 — 建议惰性构造。
- [P2] crates/rmccc-modloader/src/fabric/install_task.rs:53、quilt:48、legacyfabric:16、optifine:26 — 各 install task 内 `reqwest::Client::new()` 临时客户端：无超时、绕过 core 的共享连接池/代理/镜像配置。
- [P3] crates/rmccc-modloader/src/lib.rs:247-254 — `list_versions` 把所有版本 `stable: true` 硬编码（loader 的 stable 信息被丢弃）。
- [P3] crates/rmccc-modloader/src/optifine/version_list.rs:11-12、39-40 — 硬编码 BMCLAPI 根（不可切回官方源），`refresh(None)` 时 `unwrap_or("1.21")` 魔法默认值。
- [P3] crates/rmccc-modloader/src/liteloader/version_list.rs:12 — `http://dl.liteloader.com/...` 明文 HTTP；61-65 行回退 URL 把 `file` 拼了两次，疑似笔误。
- [P3] crates/rmccc-modloader/src/patch.rs:25-26 — 注释"OptiFine (must override loader mainClass)"与常量矛盾：`PRIORITY_OPTIFINE=10000 < PRIORITY_LOADER=30000`，升序应用后 loader 的 mainClass 覆盖 OptiFine。行为可能符合 HMCL（OptiFine 作为 tweaker 时），但注释必须改。
- [P3] crates/rmccc-modloader/src/fabric/version_list.rs:108-110 等 8 处 — RwLock 毒化时 `if let Ok(...)` 静默保留旧数据（无日志）。
- 亮点：analyzer.rs 的库→加载器识别（含 NeoForge/Forge、LegacyFabric/Fabric 消歧）实现细致且有真实 manifest 回归测试；installer.rs:98-114 以"磁盘上新出现的 versions 目录"确定安装器产物 id，避免猜 id，是好设计。

## 8. rmccc-plugin（7,640 行，106 测试）

声明式插件系统（manifest/包/安装事务/权限/能力/配置/状态机/回滚）。**这是全部 crate 中工程质量最高的一个**：package.rs:79-452 对 zip-slip、zip 炸弹（200:1 压缩比、64MB 总量、16MB/条目、512 条目）、符号链接/设备文件、Windows 保留名、大小写不敏感路径冲突全部拦截且有对应单测；permissions.rs（损坏备份重建 + 高版本只读降级）、manager.rs 的状态机 + rollback 一致性校验都经测试锁定。

- [P2] crates/rmccc-plugin/src/manager.rs:733 — rollback 时 `sha256_file(...).ok()` 失败静默将 `manifest_sha256` 置 None — 完整性基线可悄悄丢失。
- [P3] crates/rmccc-plugin/src/capability.rs:131 — `ai.chat` 能力已入目录（Medium 风险），但按 ADR-013 决策 1，宿主桥方法 `llm.chat` 的执行通道属后续切片 — 能力定义先行、执行未接，插件申请该能力目前无实际效果（文档与实现进度差异，非缺陷）。
- [P3] crates/rmccc-plugin/src/logic.rs / ui.rs — wasm 仅允许 `logic/*.wasm`、UI 资产走白名单双入口复用，边界清晰；未见问题。

## 9. rmccc-logic（1,116 行，17 测试）

逻辑插件协议（ABI 常量 + 行 JSON 协议 + 每插件一个 helper 进程的 Supervisor/Runner）。隔离设计是教科书级：进程隔离 + wasmi fuel/memory 限制、仅允许 `rmccc.log` 一个 import、超时即 kill、版本握手。

- [P2] crates/rmccc-logic/src/supervisor.rs:226-241 — `request()` 同步 `recv_timeout`（最长 30s）；配合 src-tauri/src/state.rs:33 的 `Arc<std::sync::Mutex<LogicRunner>>`，async IPC 命令线程会被同步阻塞且**所有插件逻辑调用全局串行**（每插件一个 helper 的设计收益被全局锁抵消）— 建议 per-plugin 锁或 async 化。
- [P3] crates/rmccc-logic/src/supervisor.rs:286-296 — `RMCCC_LOGIC_HOST` 环境变量可替换 helper 可执行文件路径（风险低，建议发布版仅信任同目录 sibling）。

## 10. rmccc-llm（1,827 行，31 测试）

ADR-013 轻量 AI 服务：OpenAI 兼容流式客户端（SSE 解析、指数退避、CancelToken、限流）+ AES-256-GCM 凭据 vault（与账号 vault 分密钥）+ 落盘前脱敏。端点策略（HTTPS 或 loopback HTTP）和手写 Debug 打码是同类代码里的最佳实践。

- [P3] crates/rmccc-llm/src/llm.rs:365-371 — `tokio::time::timeout` 包住**整个** `run_stream`（含重试退避与整段流式回复），默认 120s：长对话流会被整体掐断 — 建议分离"连接超时/读超时"与"总预算"。
- [P3] crates/rmccc-llm/src/llm.rs:319-324、411-414 — 每个 Provider 实例、每次 `probe_models` 各建 reqwest Client，不走 core 共享客户端。
- [P3] crates/rmccc-llm/src/llm.rs:418-427 — `probe_models` 探测错误响应可能含回显的 key，未过 redact（低风险）。

## 11. rmccc-agent（1,091 行，14 测试）

ADR-013 瘦身后的纯逻辑残留：Fabric 依赖扫描/补装计划、JSONL 审计（写盘前 redact、追加锁串行化、双权限收紧）、两阶段确认快照（id 白名单校验 + payload 脱敏）。瘦身执行干净：无残留执行循环/会话存储，与 ADR-013 决策 4 一致。

- [P3] crates/rmccc-agent/src/mods.rs:309-339 — `read_fabric_metadata` 仅识别 `fabric.mod.json`（文档已声明 Fabric-only，Forge `mods.toml` 支持缺失是已知边界而非缺陷）。

## 12. rmccc-core（1,725 行，16 测试）

DI 容器 + 唯一编排入口（install/repair/launch 三条 pipeline，SetupReport 显式失败计数）。收敛方向正确，但 launch 链路的 Java 检测有实错。

- [P1] crates/rmccc-core/src/pipeline.rs:545-565 — 自定义 `java_path` 时直接**伪造** `JavaInstallation { major_version: required_java, is_64bit: true }`（556-561 行）：不探测真实版本号 — 后果：(a) `--illegal-access`(16)/`--sun-misc-unsafe-memory-access`(24/25)/encoding 分支等所有按 jv 门控的 JVM 参数按"期望版本"生成，用户装错 Java 时参数全错；(b) 完全跳过 `JavaVersionMismatch` 校验。应对自定义路径跑 `probe_java_binary`（platform/java.rs:97 已有）。
- [P1] crates/rmccc-core/src/pipeline.rs:143-196 — 第三套 `inheritsFrom` 合并实现：只合并 mainClass/libraries，`arguments` 直接**整体替换**父版本（189-193 行）而非拼接，assetIndex/javaVersion/downloads/logging 不继承 — 与另外两套合并器语义互不相同（见跨 crate #1）。
- [P2] crates/rmccc-core/src/lib.rs:184-196 — `mirror_url` 只识别 `"bmclapi"` 与精确 URL 两种拼写，其余值**静默**回退官方源（用户配错无任何提示）；且 provider 在启动时定死，运行时切镜像需重启。
- [P2] crates/rmccc-core/src/pipeline.rs:412-436 — `extract_natives` 写入 legacy `versions/{id}/natives`，而 launch 侧优先 `natives-{platform}` — 双命名并存，任何一个目录被手工创建就会出现"两份 natives"。
- [P3] crates/rmccc-core/src/pipeline.rs:78-96 — `FailureTally` 依赖"最终回调 `current_file == None`"的隐式约定（注释自知脆弱）。
- [P3] crates/rmccc-core/src/modloader.rs:100-101 — 每次 `install_loader` 新建 `DependencyManager`（重建 8 个 version list 注册表）。

## 13. rmccc-profile（712 行，9 测试）

HMCL `overrideProperties` 双层模型（值 + 是否覆盖分离）实现正确且测试充分；`delete` 的共享游戏根防护有专门测试。

- [P1] crates/rmccc-profile/src/lib.rs:474-476 — `profile_path(id)` = `profiles_dir.join(format!("{id}.json"))`，**id 无任何校验**：`save("../evil")` 写任意 `.json`、`get/delete("../x")` 读删任意文件；461-463 行 tmp 文件名同样拼接。id 虽通常为内部生成 UUID，但 store API 是 pub 且直接暴露给 IPC 层。建议复用 rmccc-agent/confirm.rs:103 的 `validate_id` 思路。
- [P2] crates/rmccc-profile/src/lib.rs:437-462 — `delete(id, cleanup_game_dir=true)` 对 profile JSON **自身携带**的 `game_dir` 执行 `remove_dir_all`，唯一防护是"目录里存在 versions/ 或 libraries/ 就跳过"（456-461 行）— 一个 game_dir 被改写为 `~/Documents` 的 profile JSON（用户手工编辑/恶意文件替换）在 cleanup 时会整树删除该目录。建议额外校验 game_dir 必须位于 `resolved_game_root()` 之下。
- [P3] crates/rmccc-profile/src/lib.rs:395-476 — ProfileStore 无锁：并发 `touch`（launch 成功路径 pipeline.rs:643 调用）与 `save` 读改写互相覆盖丢更新。

## 14. rmccc-mods（556 行，4 测试）

Modrinth v2 客户端 + `ModSource` trait（默认 `install` 实现有 SHA1 校验，modrinth.rs:137 填充了 sha1——这点实现正确）。

- [P2] crates/rmccc-mods/src/modrinth.rs:112-116 — `list_versions` 无条件附加 `loaders=["fabric","forge","neoforge","quilt"]` 过滤：LiteLoader/OptiFine 版本永远查不到，且与调用方意图无关。
- [P3] crates/rmccc-mods/src/lib.rs:122-127 — `install` 用 `std::fs::write` 直写目标（非 tmp+rename）；`download_bytes` 全量入内存。
- [P3] crates/rmccc-mods/src/modrinth.rs:170-191 — `download_bytes` 不校验 URL 域（CDN 任意重定向跟随），mod jar 下载无大小上限。
- [P3] 文档漂移 — ARCHITECTURE.md §3.2 的 `ModSource` trait 声明了 `get_mod`/CurseForgeSource，实现均不存在。
- 测试覆盖薄：仅 4 个集成测试（search/list_versions 解析），`install` 的 SHA1 失败路径无测试。

## 15. rmccc-platform（1,774 行，38 测试）

平台适配层（descriptor/fs/process/shell/java + macOS 后端 + Windows/Linux deferred + mock）。macOS 后端的 `kill(-pid, SIGKILL)` 进程组终止、`process_group(0)`、0600/0755 权限语义均有真机测试；descriptor 把 Mojang 规则名/原生分类候选/classpath 分隔符集中收敛，是版本 crate 正确性的基石。

- [P3] crates/rmccc-platform/src/unsupported.rs — Windows/Linux 后端 deferred（15 处 TODO，与 ADR-001/012 一致，非缺陷，但意味着全项目当前只有 macOS 真实可跑，`batch_quote` 等 Windows 路径完全无实测）——**这是与 HMCL 跨平台能力最大的结构性差距，非 macOS 平台的"必须验证"项全部 release-blocked**。
- [P3] crates/rmccc-platform/src/java.rs:97-153 — `probe_java_binary` 每次 spawn `java -XshowSettings:properties -version`（同步阻塞、每个候选 JVM 一次、无缓存）— detect_all 对多 JVM 机器可阻塞数百 ms。
- [P3] crates/rmccc-platform/src/macos.rs:145-165 — `detect_all` 串行探测 3 个固定路径 + java_home（无并行）。
- [P3] crates/rmccc-platform/src/shell.rs — `open_url` 已拒绝非 http(s)（测试覆盖），实现正确。

## 16. rmccc-dashboard（1,198 行，17 测试）

布局 Schema + 确定性归一化纯函数 + 主题令牌 + 持久化（损坏备份重建、原子写经平台层）。纯函数化设计使 17 个测试基本锁死行为。

- [P3] crates/rmccc-dashboard/src/store.rs:45 — `format!("{layout_id}.json")` 未校验 layout_id（IPC 传入 `../x` 可写任意 `.json`；读取同样拼接）— 与 profile/layout id 校验缺失同根。
- [P3] crates/rmccc-dashboard/src/theme.rs:113-119 — 语义色缺失时的 fallback 链不对称：error 色 fallback 到纯白在浅色主题下不可见。

---

## 跨 crate 问题

1. **[P1] 三套 `inheritsFrom` 合并实现并存且语义互斥**：rmccc-version/src/repository.rs:21-81（不合 arguments/downloads/logging）、rmccc-version/src/default_game_repository.rs:85-290（全字段追加合并）、rmccc-core/src/pipeline.rs:143-196（arguments 整体替换、不合 assetIndex/javaVersion）。同一份版本 JSON 在"安装 loader → 修复/启动 → 安装"三条链路解析结果不同。必须收敛到 default_game_repository 一处。
2. **[P1] 三套规则评估语义**：arguments.rs:219-252（任意匹配+disallow 否决，违背 Mojang）、libraries.rs:24-53（last-match-wins/默认 deny，正确）、compatibility.rs:81-104（last-match-wins/默认 allow，注释与 libraries.rs 互相矛盾，且全工程无人调用）。同 crate 三份"统一"尝试反而制造了分叉。
3. **[P1·范围修正] 非 ASCII 模板损坏链路**：`replace_templates`（version/arguments.rs:164）的字节级 `as char` 是根因，影响**模板文本**中的非 ASCII（Profile 自定义参数等）；替换值本身不受损。另 `build_template_vars` 的 `format!("{}/libraries", game_dir.trim_end_matches('/'))`（arguments.rs:114）在 Windows 反斜杠路径上产出混合分隔符。
4. **镜像体系四处脱节**：assets 硬编码官方源（version/assets.rs:84-87）、Fabric/Quilt/Forge/NeoForge/LegacyFabric 元数据硬编码各自官方源（各 version_list，BMCLAPI 的 fabric-meta 改写规则 provider.rs:162 永不生效）、OptiFine 硬编码 BMCLAPI（无法关掉）、DownloadProvider 注入仅覆盖 pipeline 组装的 DownloadItem。选了 BMCLAPI 的用户，资源文件与 loader 元数据仍直连官方/第三方源。
5. **installer 下载三份重复实现**：modloader/installer.rs:124-156、forge/install_task.rs:111-133（临时文件泄漏+固定名）、neoforge/install_task.rs:100-118（固定名、无 ZIP 校验）— 各自的临时文件命名、内存策略、校验强度都不同，应合并到 installer.rs 一份。
6. **死代码群**：launch/process.rs:459-643 `LaunchArgBuilder`（185 行平行 JVM 参数实现）、`build_game_env`、`spawn_managed`；download/provider.rs:271-286 `create_provider`；config/paths.rs:150-186 `safe_join`（宣称的防穿越从未接线）、paths.rs:99-119 agent_* 四个函数；version/compatibility.rs:81-104 `rules_allow`；error/lib.rs:46-48 `DeviceCodePending`。合计约 400+ 行。
7. **IPC 标识符校验缺失是系统性问题**：profile id（profile/lib.rs:474）、instance id（version/default_game_repository.rs:507-577 rename/remove）、layout_id（dashboard/store.rs:45）都直接进路径拼接，而项目里已有两套现成校验（plugin/path.rs:71-94 的反向域名、agent/confirm.rs:103-122 的 `[A-Za-z0-9_-]`）可复用。zip 解压侧（plugin、launch/natives）防护反而完备 — 防护重心错配。
8. **同步 IO 阻塞 async runtime 群**：natives 解压（launch 全同步）、CacheRepository std::fs（download/lib.rs:564/626）、assets.rs:65-67、completion.rs 全部、GameRepository refresh、LogicRunner 全局 std Mutex + 30s 同步等待。
9. **MCP 面归属澄清**：ADR-013 决策 2（hyper JSON-RPC MCP 服务器、31 工具）实现在 `src-tauri/src/mcp/`（见 02 文档安全专项），crates/ 中无实现是正确的分工；rmccc-agent 只保留 confirm/audit 原语。
10. **文档与实现冲突清单**：ARCHITECTURE.md §3.1 依赖图缺 auth→platform 等边；§3.2 `ModSource.get_mod`/CurseForgeSource 不存在；`ModLoaderProvider` trait 已被 DependencyManager 取代但文档仍展示；provider.rs:96-107 注释与规则表不符；cache.rs 注释与行为相反；patch.rs:25-26 注释与常量相反。依赖方向本身健康（rmccc-error 唯一叶子、无环、无反向依赖），问题只在文档滞后。
11. **测试盲区**：rmccc-error 0 测试；rmccc-mods 4 测试；rmccc-modloader 40 个测试几乎全在"URL 拼接/patch 构造"纯函数上，**没有任何 installer 执行路径（InstallerJarRunner/ForgeNewInstallTask processors）的测试**；launch 的 `launch()` 全链路仅 1 个（default_launcher.rs:1447 fake java）；版本 JSON 解析（manifest.rs:178-389）只有 1 个 happy-path 测试。

---

## 各 crate 健康度小结表

| crate | 行数 | 测试数 | 问题数(P0/P1/P2/P3) | 一句话评价 |
|---|---|---|---|---|
| rmccc-error | 279 | 0 | 0/0/0/3 | 干净的叶子，但 0 测试 + 1 个死变体 |
| rmccc-auth | 2,924 | 31 | 0/1/3/4 | 生命周期对齐 HMCL 很完整；key 文件权限窗口与日志脱敏缺口是主要隐患 |
| rmccc-config | 2,414 | 39 | 0/0/2/4 | 设置状态机质量高；safe_join 假防护 + agent 死代码需清理 |
| rmccc-download | 1,514 | 24 | 0/1/5/3 | 暂存-校验-发布设计好；sha1 路径未校验、Notify 等待者死锁、缓存同步 IO |
| rmccc-version | 4,832 | 67 | 0/4/5/3 | 核心最重灾区：UTF-8 模板损坏 + 三套规则语义 + 短 hash panic + 双合并器 |
| rmccc-launch | 4,173 | 60 | 0/1/5/5 | 管线复刻到位；legacy 参数翻倍 + 死的第二套参数实现；kill 双轨不一致 |
| rmccc-modloader | 5,384 | 40 | 0/4/7/6 | 分流架构对，但 OptiFine/LiteLoader 产物不可用、Cleanroom 半成品、Forge 处理器全平台必坏 |
| rmccc-plugin | 7,640 | 106 | 0/0/1/2 | 全项目最佳：zip 防护/权限/状态机教科书级 |
| rmccc-logic | 1,116 | 17 | 0/0/1/2 | 双层隔离设计优秀；同步阻塞 + 全局锁抵消并发设计 |
| rmccc-llm | 1,827 | 31 | 0/0/0/3 | 凭据/端点策略/脱敏皆佳；超时语义与小重复 |
| rmccc-agent | 1,091 | 14 | 0/0/0/1 | ADR-013 瘦身执行干净，纯逻辑原语质量好 |
| rmccc-core | 1,725 | 16 | 0/2/2/3 | 编排收敛正确；自定义 Java 版本伪造与第三套合并器是硬伤 |
| rmccc-profile | 712 | 9 | 0/1/1/1 | overrideProperties 模型好；id 零校验 + 删目录信任 JSON 内容 |
| rmccc-mods | 556 | 4 | 0/0/1/3 | SHA1 校验已接线；硬编码 loader 过滤 + 覆盖最薄 |
| rmccc-platform | 1,774 | 38 | 0/0/0/3 | macOS 后端扎实（进程组/权限/探测有真机测试）；Win/Linux 仍 deferred |
| rmccc-dashboard | 1,198 | 17 | 0/0/0/2 | 纯函数归一化 + 测试锁定；仅路径拼接小洞 |

**总计（修正后）：0 P0 / 14 P1 / 36 P2 / 49 P3。**

建议优先级：P1 中的自定义 Java 伪造（core/pipeline.rs:556）、`asset_object` panic（version/file_layout.rs:122）、三套合并器收敛、Forge/OptiFine/LiteLoader 三类不可用安装、`replace_templates` 编码修复——这批直接影响"能否正确启动游戏"这一核心路径；安全类（key 权限窗口、profile id 穿越、installer 无校验）随后迭代内修复；死代码与文档漂移批量清理。
