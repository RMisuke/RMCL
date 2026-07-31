//! Offline authentication provider.
//!
//! Generates a deterministic UUID v3 from the username for consistent
//! offline identity. No network access required.

use rmccc_error::Result;
use uuid::Uuid;

use crate::types::{AuthProvider, AuthSession, ProgressCallback};

/// Offline auth provider — no network, just username → UUID.
pub struct OfflineAuthProvider;

impl OfflineAuthProvider {
    pub fn new() -> Self {
        Self
    }

    /// Generate a deterministic UUID from a username.
    /// Uses UUID v5 (SHA-1 based) with the DNS namespace for consistency.
    /// This is the canonical offline UUID function used everywhere.
    pub fn uuid_from_username(username: &str) -> String {
        let uuid = Uuid::new_v5(&Uuid::NAMESPACE_DNS, username.as_bytes());
        uuid.to_string()
    }
}

impl Default for OfflineAuthProvider {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl AuthProvider for OfflineAuthProvider {
    fn provider_id(&self) -> &str {
        "offline"
    }

    async fn authenticate(
        &self,
        _progress: Option<&ProgressCallback>,
    ) -> Result<AuthSession> {
        // This will be called with the username passed via IPC
        // The IPC handler extracts the username and calls this
        // For now, return a placeholder — the IPC command provides the username
        Err(rmccc_error::RmcccError::AuthFailed(
            "Use auth_offline_login IPC command with username parameter".into(),
        ))
    }

    async fn refresh(&self, session: &AuthSession) -> Result<AuthSession> {
        // Offline sessions don't expire, just return the same session
        Ok(session.clone())
    }

    async fn validate(&self, session: &AuthSession) -> bool {
        session.provider == "offline" && !session.username.is_empty()
    }
}

/// Create an offline AuthSession from a username.
pub fn create_offline_session(username: &str) -> AuthSession {
    AuthSession {
        provider: "offline".into(),
        access_token: Uuid::new_v4().to_string(),
        username: username.to_string(),
        uuid: OfflineAuthProvider::uuid_from_username(username),
        refresh_token: None,
        expires_at: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_deterministic_uuid() {
        let uuid1 = OfflineAuthProvider::uuid_from_username("Steve");
        let uuid2 = OfflineAuthProvider::uuid_from_username("Steve");
        let uuid3 = OfflineAuthProvider::uuid_from_username("Alex");

        assert_eq!(uuid1, uuid2, "Same username should produce same UUID");
        assert_ne!(uuid1, uuid3, "Different usernames should produce different UUIDs");
    }

    #[test]
    fn test_offline_session() {
        let session = create_offline_session("TestPlayer");
        assert_eq!(session.provider, "offline");
        assert_eq!(session.username, "TestPlayer");
        assert!(!session.uuid.is_empty());
        assert!(session.refresh_token.is_none());
        assert!(session.expires_at.is_none());
    }
}
