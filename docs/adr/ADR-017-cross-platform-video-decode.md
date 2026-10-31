# ADR-017: 本地视频壁纸的跨平台硬件解码代理

- **状态**：superseded by [ADR-020](ADR-020-native-resolution-wallpaper-engine.md)（2026-10-31；1080p 降级代理已由原生分辨率 HEVC 引擎取代）
- **日期**：2026-10-30
- **阶段**：Phase 3+（外观/主题）
- **关联**：指南 §5.7（平台适配层）；[ADR-014](ADR-014-theme-v2-appearance-skin.md)；`docs/roadmap/PLATFORM_ADAPTATION.md`；`docs/ui-design/13_NOTES.md`

## 背景

本地 MP4 壁纸由 Tauri 的 `<video>` 播放，解码发生在系统 WebView 内：

- macOS 是 WKWebView + VideoToolbox；
- Windows 是 WebView2（Chromium）+ Media Foundation / D3D11；
- Linux 是 WebKitGTK（本项目暂不承诺）。

H.264 的硬件解码尺寸上限不是无限的。实测 Apple Silicon 对
**4126×2596 H.264** 的 VideoToolbox 初始化失败：

```text
Failed setup for format videotoolbox_vld:
hwaccel initialisation returned error.
```

浏览器内核因此回退软件解码，CPU 占用显著上升。Windows 的 H.264 硬解同样受
GPU/驱动与编码 level 限制，4126 宽也超出常见 4096 单轴上限。

此前的 v3 实现把 `/usr/bin/avconvert` 和 `#[cfg(target_os = "macos")]` 直接写在
Tauri 命令里，违反项目平台适配规则（指南 §5.7.1：领域/应用层不得内联平台执行
分支），且只能解决 macOS。v3 还使用了 HEVC：Windows 未安装 HEVC Video
Extensions 时，WebView2 可能无法解码，不能作为跨平台默认。

另外，用户明确要求代理**与原视频帧率/帧数一致**。`avconvert` 的
`PresetHEVC1920x1080` 在实测源上碰巧保留了 `30000/1001`，但把“预设正好保留”
当成契约并不可靠；必须有显式探测与转码后校验。

## 目标

1. 把视频代理的策略/缓存/编码器全部收归平台适配层；
2. macOS 与 Windows 都能产出**可被本平台 WebView 硬解**的代理；
3. 代理保持源视频的**有理数帧率与总帧数**，不一致就拒绝；
4. 不把 ffmpeg 等大体积二进制塞进 20 MiB 主程序；
5. 平台不支持或转码失败时回退原视频，不 panic、不假称已硬解。

**不解决**：Linux 的 VA-API 后端；HDR/10-bit 色彩管理；在线流媒体。

## 选项

### A. 继续在 Tauri 命令里调用 macOS `avconvert`

- 优点：改动最小，已验证。
- 缺点：Windows 无 `avconvert`；平台分支污染应用层；无法满足跨平台目标。
- 结论：拒绝。

### B. 打包 ffmpeg / libavcodec

- 优点：一次实现全平台，`-r` 可精确控制帧率，`hevc_videotoolbox` /
  `h264_nvenc` / `h264_qsv` 可选。
- 缺点：静态 ffmpeg 远超 20 MiB 体积红线；动态库分发/签名/更新复杂；与
  “使用系统 WebView 硬解”的架构重复。
- 结论：拒绝。

### C. 平台适配层 + 各系统原生媒体 API（采纳）

- **公共层**（`rmccc-platform::video`）：MP4 元数据探测、阈值判断、内容哈希
  缓存、转码后帧率/帧数/时长校验、临时文件清理。
- **macOS**：系统自带 `/usr/bin/avconvert`，`Preset1920x1080` 输出 H.264。
- **Windows**：WinRT `Windows.Media.Transcoding.MediaTranscoder`，
  `SetHardwareAccelerationEnabled(true)`，输出 H.264。
- **Linux/其它**：`UnsupportedPlatform`，回退原视频（deferred）。

### D. 不转码，依赖 WebView 自动硬解

- 优点：零缓存、零转码。
- 缺点：源尺寸超限时必然软件解码；用户已实测到 CPU 问题。
- 结论：只作为代理失败时的回退路径。

## 决策

采纳 **C**，并补充以下硬性约束：

1. **代理编码固定为 H.264**（不是 HEVC）。1080p H.264 在 Apple Silicon 与
   Windows 的 D3D11/DXVA 上都是普遍硬解路径；Windows 不依赖 HEVC 扩展。
2. **目标尺寸**：最长边 1920，保持宽高比，宽高取偶数（4:2:0 要求）。
   4126×2596 → 1920×1208。
3. **帧率/帧数**：纯 Rust 解析 MP4 的 `tkhd/mdhd/hdlr/stsd/stts/stsz`，得到
   有理数帧率（如 30000/1001）与总帧数；转码后用同一解析器校验输出，
   帧数必须完全相等，帧率误差 ≤ 0.02 fps，时长误差 ≤ 200 ms。
4. **触发条件**：宽 > 4096 或高 > 2304。低于阈值时 WebView 本来就走 GPU，
   不重复转码。
5. **缓存**：`{data_dir}/cache/background-video/<sha1>.mp4`，哈希含源路径、
   文件大小/mtime、宽高、帧率、帧数、编码与配方版本 `h264-1080-fps-v3`。
   配方或校验规则变化时递增版本，旧缓存自然失效。
6. **失败回退**：任何一步失败（后端不支持、转码失败、校验失败）都返回
   `None`，前端播放原视频并在设置页显示提示；不阻塞启动。
7. **平台约束**：`#[cfg]` 只在 `rmccc-platform` 后端文件与选择器出现；
   Tauri 命令只调用 `Platform::video`。

## 后果

正面：

- macOS 与 Windows 共用同一套策略/缓存/校验；Tauri 层零平台分支。
- 代理 H.264 1080p 的兼容性优于 HEVC，Windows 不需要额外编解码器。
- 帧率/帧数不一致会显式失败，避免“看起来能播但节奏不对”。
- 不新增 macOS 运行依赖（`avconvert` 是系统自带），Windows 只用系统 WinRT。

负面/代价：

- 首次为超大视频生成代理需要一次全片转码。实测源 128s、4126×2596 →
  1920×1208 H.264 约 **145s**，缓存约百 MB；之后启动直接复用。
- Windows 后端使用 `MediaTranscoder`，其硬件编码器是否存在取决于 GPU/驱动；
  没有硬件编码时仍可软件编码出标准 H.264（播放端仍可硬解），但首次耗时更长。
- Linux 仍无代理后端，超大视频只能回退软件解码（deferred）。
- 纯 Rust MP4 探测不支持 fragmented MP4；这类文件回退原视频。

## 迁移影响

- `src-tauri/commands/external.rs`：删除 `avconvert`、`Sha1`、macOS `#[cfg]`
  与缓存逻辑；命令签名从 `(path, width, height)` 改为 `(path)`。
- 前端 `backgroundMedia.prepare(path)` 同步去掉宽高参数；阈值判断仍用
  `videoWidth/videoHeight` 决定何时请求代理。
- 旧 v3 的 HEVC 代理哈希与 `hevc1080` 配方不匹配，不会被新代码命中；可在
  “设置 → 存储管理”清理。
- 回滚：把 `Platform::video` 换回 `UnsupportedVideoTranscoder` 即可；原视频
  路径不受影响。

## 跨平台验证方式

- **macOS（本机，已实测 2026-10-30）**：
  - `probe_mp4` 解析源：4126×2596、30000/1001、3840 帧、128128 ms；
  - `avconvert Preset1920x1080` 输出：1920×1208、30000/1001、3840 帧、
    128128 ms、H.264；
  - `ffmpeg -hwaccel videotoolbox` 对 1080p H.264 测试通过；
  - 真实 `.app` 冷启动加载代理，不重复转码。
- **Windows（deferred / release-blocked）**：
  - 需在原生 Windows CI 上首次编译 `windows` 0.61 WinRT 后端；
  - 用 NVIDIA/Intel/AMD 各一台验证 `MediaTranscoder` 输出 H.264 1080p、
    WebView2 实际走 D3D11 硬解（GPU 进程/任务管理器 GPU Video Decode）；
  - 未通过前不得在发布说明中宣称 Windows 硬解已验收。
- **Linux（deferred）**：VA-API/GStreamer 后端未实现，返回
  `UnsupportedPlatform` 并回退原视频。

## 状态变更记录

- 2026-10-30：accepted；macOS H.264 代理与帧率/帧数校验实测通过；
  Windows WinRT 后端代码落地，原生 CI 验证 deferred。
