# 04 · RMCL × HMCL 后端功能差距比对

> 对照库：`/Volumes/APPLE 1TB/Code/HMCL-main`（HMCL 的深度重构版 fork，非上游原版：以 `GameInstanceManifest`/`GameInstancePatch` 实例+补丁模型取代上游 `Version` 类，新增 HMCLBoot 引导模块与重构版设置系统；仓库根 `explain/` 有 13 篇中文架构文档可对照）。
> 口径：以「能力」而非「类名」对齐。HMCL 侧约 **118 个后端功能点**（HMCLCore 425 个 Java 文件）；RMCL 侧以代码证据为准（详见 01/02/03 文档与功能矩阵盘点）。
> 结论速览：RMCL 在「基础启动链路 + 插件平台 + AI/MCP」上有独有优势；与 HMCL 的差距集中在 **整合包、CurseForge、外置登录（authlib-injector）、Java 自动下载、非 macOS 平台、mod 元数据多格式** 六大块，另有多处"流水线存在但产物不可用"的加载器缺陷（见 01 文档）。

---

## 一、总览矩阵

| # | 域 | HMCL 功能点 | RMCL 现状 | 差距评级 |
|---|---|---:|---|---|
| 1 | 认证 | 12 | MS device flow 完整链 + 离线 + 加密仓库 + 多账号；**无外置登录** | 🟡 中（外置登录缺 = 国服生态断腿） |
| 2 | 下载引擎 | 10 | 并发/SHA1/断点/退避/缓存仓库 + BMCLAPI 28 规则；**assets 与 loader 元数据不走镜像** | 🟡 中（引擎达标，镜像贯通与多源回退缺） |
| 3 | 版本管理 | 12 | 清单 TTL/继承合并（三套互斥）/规则（三套互斥）/三级完整性校验+修复 | 🟡 中（能力面覆盖 ~70%，正确性有硬伤） |
| 4 | 加载器安装 | 12 | 8 种注册；Forge processors 全平台必坏、OptiFine/LiteLoader 产物不可用、Cleanroom stub | 🔴 大（4/8 加载器实际不可用或残缺） |
| 5 | Mod 管理 | 7 | Modrinth 搜索/版本/下载 + 本地启停 + fabric-only 依赖扫描（独有特色）；**无 CurseForge、无更新检查、仅识别 fabric.mod.json** | 🔴 大 |
| 6 | 整合包 | 9 | **完全缺失**（前端占位卡） | 🔴 全缺 |
| 7 | Java 管理 | 7 | macOS 探测 + 版本匹配 + vendor；**无自动下载；Win/Linux deferred** | 🔴 大 |
| 8 | 启动 | 15 | argfile/natives/log4j/优先级/wrapper/pre-post/脚本导出(CLI)/ExitType/AI 日志分析；**无 QuickPlay、无崩溃规则引擎、无渲染器切换** | 🟡 中（核心齐，边角缺） |
| 9 | 服务器 | 0（fork 也无 ping） | 后端无；**前端画了无标识的假数据卡** | ⚪ 双方都缺（RMCL 多一个诚信问题） |
| 10 | 皮肤/资料 | 5 | 3D skinview3d 查看 + auth_skin_url；**无上传、无 64×64 规范化、无 cape 管理** | 🟡 中 |
| 11 | 世界/资源包/数据包 | 4 | 仅路径常量，**无任何管理功能** | 🔴 全缺 |
| 12 | 其他（更新/代理/i18n/设置/硬件） | 12 | self-update 占位、**无代理设置**、双语、设置模型含字段级继承锁（独有优势） | 🟡 中 |

粗估：HMCL 118 个功能点中 RMCL 已覆盖约 **55–60 个（≈50%）**，其中约 10 个覆盖项带正确性硬伤（见 01 文档 P1）。

---

## 二、分域详述

### 1. 认证

**RMCL 已有**：Microsoft device code flow（`MicrosoftAuthProvider`，scope `XboxLive.signin offline_access`，microsoft.rs:67/103）；XBL→XSTS→MC token→profile 完整链（microsoft.rs:175/213/256/290）；token 提前刷新窗口 600s + WrongAccount/CredentialExpired（microsoft.rs:612/705-715/721）；离线登录（**MD5 版本 3 UUID**，`OfflineAuthProvider::uuid_from_username` offline.rs:37-46，测试锁定）；AES-256-GCM TokenVault + 明文/密文分离（vault.rs:38-146）；多账号仓库 + 9 条 IPC。

**HMCL 多出的**：
- **yggdrasil / authlib-injector 外置登录全套**（RMCL 全仓仅 2 处注释提及）：`-javaagent=…=serverUrl` + `-Dauthlibinjector.yggdrasil.prefetched=<base64 metadata>` 注入参数构建、ALI 服务器元数据获取/缓存/失效、authlib-injector jar 增量下载更新、拖拽添加服务器（`authlib-injector:yggdrasil-server:<url>` URI 方案）。**对中国用户群（LittleSkin 等皮肤站生态）这是硬需求**。
- 离线账号本机伪 Yggdrasil 服务器（随机端口拉起 `/api/profiles/minecraft`、`hasJoined`、textures 端点 + RSA 签名元数据）——离线账号也能挂皮肤。
- 离线皮肤 10 种来源（内置 8 皮肤/本地文件/LittleSkin/CustomSkinLoader API）+ slim/wide 模型。
- 皮肤上传（微软 multipart variant classic/slim + yggdrasil）。
- 自定义离线 UUID；授权码流程（PKCE + 本地回调 HTTP 服务器）作为 device flow 之外的备选；Xbox 错误码细分（被封 2148916227/缺账户/地区不可用/需加入家庭 四类）；`slow_down` 按 RFC 8628 增大间隔（RMCL 仅 continue，01 文档 P3）。

### 2. 下载引擎

**RMCL 已有**：tokio worker 池并发（默认 16，BMCLAPI 声明 32）、SHA1 流式校验、`.part` 暂存→校验→原子发布、Range 断点续传（416 删半重下）、多候选 URL × 指数退避、SHA1 内容寻址缓存仓库、BMCLAPI 24 primary + 4 fallback 规则表（覆盖 Mojang 四域名/Forge×3/NeoForge/LiteLoader×2/Fabric meta+maven/authlib-injector/Maven Central→腾讯云×2/Modrinth+CurseForge→mcimirror）、`x-bmclapi-hash` 头缓存命中。

**HMCL 多出的 / RMCL 断点**：
- **assets 对象不走镜像**：RMCL 硬编码 `resources.download.minecraft.net`（assets.rs:85），BMCLAPI 规则表无此域（provider.rs 单测自证"no rule for resources"）——镜像用户最痛的资源文件（数量最大、体积最大）仍然直连官方。HMCL 经 getAssetObjectCandidates 完整镜像化。
- **版本清单/版本 JSON 不受镜像设置影响**（MojangVersionProvider 恒用官方 piston-meta；`version_manifest_url()` 从未接线）。
- **运行时切换镜像不生效**（provider 启动时定死，需重启）；HMCL OFFICIAL/MIRROR/AUTO 三模式 + 并发度自适应。
- 多候选回退语义缺陷：RMCL 主规则命中即丢弃原官方 URL（01 文档 P2）。
- 文件级 ETag/Last-Modified 条件缓存（RMCL 仅版本清单有）。
- **启动器全局代理设置**（HMCL ProxyManager：HTTP/SOCKS+认证+系统代理继承+游戏进程透传）——RMCL 完全没有（仅游戏 JVM 侧 `-Dhttp.proxy` 参数透传）。

### 3. 版本管理

**RMCL 已有**：版本清单 TTL+ETag 缓存；本地仓库双视图（launch/standalone）；坏 JSON 容错；三级完整性校验（Presence/Size/Hash）+ 修复管线 + 启动前 ensure_launch_ready；legacy minecraftArguments 解析；HMCL 整合包目录过滤。

**RMCL 内伤（01 文档 P1，HMCL 只有一套）**：三套 `inheritsFrom` 合并语义互斥、三套 rules 求值语义互斥（其中一套违背 Mojang last-match-wins）。

**HMCL 多出的**：
- MaintainTask 三代修复：LaunchWrapper tweaker 重排（OptiFine+Forge 共存切换）、CpwModLauncher transformer 注入（1.13+）、BootstrapLauncher（1.17+）——RMCL 的 repair 只做文件完整性，不做版本结构修复。
- 客户端 jar 常量池直接探测游戏版本（jenkinsci scanner）；30+ 版本号排序体系（`GameVersionNumber`：release/snapshot/legacy/愚人节特例 + VersionRange）——RMCL 前端用字符串比较（03 文档 P3）。
- `map_to_resources`/虚拟资源目录（pre-1.6 布局）——RMCL 只有常量无消费者。
- TLauncher 库格式兼容；无 JSON 的 CLASSIC_MANIFEST 老版本。
- MC→Java 版本约束 8 条规则（VANILLA 最低版本表/MODDED_JAVA_7/8/16/VANILLA_LINUX_JAVA_8/架构/MODLAUNCHER_8 规避 JDK-8273826 等）——RMCL 仅单一 javaVersion 门控。

### 4. 加载器安装（差距最大的域之一）

**RMCL 现状（8 种注册，实况分层）**：

| 加载器 | RMCL 实况 | HMCL 对照 |
|---|---|---|
| Fabric | ✅ 可用（但 launcherMeta 取 common+server 侧，client 侧库缺失，01 P2） | 完整 + Fabric API 附带安装 |
| Quilt | ✅ 可用（intermediary/hashed 双映射并存冲突，01 P2） | 完整 + Quilt API |
| LegacyFabric | ✅ 可用（同 Fabric 问题） | 完整 |
| Forge ≤1.12 | ✅ 可用（universal JAR + patch） | 完整 |
| **Forge 1.13+** | ❌ **processors 全平台必坏**（classpath 分隔符错 + 内嵌库未提取 + MINECRAFT_JAR 指向目录，01 P1） | 完整（外部 java 进程执行 processor + outputs 幂等跳过 + mappings 下载） |
| NeoForge | 🟡 走 installer JAR，但 `install_from_file` 双轨路由进坏的 processors 路径（01 P2） | 完整（新旧双格式） |
| **OptiFine** | ❌ **产物不可用**（不运行 patcher，patch 引用不存在的 Maven 坐标，01 P1） | 完整（运行 `optifine.Patcher` 解出真实库 + launchwrapper 捆绑处理 + 与 Forge 共存切换） |
| **LiteLoader** | ❌ **库必然下载失败**（坐标只查 Maven Central/minecraft.net，01 P1） | 完整（dl.liteloader.com 可镜像） |
| **Cleanroom** | ❌ 纯 stub 但 UI 可选（01 P1） | 完整 |
| Fabric API / Quilt API 安装 | ❌ 无 | ✅ |
| 本地 installer 导入 | 🟡 有入口但路由双轨（lib.rs:364-437） | ✅ 读 install_profile.json 判型 + 版本不匹配报错 |
| 版本列表双源 | ❌ 各 loader 硬编码单一源（OptiFine 硬编码 BMCLAPI 不可关） | ✅ 官方/BMCL 双 VersionList |

### 5. Mod 管理

**RMCL 已有**：Modrinth search/versions/download_url/download_bytes + SHA1 校验安装；本地 mod 列表/启停（mods/ ↔ mods/disabled/）/删除/本地 jar 安装（含原生选择器）；**Fabric 依赖扫描 + 补装计划两阶段确认（mods_dependency_scan/mods_apply_plan，独有特色但 fabric-only、无 semver 求解）**。

**HMCL 多出的**：
- **CurseForge API 全套**（RMCL 零实现：搜索/版本/分类/section/分页/API key 管理/不可用优雅降级；MurmurHash2 指纹）。
- **本地 mod 文件→远端版本识别（更新检查）**：Modrinth SHA-1 → `/v2/version_file/:sha1`、CurseForge MurmurHash2 → `/v1/fingerprints/432`，双源 checkUpdates——RMCL 完全没有 mod 更新检查。
- **6 种 mod 元数据格式识别**：fabric.mod.json / quilt.mod.json / META-INF/mods.toml / mcmod.info + .lst / litemod.json / pack.mcmeta（RMCL 仅 fabric.mod.json；Forge/NeoForge 实例的依赖扫描直接不可用）。
- 多文件按 (modId, loaderType) 归并、`.old` 旧版本标记与版本回退、重复文件名检测。
- 远端仓库覆盖 4 类资源（mods/modpacks/resource_packs/shader_packs）——RMCL 商店只有 mods。

### 6. 整合包（全缺）

HMCL：**5 种格式导入**（Curse manifest v1/v2 + overrides + projectID→URL 补全、Modrinth modrinth.index.json（sha1+sha512/env/多 URL）、MultiMC（instance.cfg + mmc-pack.json 组件映射 + patches/*.json）、MCBBS（addons 坐标/launchArguments/authlibInjectorServer）、服务器自动更新包（server-manifest.json 哈希 diff + forceUpdate））+ **4 种格式导出**（ModAdviser 建议保留清单/白名单/版本/作者/fileApi）+ 更新失败回滚 + ModpackConfiguration 装机清单 diff + 任意 zip 手动建实例 + zip 编码探测。

RMCL：**后端 0 实现**，前端 Download 第 4 tab 占位卡（"整合包安装规划中"）。这是与 HMCL 差距最完整的一个域。

### 7. Java 管理

RMCL：macOS 探测（`/usr/libexec/java_home -V` + 固定路径）+ `-XshowSettings:properties` 解析版本/vendor/64bit + 精确/最近更高匹配；**无下载；Win/Linux 后端 UnsupportedPlatform（15 处 TODO，ADR-012 deferred，release-blocked）**。

HMCL 多出的：15+ 平台路径探测（Windows 注册表 JNA/Program Files×2/6 厂商目录/微软商店 MC runtime、Linux /usr/lib/jvm 三变体/SDKMAN、macOS Homebrew/Cellar）、`HMCL_JRES` 环境变量、release 文件**免解压**识别（zip/tar 内直接读）、vendor 归一化、架构转译兼容（Win ARM64/Rosetta 2）、**双源 JDK 自动下载安装**（Mojang 官方 runtime 按 8/17/21 + foojay disco：Temurin/Zulu/GraalVM）+ 托管目录安装/卸载、JavaVersionConstraint 8 条规则双向（约束+推荐）、重复发现择优。

### 8. 启动

**RMCL 已有（对齐 HMCL 核心）**：@argfile 防 token 泄露 + Java argfile 转义；natives 解压（zip-slip 防护）；log4j2 配置注入 4 变体 + formatMsgNoLookups RCE 防护；进程优先级（Unix nice）/wrapper/pre-launch/post-exit 命令；脚本导出 sh/bat/ps1（仅 CLI，无 IPC/UI）；ManagedProcess 双管道泵 + ExitType{Normal,Abnormal} + .abnormal 标记；IPC 层进程组终止（terminate_tree）；隔离三态（never/always/modded）50 项测试；**AI 流式日志分析（独有替代路线）**。

**RMCL 内伤**：legacy minecraftArguments 参数全量翻倍（01 P1）；自定义 Java 版本伪造（01 P1）；natives 双命名目录并存。

**HMCL 多出的**：
- **QuickPlay 直连**（`--quickPlayMultiplayer` 新版 / `--server --port` 旧版按版本门控、`--quickPlaySingleplayer`、Realms）——RMCL 全缺（唯一 `--server` 出现在单测）。
- **崩溃规则引擎 CrashReportAnalyzer：30+ 条正则规则**（需 JDK11/显卡驱动崩溃码含 ig/atio/nvoglv/OOM/重复 mod/Fabric ModResolution 全家族/Forge 依赖缺失/ModLauncher8/mod 被解压/ID 上限/Mixin/Java 版本过高/模块名非法/Forge 安装不完整/Shaders 冲突等）→ 结构化建议。RMCL 的 AI 分析是差异化路线，但**离线不可用**且不可规则化沉淀。
- 渲染器/显卡切换（Windows mesa-loader：llvmpipe/zink/VK_ICD_FILENAMES；macOS MoltenVK；`--graphicsBackend` 26.2+）。
- 编码感知日志泵（Java 19 前后 stdout/stderr.encoding 键差异）；非 ASCII 路径 natives 符号链接 workaround（<1.19）；JDK 25-26 CompactObjectHeaders、Java16 `--illegal-access=permit`、Java24/25 `--sun-misc-unsafe-memory-access` 等 JVM 参数代际矩阵（RMCL 部分覆盖于 custom Java 版本伪造问题中失准）。
- 游戏卡死线程转储（GameDumpGenerator）、日志导出 zip（liteconfig/logs/crash-reports/启动脚本）、关启动器策略（窗口检测后再隐藏/退出）。
- modded 环境变量标记（INST_FORGE/INST_FABRIC/…）+ APPDATA 重定向。

### 9. 服务器

**该 HMCL fork 也没有服务器 ping/MOTD/服务器列表**（全库检索确认 0 实现；仅有 ServerAddress 解析与 QuickPlay 直连）。RMCL 后端同样没有——**但 RMCL 前端 Home 页画了无标识的假 MOTD/假玩家数/假公告卡（03 文档 P0-2）**，这是 RMCL 独有的诚信问题：要么实现 SLP ping（协议握手 + favicon + 玩家数），要么给演示卡加标识或下线。

### 10. 皮肤/资料

RMCL：3D skinview3d 预览（常驻舞台层，WebView 适配下的工程亮点）+ auth_skin_url 代理（无缓存、每次新建 client）。

HMCL 多出的：64×64 皮肤规范化（老格式 64×32→64×64 自动补腿臂镜像、slim 模型 alpha 像素检测、HD 多倍尺寸）、cape 全链路、皮肤上传、按 UUID hash 的默认皮肤回退、JavaFX 3D 渲染含挥手/跑步动画。

### 11. 世界/资源包/数据包（全缺）

HMCL：World 存档管理（NBT 读世界名/GameType、**改世界名写 NBT**、icon 缩略、复制备份排除 session.lock、WorldLockedException）；资源包管理（pack.mcmeta/zip/目录双形态/.disabled/CurseForge+Modrinth 远端下载）；数据包管理（1.13 前后 zip→目录转换兼容）；.litematic 解析 + 通用 NBT 查看器。

RMCL：仅 `resourcepacks_dir`/`saves_dir` 路径常量（file_layout.rs:142/147），无任何扫描/管理命令与 UI。

### 12. 其他

| 项 | HMCL | RMCL |
|---|---|---|
| 自动更新 | 三通道（stable/nightly/dev）元数据服务器 + 更新包校验和 + Windows exe 头保留 + 重启替换 | tauri-plugin-updater 已装但 endpoints 占位 + pubkey 示例值 + 前端无调用（三处断线，02 文档 P2） |
| 代理设置 | 全局 ProxySelector + Authenticator + 游戏透传 | **无** |
| 硬件检测 | Win(WMI+FastFetch)/Linux(/proc+lspci)/macOS(system_profiler) CPU/GPU/内存 → 自动内存推荐 | 无硬件检测（auto_memory 为简单开关） |
| i18n | 10 语言（含文言文、RTL） | 2 语言（zh-CN/en，机制健康） |
| 便携版 | 支持（portable 模式、账户数据只读区分） | 无显式便携模式（data_dir 逻辑未审计出 portable 分支） |
| 设置模型 | JsonSchema 化 + 实例级 InheritableProperty + 预设 + 旧配置迁移器（Receipt） | $schema 版本协商 + 字段级 override_properties（**RMCL 更细粒度，独有优势**）+ 未知字段透传 |
| 崩溃日志 | 自带 Logger + Log4jLevel 着色 + 导出 + Countly opt-in 上报 | tracing + 前端 LogDrawer + AI 分析 |
| 事件总线 | EventManager | Tauri 事件 + ThrottledEmitter |

---

## 三、RMCL 独有能力（HMCL 没有的，应保持并作为卖点）

1. **插件平台**：`.rmplugin` 声明式包 + zip 防护（slip/炸弹/symlink/保留名全覆盖）+ 权限/能力/配置/状态机/回滚 + wasm 逻辑插件（独立 helper 进程 + wasmi fuel/memory 限额 + 仅 `rmccc.log` import）+ 隔离 UI 源（CSP `default-src 'none'`）+ dashboard 卡片/主题贡献。
2. **AI/MCP 面**：LLM 日志流式分析（OpenAI 兼容任意端点 + 凭据独立 vault + 脱敏）；31 工具 MCP 服务器（回环双检 + Bearer 恒时比较 + Origin 防 rebinding + 破坏性工具两阶段确认 120s + 审计 fail-closed）+ stdio 桥。
3. **体积与栈**：Rust + SolidJS（主 chunk 145 KB / gzip 47 KB vs JavaFX 数百 MB 运行时）；release profile 与 20 MiB 红线工程化（strip/LTO/门禁）。
4. **字段级继承锁**（override_properties 显式集合）与**三向 IPC 契约测试**（注册↔定义↔前端 wrapper 钉死）。
5. **Fabric 依赖扫描 + 补装计划**（扫描→计划→两阶段确认→apply），HMCL 无等价的 mod 依赖补装流程。

---

## 四、差距优先级排序（建议，细则见 05 文档）

**T1（补齐后 RMCL 才算"可用的大众启动器"）**
1. Forge 1.13+ processors 修复（classpath 分隔符 + 内嵌库提取 + MINECRAFT_JAR）——最常见 modded 链路
2. OptiFine 真实 patcher 运行 / LiteLoader 库源修复 / Cleanroom UI 下架（三选一处置：修或藏）
3. CurseForge API 接入（与 Modrinth 对等，ModSource trait 已预留）
4. 整合包导入：Modrinth .mrpack（格式最简单：单 JSON + files 哈希表）先行，Curse manifest 次之
5. assets 与 loader 元数据镜像贯通（规则表补 resources.download.minecraft.net + 各 loader meta 域；version_manifest_url 接线）
6. authlib-injector 外置登录（登录 provider + -javaagent 注入 + 服务器元数据）
7. Java 自动下载（Mojang runtime 清单起步，macOS 单平台即可先行）

**T2（体验对齐）**：mod 元数据多格式识别（quilt.mod.json/mods.toml/mcmod.info）→ mod 更新检查（SHA1 指纹）→ QuickPlay 直连 → 全局代理设置 → self-update 产线（endpoints + CI latest.json + 前端接线）→ 版本号比较体系 → 崩溃规则引擎（离线兜底，AI 在线增强）→ modloader 移除命令（前端已欠）。

**T3（长尾）**：世界/资源包/数据包管理 → 服务器 SLP ping（或先给假数据卡加演示标识/下线）→ 皮肤上传与规范化 → 离线伪 Yggdrasil 皮肤服务器 → i18n 扩展 → 硬件检测自动内存 → 脚本导出 UI 化。

**结构性前提**：Windows/Linux 平台后端（unsupported.rs 15 处 TODO）按 ADR-012 走平台适配路线图，在此之前所有跨平台声称均为 release-blocked——HMCL 在该维度的优势不是单点功能而是全量覆盖。
