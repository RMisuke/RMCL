//! `AuthInfo` — the launch-ready authentication result value object.
//!
//! Mirrors HMCL's [`AuthInfo`]: an immutable snapshot of the player identity
//! used to fill launch argument templates (`${auth_player_name}`,
//! `${auth_uuid}`, `${auth_access_token}`, `${user_type}`, ...).
//!
//! [`AuthInfo`]: https://github.com/HMCL-dev/HMCL/blob/master/HMCLCore/src/main/java/org/jackhuang/hmcl/auth/AuthInfo.java

use crate::types::AuthSession;

/// Known user type constants (aligned with HMCL `AuthInfo`).
pub mod user_type {
    /// Microsoft account (Xbox/MSA).
    pub const MSA: &str = "msa";
    /// Mojang account.
    pub const MOJANG: &str = "mojang";
    /// Legacy account (offline / old).
    pub const LEGACY: &str = "legacy";
}

/// Authentication result injected into game launch arguments.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthInfo {
    /// Player display name.
    pub username: String,
    /// Player UUID (compact or hyphenated, consistent with the session).
    pub uuid: String,
    /// Access token passed as `--accessToken` / `${auth_access_token}`.
    pub access_token: String,
    /// `msa`, `mojang`, or `legacy` (see [`user_type`]).
    pub user_type: String,
    /// User properties JSON (`"{}"` when none).
    pub user_properties: String,
}

impl AuthInfo {
    /// Creates an [`AuthInfo`] value object.
    pub fn new(
        username: impl Into<String>,
        uuid: impl Into<String>,
        access_token: impl Into<String>,
        user_type: impl Into<String>,
        user_properties: impl Into<String>,
    ) -> Self {
        Self {
            username: username.into(),
            uuid: uuid.into(),
            access_token: access_token.into(),
            user_type: user_type.into(),
            user_properties: user_properties.into(),
        }
    }

    /// Derives an [`AuthInfo`] from a persisted session.
    ///
    /// The user type is inferred from the provider: `msa` for Microsoft and
    /// offline accounts (HMCL's offline `logIn`/`playOffline` also use `msa`),
    /// `legacy` for legacy Mojang and unknown providers.
    pub fn from_session(session: &AuthSession) -> Self {
        let user_type = match session.provider.as_str() {
            "microsoft" | "offline" => user_type::MSA,
            _ => user_type::LEGACY,
        };
        Self {
            username: session.username.clone(),
            uuid: session.uuid.clone(),
            access_token: session.access_token.clone(),
            user_type: user_type.to_string(),
            user_properties: "{}".to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::account_id::AccountID;
    use crate::types::AuthSession;

    #[test]
    fn test_from_session_microsoft() {
        let session = AuthSession {
            account_id: AccountID::generate().to_string(),
            provider: "microsoft".into(),
            access_token: "tok".into(),
            username: "Steve".into(),
            uuid: "1234".into(),
            refresh_token: Some("rt".into()),
            expires_at: None,
        };
        let info = AuthInfo::from_session(&session);
        assert_eq!(info.user_type, user_type::MSA);
        assert_eq!(info.username, "Steve");
        assert_eq!(info.user_properties, "{}");
    }

    #[test]
    fn test_from_session_offline_is_msa() {
        let session = AuthSession {
            account_id: AccountID::generate().to_string(),
            provider: "offline".into(),
            access_token: "0".into(),
            username: "Alex".into(),
            uuid: "abcd".into(),
            refresh_token: None,
            expires_at: None,
        };
        let info = AuthInfo::from_session(&session);
        // P2：离线账户对齐 HMCL — user_type 为 msa（随机 token）。
        assert_eq!(info.user_type, user_type::MSA);
    }
}
