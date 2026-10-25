# 02 · src-tauri/ 审计（Tauri 2.x 应用层，99 条 IPC 命令，约 9.5 千行）

> 审计对象：`src-tauri/src/`（main.rs, handlers.rs, state.rs, events.rs, commands/, mcp/, ipc_tests.rs, bindings.rs, bundle.rs）、Cargo.toml、tauri.conf.json、capabilities/。
> 定级同 01 文档。总计：**0 P0 / 2 P1 / 14 P2 / 12 P3**。

---

## 第一节：IPC 命令面完整清单（99 条，注册表 = handlers.rs:14-117）

注册机制：`main.rs:223` → `handlers::invoke_handlers()` → `tauri::generate_handler!`（handlers.rs:127-246）。`grep '#\[tauri::command\]'` 计 99 处，与注册表一致，**不存在"定义了但未注册"的命令**。`ipc_tests.rs:71-102` 双向交叉校验清单↔宏、`ipc_tests.rs:105-142` 校验前端 api.ts 覆盖、`ipc_tests.rs:145-153` 锁定总数 99。

| 域 | 命令（定义位置） |
|---|---|
| external (2) | open_external_url (commands/external.rs:12)、dialog_pick_directory (external.rs:45) |
| cache (4) | cache_icon_fetch (commands/cache.rs:263)、cache_icon_stats (cache.rs:432)、cache_icon_set_config (cache.rs:449)、cache_icon_clear (cache.rs:478) |
| auth (9) | auth_microsoft_login (commands/auth.rs:42)、auth_offline_login (auth.rs:94)、auth_logout (auth.rs:136)、auth_get_session (auth.rs:219)、auth_skin_url (auth.rs:151)、auth_refresh_token (auth.rs:239)、auth_list_accounts (auth.rs:261)、auth_switch_account (auth.rs:271)、auth_remove_account (auth.rs:289) |
| download (3) | download_cancel (commands/download.rs:10)、download_pause (download.rs:21)、download_resume (download.rs:32) |
| launch (3) | launch_start (commands/launch.rs:57)、launch_kill (launch.rs:128)、launch_status (launch.rs:154) |
| logs (2) | log_get (commands/logs.rs:11)、log_clear (logs.rs:50) |
| ui (1) | frontend_log (commands/ui.rs:22) |
| ai (5) | ai_config_get (commands/ai.rs:98)、ai_config_set (ai.rs:118)、ai_models_probe (ai.rs:176)、ai_analyze_log (ai.rs:271)、ai_analyze_cancel (ai.rs:407) |
| modloader (2) | modloader_list (commands/modloader.rs:35)、modloader_install (modloader.rs:54) |
| mods (11) | mod_search (commands/mods.rs:13)、mod_get_versions (mods.rs:53)、mod_get_download_url (mods.rs:70)、mod_list (mods.rs:90)、mod_toggle (mods.rs:145)、mod_install (mods.rs:236)、mod_install_local (mods.rs:287)、mod_pick_and_install (mods.rs:339)、mod_remove (mods.rs:406)、mods_dependency_scan (mods.rs:499)、mods_apply_plan (mods.rs:644) |
| profile (4) | profile_list (commands/profile.rs:50)、profile_create (profile.rs:58)、profile_update (profile.rs:107)、profile_delete (profile.rs:224) |
| settings (6) | settings_get (commands/settings.rs:46)、settings_set (settings.rs:53)、settings_get_user (settings.rs:207)、settings_set_user (settings.rs:216)、settings_supported_locales (settings.rs:262)、java_detect (settings.rs:155) |
| version (2) | version_list (commands/version.rs:11)、version_install (version.rs:42) |
| instance (9) | instance_list (commands/instance.rs:79)、instance_refresh (instance.rs:144)、instance_rename (instance.rs:199)、instance_duplicate (instance.rs:221)、instance_remove (instance.rs:243)、instance_check_completion (instance.rs:257)、instance_repair (instance.rs:289)、instance_resolved_settings (instance.rs:326)、instance_paths (instance.rs:358) |
| plugin (15) | plugin_list (commands/plugin.rs:34)、plugin_inspect (plugin.rs:47)、plugin_install (plugin.rs:57)、plugin_pick_and_install (plugin.rs:69)、plugin_enable (plugin.rs:103)、plugin_disable (plugin.rs:113)、plugin_remove (plugin.rs:123)、plugin_rollback (plugin.rs:136)、plugin_grant (plugin.rs:146)、plugin_revoke (plugin.rs:157)、plugin_config_get (plugin.rs:168)、plugin_config_set (plugin.rs:179)、plugin_config_reset (plugin.rs:190)、plugin_capabilities (plugin.rs:200)、plugin_contributions (plugin.rs:209) |
| plugin_ui (1) | plugin_ui_invoke (commands/plugin_ui.rs:209) |
| plugin_logic (2) | plugin_logic_invoke (commands/plugin_logic.rs:72)、plugin_logic_cancel (plugin_logic.rs:144) |
| dashboard (9) | dashboard_get_layout (commands/dashboard.rs:21)、dashboard_save_layout (dashboard.rs:36)、dashboard_reset_layout (dashboard.rs:50)、dashboard_list_widgets (dashboard.rs:61)、dashboard_list_themes (dashboard.rs:70)、dashboard_get_theme_tokens (dashboard.rs:88)、dashboard_set_theme_preference (dashboard.rs:137)、dashboard_get_theme_preference (dashboard.rs:161)、dashboard_get_theme_json (dashboard.rs:171) |
| mcp (9) | mcp_status (commands/mcp.rs:112)、mcp_set_enabled (mcp.rs:118)、mcp_regenerate_token (mcp.rs:133)、mcp_tools_list (mcp.rs:142)、mcp_tools_set (mcp.rs:170)、mcp_sessions_disconnect (mcp.rs:194)、mcp_confirmations_list (mcp.rs:206)、mcp_confirmation_resolve (mcp.rs:226)、mcp_recent_calls (mcp.rs:236) |

**命令面缺口/冗余结论**

- `mod_install_local`（mods.rs:287-334）：把任意本机路径的 jar 复制进实例 mods 目录。唯一调用方是 MCP 工具 `mods.install_local`（mcp/tools.rs:568-576），前端无 wrapper 属刻意为之（ipc_tests.rs:110-112 注明"供脚本/未来拖拽复用，主路径走原生选择器"）。合理的双入口冗余，非缺口。
- BACKEND_ONLY 共 16 条（ipc_tests.rs:109-130）：mod_install_local + 4 条 instance 仓库维护命令（ADR-002 版本视角）+ 9 条 mcp_*（后端已实现，GUI 消费者已随前端重构落地：stores/mcp.ts + Settings MCP 分区）。
- bindings.rs:21-132 的 types 数组覆盖全部 99 条命令的 DTO（ts-rs），且 `test_types_binding_up_to_date`（bindings.rs:164-178）强制生成文件与源同步——类型面与前端 types.ts 同步性良好。
- 缺一项：**events.rs 常量与 src/ipc/events.ts 没有类似命令面的交叉校验测试**（两侧目前手工镜像且含 3 个死常量）。

---

## 第二节：状态与锁（核心症结，★★☆☆☆）

- [P1] src-tauri/src/commands/auth.rs:229（及 auth.rs:248）— auth_get_session / auth_refresh_token 在持有 `core.read()` 守卫（auth.rs:222/242）的情况下调用 try_refresh_if_needed（auth.rs:32-38），后者再次 `state.core.read().await`（auth.rs:36）——tokio RwLock 公平调度下，一旦有写者（如 settings_set）排队，内层 read 会排到写者之后、写者又在等外层 read，**形成嵌套读锁死锁路径**，所有依赖 core 的 IPC 将挂起 — 建议 try_refresh_if_needed 内部改为借用传入的 core，或先 drop 外层守卫。
- [P1] src-tauri/src/commands/version.rs:147-150 — run_install_background 在 `core.read().await` 守卫下执行整个 `install_game`（注释自述"长锁覆盖整个下载过程"），下载可持续数分钟；期间所有写锁命令（settings_set/set_user、plugin_*、dashboard_set_theme_preference）阻塞，且 tokio 公平 RwLock 会让新读请求排在等待的写者之后 → **整包游戏下载期间几乎所有 IPC 卡死** — 建议 pipeline 接受 CoreContext 的浅克隆句柄（内部组件用 Arc）后尽早释放锁。
- [P2] src-tauri/src/commands/launch.rs:91-115 — launch_start 同样在读锁内执行 ensure_launch_ready（可能触发修复下载）+ launch_game（Java 探测/进程 spawn），锁持有时长不可控，与上一条同源。
- [P2] src-tauri/src/commands/auth.rs:75-87 — auth_microsoft_login 持读锁贯穿整个设备码流（等用户在浏览器完成授权，可达分钟级）。
- [P2] src-tauri/src/commands/plugin.rs:51、plugin.rs:172 — plugin_inspect / plugin_config_get 是只读操作却取 `core.write().await`，独占写锁期间阻塞全部读者 — 改 read。
- [P2] src-tauri/src/state.rs:58-63 — ui_http_client 构建失败时 `unwrap_or_else(|_| reqwest::Client::new())` 兜底——兜底 client **没有禁用重定向**，`network.request` 的"30x 不能逃出域名白名单"保证（state.rs:20-22 注释）在兜底分支静默失效。
- [P2] src-tauri/src/commands/plugin_logic.rs:124-137 + 149-157 — 全局 `Arc<std::sync::Mutex<LogicRunner>>`（state.rs:33）把所有插件逻辑调用串行化；invoke 在 blocking 线程持锁最长 30s；`plugin_logic_cancel` 需要同一把锁 → **cancel 在 invoke 进行中会被锁挡住，最长等 30s 才能执行，"中止在途调用"实际失效** — 建议 cancel 改为独立 per-plugin kill 通道。
- [P3] src-tauri/src/main.rs:228-236 — RunEvent::Exit 用 `try_read()` flush FileSaver：退出时若安装仍在进行，flush 被静默跳过，防抖中的最后写盘可能丢失。

## 第三节：阻塞问题（async 命令内的同步 IO/子进程，★★★☆☆）

- [P2] src-tauri/src/commands/settings.rs:156-159 — java_detect 在 async 命令里直接同步调用 `detect_all()`（spawn 多个 `java -version` 子进程，秒级）——未 spawn_blocking。
- [P2] src-tauri/src/commands/mods.rs:508-509、mods.rs:665 — mods_dependency_scan 的 `scan_instance_mods`（逐 jar 解压读 fabric.mod.json）与 mods_apply_plan 的 apply 段都在 async worker 上直接执行（block_in_place 只保护了 apply 段，前面的重扫描没有）。
- [P2] src-tauri/src/commands/cache.rs:186-191 + 337-363 — with_cache 在持进程级 `std::sync::Mutex` 的闭包内做同步文件写（≤2 MiB 图标 + index.json 落盘）；rmcccicon:// 协议路径（main.rs:148）与所有 cache_icon_* 命令共用这把锁，慢盘写会串行化全部图标请求。
- [P3] src-tauri/src/commands/cache.rs:219 — lookup 的 `std::fs::read`（≤2 MiB）在协议 handler 的 async 任务上执行（有 5 分钟缓存缓解，风险低）。
- [P3] src-tauri/src/commands/logs.rs:39-45 — log_get 把整个日志文件 `read_to_string` 再切片，无大小上限、无 spawn_blocking。
- [P3] src-tauri/src/commands/ai.rs:234 — gather_analysis_input 同步 `read_to_string` 整个 latest.log（可为数十 MB）。
- [P3] src-tauri/src/commands/auth.rs:156 — auth_skin_url 每次调用新建 `reqwest::Client`，应复用 `state.http_client`。
- 良好对照：三个文件选择器命令（external.rs:50、mods.rs:346、plugin.rs:75）都正确用了 `spawn_blocking + blocking_pick_*`；插件 UI/图标两个协议都注册为异步协议（main.rs:130/140），不阻塞 WKWebView 主线程。

## 第四节：错误处理一致性（★★★★☆）

- 命令统一返回 `Result<T, IpcError>`：99 条全部一致；plugin_ui_invoke 特意恒返回 Ok 并在线上错误信封里携带 BridgeErrorCode（plugin_ui.rs:126-175）。
- [P2] src-tauri/src/commands/version.rs:128 — `*cb_last_batch.lock().unwrap()`：违反本仓库自定的禁 unwrap 规则（events.rs:161 对同类锁用了 poisoned 恢复）；release `panic = "abort"` 下一旦 poison 即整进程 abort。
- [P3] src-tauri/src/mcp/mod.rs:372 — `app()` 用 `.expect(...)`：MCP 服务任务里的 panic 路径。
- 错误信息泄露评估：IpcError message 会带出本地绝对路径（instance.rs:281、mods.rs:656、ai.rs:238）——前端是本机可信 UI，可接受；对外面（MCP）的 `logs.game` 已做 home→~ 脱敏（mcp/tools.rs:794-797），`settings.get` 递归剔除 token/key/secret/password 键（tools.rs:709-728），未发现 token 外泄路径。未发现静默吞错。

## 第五节：commands/ 各文件逐项

- [P3] src-tauri/src/commands/instance.rs:149-188 — instance_refresh 手工复刻了 list_instances（instance.rs:105-140）的整段逻辑，双份实现有漂移风险 — 抽公共函数。
- [P3] src-tauri/src/commands/modloader.rs:107-113 — "completed" 进度事件把 `installed.version_id` 填进 `game_version` 字段（与 started 事件传 base_game_version 不一致），前端按字段渲染会显示错值。
- [P3] src-tauri/src/commands/launch.rs:62-82 — running_games 的"查重+并发上限"是 check-then-insert，两个并发 launch_start 可同时通过检查（TOCTOU）；影响轻微。
- [P3] src-tauri/src/commands/auth.rs:136-142 — auth_logout 直接 `remove` 活跃账号（连凭据一起删），多账号场景下不自动切换到下一个账号，语义是"删除账号"而非"登出"，与命令名有偏差。
- [P2] src-tauri/src/commands/version.rs:42-92 — version_install 立即返回 Ok，后台失败仅经 `download://error` 事件上报（batch_id 空串占位）；若前端未监听该事件，用户对失败无感知（fire-and-forget 模式，设计取舍需前端配合）。
- 良好项：mods.rs 的 sanitize/validate 双闸（mods.rs:195-231，契约测试覆盖）、launch.rs:135 的 dashmap 守卫先拷贝再 remove（自死锁注释）、launch_kill 失败仍清表项、profile_update 的 override 字段白名单过滤（profile.rs:139-143）。launch_kill 走 platform terminate_tree（进程组）路径正确。

## 第六节：plugin_ui.rs（隔离 UI / Host Bridge）

安全面整体扎实：原始 JSON 由 Rust 重新解析（plugin_ui.rs:209-217）、每条调用重授权（231-244）、network.request 作用域=目标 host 一并授权（221-228）、无重定向 client + 2 MiB/64 KiB 上限 + 30s 超时（345-425）、llm.chat 每插件 6 RPM 限流（531-551）、密钥永不回传（436-437）。

- [P3] src-tauri/src/commands/plugin_ui.rs:482-526 — llm.chat 流式把每个 delta 原样经 `plugin://push` 转发，无节流；长回复高频 delta 会对前端产生事件风暴，可参照 ThrottledEmitter 合并。
- [P3] src-tauri/src/commands/plugin_ui.rs:405-419 — network_request 的响应体以 `String::from_utf8_lossy` 返回，二进制响应会损坏（协议定位即文本桥，可接受，但 schema 未言明）。
- 资源服务 serve_asset（35-104）：ui/ 前缀 + 扩展白名单 + 禁隐藏文件 + 禁 `%` 编码（rmccc-plugin/ui.rs:341-373）、文档响应带 CSP `default-src 'none'`、打包时拒绝 symlink、主题路径校验——**未见路径穿越或 symlink 逃逸路径**。

## 第七节：cache.rs（rmcccicon://）

- 键为 sha1(url)，`file_for` 强制 40 位 hex（cache.rs:146-152）→ 无路径穿越；抓取仅允许 3 个已知图片域名（cache.rs:275-287）、单图标 ≤2 MiB、容量下限 64 MiB 防抖动。
- [P3] src-tauri/src/commands/cache.rs:349-353 — 304 unchanged 提交时若索引中无先前条目，content_type 兜底 "image/png"（与实际类型无关）。
- [P3] src-tauri/src/commands/cache.rs:231 — last_used 只推进内存，进程异常退出索引不落盘，LRU 统计退化（注释已说明取舍）。
- main.rs:140-179 协议 handler：命中 200 + `Cache-Control: max-age=300` + nosniff（近期 no-store→max-age=300 改动有详注释：规避 wry 0.55 scheme responder TOCTOU 闪退，理由成立）；未命中 404 让前端回退。无穿越面。

## 第八节：mcp/（MCP 服务器，★★★★☆）

- [P3] src-tauri/src/mcp/protocol.rs:116-123 — ping/tools/list 只校验 `Mcp-Session-Id` 头非空，不校验会话是否已注册（tools/call 经 check_rate_limit 间接校验）——持 token 客户端可用伪造 session id 无限调用 tools/list 绕过会话速率语义。
- [P3] src-tauri/src/mcp/mod.rs:251-272 — 会话无空闲超时：4 个槽位可被只 initialize 不 DELETE 的客户端永久占用；stop() 也不发 `mcp://session disconnected` 事件，GUI 会话列表不会因停服清空。
- [P3] src-tauri/src/mcp/http.rs:64-68 — hyper 连接无读写超时，loopback-only 下慢速攻击风险低。
- [P3] src-tauri/src/mcp/http.rs:124-196 — POST 未校验 `Content-Type: application/json`（MCP 规范建议）。
- 正面确认：仅 127.0.0.1 绑定 + 对端二次校验（http.rs:55-58、212-214）、Bearer 比较长度先行+XOR 折叠（http.rs:210-226，附单测）、Origin 回环校验防 DNS rebinding（http.rs:138-141、228-236）、请求体 1 MiB 上限、破坏性工具默认 confirm + 120s 超时拒绝（protocol.rs:227-258）、破坏性操作审计 fail-closed（protocol.rs:356-378）、错误以 `isError:true` 成功信封返回符合 MCP 约定。31 工具 = 15 只读/11 操作/5 破坏性 有测试钉死（protocol.rs:401-435）。工具 dispatch 直接复用 commands::*（tools.rs:395-651），与 GUI 同一实现——职责边界正确。

## 第九节：events.rs / 事件面

- [P2] src-tauri/src/events.rs:17-23 — DOWNLOAD_PAUSED / DOWNLOAD_RESUMED / AUTH_STATE_CHANGED 三常量后端无任何发射点，前端同样仅定义不监听——双侧死代码，建议删除或补实现（与前端"暂停/恢复纯乐观 UI"降级形态直接相关：TaskCenter 的暂停按钮目前无后端事件对账）。
- [P3] src-tauri/src/events.rs:177-180 — ThrottledEmitter::emit_immediate 带 allow(dead_code) 预留未用。
- 发射点核对：DOWNLOAD_PROGRESS（version.rs:129，节流 100ms）、DOWNLOAD_COMPLETE/ERROR（version.rs:162/79/181）、AUTH_DEVICE_CODE（auth.rs:53）、LAUNCH_*（launch.rs:30/38/118）、MODLOADER_INSTALL_PROGRESS（modloader.rs:20）、INSTANCES_REFRESHED（instance.rs:190、version.rs:190、modloader.rs:117）、AI_*（ai.rs:360-394）、MCP_*（mcp/mod.rs:355/442/466）、PLUGIN_PUSH（plugin_ui.rs:487）——除 3 个死常量外全部有发射且有消费。

## 第十节：main.rs / 启动顺序 / 协议

- 启动顺序：ensure_dirs → 日志（失败降级 stderr）→ panic hook → types.ts 生成 → Builder（协议注册、插件、setup）→ invoke_handler。合理。
- [P2] src-tauri/src/bindings.rs:143-153（经由 main.rs:121-123 每次启动调用）— 运行时向 `env!("CARGO_MANIFEST_DIR")/../src/ipc/types.ts` 写文件：打包后的 release 在用户机器上该路径来自**构建机**，通常不存在（warn 降级），但若恰好存在会由生产进程改写源码树 — 建议移到 `#[cfg(debug_assertions)]` 或 test。
- [P3] src-tauri/src/main.rs:104-105 — 注释"体积红线 <10MB"与 ADR-011 的 20 MiB 红线不一致（文档漂移）。
- 良好项：bundle 完整性自检 + data: URL 错误页（bundle.rs:55-137）、RMCCC_WEBVIEW_PROBE 诊断、window-state 排除 VISIBLE 保证 setup 里 show() 生效、Exit 时 flush FileSaver。

## 第十一节：Cargo.toml / tauri.conf.json / capabilities

- Cargo.toml：依赖全部走 workspace、无 libc 直依赖、hyper/hyper-util/http-body-util 最小 feature 复用 reqwest 依赖树、dev-only 的 wat/thirtyfour 不进 release——合理。
- Release profile（workspace Cargo.toml:114-123）：`opt-level="s"` + `lto=true` + `codegen-units=1` + `strip=true` + `panic="abort"`——与 20 MiB 目标方向一致；注意 `panic="abort"` 会把任何残余 panic（见 version.rs:128、mcp/mod.rs:372）直接变成闪退。
- [P2] src-tauri/tauri.conf.json:45-48 — updater 已启用且嵌入 pubkey，但 endpoints 是占位符 `https://github.com/<owner>/<repo>/...`，生产环境更新必失败；CI 尚无 latest.json 产线——功能不完整（前端已灰置"即将支持"，口径一致）。
- [P3] src-tauri/tauri.conf.json:2 — $schema 指向 fork `raw.githubusercontent.com/nicois/tauri/...` 而非官方 `schema.tauri.app/config/2`，疑似笔误。
- [P3] src-tauri/tauri.conf.json:30 — macOSPrivateApi: true + transparent + decorations:false（macOS 外观所需），需注意分发渠道限制（记录在案）。
- capabilities/default.json：无 fs/shell/http 插件权限（shell 插件未引入），dialog 仅 allow-open，窗口权限逐项列出——整体收敛良好。唯一放宽点见安全专项 #3。

## 第十二节：ipc_tests.rs 覆盖评估

已覆盖：注册表三向契约（59-153）、错误码映射与 retryable 语义（158-332）、mods 文件名校验（336-369）、URL scheme/loader 白名单（374-398）、profile/settings/auth/logs/instance/launch/version/mods 正反向契约（402-766）、frontend_log（770-802）、plugin_ui 协议形状（806-872）、logic 白名单与端到端（876-1074）。

缺失（P2 级测试债）：
- cache_icon_* 4 条命令与 `decode_icon_uri_path` 无任何契约测试。
- mcp_* 9 条命令、mcp/http 层无 route 级测试（401/403/413/会话头分支）。
- ai_* 5 条命令层无契约测试；auth 命令层（刷新窗口）无测试。
- **events.rs 常量 ↔ src/ipc/events.ts 无同步测试**（命令有、事件没有，事件名拼错只能靠手测发现）。

---

## 安全专项结论

1. **MCP 面**：默认关闭；仅回环双检；Bearer 64-hex token 存 `<data>/mcp-token`（0600）恒时比较；Origin 回环校验；会话上限 4 + 60 RPM/会话（对 tools/call 生效）；破坏性 5 工具默认 confirm、120s 超时即拒、审计 fail-closed；settings.get 递归脱敏 + settings.set 字段白名单；日志工具 home→~ 脱敏。stdio 桥在 src-cli（读 mcp-endpoint/mcp-token 文件转发）。**残余风险均为 P3**：ping/tools/list 不验会话真实性、会话无空闲回收、连接无超时。
2. **协议面**：`rmcccplugin://`（独立源 + CSP `default-src 'none'` + 资源白名单 + 启用/能力双重门禁 + 4 MiB 上限 + 打包拒 symlink）；`rmcccicon://`（sha1 键无穿越 + 域名白名单 + 2 MiB 上限 + nosniff + max-age=300）。**未发现穿越/逃逸路径**。
3. **capabilities**：[P2] `opener:default`（capabilities/default.json:21）授权前端 JS 直接调用 `open_url`（无 scope）与 `reveal_item_in_dir`——绕过 `open_external_url` 命令的 http/https 闸（额外放开 mailto:/tel:，且可在 Finder 中 reveal 任意路径）。前端 XSS 场景下的增量风险小但与自建闸门目标相悖，建议收窄为带 scope 的 `opener:allow-open-url` 或移除。其余权限收敛良好；无 fs、无 shell、无 http 插件。
4. **Token/凭据**：Microsoft token 在 AccountRepo/vault，IPC 层不触碰；AI key 入 vault 且 `AiConfigView` 只回 `has_key`；插件 llm.chat 由宿主代持 key；MCP token 0600 + regenerate 即断会话。**未发现凭据泄露路径**。

---

## src-tauri 健康度小结

| 维度 | 评价 | 关键证据 |
|---|---|---|
| IPC 命令面完整性 | ★★★★★ 99/99 注册+契约测试三向钉死，无孤儿命令 | handlers.rs:14-117、ipc_tests.rs:59-153 |
| Rust↔TS 类型同步 | ★★★★★ ts-rs 全量导出 + 新鲜度测试 | bindings.rs:21-132、164-178 |
| 错误处理一致性 | ★★★★☆ 统一 IpcError；2 处 unwrap/expect 残留（abort 级） | version.rs:128、mcp/mod.rs:372 |
| 锁与并发 | ★★☆☆☆ 核心症结：core RwLock 长持锁（安装/登录/启动）+ 嵌套读锁死锁路径 + LogicRunner 全局串行 | auth.rs:229/36、version.rs:147、launch.rs:91、plugin_logic.rs:124 |
| 阻塞 IO 纪律 | ★★★☆☆ 对话框已 spawn_blocking；Java 探测/jar 扫描/缓存写盘仍占 async 线程 | settings.rs:157、mods.rs:509/665、cache.rs:186 |
| MCP 安全 | ★★★★☆ 回环+Bearer+确认流+审计 fail-closed 齐备；会话/超时细节待补 | mcp/http.rs、protocol.rs、mod.rs |
| 协议面安全 | ★★★★★ 双自定义协议均有白名单/上限/隔离源/CSP | plugin_ui.rs:35-104、cache.rs |
| capabilities | ★★★★☆ 整体最小化；opener:default 一处过宽 | capabilities/default.json:21 |
| 测试 | ★★★★☆ 契约测试体系独树一帜；cache/mcp/ai 命令与事件名同步缺测 | ipc_tests.rs |
| 构建/体积 | ★★★★☆ release profile 与 20 MiB 目标一致；updater 端点占位、schema 指向 fork | workspace Cargo.toml:114-123、tauri.conf.json:2/45-48 |

**最优先修复**：auth.rs 嵌套读锁死锁（P1）→ version.rs/launch.rs/auth 长持读锁（P1/P2）→ plugin_logic cancel 被互斥锁抵消（P2）→ state.rs ui_http_client 兜底丢 no-redirect（P2）→ capabilities opener:default 收窄（P2）。
