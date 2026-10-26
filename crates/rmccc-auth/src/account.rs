//! `Account` trait — the persisted account abstraction.
//!
//! Mirrors HMCL's [`Account`]: an account holds its own credentials, knows how
//! to log in (silently refreshing tokens) and how to serialize public metadata
//! versus private credential data. Concrete types are produced by an
//! [`AccountFactory`](crate::factory::AccountFactory) and restored from
//! storage via `from_storage`.
//!
//! [`Account`]: https://github.com/HMCL-dev/HMCL/blob/master/HMCLCore/src/main/java/org/jackhuang/hmcl/auth/Account.java

use crate::account_id::AccountID;
use crate::auth_info::AuthInfo;
use crate::types::{AuthSession, ProgressCallback};
use rmccc_error::{Result, RmcccError};

/// A persisted account entry.
///
/// Implementations must be `Send + Sync` so they can be shared across async
/// tasks (Tauri IPC / CLI).
#[async_trait::async_trait]
pub trait Account: Send + Sync {
    /// The stable launcher account record ID (independent of the MC profile ID).
    fn account_id(&self) -> &AccountID;

    /// The authentication provider id (`microsoft`, `offline`, ...).
    fn provider(&self) -> &str;

    /// The in-game profile name.
    ///
    /// Returns an owned `String` because implementations may hold their
    /// credentials behind interior mutability (e.g. a refreshed session), which
    /// cannot yield a stable borrowed `&str`.
    fn profile_name(&self) -> String;

    /// Whether the most recent [`Account::log_in`] (or re-auth) mutated the
    /// stored session and the account therefore needs persisting.
    ///
    /// Perf audit C5: every launch used to rewrite + re-encrypt the whole
    /// account store even when the token was untouched. Implementations that
    /// cannot track this should keep the conservative `true` default.
    fn needs_save(&self) -> bool {
        true
    }

    /// The Minecraft profile UUID.
    fn profile_id(&self) -> String;

    /// Log in with stored credentials, silently refreshing expired tokens when
    /// possible (P2: advance-refresh window + profile UUID consistency check).
    ///
    /// When the token is still valid (outside the advance-refresh window) this
    /// is a cheap local operation. Otherwise the token is refreshed and the
    /// refreshed profile UUID is verified against the stored one.
    ///
    /// # Errors
    ///
    /// Returns [`RmcccError::CredentialExpired`] when the stored credentials
    /// are permanently invalid — callers should then route the user through
    /// [`Account::log_in_when_credentials_expired`].
    ///
    /// Returns [`RmcccError::WrongAccount`] when the refreshed credentials
    /// belong to a different Minecraft profile (mirrors HMCL's
    /// `OAuthAccount.WrongAccountException`).
    async fn log_in(&self) -> Result<AuthInfo>;

    /// Re-authorizes when the stored credentials are permanently expired.
    ///
    /// Runs a full interactive flow (e.g. the Microsoft device-code flow),
    /// reporting progress through `progress`. Offline accounts never expire and
    /// inherit the default implementation, which returns
    /// [`RmcccError::CredentialExpired`].
    ///
    /// Mirrors HMCL's `OAuthAccount.logInWhenCredentialsExpired()`.
    async fn log_in_when_credentials_expired(
        &self,
        _progress: Option<&ProgressCallback>,
    ) -> Result<AuthInfo> {
        Err(RmcccError::CredentialExpired)
    }

    /// Produce offline launch info without any network interaction.
    fn play_offline(&self) -> AuthInfo;

    /// Writes public account metadata (no credentials) for `accounts.json`.
    fn write_metadata(&self) -> serde_json::Value;

    /// Writes private account data (tokens, refresh tokens) for the encrypted
    /// private data store.
    fn write_private_data(&self) -> serde_json::Value;

    /// Returns the current persisted session snapshot (with the latest
    /// credentials) so callers can write it back through the repository after
    /// a refresh or re-authorization.
    fn to_session(&self) -> AuthSession;
}
