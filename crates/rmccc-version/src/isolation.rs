//! Instance isolation strategy (HMCL `DefaultIsolationType`).
//!
//! - `Never`  — run in the shared `.minecraft` game directory.
//! - `Modded` — isolate only instances that carry mod loader patches (default,
//!   matching HMCL's global `DEFAULT_ISOLATION`).
//! - `Always` — run in the isolated instance root (`versions/{id}`).

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

use super::file_layout;

/// Instance isolation type.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IsolationType {
    /// Never isolate — share libraries / assets / saves in the game directory.
    Never,
    /// Isolate only modded instances (those with patches).
    #[default]
    Modded,
    /// Always isolate — the instance root is the run directory.
    Always,
}

impl IsolationType {
    /// Stable wire/JSON name (matches `rmccc_config::ISOLATION_VALUES`).
    pub fn as_str(self) -> &'static str {
        match self {
            IsolationType::Never => "never",
            IsolationType::Modded => "modded",
            IsolationType::Always => "always",
        }
    }

    /// Parses a normalised isolation value, case-insensitively.
    pub fn parse(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "never" => Some(IsolationType::Never),
            "modded" => Some(IsolationType::Modded),
            "always" => Some(IsolationType::Always),
            _ => None,
        }
    }

    /// Every variant, in picker order.
    pub const ALL: [IsolationType; 3] = [
        IsolationType::Never,
        IsolationType::Modded,
        IsolationType::Always,
    ];
}

impl std::str::FromStr for IsolationType {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        IsolationType::parse(s).ok_or(())
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
    fn test_isolation_round_trips() {
        for value in IsolationType::ALL {
            assert_eq!(IsolationType::parse(value.as_str()), Some(value));
        }
        assert_eq!(IsolationType::default(), IsolationType::Modded);
        assert_eq!(IsolationType::parse("ALWAYS"), Some(IsolationType::Always));
        assert_eq!(IsolationType::parse(" sometimes "), None);
    }

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
