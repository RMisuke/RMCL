# Windows 兼容性审计与整改（2026-11-01）

> **触发背景**：准备把项目搬到 Windows 上编译与测试，需要先确认"代码里有多少东西在
> Windows 上跑不起来"，以及"哪些 macOS 专属实现必须先改掉"。
> **审计范围**：`crates/`、`src-tauri/`、`src-cli/`、`src-logic-host/`、`src/`、`.github/workflows/`、
> `src-tauri/tauri.conf.json`、`src-tauri/icons/`。
> **本文性质**：审计结论 + 本轮整改销账台账。未销账项一律标"待原生 Windows 验收"，
> 不以"代码已写"代替"已验收"。

---

## 0. 结论摘要

1. **架构层面没有阻塞**：平台差异已经收在 `crates/rmccc-platform`（ACL / 进程树 / ShellExecuteW /
   Java 探测 / 内存 / WinRT 转码），领域 crate 里没有裸 `#[cfg(target_os)]` 执行分支，
   classpath 分隔符、`exe` 后缀、native classifier、`-Xdock:name` 都是描述符数据。
2. **真正的问题在"适配层之外"**：图标资源、逻辑插件 helper 的可执行名、JVM 日志编码、
   权限收紧的失败语义、前后置命令的 shell、数据目录位置、WinRT 转码的写入方式与目标编码。
   这些都不在 `rmccc-platform` 的 trait 边界内，因此历次"平台适配完成度"盘点看不见它们。
3. 本轮修复 7 项（P0×2、P1×5），并把打包/发布链路里会静默吞掉 Windows 产物的问题一并修掉。
4. **仍未验收**：Windows 原生编译、ACL/Java/进程/WebView2/壁纸的实机行为、NSIS/MSI 安装包、
   插件隔离 UI（ADR-006 仍 release-blocked）。在原生 Windows 跑通前，文档口径保持
   `deferred / release-blocked`。

---

## 1. 审计方法与限制

- **静态审计**：逐项读 `rmccc-platform` 各后端、`rmccc-launch` 的启动链路、`rmccc-config` 的路径与
  日志、`rmccc-logic` 的 helper 启动、`src-tauri` 的 IPC/协议/图标缓存、前端缓存与窗口壳。
- **产物核对**：`file` / `xxd` 校验 `src-tauri/icons/*` 的真实格式；`Cargo.lock` 核对
  `windows`（0.61.3）、`ico`（0.5.0）、`tauri-winres`（0.3.6）实际参与构建的依赖。
- **限制**：本机只有 macOS（Apple Silicon），**没有**运行过 `cargo check --target x86_64-pc-windows-msvc`，
  因此 `windows.rs` / `video_windows.rs` 的 Win32/WinRT 调用签名是否与 `windows` 0.61 匹配，
  只能由首次原生 Windows 编译确认（CI 的 `windows-latest` job 已覆盖 `cargo check` / `test`）。

---

## 2. 发现清单与处置

| 编号 | 严重度 | 问题 | 证据 | Windows 影响 | 处置 |
|---|---|---|---|---|---|
| W-1 | P0 | 五个图标文件都是同一个 **1×1 PNG**（各 70 B）冒充 `.ico`/`.icns` | `src-tauri/icons/*`；`file` 报 `PNG image data, 1 x 1` | `tauri-build → tauri-winres → ico` 解析 `icon.ico` 失败 → **构建中断**；即使侥幸嵌入，exe/任务栏/托盘/安装包图标全空 | ✅ 用上传图重新生成（见 §3） |
| W-2 | P0 | `resolve_helper()` 用 `with_file_name("SNGL-logic-host")`，Windows 上真实文件名是 `rmccc-logic-host.exe` | `crates/rmccc-logic/src/supervisor.rs:313` | `is_file()` 恒假 → 所有逻辑插件报 "helper not found" | ✅ 按 `EXE_SUFFIX` 拼名 + 无后缀回退 |
| W-3 | P1 | JVM 用 `file.encoding`（中文 Windows = GBK）输出日志，日志泵却用 `BufReader::lines()`（UTF-8 only） | `descriptor.rs` `default_encoding()` + `managed.rs` `read_lines()` | 第一条非 UTF-8 行 → `InvalidData` → 泵退出：**日志冻结**，管道写满后**游戏进程阻塞** | ✅ 日志流强制 UTF-8（`stdout_encoding`）+ 泵改字节级 lossy 解码 |
| W-4 | P1 | `build_response_file` 把 `restrict_owner_only` 当硬前提（`?` 传播） | `crates/rmccc-launch/src/process.rs:46` | `icacls` 在 exFAT/FAT32 移动盘、网络盘、受限账户上失败 → **游戏装在移动盘就无法启动** | ✅ 降级为 warn + 继续（文件仍在 5s 后被删除） |
| W-5 | P1 | 前后置命令 `split_whitespace()` 后直接 `Command::new(tokens[0])`，不经 shell | `default_launcher.rs:1181` `run_external_command` | `C:\Program Files\...` 必炸、`.bat` 与内建命令不执行、引号参数被拆碎、闪黑框 | ✅ `cmd /C` / `sh -c` 包装 + `hide_console_window` |
| W-6 | P1 | 数据目录取 `dirs::data_dir()`（Windows = `%APPDATA%` Roaming） | `crates/rmccc-config/src/paths.rs:8` | GB 级 `assets/`+`libraries/` 进 Roaming，域/漫游配置环境每次登录同步 | ✅ 改 `data_local_dir()`（`%LOCALAPPDATA%`）+ 旧目录一次性迁移 |
| W-7 | P1 | WinRT 转码先 `File::create(output)` 再交给 MediaTranscoder；输出目标为 HEVC | `video_windows.rs:45` | 目标文件已存在 → `PrepareFileTranscodeAsync` 失败；HEVC 需商店付费"HEVC 视频扩展"，WebView2 默认播不了 | ✅ 删预建文件 + 目标改 H.264（`CreateMp4`）+ 缓存 key 分后端 |
| W-8 | P1 | 发布产物路径写死 `src-tauri/target/release/bundle`，但 workspace 产物在仓库根 `target/` | `.github/workflows/release.yml:73`，本地口径见 `AGENTS.md` | Windows 安装包不会进 release artifact | ✅ 改 `target/**/release/bundle/**/*` |
| W-9 | P1 | `TAURI_SIGNING_PRIVATE_KEY_PATH` 兜底 `/tmp/rmccc.key`（Windows 无 `/tmp`，且文件永不存在的路径） | `release.yml` | Windows runner 上 updater 签名配置不成立 | ✅ 改用 secret 内容注入，未配置则跳过 |
| W-10 | P2 | `bundle` 无 `resources`/`externalBin`：`SNGL-logic-host` 不进安装包 | `tauri.conf.json` | 打包版逻辑插件失效（各平台） | ✅ `externalBin` + `pnpm prep:host` 暂存 |
| W-11 | P2 | `bundle.windows.webviewInstallMode` / `plugins.updater.windows.installMode` 缺失 | `tauri.conf.json` | WebView2 安装策略与更新安装模式落到 Tauri 默认值，行为不确定 | ✅ 显式配置 |
| W-12 | P2 | E2E job 跑在 `macos-14`，而 `tauri-driver` **不支持 macOS**；测试还把 caps 换成 `chrome()` 并 `goto` 外部 URL | `.github/workflows/ci.yml:101`、`src-tauri/tests/e2e_smoke.rs` | 唯一的 WebView2 自动化窗口被浪费，E2E 实际从未驱动过应用 | ✅ 迁到 `windows-latest` + 改用 `tauri:options.application` |
| W-13 | P2 | `transparent: true` + `decorations: false` 只在 macOS 验证 | `tauri.conf.json:25-26` | Windows 丢 DWM 阴影/Aero Snap，Win10 透明+最大化表现待测 | 🟡 代码侧已处理：关闭 DWM 非客户区渲染（矩形阴影/1px 边框）、`set_shadow(false)`、系统圆角关闭；最大化时 CSS 圆角归零；待 Windows 实机（见 §5） |
| W-16 | P1 | 主程序未声明 `windows_subsystem`；游戏 `java.exe` spawn 未设 `CREATE_NO_WINDOW` | `src-tauri/src/main.rs:1`；`crates/rmccc-launch/src/process.rs:156` | release 主程序弹控制台；启动游戏闪黑框 | 🟡 代码已加 `cfg_attr` + `hide_console_window`；待 Windows 实机 |
| W-14 | P2 | 长路径（>260）无 `longPathAware`，深目录整合包可能失败 | — | 解压/安装报 OS error 108 | ⏳ 待实机确认触发面 |
| W-15 | 已知降级 | 插件隔离 UI 在 `platform_id` 以 `windows` 开头时禁用 | `crates/rmccc-plugin/src/manager.rs:278`、ADR-006 | 插件卡片/页面降级为声明式 | ⏳ release-blocked（非本轮范围） |

> **两处后续变更（2026-11-05 登记，避免按本表字面执行）**：
> 1. W-2 的判据仍然成立（Windows 必须带 `.exe`，并有裸名回退），但**交付文件名已随产品改名**：
>    2026-11-04 起 helper 二进制为 `SNGL-logic-host[.exe]`（`HELPER_NAME`、`bundle.externalBin`、
>    `scripts/prepare-logic-host.mjs` 同步；crate/包名仍是 `rmccc-logic-host`）。
> 2. W-9 涉及的 updater 签名链路已随 **`tauri-plugin-updater` 于 2026-11-04 下线**而失去对象；
>    `release.yml` 仍注入 `TAURI_SIGNING_PRIVATE_KEY`，属残留配置，待"是否恢复自动更新"决策后清理
>    （见 [`../PROJECT_STATUS.md`](../PROJECT_STATUS.md) 2026-11-05 轮）。

> 已确认**无需修改**的点：`classpath` 分隔符、`exe` 后缀、native classifier、`-Xdock:name`、
> `needs_non_ascii_natives_workaround`、`taskkill /T /F`、`ShellExecuteW`、`GlobalMemoryStatusEx`、
> `icacls` 多候选账户、embeddable Java 目录扫描、`hide_console_window`、图标协议的
> `http://rmcccicon.localhost` 分支、原生拖放 `onDragDropEvent`、argfile 反斜杠转义与延迟删除。

---

## 3. 本轮整改明细（逐项销账）

### W-1 图标

- 源图：用户提供的品牌图（1115×1113，非正方形）→ 居中补透明边成正方形，
  存为 `icon resource/app-icon.png`（后续唯一再生成入口）。
- 生成：`pnpm tauri icon "icon resource/app-icon.png"`，产物落在 `src-tauri/icons/`。
- 校验：`icon.ico` → `MS Windows icon resource - 6 icons`；`icon.icns` → `Mac OS X icon`；
  `32x32.png`/`128x128.png`/`128x128@2x.png` → 真实 PNG 位图。
- 顺手清理：删除 CLI 顺带生成的 `icons/android/`、`icons/ios/`（本项目只做桌面）。

### W-2 helper 可执行名

- `helper_file_name()` 按 `std::env::consts::EXE_SUFFIX` 拼名；查找顺序：带后缀 → 裸名（兼容手拷/包装脚本）。
- 新增单测 `helper_file_name_carries_the_platform_exe_suffix`（Windows 断言 `.exe`，Unix 断言裸名）。

### W-3 日志编码

- `PlatformDescriptor` 拆成两个概念：`default_encoding()`（`file.encoding`，Windows 保持 GBK，
  避免老 mod 读写配置文件乱码）与 `stdout_encoding()`（日志流，**所有平台 UTF-8**）。
- `default_launcher` 分别写入 `-Dfile.encoding` 与 `-Dsun|stdout.encoding`。
- `managed::read_lines` 从 `BufReader::lines()` 改为 `read_until(b'\n')` + 去 CR/LF +
  `String::from_utf8_lossy`：任何编码的字节都不会终止泵。
- 新增回归测试 `read_lines_survives_non_utf8_bytes`（喂 GBK 字节 + CRLF + 无换行末行）。

### W-4 权限收紧降级

- argfile：`restrict_owner_only` 失败 → `warn` 并继续启动（日志里写明暴露窗口与文件路径）。
- `WindowsFs` 的 ACL 错误消息补充"该卷可能不支持 NTFS ACL（exFAT/FAT32/网络盘），敏感文件请放
  `%LOCALAPPDATA%`"，让失败可诊断。
- 凭据 vault **仍然**保持 fail-closed（密钥文件不能悄悄变成全局可读），只改错误文案。

### W-5 前后置命令

- `run_external_command` 增 `shell_is_windows` 参数（由 `PlatformDescriptor` 注入，领域层不写 `cfg`）：
  Windows `cmd /C <整串>`，其余 `sh -c <整串>`；并统一 `hide_console_window`。

### W-6 数据目录

- `data_root()` = `data_local_dir()` → 回落 `data_dir()`；macOS 上两者相同，行为不变。
- `ensure_dirs()` 前执行 `migrate_legacy_data_dir()`：仅当"新目录不存在 + 旧目录存在 + 两者不同"时
  `rename`，失败只告警（数据留在旧位置，用户重建即可）。
- 新增单测 `default_data_dir_prefers_the_local_data_dir`。

### W-7 Windows 壁纸

- `VideoTranscoder::target_codec()` 新增（默认 HEVC，macOS 走 `avconvert`）。
  Windows 覆盖为 **H.264**：干净 Windows 的 WebView2 可直接硬解，HEVC 要另装扩展。
- `video_windows.rs`：`CreateHevc` → `CreateMp4`；删除 `File::create(output)`（目标已存在会让
  MediaTranscoder 失败）；输出目录按需清理同名残留。
- `validate_metadata` 的"期望编码"改为参数（不再写死 HEVC）；
  `PREPARE_RECIPE` 升为 `native-timing-v2`，缓存 key 加入后端名与目标编码，避免 macOS 的 HEVC
  缓存被 Windows 复用。
- 新增测试 `validation_accepts_the_backend_target_codec`。

### W-10 / W-12 打包与 CI

- `tauri.conf.json`：`bundle.externalBin: ["binaries/SNGL-logic-host"]`，
  `beforeBuildCommand` 追加 `pnpm prep:host`；补 `bundle.windows.webviewInstallMode` 与
  `plugins.updater.windows.installMode`。
- `scripts/prepare-logic-host.mjs`：读 `rustc -vV` 的 host triple，`cargo build -p rmccc-logic-host --release`（crate 名不变），
  复制成 `src-tauri/binaries/SNGL-logic-host-<triple>[.exe]`（Tauri 打包时会去掉 triple，
  与 `resolve_helper` 的"同目录"查找对上）；`src-tauri/binaries/` 进 `.gitignore`。
- E2E：job 移到 `windows-latest`（`continue-on-error` 直至 Windows 实机验收完成），
  用 `--no-bundle` 构建后由 `tauri-driver` 驱动；测试改用
  `CapabilitiesHelper::set_base_capability("tauri:options", {application})`，不再 `goto` 外部 URL。

---

## 4. 本机验证（macOS）

| 检查 | 命令 | 结果 |
|---|---|---|
| 格式 | `cargo fmt --all -- --check` | ✅ |
| Lint（全 target，warnings = error） | `cargo clippy --workspace --all-targets -- -D warnings` | ✅ |
| Rust 测试 | `cargo test --workspace` | ✅ 744 passed / 0 failed / 3 ignored（含新增的日志泵、helper 命名、编码选择、缓存 key 测试） |
| 前端类型 | `pnpm typecheck` | ✅ |
| 前端单测 | `pnpm test` | ✅ 67 passed |
| 前端构建 | `pnpm build` | ✅ |
| 桌面打包 | `pnpm tauri build --bundles app` | ✅ 见 §4.1 |

### 4.1 打包产物

```
target/release/bundle/macos/SNGL.app
  Contents/MacOS/SNGL               10,684,976 B   ← strip 后的主可执行（< 20 MiB，ADR-011）
  Contents/MacOS/SNGL-logic-host     1,116,256 B   ← externalBin 生效（on-demand spawn，不计入主程序）
  Contents/Resources/icon.icns      Mac OS X icon, "is32"
```

- 主可执行 10,684,976 B：与整改前（10,684,960 B）基本持平，新增 16 B 为编码选择与
  helper 路径分支的差异。
- 打包前的 `beforeBuildCommand` 实际执行了 `pnpm build && pnpm prep:host`，因此
  `src-tauri/binaries/SNGL-logic-host-<triple>` 在构建时可被 CLI 找到并落入 `Contents/MacOS/`。

---

## 5. 仍需 Windows 原生验收（release-blocked 清单）

1. `cargo check/clippy/test --workspace` 在 `x86_64-pc-windows-msvc` 上通过（含 `windows` 0.61 API 差异与
   `-D warnings` 下的 cfg 未使用告警）。
2. `icon.ico` 能被 `tauri-winres`/`rc.exe` 接受，exe 与任务栏图标正确显示。
3. `icacls` 收紧成功路径（NTFS `%LOCALAPPDATA%`）与失败降级路径（exFAT 移动盘）。
4. Java 探测命中（`JAVA_HOME` / PATH / Program Files 各发行版），且探测不闪控制台。
5. 启动 → 主菜单 → 停止：日志面板持续更新（含中文输出）、`taskkill /T /F` 收干净进程树。
6. 实例/游戏路径含中文、空格、深目录（长路径）。
7. 壁纸：H.264 超大 MP4 的 WinRT 转码成功，且 WebView2 能播放（`<video>` 首帧）。
8. `transparent + decorations:false` 的窗口行为：DWM 系统圆角/边框是否被关闭、CSS 圆角是否无双边、阴影、贴边、最大化、多屏 DPI。
9. release 主程序启动不出现控制台；启动游戏、Forge installer/processor 时均不闪黑框（`windows_subsystem` + `CREATE_NO_WINDOW`）。
10. NSIS/MSI 安装包构建与升级（`bundle.windows` 配置生效）。
11. 插件隔离 UI 的 WebView2 注入问题（ADR-006），解除后需验证 `rmcccplugin://` →
    `http://rmcccplugin.localhost` 的 URL 形态。

---

## 6. 规则沉淀

- **适配层的"完成度"不等于"能跑"**：资源文件、可执行名、编码、打包清单这些"非 trait"的东西
  同样能一票否决。平台适配盘点表应补一列"非代码资产"。
- **两种字符集必须分开命名**：文件字符集（`file.encoding`）与日志流字符集（`stdout.encoding`）
  混用一次就足以让 Windows 上的日志与进程双双卡死。
- **权限收紧是尽力而为，不是前置条件**：只有凭据 vault 允许 fail-closed，其余敏感临时文件
  一律"能收紧就收紧，收不了就告警 + 缩短暴露窗口"。
- **"沉默的成功"最危险**：1×1 的 PNG 图标、`/tmp` 兜底路径、macOS 上的 E2E job，
  都不会让任何门禁变红，却会让 Windows 交付缺一块。
