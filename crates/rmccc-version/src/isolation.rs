//! Instance isolation strategy (HMCL `DefaultIsolationType`).
//!
//! - `Never`  — run in the shared `.minecraft` game directory (default).
//! - `Always` — run in the isolated instance root (`versions/{id}`).
//! - `Modded` — isolate only instances that carry mod loader patches.

use std::path::{Path, PathBuf};

use super::file_layout;

/// Instance isolation type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IsolationType {
    /// Never isolate — share libraries / assets / saves in the game directory.
    Never,
    /// Always isolate — the instance root is the run directory.
    Always,
    /// Isolate only modded instances (those with patches).
    Modded,
}

impl Default for IsolationType {
    fn default() -> Self {
        Self::Never
    }
}

/// Select the effective run directory for an instance.
///
/// Mirrors HMCL `HMCLGameRepository.getRunDirectory`: ALWAYS → instance root;
/// MODDED → instance root when modded; NEVER → shared game directory.
pub fn run_directory(game_dir: &Path, id: &str, isolation: IsolationType, modded: bool) -> PathBuf {
    let instance_root = file_layout::instance_root(game_dir, id);
    match isolation {
        IsolationType::Always => instance_root,
        IsolationType::Modded if modded => instance_root,
        _ => game_dir.to_path_buf(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_never_uses_shared_dir() {
        let dir = run_directory(Path::new("/g"), "1.20.4", IsolationType::Never, true);
        assert_eq!(dir, Path::new("/g"));
    }

    #[test]
    fn test_always_uses_instance_root() {
        let dir = run_directory(Path::new("/g"), "1.20.4", IsolationType::Always, false);
        assert_eq!(dir, Path::new("/g/versions/1.20.4"));
    }

    #[test]
    fn test_modded_only_isolates_modded() {
        let modded = run_directory(Path::new("/g"), "1.20.4", IsolationType::Modded, true);
        assert_eq!(modded, Path::new("/g/versions/1.20.4"));
        let vanilla = run_directory(Path::new("/g"), "1.20.4", IsolationType::Modded, false);
        assert_eq!(vanilla, Path::new("/g"));
    }
}
