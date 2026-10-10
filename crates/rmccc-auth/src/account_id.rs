//! Stable identifier for a persisted account record.
//!
//! Mirrors HMCL's [`AccountID`] design: this ID identifies the launcher account
//! record itself and must not be confused with the Minecraft profile ID, login
//! name, or any other authentication detail that may be shared by more than one
//! account record. It is serialized as a prefixed string `account:<uuid>`.
//!
//! [`AccountID`]: https://github.com/HMCL-dev/HMCL/blob/master/HMCLCore/src/main/java/org/jackhuang/hmcl/auth/AccountID.java

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::fmt;
use std::str::FromStr;
use uuid::Uuid;

/// The serialized account ID prefix.
pub const ACCOUNT_ID_PREFIX: &str = "account";

/// Stable ID for a persisted account entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct AccountID(Uuid);

impl AccountID {
    /// Creates a new random account ID (UUID v4).
    pub fn generate() -> Self {
        Self(Uuid::new_v4())
    }

    /// Returns the underlying UUID payload.
    pub fn as_uuid(&self) -> Uuid {
        self.0
    }

    /// Parses a prefixed account ID string (`account:<uuid>`).
    ///
    /// # Errors
    ///
    /// Returns a descriptive [`ParseError`] when the string lacks the expected
    /// prefix or contains an invalid UUID.
    pub fn parse(value: &str) -> Result<Self, ParseError> {
        let rest = value
            .strip_prefix(&format!("{ACCOUNT_ID_PREFIX}:"))
            .ok_or(ParseError::MissingPrefix)?;
        let uuid = Uuid::parse_str(rest).map_err(|_| ParseError::InvalidUuid)?;
        Ok(Self(uuid))
    }
}

impl fmt::Display for AccountID {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{ACCOUNT_ID_PREFIX}:{}", self.0)
    }
}

impl FromStr for AccountID {
    type Err = ParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::parse(s)
    }
}

impl Serialize for AccountID {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_string())
    }
}

impl<'de> Deserialize<'de> for AccountID {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let s = String::deserialize(deserializer)?;
        Self::parse(&s).map_err(serde::de::Error::custom)
    }
}

/// Errors that can occur when parsing an [`AccountID`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParseError {
    /// The string does not start with the `account:` prefix.
    MissingPrefix,
    /// The UUID part is not a valid UUID.
    InvalidUuid,
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingPrefix => write!(f, "account ID missing '{ACCOUNT_ID_PREFIX}:' prefix"),
            Self::InvalidUuid => write!(f, "account ID contains an invalid UUID"),
        }
    }
}

impl std::error::Error for ParseError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_and_display() {
        let id = AccountID::generate();
        let s = id.to_string();
        assert!(s.starts_with("account:"), "unexpected format: {s}");
        // Parsing our own serialization round-trips.
        assert_eq!(AccountID::parse(&s).unwrap(), id);
    }

    #[test]
    fn test_parse_roundtrip() {
        let uuid = Uuid::new_v4();
        let id = AccountID::parse(&format!("account:{uuid}")).unwrap();
        assert_eq!(id.as_uuid(), uuid);
        assert_eq!(id.to_string(), format!("account:{uuid}"));
    }

    #[test]
    fn test_parse_errors() {
        assert_eq!(
            AccountID::parse("uuid:abc").unwrap_err(),
            ParseError::MissingPrefix
        );
        assert_eq!(
            AccountID::parse("account:not-a-uuid").unwrap_err(),
            ParseError::InvalidUuid
        );
    }

    #[test]
    fn test_serde_json_string_form() {
        let id = AccountID::generate();
        let json = serde_json::to_string(&id).unwrap();
        assert_eq!(json, format!("\"{id}\""));
        let back: AccountID = serde_json::from_str(&json).unwrap();
        assert_eq!(back, id);
    }
}
