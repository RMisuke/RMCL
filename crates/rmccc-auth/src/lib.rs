//! Authentication module: Microsoft OAuth2, offline auth, and encrypted storage.

#![allow(clippy::double_must_use)] // 工具链 1.99.1 新 lint：async_trait 宏生成的 #[must_use] 与 Future/Result 重叠，非业务逻辑

pub mod account;
pub mod account_id;
pub mod auth_info;
pub mod factory;
pub mod microsoft;
pub mod offline;
pub mod repository;
pub mod types;
pub mod vault;

// Re-export key types
pub use account::Account;
pub use account_id::AccountID;
pub use auth_info::AuthInfo;
pub use factory::{AccountFactory, CreateAccountContext};
pub use microsoft::{
    MicrosoftAccount, MicrosoftAccountFactory, MicrosoftAuthProvider, REFRESH_WINDOW_SECS,
};
pub use offline::{
    create_offline_session, OfflineAccount, OfflineAccountFactory, OfflineAuthProvider,
};
pub use repository::{AccountMetadata, AccountRepository};
pub use types::{AuthProgress, AuthProvider, AuthSession, ProgressCallback};
pub use vault::TokenVault;
