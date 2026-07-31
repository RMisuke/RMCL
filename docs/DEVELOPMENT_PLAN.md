# RMCCC 开发路线图

> 按 Phase 分阶段交付，每阶段产出可运行的增量版本。

---

## Phase 1: Foundation（Week 1–2）

**目标**：项目能跑起来，能登录，能下载文件。

### Rust 侧
| 任务 | Crate | 产出 |
|------|-------|------|
| Workspace 初始化 | root | `Cargo.toml` workspace，所有 crate 骨架 |
| 路径 & 配置管理 | `rmccc-config` | 跨平台数据目录、日志初始化、热重载配置 |
| 统一错误类型 | `rmccc-core` | `RmcccError` 枚举 + `IpcError` 桥接 |
| 下载引擎 | `rmccc-download` | 并发下载、SHA1 校验、指数退避重试、断点续传 |
| 认证 | `rmccc-auth` | Microsoft OAuth2 device flow + 离线认证 + AES-256-GCM 加密存储 |

### Tauri 侧
| 任务 | 说明 |
|------|------|
| Tauri 2.x 项目初始化 | capabilities、窗口配置、ready-to-show |
| IPC 命令注册框架 | auth 相关 4 个命令 |
| 事件系统骨架 | `events.rs` 常量定义 + `ThrottledEmitter` |

### 前端侧
| 任务 | 说明 |
|------|------|
| SolidJS + Vite 项目初始化 | Tailwind CSS v4、路由、目录结构 |
| 布局骨架 | 三栏卡片布局（参照 UI 设计稿） |
| 登录页 | Microsoft OAuth 流程 + 离线登录表单 |
| IPC 层 | `api.ts` + `events.ts` + `safeInvoke` |
| 错误处理 UI | 根据 `IpcError` 显示 toast / 重试按钮 |

### 验收标准
- [ ] `cargo check --workspace` 零错误
- [ ] `pnpm dev` 启动窗口，显示登录页
- [ ] Microsoft OAuth 登录成功，token 加密存储
- [ ] 离线登录可用
- [ ] 下载单个文件并校验 SHA1

---

## Phase 2: Core Game（Week 3–4）

**目标**：能下载并启动原版 Minecraft。

### Rust 侧
| 任务 | Crate | 产出 |
|------|-------|------|
| Mojang API | `rmccc-version` | 版本清单解析、版本详情获取 |
| 离线缓存 | `rmccc-version` | 版本列表本地缓存，断网降级 |
| 资源下载 | `rmccc-version` | Asset index 解析、libraries/natives 下载 |
| JVM 检测 | `rmccc-launch` | 自动发现系统 Java，版本匹配 |
| 启动参数构建 | `rmccc-launch` | Classpath 拼接、response file、参数模板 |
| 进程管理 | `rmccc-launch` | 启动游戏进程、stdout/stderr 捕获 |
| Profile 管理 | `rmccc-profile` | 创建/删除/更新/列表 profile |

### Tauri 侧
| 任务 | 说明 |
|------|------|
| version_* IPC 命令 | 列表 + 安装 |
| launch_* IPC 命令 | 启动 + 终止 + 状态查询 |
| profile_* IPC 命令 | CRUD 四个操作 |

### 前端侧
| 任务 | 说明 |
|------|------|
| 主界面 | 三栏卡片布局完善（服务器选择 + 版本选择 + 启动） |
| 版本安装流程 | 进度条 + 完成提示 |
| 游戏启动 | Launch 按钮 + 日志输出面板 |
| Profile 管理 | 创建/切换/删除 profile |

### 验收标准
- [ ] 能列出 Mojang 官方版本列表
- [ ] 能下载并安装一个 MC 版本（client + assets + libraries）
- [ ] 能检测到系统 Java 并校验版本
- [ ] 能成功启动 Minecraft 并进入游戏
- [ ] 游戏日志实时显示在 UI 中

---

## Phase 3: Mod Loaders（Week 5–6）

**目标**：支持 Fabric / Forge / NeoForge 安装。

### Rust 侧
| 任务 | Crate | 产出 |
|------|-------|------|
| Fabric 安装 | `rmccc-modloader` | Meta API 查询 + profile JSON 生成 + JAR 下载 |
| Forge 安装 | `rmccc-modloader` | Installer JAR 下载 + 无头安装执行 |
| NeoForge 安装 | `rmccc-modloader` | 同 Forge，适配 NeoForge API |

### Tauri 侧
| 任务 | 说明 |
|------|------|
| modloader_* IPC 命令 | 列表 + 安装 |

### 前端侧
| 任务 | 说明 |
|------|------|
| Mod loader 选择 UI | 在 profile 详情中选择/切换 loader |
| 安装进度 | 安装过程进度反馈 |

### 验收标准
- [ ] 能为已有 profile 安装 Fabric loader
- [ ] 能为已有 profile 安装 Forge loader
- [ ] 能为已有 profile 安装 NeoForge loader
- [ ] 安装后能正常启动带 loader 的 Minecraft

---

## Phase 4: Mod Management（Week 7–8）

**目标**：搜索、安装、管理 Mod。

### Rust 侧
| 任务 | Crate | 产出 |
|------|-------|------|
| Modrinth API | `rmccc-mods` | 搜索、详情、版本列表、下载链接 |
| CurseForge API | `rmccc-mods` | 同上 + API key 管理 |
| 本地 Mod 管理 | `rmccc-mods` | 安装/卸载/启用/禁用（移动到 disabled/ 子目录） |
| 依赖解析 | `rmccc-mods` | 检测并提示缺失依赖 |

### Tauri 侧
| 任务 | 说明 |
|------|------|
| mod_* IPC 命令 | search / install / remove / list / toggle |

### 前端侧
| 任务 | 说明 |
|------|------|
| Mod 浏览器 | 搜索框 + 结果列表（虚拟滚动） |
| Mod 详情页 | 版本选择 + 依赖提示 + 安装按钮 |
| Mod 管理列表 | 已安装 mods 列表，toggle 开关，删除按钮 |

### 验收标准
- [ ] 能搜索 Modrinth 上的 mod
- [ ] 能搜索 CurseForge 上的 mod（需 API key）
- [ ] 能安装 mod 到指定 profile
- [ ] 能启用/禁用已安装的 mod
- [ ] 缺失依赖时有提示

---

## Phase 5: Polish & Ship（Week 9–10）

**目标**：生产级质量，可发布。

### 任务清单

| 类别 | 任务 | 说明 |
|------|------|------|
| 性能 | `cargo bloat` 审计 | 确保最终 binary < 10MB |
| 性能 | 前端 bundle 分析 | `vite build --mode production` 检查产物大小 |
| 性能 | `pool_max_idle_per_host` 调优 | 大量小文件下载的连接池优化 |
| 测试 | 单元测试补全 | 所有 crate 关键路径覆盖 |
| 测试 | 集成测试 | wiremock 模拟 Mojang/Modrinth/CurseForge API |
| 测试 | E2E 测试 | tauri-driver 覆盖 happy path |
| CI/CD | GitHub Actions | 三平台构建 + 自动发布 |
| 功能 | 自动更新 | tauri-plugin-updater |
| 功能 | 系统托盘 | 游戏运行时最小化到托盘 |
| 功能 | 窗口状态持久化 | tauri-plugin-window-state |
| 功能 | Deep link | `rmccc://profile/{id}` 一键导入 |
| UI | 暗色主题 | 完整的 dark mode（当前默认即为暗色） |
| UI | 响应式布局 | 窗口缩放时卡片自适应 |
| 文档 | README | 安装说明 + 使用指南 + 开发者文档 |

### 验收标准
- [ ] 三平台（Windows/macOS/Linux）构建产物均 < 10MB
- [ ] CI 全绿：cargo test + clippy + fmt + typecheck
- [ ] E2E 测试覆盖：登录 → 建 profile → 安装版本 → 启动游戏
- [ ] 自动更新可用
- [ ] 无 P0/P1 未解决 bug
