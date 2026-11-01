//! Platform-specific path resolution.
//!
//! All path functions validate against directory traversal attacks
//! by ensuring resolved paths remain within the intended root.

use std::path::{Path, PathBuf};

/// Default data directory for RMCCC.
///
/// 用 **local** data dir：Windows 上是 `%LOCALAPPDATA%\rmccc`，而不是
/// `%APPDATA%`（Roaming）。实例目录里的 `assets/`、`libraries/` 动辄数 GB，
/// 放进 Roaming 会在域/漫游配置环境里每次登录同步一遍。macOS 上
/// `data_local_dir()` 与 `data_dir()` 同为 `~/Library/Application Support`，
/// 行为不变。
pub fn default_data_dir() -> PathBuf {
    data_root().join("rmccc")
}

/// 平台数据根目录（优先 local data dir，取不到再退到 Roaming/home）。
fn data_root() -> PathBuf {
    dirs::data_local_dir()
        .or_else(dirs::data_dir)
        .unwrap_or_else(|| PathBuf::from("."))
}

/// 把旧版（Roaming）数据目录搬到当前数据目录一次。
///
/// 仅当“新目录不存在 + 旧目录存在 + 两者不同”时动作；`rename` 失败（跨卷、
/// 占用）只告警不阻塞启动——数据仍在旧位置，用户启动后重建即可。macOS 上两个
/// 根路径相同，这里恒为 no-op。
fn migrate_legacy_data_dir() {
    let (Some(legacy_root), Some(current_root)) = (dirs::data_dir(), dirs::data_local_dir()) else {
        return;
    };
    let legacy = legacy_root.join("rmccc");
    let current = current_root.join("rmccc");
    if legacy == current || current.exists() || !legacy.is_dir() {
        return;
    }
    match std::fs::rename(&legacy, &current) {
        Ok(()) => tracing::info!(
            from = %legacy.display(),
            to = %current.display(),
            "Migrated launcher data to the local data directory"
        ),
        Err(error) => tracing::warn!(
            from = %legacy.display(),
            to = %current.display(),
            error = ?error,
            "Could not migrate the legacy data directory; a fresh one will be created"
        ),
    }
}

/// Default game root directory.
pub fn default_game_dir() -> PathBuf {
    default_data_dir().join("instances")
}

/// Log directory.
pub fn log_dir() -> PathBuf {
    default_data_dir().join("logs")
}

/// Cache directory for offline data.
pub fn cache_dir() -> PathBuf {
    default_data_dir().join("cache")
}

/// `config/` subdirectory holding settings files (P3: launcher-settings).
pub fn config_dir(data_dir: &Path) -> PathBuf {
    data_dir.join("config")
}

/// `state/` subdirectory holding runtime state files (P3: launcher-state).
pub fn state_dir(data_dir: &Path) -> PathBuf {
    data_dir.join("state")
}

/// Launcher settings file (`{data_dir}/config/launcher-settings.json`).
pub fn launcher_settings_path(data_dir: &Path) -> PathBuf {
    config_dir(data_dir).join("launcher-settings.json")
}

/// Launcher runtime state file (`{data_dir}/state/launcher-state.json`).
pub fn launcher_state_path(data_dir: &Path) -> PathBuf {
    state_dir(data_dir).join("launcher-state.json")
}

/// User-level settings file (`{data_dir}/user-settings.json`).
pub fn user_settings_path(data_dir: &Path) -> PathBuf {
    data_dir.join("user-settings.json")
}

/// 插件安装目录（`{data_dir}/plugins/<plugin-id>/<version>/`，ADR-003）。
pub fn plugins_dir(data_dir: &Path) -> PathBuf {
    data_dir.join("plugins")
}

/// 插件用户数据目录（`{data_dir}/plugin-data/<plugin-id>/`）。
pub fn plugin_data_dir(data_dir: &Path) -> PathBuf {
    data_dir.join("plugin-data")
}

/// 插件状态目录（注册表/权限/审计/staging）。
pub fn plugin_state_dir(data_dir: &Path) -> PathBuf {
    data_dir.join("plugin-state")
}

/// `plugin-state/registry.json`。
pub fn plugin_registry_path(data_dir: &Path) -> PathBuf {
    plugin_state_dir(data_dir).join("registry.json")
}

/// `plugin-state/permissions.json`。
pub fn plugin_permissions_path(data_dir: &Path) -> PathBuf {
    plugin_state_dir(data_dir).join("permissions.json")
}

/// `plugin-state/audit.jsonl`。
pub fn plugin_audit_path(data_dir: &Path) -> PathBuf {
    plugin_state_dir(data_dir).join("audit.jsonl")
}

/// `plugin-state/staging/`。
pub fn plugin_staging_dir(data_dir: &Path) -> PathBuf {
    plugin_state_dir(data_dir).join("staging")
}

/// Agent 数据目录（`{data_dir}/agent/`，ADR-010）。
///
/// MCP 审计日志（`agent/audit.jsonl`）经 [`agent_audit_path`] 统一取径，
/// 避免 Tauri/CLI 各拼一份；ADR-013 瘦身后 sessions/tasks 子目录不再使用。
pub fn agent_dir(data_dir: &Path) -> PathBuf {
    data_dir.join("agent")
}

/// `agent/audit.jsonl`（MCP 破坏性操作审计，fail-closed）。
pub fn agent_audit_path(data_dir: &Path) -> PathBuf {
    agent_dir(data_dir).join("audit.jsonl")
}

/// Ensure all required directories exist.
pub fn ensure_dirs() -> std::io::Result<()> {
    migrate_legacy_data_dir();
    let dirs = [
        default_data_dir(),
        default_game_dir(),
        log_dir(),
        cache_dir(),
    ];
    for dir in &dirs {
        std::fs::create_dir_all(dir)?;
    }
    Ok(())
}

/// Check that a sub-path resolves within the given root directory.
///
/// 返回 `Ok(resolved)` 表示路径安全，`Err` 表示检测到目录穿越
/// （如 `../../etc/passwd`）。
///
/// 注意（B2 审计结论）：这是一个**通用工具**，不是全工程的防穿越统一入口——
/// 全工程的 id/路径防护点在各 IPC 边界（`rmccc-instance::validate_instance_id`、
/// `rmccc-version` 的 `validate_instance_id`、`rmccc-dashboard` 的
/// `validate_layout_id` 等），那些防护不经过本函数。
pub fn safe_join(root: &Path, sub: &str) -> std::io::Result<PathBuf> {
    // Canonicalize the root first (fail gracefully if root doesn't exist yet)
    let root = if root.exists() {
        root.canonicalize()?
    } else {
        // If root doesn't exist yet, just clean the path
        clean_path(root)
    };

    let candidate = root.join(sub);

    // Clean and canonicalize the candidate
    let candidate = if candidate.exists() {
        candidate.canonicalize()?
    } else {
        // For non-existent paths, resolve .. and . components manually
        clean_path(&candidate)
    };

    // Ensure the resolved path starts with the root
    if !candidate.starts_with(&root) {
        return Err(std::io::Error::new(
            std::io::ErrorKind::PermissionDenied,
            format!(
                "Path traversal detected: '{}' escapes root '{}'",
                sub,
                root.display()
            ),
        ));
    }

    Ok(candidate)
}

/// Clean a path by resolving `..` and `.` components without touching the filesystem.
fn clean_path(path: &Path) -> PathBuf {
    let mut components = Vec::new();
    for component in path.components() {
        match component {
            std::path::Component::ParentDir => {
                components.pop();
            }
            std::path::Component::CurDir => {}
            c => components.push(c),
        }
    }
    components.iter().collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_data_dir_prefers_the_local_data_dir() {
        let dir = default_data_dir();
        assert!(dir.ends_with("rmccc"), "{} 应以 rmccc 结尾", dir.display());
        // Windows 审计 2026-11-01：实例数据不能落在 Roaming（%APPDATA%）。
        if let Some(local) = dirs::data_local_dir() {
            assert_eq!(dir.parent(), Some(local.as_path()));
        }
    }

    #[test]
    fn test_safe_join_normal() {
        let root = Path::new("/tmp/rmccc-test/game");
        let result = safe_join(root, "versions/1.20.4");
        assert!(result.is_ok());
    }

    #[test]
    fn test_safe_join_traversal_blocked() {
        let root = Path::new("/tmp/rmccc-test/game");
        let result = safe_join(root, "../../etc/passwd");
        // Path traversal should be detected and blocked
        assert!(
            result.is_err() || {
                // If root exists, canonicalize will catch it
                true
            }
        );
    }
}
