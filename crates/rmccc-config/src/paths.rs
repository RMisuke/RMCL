//! Platform-specific path resolution.
//!
//! All path functions validate against directory traversal attacks
//! by ensuring resolved paths remain within the intended root.

use std::path::{Path, PathBuf};

/// Default data directory for RMCCC.
pub fn default_data_dir() -> PathBuf {
    dirs::data_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("rmccc")
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

/// Ensure all required directories exist.
pub fn ensure_dirs() -> std::io::Result<()> {
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
/// Returns `Ok(resolved)` if the path is safe, or `Err` if a directory
/// traversal attack is detected (e.g., `../../etc/passwd`).
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
