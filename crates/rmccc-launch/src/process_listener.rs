//! Process lifecycle callbacks — mirrors HMCL's `ProcessListener` /
//! `ExitWaiter` contract.
//!
//! A [`ProcessListener`] receives every game output line (with a stderr flag)
//! and a single terminal `on_exit` notification carrying the exit code and an
//! [`ExitType`] (normal vs. abnormal). The orchestration layer (Tauri / CLI)
//! implements this trait to forward lines to the UI and to react to exits
//! (e.g. clear the `.abnormal` marker).

use crate::managed::ManagedProcess;

/// How a game process terminated (HMCL `ExitType`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExitType {
    /// Process exited with code 0.
    Normal,
    /// Process exited with a non-zero code, was killed, or never started.
    Abnormal,
}

/// Callback interface for game process events (HMCL `ProcessListener`).
pub trait ProcessListener: Send + Sync {
    /// Called once with the managed process wrapper after it is created.
    fn set_process(&self, _process: &ManagedProcess) {}

    /// A single output line from the game (`is_stderr` marks the error stream).
    fn on_log(&self, line: String, is_stderr: bool);

    /// The game process has fully exited.
    fn on_exit(&self, exit_code: i32, exit_type: ExitType);
}

/// Launch-time auth credentials (HMCL `AuthInfo`).
///
/// Kept local to `rmccc-launch` so the crate does not depend on the auth
/// subsystem; callers (Tauri / CLI) convert their session into this struct.
#[derive(Debug, Clone)]
pub struct AuthInfo {
    /// Player display name (`${auth_player_name}`).
    pub username: String,
    /// Player UUID, compact form (`${auth_uuid}`).
    pub uuid: String,
    /// Access token (`${auth_access_token}` / `--accessToken`).
    pub access_token: String,
    /// `msa`, `mojang`, or `legacy` (`${user_type}`).
    pub user_type: String,
    /// User properties JSON (`"{}"` when none).
    pub user_properties: String,
}

impl AuthInfo {
    /// Build auth info from the standard four fields (user properties `{}`).
    pub fn new(
        username: impl Into<String>,
        uuid: impl Into<String>,
        access_token: impl Into<String>,
        user_type: impl Into<String>,
    ) -> Self {
        Self {
            username: username.into(),
            uuid: uuid.into(),
            access_token: access_token.into(),
            user_type: user_type.into(),
            user_properties: "{}".to_string(),
        }
    }
}

impl Default for AuthInfo {
    fn default() -> Self {
        Self {
            username: String::new(),
            uuid: String::new(),
            access_token: String::new(),
            user_type: "msa".to_string(),
            user_properties: "{}".to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_exit_type_from_code() {
        assert_eq!(ExitType::Normal, ExitType::Normal);
        assert_ne!(ExitType::Normal, ExitType::Abnormal);
    }

    #[test]
    fn test_auth_info_new() {
        let a = AuthInfo::new("Steve", "0000", "token", "msa");
        assert_eq!(a.username, "Steve");
        assert_eq!(a.user_type, "msa");
        assert_eq!(a.user_properties, "{}");
    }
}
