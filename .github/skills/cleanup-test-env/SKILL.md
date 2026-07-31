---
name: cleanup-test-env
description: 全面清理 Rust/Tauri 项目的构建缓存、编译器临时文件、IDE 生成文件、测试实例文件和系统测试残留。适用于需要在测试完成后恢复项目至干净初始状态的场景。触发词：清理环境、cleanup、清除缓存、删除 target、清理测试数据、恢复初始状态。
---

# 全面清理测试环境

## 适用场景

在完成端到端测试、构建验证或长时间开发后，需要彻底清理以下类型文件以恢复项目至干净状态：

- Rust 编译缓存（`target/`）
- JS 依赖与构建产物（`node_modules/`, `dist/`）
- macOS 系统临时测试数据（`/tmp/` 下测试目录）
- 应用支持目录下的实例文件（`~/Library/Application Support/rmccc/`）
- 隐藏元数据文件（`.DS_Store`）
- IDE/编译器临时文件（`flycheck`、增量编译缓存）
- 运行中的游戏/Java 进程

## 执行步骤

按以下顺序执行，每一步均有错误处理（进程/目录不存在时静默跳过）。

### 步骤 1：终止运行中的游戏/Java 进程

清理前必须终止可能仍在运行的 Minecraft 游戏进程，防止文件占用导致删除失败。

```bash
# 终止 Minecraft 主类进程（不存在则静默跳过）
pkill -f "net.minecraft.client.main.Main" 2>/dev/null || true
# 终止所有 Java 游戏进程
pkill -f "java" 2>/dev/null || true
# 等待进程完全退出
sleep 2
```

> **说明**: `2>/dev/null` 丢弃无匹配时的错误信息，`|| true` 确保命令不因非零退出码中断脚本。

### 步骤 2：清理 macOS 系统临时测试目录

删除 `/tmp/` 下所有测试相关的数据目录。

```bash
# 清理 RMCLI 测试数据目录
for dir in /tmp/rmcli-test /tmp/rmcli-test2 /tmp/rmcli-playtest /tmp/rmcli-retest; do
  rm -rf "$dir" 2>/dev/null && echo "已删除: $dir" || true
done
```

> **自定义**: 如有其他 `/tmp/rmcli-*` 测试目录，在列表中追加即可。

### 步骤 3：清理应用支持目录下的实例文件

删除 macOS `~/Library/Application Support/rmccc/` 下的所有运行时数据，包括实例文件、配置文件、认证信息、日志等。

```bash
# 清理 RMCCC 应用支持目录
RMCCC_DIR="$HOME/Library/Application Support/rmccc"
if [ -d "$RMCCC_DIR" ]; then
  rm -rf "$RMCCC_DIR" && echo "已清理: $RMCCC_DIR"
else
  echo "目录不存在，跳过: $RMCCC_DIR"
fi
```

> **注意**: 这将删除所有已下载的 Minecraft 游戏文件（JAR、依赖库、资源文件、存档等）。确认无需保留后再执行。

### 步骤 4：清理项目构建缓存

删除 Rust 编译产物 `target/` 目录（体积最大，通常数 GB）。

```bash
# 清理 Rust 编译缓存
if [ -d "target" ]; then
  rm -rf target && echo "已删除: target/"
else
  echo "目录不存在，跳过: target/"
fi
```

> **后续**: 下次 `cargo build` 时会重新编译全部依赖，首次编译时间较长。

### 步骤 5：清理 JS 依赖和前端构建产物

```bash
# 清理 node_modules（如果存在）
if [ -d "node_modules" ]; then
  rm -rf node_modules && echo "已删除: node_modules/"
else
  echo "目录不存在，跳过: node_modules/"
fi

# 清理前端构建产物
if [ -d "dist" ]; then
  rm -rf dist && echo "已删除: dist/"
else
  echo "目录不存在，跳过: dist/"
fi
```

> **说明**: `node_modules/` 常见于 npm/yarn/pnpm 项目；`dist/` 是 Vite/webpack 等构建工具的输出目录。

### 步骤 6：清理编译器/IDE 临时文件

```bash
# 清理 Rust flycheck 临时编译目录（rust-analyzer / flycheck 产生）
rm -rf target/flycheck* 2>/dev/null || true

# 清理 Rust 增量编译缓存（可选，体积中等）
# rm -rf target/debug/incremental 2>/dev/null || true
```

> **注意**: `incremental` 目录的清理被注释掉了，保留可加速下次编译。如需完全干净编译，取消注释即可。

### 步骤 7：清理 macOS 隐藏元数据文件

```bash
# 删除项目下所有 .DS_Store 文件
find . -name ".DS_Store" -delete 2>/dev/null
echo "已清理 .DS_Store 文件"
```

### 步骤 8：最终状态确认

验证清理结果，确保项目仅包含核心源代码和配置文件。

```bash
echo ""
echo "=== 清理完成状态 ==="
echo ""

# 确认关键目录已被删除
for dir in target node_modules dist; do
  if [ -d "$dir" ]; then
    echo "⚠️   $dir/ 仍存在（可能需手动检查）"
  else
    echo "✅ $dir/ 已删除"
  fi
done

# 确认 /tmp 测试目录已删除
for dir in /tmp/rmcli-test /tmp/rmcli-test2 /tmp/rmcli-playtest /tmp/rmcli-retest; do
  if [ -d "$dir" ]; then
    echo "⚠️   $dir 仍存在"
  else
    echo "✅ $dir 已删除"
  fi
done

# 确认应用支持目录已删除
RMCCC_DIR="$HOME/Library/Application Support/rmccc"
if [ -d "$RMCCC_DIR" ]; then
  echo "⚠️   $RMCCC_DIR 仍存在"
else
  echo "✅ $RMCCC_DIR 已删除"
fi

# 检查是否有运行中的 Java 进程
JAVA_COUNT=$(pgrep -f "java" 2>/dev/null | wc -l | tr -d ' ')
if [ "$JAVA_COUNT" -gt 0 ] 2>/dev/null; then
  echo "⚠️   仍有 $JAVA_COUNT 个 Java 进程运行中"
else
  echo "✅ 无 Java 进程运行"
fi

echo ""
echo "=== 项目已恢复至干净初始状态 ==="
```

## 快速执行（一键脚本）

将上述所有步骤合并为单一命令：

```bash
# 终止进程 → 清理 /tmp → 清理 ~/Library → 清理 target → 清理 node_modules/dist → 清理 .DS_Store
pkill -f "net.minecraft.client.main.Main" 2>/dev/null || true
sleep 1
rm -rf /tmp/rmcli-test /tmp/rmcli-test2 /tmp/rmcli-playtest /tmp/rmcli-retest 2>/dev/null
rm -rf "$HOME/Library/Application Support/rmccc" 2>/dev/null
rm -rf target node_modules dist 2>/dev/null
rm -rf target/flycheck* 2>/dev/null
find . -name ".DS_Store" -delete 2>/dev/null
echo "✅ 全面清理完成"
```

## 保留清单

以下内容和目录会被**保留**，不受清理影响：

| 内容 | 路径 |
|------|------|
| Rust 源码头 | `crates/` |
| CLI 源代码 | `src-cli/` |
| Tauri 应用代码 | `src-tauri/` |
| SolidJS 前端代码 | `src/` |
| 项目文档 | `docs/` |
| Rust 配置 | `Cargo.toml`, `Cargo.lock` |
| JS 配置 | `package.json`, `pnpm-lock.yaml`, `tsconfig.json`, `vite.config.ts` |
| 项目说明 | `AGENTS.md`, `index.html` |
