//! Instance and game-directory identifiers.
//!
//! Instance ids are **user-visible names and on-disk folder names at the same
//! time** (HMCL `GameInstanceID`): `versions/{id}/{id}.json`. They must be safe
//! as a single path component on every platform, because every instance
//! operation eventually joins the id into a path.

use rmccc_error::{Result, RmcccError};
use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;
use ts_rs::TS;

/// Maximum instance-id length in bytes.
///
/// The old profile store allowed 64; HMCL has no explicit cap. 128 keeps the
/// id usable as a folder name while leaving room for loader suffixes.
pub const MAX_INSTANCE_ID_LEN: usize = 128;

/// A version-backed instance id (also the `versions/{id}` folder name).
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, TS)]
#[ts(export_to = "types.ts", as = "String")]
pub struct InstanceId(String);

impl InstanceId {
    /// Parse and validate an instance id.
    pub fn parse(raw: &str) -> Result<Self> {
        validate_instance_id(raw)?;
        Ok(Self(raw.to_string()))
    }

    /// Return the id as a string slice.
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Consume the id and return its string.
    pub fn into_string(self) -> String {
        self.0
    }
}

impl Serialize for InstanceId {
    fn serialize<S: serde::Serializer>(
        &self,
        serializer: S,
    ) -> std::result::Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.0)
    }
}

impl<'de> Deserialize<'de> for InstanceId {
    fn deserialize<D: serde::Deserializer<'de>>(
        deserializer: D,
    ) -> std::result::Result<Self, D::Error> {
        let value = String::deserialize(deserializer)?;
        Self::parse(&value).map_err(serde::de::Error::custom)
    }
}

impl TryFrom<String> for InstanceId {
    type Error = RmcccError;
    fn try_from(value: String) -> Result<Self> {
        Self::parse(&value)
    }
}

impl From<InstanceId> for String {
    fn from(value: InstanceId) -> Self {
        value.0
    }
}

impl fmt::Display for InstanceId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl AsRef<str> for InstanceId {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl FromStr for InstanceId {
    type Err = RmcccError;
    fn from_str(s: &str) -> Result<Self> {
        Self::parse(s)
    }
}

/// Whether `raw` is a valid instance id.
///
/// Rules (stricter than HMCL's bare non-empty check because RMCCC also uses the
/// id in IPC keys and frontend routes):
/// - non-empty and at most [`MAX_INSTANCE_ID_LEN`] bytes;
/// - `.` / `..` rejected; path separators, `:`, NUL and control chars rejected;
/// - no trailing dot or space (Windows silently strips them, causing
///   id↔folder mismatches);
/// - Windows reserved device names rejected case-insensitively.
pub fn validate_instance_id(raw: &str) -> Result<()> {
    let invalid = |reason: &str| {
        Err(RmcccError::Validation(format!(
            "invalid instance id {raw:?}: {reason}"
        )))
    };

    if raw.is_empty() {
        return invalid("must not be empty");
    }
    if raw.len() > MAX_INSTANCE_ID_LEN {
        return invalid("too long");
    }
    if raw == "." || raw == ".." {
        return invalid("reserved path component");
    }
    if raw.ends_with('.') || raw.ends_with(' ') {
        return invalid("must not end with a dot or space");
    }
    if raw.chars().any(char::is_control) {
        return invalid("contains control characters");
    }
    if raw.contains(['/', '\\', ':', '\0']) {
        return invalid("contains a path separator or ':'");
    }

    // Windows device names are reserved even with an extension (`CON.json`).
    let stem = raw.split('.').next().unwrap_or(raw).to_ascii_uppercase();
    const RESERVED: &[&str] = &[
        "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8",
        "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
    ];
    if RESERVED.contains(&stem.as_str()) {
        return invalid("reserved Windows device name");
    }

    Ok(())
}

/// A registered game directory id.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, TS)]
#[ts(export_to = "types.ts", as = "String")]
pub struct GameDirectoryId(String);

impl GameDirectoryId {
    /// Parse a game-directory id (`[a-z0-9][a-z0-9-]{0,63}`).
    pub fn parse(raw: &str) -> Result<Self> {
        let ok = !raw.is_empty()
            && raw.len() <= 64
            && raw
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
            && raw.as_bytes()[0].is_ascii_alphanumeric();
        if !ok {
            return Err(RmcccError::Validation(format!(
                "invalid game directory id: {raw:?}"
            )));
        }
        Ok(Self(raw.to_string()))
    }

    /// Generate a fresh stable id for a newly registered directory.
    pub fn generate() -> Self {
        Self(format!("dir-{}", uuid::Uuid::new_v4()))
    }

    /// The built-in primary directory id.
    pub fn default_id() -> Self {
        Self("default".to_string())
    }

    /// Return the id as a string slice.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Serialize for GameDirectoryId {
    fn serialize<S: serde::Serializer>(
        &self,
        serializer: S,
    ) -> std::result::Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.0)
    }
}

impl<'de> Deserialize<'de> for GameDirectoryId {
    fn deserialize<D: serde::Deserializer<'de>>(
        deserializer: D,
    ) -> std::result::Result<Self, D::Error> {
        let value = String::deserialize(deserializer)?;
        Self::parse(&value).map_err(serde::de::Error::custom)
    }
}

impl TryFrom<String> for GameDirectoryId {
    type Error = RmcccError;
    fn try_from(value: String) -> Result<Self> {
        Self::parse(&value)
    }
}

impl From<GameDirectoryId> for String {
    fn from(value: GameDirectoryId) -> Self {
        value.0
    }
}

impl fmt::Display for GameDirectoryId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl AsRef<str> for GameDirectoryId {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl FromStr for GameDirectoryId {
    type Err = RmcccError;
    fn from_str(s: &str) -> Result<Self> {
        Self::parse(s)
    }
}

/// A composite instance key: `{directory_id}/{instance_id}`.
///
/// `instance_id` may not contain `/`, and directory ids are restricted to
/// ASCII lowercase/digit/dash, so the first `/` is always the separator.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, TS)]
#[ts(export_to = "types.ts")]
pub struct InstanceKey {
    pub directory: GameDirectoryId,
    pub id: InstanceId,
}

impl InstanceKey {
    /// Build a key from its parts.
    pub fn new(directory: GameDirectoryId, id: InstanceId) -> Self {
        Self { directory, id }
    }

    /// Parse `directory/id`.
    pub fn parse(raw: &str) -> Result<Self> {
        let (directory, id) = raw.split_once('/').ok_or_else(|| {
            RmcccError::Validation(format!(
                "invalid instance key {raw:?}: expected '<directory>/<instance>'"
            ))
        })?;
        Ok(Self {
            directory: GameDirectoryId::parse(directory)?,
            id: InstanceId::parse(id)?,
        })
    }

    /// Canonical wire/UI key.
    pub fn as_key(&self) -> String {
        format!("{}/{}", self.directory, self.id)
    }
}

impl fmt::Display for InstanceKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}/{}", self.directory, self.id)
    }
}

impl FromStr for InstanceKey {
    type Err = RmcccError;
    fn from_str(s: &str) -> Result<Self> {
        Self::parse(s)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_normal_instance_ids() {
        for id in [
            "1.20.4",
            "1.20.4-fabric",
            "我的整合包",
            "a b",
            "a..b",
            "v1.2.3",
        ] {
            assert!(validate_instance_id(id).is_ok(), "{id:?}");
            assert_eq!(InstanceId::parse(id).unwrap().as_str(), id);
        }
    }

    #[test]
    fn rejects_unsafe_instance_ids() {
        for id in [
            "", ".", "..", "a/b", "a\\b", "a:b", "a\0b", "a\nb", "a.", "a ", "CON", "con.json",
            "LPT1",
        ] {
            assert!(validate_instance_id(id).is_err(), "{id:?} must be rejected");
        }
        assert!(validate_instance_id(&"x".repeat(MAX_INSTANCE_ID_LEN + 1)).is_err());
    }

    #[test]
    fn key_round_trips() {
        let key = InstanceKey::new(
            GameDirectoryId::parse("default").unwrap(),
            InstanceId::parse("1.20.4-fabric").unwrap(),
        );
        assert_eq!(key.as_key(), "default/1.20.4-fabric");
        assert_eq!(InstanceKey::parse("default/1.20.4-fabric").unwrap(), key);
        assert!(InstanceKey::parse("1.20.4-fabric").is_err());
        assert!(InstanceKey::parse("Default/1.20.4").is_err());
    }
}
