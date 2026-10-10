//! Offline authentication — canonical offline UUID + offline account/factory.
//!
//! Generates the canonical offline UUID from a username so identity stays
//! consistent across the vanilla/HMCL ecosystem. No network access required.
//!
//! Mirrors HMCL's `OfflineAccount` / `OfflineAccountFactory`: `logIn` /
//! `playOffline` return `user_type=msa` with a fresh random access token.

use md5::{Digest, Md5};
use rmccc_error::{Result, RmcccError};
use uuid::Uuid;

use crate::account::Account;
use crate::account_id::AccountID;
use crate::auth_info::{user_type, AuthInfo};
use crate::factory::{AccountFactory, CreateAccountContext};
use crate::types::{AuthProvider, AuthSession, ProgressCallback};

/// Offline auth provider — no network, just username → UUID.
pub struct OfflineAuthProvider;

impl OfflineAuthProvider {
    pub fn new() -> Self {
        Self
    }

    /// Generate the canonical offline UUID from a username.
    ///
    /// Mirrors Java's `UUID.nameUUIDFromBytes("OfflinePlayer:" + username)` and
    /// HMCL's [`OfflineAccountFactory#getUUIDFromUserName`] so whitelists, skin
    /// servers and other launchers compute the same identity.
    ///
    /// The UUID is version 3 (MD5-based): the MD5 digest of the UTF-8 bytes of
    /// `OfflinePlayer:<username>`, with the version and variant bits set.
    ///
    /// [`OfflineAccountFactory#getUUIDFromUserName`]: https://github.com/HMCL-dev/HMCL/blob/master/HMCLCore/src/main/java/org/jackhuang/hmcl/auth/offline/OfflineAccountFactory.java
    pub fn uuid_from_username(username: &str) -> String {
        let mut hasher = Md5::new();
        hasher.update(format!("OfflinePlayer:{username}").as_bytes());
        let mut bytes: [u8; 16] = hasher.finalize().into();

        // Set the UUID version to 3 (name-based, MD5) and the RFC 4122 variant.
        bytes[6] = (bytes[6] & 0x0f) | 0x30;
        bytes[8] = (bytes[8] & 0x3f) | 0x80;

        Uuid::from_bytes(bytes).to_string()
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

    async fn authenticate(&self, _progress: Option<&ProgressCallback>) -> Result<AuthSession> {
        // Offline auth doesn't require interactive flow — the IPC handler
        // calls create_offline_session directly with the username.
        // This trait method provides a sensible default for direct use.
        Ok(create_offline_session("Player"))
    }

    async fn refresh(&self, session: &AuthSession) -> Result<AuthSession> {
        // Offline sessions don't expire, just return the same session
        Ok(session.clone())
    }

    async fn validate(&self, session: &AuthSession) -> bool {
        session.provider == "offline" && !session.username.is_empty()
    }
}

/// Offline account — mirrors HMCL's `OfflineAccount`.
///
/// No credentials expire: `log_in` / `play_offline` always succeed locally and
/// return `user_type=msa` with a fresh random access token, exactly like HMCL's
/// `OfflineAccount.logInWithoutSkin()`.
pub struct OfflineAccount {
    account_id: AccountID,
    username: String,
    uuid: String,
    access_token: String,
}

impl OfflineAccount {
    /// Creates a new offline account with a fresh random account ID and token.
    pub fn new(username: impl Into<String>) -> Self {
        let username = username.into();
        let uuid = OfflineAuthProvider::uuid_from_username(&username);
        Self {
            account_id: AccountID::generate(),
            username,
            uuid,
            access_token: Uuid::new_v4().to_string(),
        }
    }

    /// Rebuilds an offline account from persisted metadata (private data is
    /// not sensitive for offline accounts — the token is always regenerated).
    pub fn from_parts(account_id: AccountID, username: String, uuid: String) -> Self {
        Self {
            account_id,
            username,
            uuid,
            access_token: Uuid::new_v4().to_string(),
        }
    }

    /// Builds the launch AuthInfo exactly like HMCL's `logInWithoutSkin()`:
    /// `user_type=msa` + a fresh random token (avoids "invalid session token" /
    /// "disconnected" on servers sensitive to the legacy user type).
    fn launch_info(&self) -> AuthInfo {
        AuthInfo::new(
            self.username.clone(),
            self.uuid.clone(),
            Uuid::new_v4().to_string(),
            user_type::MSA,
            "{}",
        )
    }
}

#[async_trait::async_trait]
impl Account for OfflineAccount {
    fn account_id(&self) -> &AccountID {
        &self.account_id
    }

    fn provider(&self) -> &str {
        "offline"
    }

    fn profile_name(&self) -> String {
        self.username.clone()
    }

    fn profile_id(&self) -> String {
        self.uuid.clone()
    }

    async fn log_in(&self) -> Result<AuthInfo> {
        Ok(self.launch_info())
    }

    fn play_offline(&self) -> AuthInfo {
        self.launch_info()
    }

    fn write_metadata(&self) -> serde_json::Value {
        serde_json::json!({
            "type": self.provider(),
            "account_id": self.account_id.to_string(),
            "profile_name": &self.username,
            "profile_id": &self.uuid,
        })
    }

    fn write_private_data(&self) -> serde_json::Value {
        serde_json::json!({
            "access_token": &self.access_token,
            "refresh_token": serde_json::Value::Null,
            "expires_at": serde_json::Value::Null,
        })
    }

    fn to_session(&self) -> AuthSession {
        AuthSession {
            account_id: self.account_id.to_string(),
            provider: "offline".into(),
            access_token: self.access_token.clone(),
            username: self.username.clone(),
            uuid: self.uuid.clone(),
            refresh_token: None,
            expires_at: None,
        }
    }
}

/// Offline account factory — mirrors HMCL's `OfflineAccountFactory`.
pub struct OfflineAccountFactory;

#[async_trait::async_trait]
impl AccountFactory for OfflineAccountFactory {
    fn login_type(&self) -> &'static str {
        "offline"
    }

    async fn create(&self, ctx: CreateAccountContext) -> Result<Box<dyn Account>> {
        let username = ctx
            .additional_data
            .as_ref()
            .and_then(|v| v.get("username").and_then(|u| u.as_str()))
            .unwrap_or("Player")
            .to_string();
        Ok(Box::new(OfflineAccount::new(username)))
    }

    fn from_storage(
        &self,
        metadata: &serde_json::Value,
        _private: Option<&serde_json::Value>,
    ) -> Result<Box<dyn Account>> {
        let account_id = metadata
            .get("account_id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| RmcccError::Other("offline metadata missing account_id".into()))?;
        let account_id = AccountID::parse(account_id)
            .map_err(|e| RmcccError::Other(format!("invalid offline account_id: {e:?}")))?;
        let username = metadata
            .get("profile_name")
            .and_then(|v| v.as_str())
            .unwrap_or("Player")
            .to_string();
        let uuid = metadata
            .get("profile_id")
            .and_then(|v| v.as_str())
            .map(str::to_string)
            .unwrap_or_else(|| OfflineAuthProvider::uuid_from_username(&username));
        Ok(Box::new(OfflineAccount::from_parts(
            account_id, username, uuid,
        )))
    }
}

/// Create an offline AuthSession from a username.
pub fn create_offline_session(username: &str) -> AuthSession {
    AuthSession {
        account_id: AccountID::generate().to_string(),
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
    use crate::auth_info::user_type;

    #[test]
    fn test_deterministic_uuid() {
        let uuid1 = OfflineAuthProvider::uuid_from_username("Steve");
        let uuid2 = OfflineAuthProvider::uuid_from_username("Steve");
        let uuid3 = OfflineAuthProvider::uuid_from_username("Alex");

        assert_eq!(uuid1, uuid2, "Same username should produce same UUID");
        assert_ne!(
            uuid1, uuid3,
            "Different usernames should produce different UUIDs"
        );
    }

    #[test]
    fn test_uuid_is_version_3_rfc4122() {
        let uuid = Uuid::parse_str(&OfflineAuthProvider::uuid_from_username("Steve")).unwrap();
        assert_eq!(
            uuid.get_version_num(),
            3,
            "offline UUID must be version 3 (MD5-based)"
        );
        assert_eq!(
            uuid.get_variant(),
            uuid::Variant::RFC4122,
            "offline UUID must use the RFC 4122 variant"
        );
    }

    #[test]
    fn test_uuid_not_v5_namespace_based() {
        // Regression guard: the old implementation used UUID v5 (SHA-1, DNS
        // namespace), which is incompatible with the vanilla/HMCL ecosystem.
        let old_style = Uuid::new_v5(&Uuid::NAMESPACE_DNS, b"Steve").to_string();
        assert_ne!(
            OfflineAuthProvider::uuid_from_username("Steve"),
            old_style,
            "offline UUID must not use the previous v5 algorithm"
        );
    }

    #[test]
    fn test_offline_session() {
        let session = create_offline_session("TestPlayer");
        assert_eq!(session.provider, "offline");
        assert_eq!(session.username, "TestPlayer");
        assert!(!session.uuid.is_empty());
        assert!(session.refresh_token.is_none());
        assert!(session.expires_at.is_none());
        // account_id must be a valid prefixed account ID.
        assert!(
            AccountID::parse(&session.account_id).is_ok(),
            "session.account_id must be a valid AccountID, got: {}",
            session.account_id
        );
    }

    #[test]
    fn test_offline_session_user_type_msa() {
        let info = crate::auth_info::AuthInfo::from_session(&create_offline_session("Alex"));
        // P2：离线账户对齐 HMCL — user_type 为 msa。
        assert_eq!(info.user_type, user_type::MSA);
    }

    #[test]
    fn test_offline_account_log_in_is_msa_with_random_token() {
        let runtime = tokio::runtime::Runtime::new().unwrap();
        let account = OfflineAccount::new("Steve");
        let info = runtime.block_on(account.log_in()).unwrap();
        assert_eq!(info.username, "Steve");
        assert_eq!(info.uuid, OfflineAuthProvider::uuid_from_username("Steve"));
        assert_eq!(info.user_type, user_type::MSA);
        // 每次 log_in / play_offline 生成新的随机 token（对齐 HMCL）。
        assert_ne!(info.access_token, account.play_offline().access_token);
    }

    #[test]
    fn test_offline_account_play_offline_matches_profile() {
        let account = OfflineAccount::new("Alex");
        let info = account.play_offline();
        assert_eq!(info.username, "Alex");
        assert_eq!(info.uuid, OfflineAuthProvider::uuid_from_username("Alex"));
        assert_eq!(info.user_type, user_type::MSA);
    }

    #[test]
    fn test_offline_account_write_metadata_roundtrip() {
        let account = OfflineAccount::new("Steve");
        let meta = account.write_metadata();
        assert_eq!(meta["type"], "offline");
        assert_eq!(meta["profile_name"], "Steve");
        assert_eq!(meta["account_id"], account.to_session().account_id);
        assert_eq!(meta["profile_id"], account.profile_id());
    }

    #[test]
    fn test_offline_account_factory_create_and_from_storage() {
        let runtime = tokio::runtime::Runtime::new().unwrap();
        let factory = OfflineAccountFactory;
        let ctx = CreateAccountContext {
            additional_data: Some(serde_json::json!({ "username": "Bob" })),
            progress: None,
        };
        let account = runtime.block_on(factory.create(ctx)).unwrap();
        assert_eq!(account.provider(), "offline");
        assert_eq!(account.profile_name(), "Bob");

        let meta = account.write_metadata();
        let private = account.write_private_data();
        let restored = factory
            .from_storage(&meta, Some(&private))
            .expect("from_storage must restore an offline account");
        assert_eq!(restored.account_id(), account.account_id());
        assert_eq!(restored.profile_name(), "Bob");
        assert_eq!(restored.profile_id(), account.profile_id());
    }
}
