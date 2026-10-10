//! JSON schema versioning & compatibility state machine.
//!
//! Mirrors HMCL's [`JsonSchema`]: every settings file carries a `$schema` marker
//! (a URL like `https://rmcl.snowebsite.top/json/schema/<id>/<version>`). On
//! load the marker is compared against the schema the current code understands,
//! producing one of eight compatibility states that drive read/save behavior:
//! older files are readable & upgradable, same-minor files keep their marker,
//! newer-minor files are read-only, and foreign/malformed markers are rejected.
//!
//! [`JsonSchema`]: https://github.com/HMCL-dev/HMCL/blob/master/HMCLCore/src/main/java/org/jackhuang/hmcl/util/gson/JsonSchema.java

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::fmt;

/// Canonical prefix of every RMCCC JSON schema URL.
pub const SCHEMA_URL_PREFIX: &str = "https://rmcl.snowebsite.top/json/schema/";

/// Prefixes accepted on read for backward compatibility.
///
/// RMCCC originally copied HMCL's schema URL verbatim, so every settings file
/// written before the namespace change carries the HMCL domain. Those files stay
/// readable, and a same-minor legacy file is reported as `ReadWrite` rather than
/// `ReadWritePreserveSchema`, which makes the loader replace the marker with
/// [`SCHEMA_URL_PREFIX`] on the next save.
pub const LEGACY_SCHEMA_URL_PREFIXES: &[&str] = &["https://hmcl.huangyuhui.net/json/schema/"];

/// Semantic version of a serialized JSON schema (`major.minor.patch`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SchemaVersion {
    pub major: u32,
    pub minor: u32,
    pub patch: u32,
}

impl SchemaVersion {
    pub const fn new(major: u32, minor: u32, patch: u32) -> Self {
        Self {
            major,
            minor,
            patch,
        }
    }

    /// Parses the strict `major.minor.patch` form.
    pub fn parse(s: &str) -> Option<Self> {
        let mut parts = s.split('.');
        let major = parts.next()?.parse().ok()?;
        let minor = parts.next()?.parse().ok()?;
        let patch = parts.next()?.parse().ok()?;
        if parts.next().is_some() {
            return None;
        }
        Some(Self {
            major,
            minor,
            patch,
        })
    }
}

impl fmt::Display for SchemaVersion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}.{}", self.major, self.minor, self.patch)
    }
}

/// A schema the current code understands for one settings file.
///
/// Serializes to its canonical `$schema` URL string and deserializes back from
/// it, so settings objects can embed a `$schema` marker directly.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfigSchema {
    /// Stable schema identifier (e.g. `launcher-settings`, `launcher-state`).
    pub id: String,
    /// Schema version supported by the current code.
    pub version: SchemaVersion,
}

/// Parsed `$schema` marker.
struct ParsedSchema {
    id: String,
    version: SchemaVersion,
    /// True when the marker used a legacy (pre-rename) prefix.
    legacy: bool,
}

impl ConfigSchema {
    pub fn new(id: impl Into<String>, version: SchemaVersion) -> Self {
        Self {
            id: id.into(),
            version,
        }
    }

    /// Canonical schema URL, e.g.
    /// `https://rmcl.snowebsite.top/json/schema/launcher-settings/1.0.0`.
    pub fn url(&self) -> String {
        format!("{SCHEMA_URL_PREFIX}{}/{}", self.id, self.version)
    }
}

impl fmt::Display for ConfigSchema {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.url())
    }
}

impl Serialize for ConfigSchema {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.url())
    }
}

impl<'de> Deserialize<'de> for ConfigSchema {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let s = String::deserialize(deserializer)?;
        let parsed = parse_schema_url(&s)
            .ok_or_else(|| serde::de::Error::custom(format!("Invalid schema URL: {s}")))?;
        Ok(ConfigSchema {
            id: parsed.id,
            version: parsed.version,
        })
    }
}

/// Compatibility state of a loaded file relative to the expected schema.
///
/// Mirrors HMCL `JsonSchema.CompatibilityResult.Status`:
/// - `ReadWrite`: older compatible schema — may be read and rewritten (upgraded).
/// - `ReadWritePreserveSchema`: same minor — may be rewritten, marker preserved.
/// - `ReadOnlyPreserveSchema`: newer minor — may be read but not overwritten.
/// - the rest: the file must be rejected.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SchemaStatus {
    /// Older compatible schema; may be read and rewritten (upgraded).
    ReadWrite,
    /// Same minor; may be rewritten while preserving the original marker.
    ReadWritePreserveSchema,
    /// Newer minor; may be read but must not be overwritten.
    ReadOnlyPreserveSchema,
    /// No `$schema` marker present.
    Missing,
    /// The `$schema` member exists but is not a JSON string.
    Invalid { invalid_value: String },
    /// The `$schema` string is not a parseable schema URL.
    Unparseable,
    /// The schema id does not match the expected one.
    UnexpectedId,
    /// The schema major version is newer than the one supported.
    UnsupportedMajor,
}

impl SchemaStatus {
    /// Whether a file with this status may be deserialized.
    pub fn readable(&self) -> bool {
        matches!(
            self,
            SchemaStatus::ReadWrite
                | SchemaStatus::ReadWritePreserveSchema
                | SchemaStatus::ReadOnlyPreserveSchema
        )
    }

    /// Whether a file with this status may be overwritten.
    pub fn allow_save(&self) -> bool {
        matches!(
            self,
            SchemaStatus::ReadWrite | SchemaStatus::ReadWritePreserveSchema
        )
    }

    /// Whether saving should preserve the original `$schema` marker.
    pub fn preserve_schema(&self) -> bool {
        matches!(
            self,
            SchemaStatus::ReadWritePreserveSchema | SchemaStatus::ReadOnlyPreserveSchema
        )
    }
}

/// Result of checking a JSON document against an expected schema.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompatibilityResult {
    pub status: SchemaStatus,
    /// True when the file's marker used a legacy namespace prefix.
    ///
    /// Needed because [`ConfigSchema`] itself does not retain the namespace once
    /// parsed, so comparing the parsed schema against the expected one cannot
    /// tell a legacy marker apart from an identical current one.
    pub legacy_namespace: bool,
}

impl CompatibilityResult {
    pub fn readable(&self) -> bool {
        self.status.readable()
    }

    pub fn allow_save(&self) -> bool {
        self.status.allow_save()
    }

    pub fn preserve_schema(&self) -> bool {
        self.status.preserve_schema()
    }
}

/// Parses an RMCCC schema URL (or a legacy one), returning the id, version and
/// whether a legacy prefix was used. `None` when the string is not a valid
/// schema URL.
fn parse_schema_url(value: &str) -> Option<ParsedSchema> {
    let (path, legacy) = if let Some(rest) = value.strip_prefix(SCHEMA_URL_PREFIX) {
        (rest, false)
    } else {
        let rest = LEGACY_SCHEMA_URL_PREFIXES
            .iter()
            .find_map(|prefix| value.strip_prefix(prefix))?;
        (rest, true)
    };

    let slash = path.find('/')?;
    if slash == 0 || slash != path.rfind('/')? || slash == path.len() - 1 {
        return None;
    }
    let id = &path[..slash];
    if !is_valid_id(id) {
        return None;
    }
    let version_str = &path[slash + 1..];
    let version = SchemaVersion::parse(version_str)?;
    if !is_accepted_version_string(version_str, version) {
        return None;
    }
    Some(ParsedSchema {
        id: id.to_string(),
        version,
        legacy,
    })
}

fn is_valid_id(id: &str) -> bool {
    if id.is_empty() {
        return false;
    }
    let mut chars = id.chars();
    if !chars.next().is_some_and(|c| c.is_ascii_lowercase()) {
        return false;
    }
    id.chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '_')
}

/// Whether a raw version string is an accepted representation of a parsed version.
fn is_accepted_version_string(version_str: &str, version: SchemaVersion) -> bool {
    version_str == version.to_string()
        || (version.patch == 0 && version_str == format!("{}.{}", version.major, version.minor))
}

/// Checks a JSON document against the schema the current code understands.
///
/// Mirrors `JsonSchema.check(JsonObject, JsonSchema)`. The `$schema` marker is
/// read from `root`, compared against `expected`, and the resulting
/// [`CompatibilityResult`] drives whether the file may be read / saved.
pub fn check_schema(root: &serde_json::Value, expected: &ConfigSchema) -> CompatibilityResult {
    /// Builds a result without a legacy marker.
    fn plain(status: SchemaStatus) -> CompatibilityResult {
        CompatibilityResult {
            status,
            legacy_namespace: false,
        }
    }

    let Some(marker) = root.get("$schema") else {
        return plain(SchemaStatus::Missing);
    };
    let Some(value) = marker.as_str() else {
        return plain(SchemaStatus::Invalid {
            invalid_value: marker.to_string(),
        });
    };
    let Some(parsed) = parse_schema_url(value) else {
        return plain(SchemaStatus::Unparseable);
    };
    if parsed.id != expected.id {
        return plain(SchemaStatus::UnexpectedId);
    }
    if parsed.version.major != expected.version.major {
        return plain(SchemaStatus::UnsupportedMajor);
    }
    if parsed.version.minor > expected.version.minor {
        return CompatibilityResult {
            status: SchemaStatus::ReadOnlyPreserveSchema,
            legacy_namespace: parsed.legacy,
        };
    }
    if parsed.version.minor == expected.version.minor {
        // A legacy marker is reported as an upgrade rather than "preserve", so
        // the loader swaps it for the canonical namespace on the next save.
        return CompatibilityResult {
            status: if parsed.legacy {
                SchemaStatus::ReadWrite
            } else {
                SchemaStatus::ReadWritePreserveSchema
            },
            legacy_namespace: parsed.legacy,
        };
    }
    CompatibilityResult {
        status: SchemaStatus::ReadWrite,
        legacy_namespace: parsed.legacy,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn expected() -> ConfigSchema {
        ConfigSchema::new("launcher-settings", SchemaVersion::new(1, 2, 0))
    }

    fn root(schema: Option<&str>) -> serde_json::Value {
        let mut v = serde_json::json!({ "version": 1 });
        if let Some(s) = schema {
            v["$schema"] = serde_json::Value::String(s.to_string());
        }
        v
    }

    #[test]
    fn test_read_write_for_older_minor() {
        let r = check_schema(
            &root(Some(
                "https://rmcl.snowebsite.top/json/schema/launcher-settings/1.1.0",
            )),
            &expected(),
        );
        assert_eq!(r.status, SchemaStatus::ReadWrite);
        assert!(r.readable() && r.allow_save() && !r.preserve_schema());
    }

    #[test]
    fn test_read_write_preserve_for_same_minor() {
        let r = check_schema(
            &root(Some(
                "https://rmcl.snowebsite.top/json/schema/launcher-settings/1.2.5",
            )),
            &expected(),
        );
        assert_eq!(r.status, SchemaStatus::ReadWritePreserveSchema);
        assert!(r.readable() && r.allow_save() && r.preserve_schema());
    }

    #[test]
    fn test_read_only_preserve_for_newer_minor() {
        let r = check_schema(
            &root(Some(
                "https://rmcl.snowebsite.top/json/schema/launcher-settings/1.3.0",
            )),
            &expected(),
        );
        assert_eq!(r.status, SchemaStatus::ReadOnlyPreserveSchema);
        assert!(r.readable() && !r.allow_save() && r.preserve_schema());
    }

    #[test]
    fn test_missing() {
        let r = check_schema(&root(None), &expected());
        assert_eq!(r.status, SchemaStatus::Missing);
        assert!(!r.readable());
    }

    #[test]
    fn test_invalid_not_a_string() {
        let mut v = root(None);
        v["$schema"] = serde_json::json!(42);
        let r = check_schema(&v, &expected());
        match &r.status {
            SchemaStatus::Invalid { invalid_value } => assert_eq!(invalid_value, "42"),
            other => panic!("expected Invalid, got {other:?}"),
        }
        assert!(!r.readable());
    }

    #[test]
    fn test_unparseable_foreign_url() {
        let r = check_schema(&root(Some("https://example.com/schema.json")), &expected());
        assert_eq!(r.status, SchemaStatus::Unparseable);
        assert!(!r.readable());
    }

    #[test]
    fn test_unparseable_malformed_version() {
        let r = check_schema(
            &root(Some(
                "https://rmcl.snowebsite.top/json/schema/launcher-settings/abc",
            )),
            &expected(),
        );
        assert_eq!(r.status, SchemaStatus::Unparseable);
    }

    #[test]
    fn test_unexpected_id() {
        let r = check_schema(
            &root(Some(
                "https://rmcl.snowebsite.top/json/schema/user-settings/1.0.0",
            )),
            &expected(),
        );
        assert_eq!(r.status, SchemaStatus::UnexpectedId);
        assert!(!r.readable());
    }

    #[test]
    fn test_unsupported_major() {
        let r = check_schema(
            &root(Some(
                "https://rmcl.snowebsite.top/json/schema/launcher-settings/2.0.0",
            )),
            &expected(),
        );
        assert_eq!(r.status, SchemaStatus::UnsupportedMajor);
        assert!(!r.readable());
    }

    #[test]
    fn test_version_parse_and_display() {
        let v = SchemaVersion::parse("1.2.3").unwrap();
        assert_eq!(v, SchemaVersion::new(1, 2, 3));
        assert_eq!(v.to_string(), "1.2.3");
        assert!(SchemaVersion::parse("1.2").is_none());
        assert!(SchemaVersion::parse("1.2.3.4").is_none());
        assert!(SchemaVersion::parse("x.2.3").is_none());
    }

    /// Files written before the namespace change still parse and, at the same
    /// minor, are reported as an upgrade so the marker gets rewritten.
    #[test]
    fn test_legacy_namespace_is_readable_and_upgraded() {
        let r = check_schema(
            &root(Some(
                "https://hmcl.huangyuhui.net/json/schema/launcher-settings/1.2.0",
            )),
            &expected(),
        );
        assert_eq!(r.status, SchemaStatus::ReadWrite);
        assert!(r.readable() && r.allow_save());
        assert!(
            !r.preserve_schema(),
            "legacy markers must be rewritten in the new namespace"
        );
    }

    /// A legacy marker from a *newer* minor stays read-only.
    #[test]
    fn test_legacy_newer_minor_stays_read_only() {
        let r = check_schema(
            &root(Some(
                "https://hmcl.huangyuhui.net/json/schema/launcher-settings/1.9.0",
            )),
            &expected(),
        );
        assert_eq!(r.status, SchemaStatus::ReadOnlyPreserveSchema);
        assert!(r.readable() && !r.allow_save());
    }

    /// Deserializing a legacy `$schema` yields the canonical schema, so a
    /// round-trip through the current code emits the new namespace.
    #[test]
    fn test_legacy_schema_deserializes_to_canonical() {
        let parsed: ConfigSchema = serde_json::from_str(
            "\"https://hmcl.huangyuhui.net/json/schema/launcher-settings/1.0.0\"",
        )
        .unwrap();
        assert_eq!(parsed.id, "launcher-settings");
        assert_eq!(parsed.version, SchemaVersion::new(1, 0, 0));
        assert_eq!(
            parsed.url(),
            format!("{SCHEMA_URL_PREFIX}launcher-settings/1.0.0")
        );
    }

    #[test]
    fn test_config_schema_url() {
        let s = ConfigSchema::new("launcher-settings", SchemaVersion::new(1, 0, 0));
        assert_eq!(
            s.url(),
            "https://rmcl.snowebsite.top/json/schema/launcher-settings/1.0.0"
        );
    }
}
