//! `AccountFactory` trait — creates and restores accounts.
//!
//! Mirrors HMCL's [`AccountFactory`]: one factory per login type. `create`
//! performs a fresh (possibly interactive) login, while `from_storage`
//! reconstructs an account from previously persisted metadata/private data so
//! it can be lazily refreshed later.
//!
//! [`AccountFactory`]: https://github.com/HMCL-dev/HMCL/blob/master/HMCLCore/src/main/java/org/jackhuang/hmcl/auth/AccountFactory.java

use crate::account::Account;
use crate::types::ProgressCallback;

/// Context passed to [`AccountFactory::create`].
///
/// Carries optional provider-specific data (e.g. the username for offline
/// accounts) and an optional progress callback so interactive login flows
/// (Microsoft device code) can stream progress back to the caller.
pub struct CreateAccountContext {
    /// Optional hint for the login flow / additional data.
    pub additional_data: Option<serde_json::Value>,
    /// Optional progress callback for interactive login flows.
    pub progress: Option<ProgressCallback>,
}

impl CreateAccountContext {
    /// Creates an empty create context.
    pub fn new() -> Self {
        Self {
            additional_data: None,
            progress: None,
        }
    }
}

impl Default for CreateAccountContext {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Debug for CreateAccountContext {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CreateAccountContext")
            .field("additional_data", &self.additional_data)
            .field("progress", &self.progress.as_ref().map(|_| "<callback>"))
            .finish()
    }
}

/// A factory that can create and restore accounts of one login type.
#[async_trait::async_trait]
pub trait AccountFactory: Send + Sync {
    /// The stable login type id (`offline`, `microsoft`, `authlib_injector`, ...).
    fn login_type(&self) -> &'static str;

    /// Performs a fresh login and returns a verified account.
    ///
    /// # Errors
    ///
    /// Returns an authentication error when login fails.
    async fn create(&self, ctx: CreateAccountContext) -> rmccc_error::Result<Box<dyn Account>>;

    /// Restores an account from previously persisted storage.
    ///
    /// Credentials may be expired; callers should refresh the account state
    /// later via [`Account::log_in`].
    ///
    /// # Errors
    ///
    /// Returns an error when the stored data is malformed or belongs to
    /// another login type.
    ///
    /// The `from_` prefix is intentional to mirror HMCL's `AccountFactory.fromStorage`.
    #[allow(clippy::wrong_self_convention)]
    fn from_storage(
        &self,
        metadata: &serde_json::Value,
        private: Option<&serde_json::Value>,
    ) -> rmccc_error::Result<Box<dyn Account>>;
}
