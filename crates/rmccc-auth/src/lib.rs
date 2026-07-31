//! Authentication module: Microsoft OAuth2, offline auth, and encrypted storage.

pub mod microsoft;
pub mod offline;
pub mod types;
pub mod vault;

// Re-export key types
pub use microsoft::MicrosoftAuthProvider;
pub use offline::{create_offline_session, OfflineAuthProvider};
pub use types::{AuthProgress, AuthProvider, AuthSession, ProgressCallback};
pub use vault::TokenVault;
