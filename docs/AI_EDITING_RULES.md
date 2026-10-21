# AI 编辑规范

> 供 AI 助手（Claude / Copilot / Cursor 等）在修改 RMCCC 代码时必须遵循的规则。

---

## 1. 通用原则

1. **先读后写**：修改任何文件前必须先读取完整内容，确认上下文再动手。
2. **最小改动**：只改必要的部分，不做无关重构。如果发现了无关问题，单独提出来而不是顺手改掉。
3. **编译优先**：每次改动后确保 `cargo check --workspace` 和 `pnpm typecheck` 都能通过。
4. **注释语言**：代码注释、commit message、文档正文使用英文；与用户的对话使用中文。

## 2. Rust 代码规范

### 2.1 风格
- 使用 `cargo fmt` 格式化（默认 rustfmt 配置）。
- `cargo clippy --workspace -- -D warnings` 必须零警告。
- 公开 API 必须写 `///` doc comment，包含 `# Errors` 和 `# Examples`（对于复杂函数）。

### 2.2 错误处理
- 所有错误使用 `rmccc-core` 中的 `RmcccError` 枚举。
- 不允许使用 `unwrap()` 或 `expect()` 在非测试代码中（除非有明确注释说明为何安全）。
- IPC handler 统一返回 `Result<T, IpcError>`，使用 `From<RmcccError>` 自动转换。
- 新增错误变体时必须同步更新 `IpcError::from()` 映射和前端 `ErrorCode` 枚举。

### 2.3 Crate 边界
- `rmccc-core` 是唯一的编排层，其他 crate 不得互相依赖（见架构图）。
- `rmccc-config` 是叶子节点，不得依赖其他业务 crate。
- `rmccc-download` 不感知业务逻辑（不知道什么是 "mod" 或 "version"），只负责下载+校验。
- 新增跨 crate 功能时，在 `rmccc-core/src/pipeline.rs` 中添加编排函数。

### 2.4 异步
- 所有 I/O 操作使用 async/await，禁止 `std::thread::spawn` 做 I/O。
- 共享状态使用 `Arc<RwLock<T>>`（tokio）或 `Arc<DashMap<K,V>>`，禁止 `Mutex` 跨 await point。

### 2.5 安全
- Token / API Key 不得出现在日志中（`tracing` span 中使用 `skip` 标记敏感字段）。
- 启动参数通过 response file 传递，不在命令行暴露 `--accessToken`。
- `.key` 文件权限 Unix `0o600`，Windows ACL 限制为当前用户。

## 3. 前端代码规范

### 3.1 风格
- TypeScript strict mode，不允许 `any`（除非对接第三方无类型库且有 `// @ts-expect-error` 注释）。
- 使用 SolidJS 原语（`createSignal`, `createStore`, `createMemo`, `createResource`），不引入额外状态管理库。
- 组件使用 PascalCase 文件名，hooks 使用 `use` 前缀。

### 3.2 IPC 调用
- 所有 `invoke()` 调用必须通过 `src/ipc/api.ts` 中的封装函数，不允许在组件中直接调用 `invoke`。
- 错误处理统一使用 `safeInvoke` 包装，根据 `error.retryable` 和 `error.code` 决定 UI 行为。

### 3.3 事件监听
- 所有 Tauri 事件名使用 `src/ipc/events.ts` 中的常量，不允许硬编码字符串。
- 事件监听必须在 `onCleanup` 中 unlisten。

### 3.4 样式
- Tailwind CSS v4 utility-first，禁止内联 style 对象（除动态计算值外）。
- 颜色使用 Tailwind 配置中的自定义色值（`--color-rmccc-red` 等），不直接写 hex。

### 3.5 构建联动（必读）
- 前端产物由 **`tauri::generate_context!` 在编译期嵌入二进制**，不是运行时读 `dist/`。
  所以"只跑 `pnpm build`"对已构建的 `.app` **没有任何作用**；改完前端必须重新
  `pnpm tauri build --bundles app`（`src-tauri/build.rs` 已对 `../dist` 发 `rerun-if-changed`，
  dist 变化会强制重编译 `rmccc`）。
- 只 `pnpm build` 不重编译的典型症状：窗口打开后是**透明矩形**（页面拿不到资源）。
  `src-tauri/src/bundle.rs` 会在启动时校验 asset map 并把缺失清单画到窗口里；
  `index.html` 的内联 boot guard 负责模块加载失败/未挂载的情况。看到这两处报错就是构建产物过期。
- 运行期前端异常的日志通道只有 `api.ts` 的 `reportFrontendMessage()`（→ IPC `frontend_log`
  → `logs/rmccc.<date>.log`）：release 构建没有 Web Inspector，`console.*` 在进程外不可见。

## 4. 提交规范

### Commit Message Format
```
<type>(<scope>): <description>

[optional body]
```

- **type**: `feat` | `fix` | `refactor` | `docs` | `test` | `chore` | `perf` | `ci`
- **scope**: crate 名或 `ui` / `tauri` / `ci`
- **description**: 英文，小写开头，不加句号，≤72 字符

示例：
```
feat(download): add HTTP Range resume support
fix(auth): handle expired token refresh race condition
refactor(core): extract pipeline orchestration from command handlers
```

### PR 要求
- 每个 PR 只包含一个逻辑变更（不要把不相关的改动混在一起）。
- 必须有 `cargo test --workspace` 和 `pnpm typecheck` 通过的 CI 状态。
- 如果改动涉及用户可见行为，附上截图或录屏。

## 5. 测试规范

- 每个 crate 的 `src/` 下使用 `#[cfg(test)] mod tests` 写单元测试。
- 集成测试放在对应 crate 的 `tests/` 目录下。
- 需要 mock HTTP 的测试使用 `wiremock` crate，fixture 数据放在 `tests/fixtures/`。
- 前端组件测试（如后续添加）使用 Vitest + @solidjs/testing-library。

## 6. 文件组织

- 新增 Rust 模块时必须更新对应 crate 的 `mod.rs` / `lib.rs`。
- 新增 IPC command 时必须同时更新：
  1. `src-tauri/src/commands/` 下的 handler
  2. `src-tauri/src/commands/mod.rs` 的 `register` 函数
  3. `src/ipc/api.ts` 的 TypeScript wrapper
  4. `src/ipc/types.ts` 的类型定义（如果是 ts-rs 自动生成则只需加 derive）
  5. `docs/ARCHITECTURE.md` 的 IPC Command Table
