//! Authentication types shared across all providers.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// Authentication session (persisted, tokens encrypted at rest).
#[derive(Clone, Serialize, Deserialize, TS)]
#[ts(export_to = "types.ts")]
pub struct AuthSession {
    /// Stable launcher account record ID (`account:<uuid>`), independent of the
    /// Minecraft profile UUID. Legacy records default to a random value and are
    /// re-assigned during the Phase 1 storage migration.
    #[serde(default = "random_account_id")]
    #[ts(type = "string")]
    pub account_id: String,
    pub provider: String,
    pub access_token: String,
    pub username: String,
    pub uuid: String,
    pub refresh_token: Option<String>,
    pub expires_at: Option<DateTime<Utc>>,
}

/// Default for records that predate the stable account ID field.
fn random_account_id() -> String {
    crate::account_id::AccountID::generate().to_string()
}

impl std::fmt::Debug for AuthSession {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AuthSession")
            .field("provider", &self.provider)
            .field("username", &self.username)
            .field("uuid", &self.uuid)
            .field("access_token", &"[REDACTED]")
            .field(
                "refresh_token",
                &self.refresh_token.as_ref().map(|_| "[REDACTED]"),
            )
            .field("expires_at", &self.expires_at)
            .finish()
    }
}

/// Progress updates during authentication (e.g., device code flow).
#[derive(Debug, Clone)]
pub enum AuthProgress {
    /// Device code received, user must visit URL and enter code.
    DeviceCode {
        user_code: String,
        verification_uri: String,
    },
    /// Polling in progress.
    Polling,
    /// Authentication successful.
    Success,
}

/// Callback for authentication progress updates.
pub type ProgressCallback = Box<dyn Fn(AuthProgress) + Send + Sync>;

/// Trait for authentication providers.
#[async_trait::async_trait]
pub trait AuthProvider: Send + Sync {
    /// Unique identifier for this provider (e.g., "microsoft", "offline").
    fn provider_id(&self) -> &str;

    /// Perform authentication. May require user interaction (browser).
    async fn authenticate(
        &self,
        progress: Option<&ProgressCallback>,
    ) -> rmccc_error::Result<AuthSession>;

    /// Refresh an expired session without user interaction.
    async fn refresh(&self, session: &AuthSession) -> rmccc_error::Result<AuthSession>;

    /// Validate whether a session is still usable.
    async fn validate(&self, session: &AuthSession) -> bool;
}
