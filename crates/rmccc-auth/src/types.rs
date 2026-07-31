//! Authentication types shared across all providers.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Authentication session (persisted, tokens encrypted at rest).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthSession {
    pub provider: String,
    pub access_token: String,
    pub username: String,
    pub uuid: String,
    pub refresh_token: Option<String>,
    pub expires_at: Option<DateTime<Utc>>,
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
