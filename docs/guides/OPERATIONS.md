# RMCCC 运维手册（P3-14）

> 面向部署与日常维护。覆盖：数据目录、备份、升级、日志排查、故障排查、安全注意事项。

## 1. 数据目录

默认数据目录（`~/.rmccc` 或平台标准目录，可用 `-d` 覆盖；完整布局见
[`../ARCHITECTURE.md`](../ARCHITECTURE.md) §15.1）：

| 子目录 / 文件 | 内容 | 说明 |
|--------|------|------|
| `config/launcher-settings.json` | 全局设置（$schema 版本化） | 含 `game_root_dir`/`mirror_url` 等 |
| `user-settings.json` | 用户级偏好（语言等） | |
| `state/launcher-state.json` | 运行态（可容忍丢失） | |
| `profiles/` | 实例 Profile（JSON） | 每实例一个文件 |
| `accounts.json` + `.key` | 账户元数据（明文）+ 账号 vault 加密 key | 缺 `.key` 无法解密 |
| `private/account-private-data.json` | 账号私密数据（AES-256-GCM） | |
| `private/agent-credentials.json` + `private/agent.key` | AI 服务凭据 vault（ADR-013，service=`llm`） | 文件名沿用 agent 时代以无缝迁移 |
| `.api_keys/curseforge` | CurseForge API key | |
| `cache/` | 下载缓存（按 SHA1 去重） | 可安全清空 |
| `instances/` | 默认游戏根目录（.minecraft 等价） | 版本/库/资源/mod |
| `logs/` | 滚动日志 | 每日滚动，保留 10 份 |

## 2. 备份

**必须备份**：`config/`、`user-settings.json`、`profiles/`、`accounts.json`、`.key`
与整个 `private/`（含 AI 凭据 vault；缺 `.key` 无法解密账号，缺 `private/agent.key`
无法解密 AI 凭据）。

**可选备份**：`instances/`（游戏本体，体积大，可仅备份配置与 mod）。

**恢复**：停止应用 → 覆盖对应文件 → 重启。

## 3. 升级

- **应用自动更新**：`tauri-plugin-updater` 已启用（需 release 仓库配置 `latest.json` + 签名）。
- **手动升级**：替换应用二进制/安装包，保留数据目录即可（配置 schema 有迁移，`migrate_config` 自动升级）。
- 版本升级前建议备份（见上）。

## 4. 日志排查

日志位于 `{data_dir}/logs/rmccc-*.log`（JSON 结构化，含 `request_id` 可用于跨模块串联）。

```bash
# 查看最近日志
tail -n 200 {data_dir}/logs/rmccc-*.log | jq

# 按 request_id 追踪一次操作
grep '"request_id":"<id>"' {data_dir}/logs/*.log
```

CLI 与 GUI 日志体系一致（均为 JSON 滚动文件）。

## 5. 故障排查

| 症状 | 排查方向 |
|------|----------|
| 启动失败 | 检查 `logs/` 最新日志；确认 `.key` 存在且 32 字节 |
| MS 登录失败 | 设备码是否展示；网络可达 `login.microsoftonline.com`；过期自动重试（最多 3 次） |
| 版本下载慢/失败 | 检查 `mirror_url` 是否配置（如 bmclapi）；看下载失败文件数与 batch_id |
| 启动游戏失败 | 检查 Java 检测；`versions/{id}/{id}.jar` 完整性；资源自动修复日志 |
| 账户解密失败 | `.key` 与 `accounts.json` 不匹配（备份时未同时备份 `.key`） |
| AI 日志分析失败 | 设置页 AI 配置是否完整（地址/模型/密钥）；端点须 HTTPS 或 loopback；`private/agent-credentials.json` 是否随 `private/agent.key` 一起备份 |
| Mod 安装失败 | `mods/` 目录权限；SHA1 校验失败日志 |

## 6. 安全注意事项

- `.key` 与 `accounts/` 属敏感文件，Windows 上 `.key` 已用 `icacls` 限制当前用户；请勿共享。
- 启动参数经 response file 传递（`@argfile`），`--accessToken` 不会出现在 `ps aux`；
  文件以 0600 权限写在游戏根目录，**在 JVM 读完参数后**由后台任务删除（不能更早——
  立即删除会让 JVM 报 `could not open .../.launch-args-*` 而启动失败）。
  上次崩溃残留的 `.launch-args-*` 会在下次启动时清扫。
- 认证失败日志已对 token 类字段脱敏（P3-10）。

## 7. 发布（维护者）

- 打 tag 触发 `release.yml`：三平台打包 + macOS 签名公证 + GitHub Release。
- 自动更新需在仓库 Release 中提供 `latest.json`（tauri-action 生成），并保证 `tauri.conf.json` 的 `endpoints` 指向真实仓库。
- 环境变量：`TAURI_SIGNING_PRIVATE_KEY_PATH`（updater 签名私钥）、`APPLE_*`（macOS 公证）。
