//! Generic settings file accessor with `$schema` compatibility handling.
//!
//! Mirrors HMCL's `JsonSchemaSetting` + `JsonSettingFile`: a typed settings
//! object is loaded through [`SettingFile::load`] with the [`schema`](crate::schema)
//! compatibility state machine, falls back to a default value (backing up
//! malformed files) and tracks whether saving is allowed.

use crate::file_utils::backup_invalid_config;
use crate::schema::{check_schema, ConfigSchema, SchemaStatus};
use rmccc_error::Result;
use serde::de::DeserializeOwned;
use serde::Serialize;
use std::path::{Path, PathBuf};

/// Access metadata attached to a settings object by [`SettingFile::load`].
///
/// Mirrors HMCL's `JsonSchemaSetting`: the `$schema` marker, whether the file
/// may be saved, and whether the next save must back up the existing file.
pub trait JsonSetting: Serialize + DeserializeOwned + Sized {
    fn schema(&self) -> &ConfigSchema;
    fn set_schema(&mut self, schema: ConfigSchema);
    fn is_savable(&self) -> bool;
    fn set_savable(&mut self, savable: bool);
    fn is_backup_on_next_save(&self) -> bool;
    fn set_backup_on_next_save(&mut self, v: bool);
}

/// Whether a loaded settings file may be read and overwritten.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingFileAccess {
    /// File may be read and rewritten.
    ReadWrite,
    /// File may be read but not overwritten (e.g. newer schema).
    ReadOnly,
    /// File must not be read (missing/malformed/foreign schema).
    Unreadable,
}

impl SettingFileAccess {
    pub fn can_save(&self) -> bool {
        matches!(self, SettingFileAccess::ReadWrite)
    }
}

/// Result of loading a settings file.
pub struct LoadResult<T> {
    pub value: T,
    pub access: SettingFileAccess,
}

/// A typed settings file accessor.
///
/// Mirrors HMCL's `JsonSettingFile`:
/// - missing file → `migrated` (or default), ReadWrite;
/// - malformed JSON → default with `backup_on_next_save`, backed up on the
///   next save;
/// - unreadable schema → default, Unreadable;
/// - older schema → deserialized and upgraded to the expected schema;
/// - same-minor schema → deserialized, original marker preserved;
/// - newer-minor schema → deserialized read-only.
pub struct SettingFile<T: JsonSetting> {
    location: PathBuf,
    display_name: &'static str,
    expected_schema: ConfigSchema,
    default_fn: fn() -> T,
}

impl<T: JsonSetting> SettingFile<T> {
    pub fn new(
        location: PathBuf,
        display_name: &'static str,
        expected_schema: ConfigSchema,
        default_fn: fn() -> T,
    ) -> Self {
        Self {
            location,
            display_name,
            expected_schema,
            default_fn,
        }
    }

    pub fn location(&self) -> &Path {
        &self.location
    }

    pub fn expected_schema(&self) -> &ConfigSchema {
        &self.expected_schema
    }

    /// Loads the settings file with `$schema` compatibility handling.
    pub fn load(&self, migrated: Option<T>) -> LoadResult<T> {
        if !self.location.exists() {
            let value = migrated.unwrap_or_else(self.default_fn);
            return LoadResult {
                value,
                access: SettingFileAccess::ReadWrite,
            };
        }

        let raw = match std::fs::read_to_string(&self.location) {
            Ok(content) => content,
            Err(e) => {
                tracing::warn!(
                    path = %self.location.display(),
                    error = %e,
                    "Cannot read {}",
                    self.display_name
                );
                return LoadResult {
                    value: (self.default_fn)(),
                    access: SettingFileAccess::Unreadable,
                };
            }
        };
        let root: serde_json::Value = match serde_json::from_str(&raw) {
            Ok(v) => v,
            Err(e) => {
                tracing::warn!(
                    path = %self.location.display(),
                    error = %e,
                    "Malformed {} — will back up and fall back to defaults",
                    self.display_name
                );
                let mut value = (self.default_fn)();
                value.set_backup_on_next_save(true);
                return LoadResult {
                    value,
                    access: SettingFileAccess::ReadWrite,
                };
            }
        };

        let schema_result = check_schema(&root, &self.expected_schema);
        match &schema_result.status {
            SchemaStatus::Missing => {
                tracing::warn!(
                    "Missing schema in {}: {}",
                    self.display_name,
                    self.location.display()
                );
            }
            SchemaStatus::Invalid { invalid_value } => {
                tracing::warn!(
                    "Invalid schema in {}: {}, actual: {}",
                    self.display_name,
                    self.location.display(),
                    invalid_value
                );
            }
            SchemaStatus::Unparseable => {
                tracing::warn!(
                    "Unparseable schema in {}: {}",
                    self.display_name,
                    self.location.display()
                );
            }
            SchemaStatus::UnexpectedId => {
                tracing::warn!(
                    "Unexpected {} schema. Expected: {}, at: {}",
                    self.display_name,
                    self.expected_schema,
                    self.location.display()
                );
            }
            SchemaStatus::UnsupportedMajor | SchemaStatus::ReadOnlyPreserveSchema => {
                tracing::warn!(
                    "Unsupported {} schema. Expected: {}, at: {}",
                    self.display_name,
                    self.expected_schema,
                    self.location.display()
                );
            }
            SchemaStatus::ReadWrite | SchemaStatus::ReadWritePreserveSchema => {}
        }

        if !schema_result.readable() {
            return LoadResult {
                value: (self.default_fn)(),
                access: SettingFileAccess::Unreadable,
            };
        }

        // 反序列化；未知字段经 `#[serde(flatten)] extra` 保留，向前兼容。
        let mut value: T = match serde_json::from_value(root) {
            Ok(v) => v,
            Err(e) => {
                tracing::warn!(
                    error = %e,
                    "Failed to deserialize {}",
                    self.display_name
                );
                let mut value = (self.default_fn)();
                value.set_backup_on_next_save(true);
                return LoadResult {
                    value,
                    access: SettingFileAccess::ReadWrite,
                };
            }
        };

        // 仅旧版本（ReadWrite）升级到当前 schema；同 minor 保留原 marker。
        if !schema_result.preserve_schema() && value.schema() != &self.expected_schema {
            value.set_schema(self.expected_schema.clone());
        }

        let access = if schema_result.allow_save() {
            SettingFileAccess::ReadWrite
        } else {
            SettingFileAccess::ReadOnly
        };
        value.set_savable(access.can_save());
        LoadResult { value, access }
    }

    /// Saves the settings object, backing up the file first when required.
    pub fn save(&self, value: &mut T) -> Result<()> {
        if value.is_backup_on_next_save() {
            value.set_backup_on_next_save(false);
            backup_invalid_config(&self.location);
        }
        self.write(value)
    }

    /// Backs up the current file and overwrites it with the given value using
    /// the expected schema (used to recover from an invalid configuration).
    pub fn backup_and_overwrite(&self, value: &mut T) -> Result<()> {
        backup_invalid_config(&self.location);
        value.set_schema(self.expected_schema.clone());
        value.set_savable(true);
        value.set_backup_on_next_save(false);
        self.write(value)
    }

    fn write(&self, value: &T) -> Result<()> {
        if let Some(parent) = self.location.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let tmp = self.location.with_extension("json.tmp");
        let content = serde_json::to_string_pretty(value)?;
        std::fs::write(&tmp, content)?;
        std::fs::rename(&tmp, &self.location)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::SchemaVersion;
    use serde::de::Deserializer;
    use serde::Deserialize;
    use std::path::PathBuf;

    fn expected() -> ConfigSchema {
        ConfigSchema::new("dummy", SchemaVersion::new(1, 2, 0))
    }

    /// A minimal settings double carrying a `$schema` marker + a `name` field,
    /// used to exercise [`SettingFile`] state-machine behavior in isolation.
    #[derive(Debug, Clone)]
    struct DummySetting {
        schema: ConfigSchema,
        savable: bool,
        backup_on_next_save: bool,
        name: String,
    }

    impl DummySetting {
        fn new(schema: ConfigSchema, name: &str) -> Self {
            Self {
                schema,
                savable: true,
                backup_on_next_save: false,
                name: name.to_string(),
            }
        }
    }

    impl JsonSetting for DummySetting {
        fn schema(&self) -> &ConfigSchema {
            &self.schema
        }
        fn set_schema(&mut self, schema: ConfigSchema) {
            self.schema = schema;
        }
        fn is_savable(&self) -> bool {
            self.savable
        }
        fn set_savable(&mut self, savable: bool) {
            self.savable = savable;
        }
        fn is_backup_on_next_save(&self) -> bool {
            self.backup_on_next_save
        }
        fn set_backup_on_next_save(&mut self, v: bool) {
            self.backup_on_next_save = v;
        }
    }

    impl Serialize for DummySetting {
        fn serialize<S: serde::Serializer>(
            &self,
            serializer: S,
        ) -> std::result::Result<S::Ok, S::Error> {
            use serde::ser::SerializeStruct;
            let mut s = serializer.serialize_struct("DummySetting", 2)?;
            s.serialize_field("$schema", &self.schema.url())?;
            s.serialize_field("name", &self.name)?;
            s.end()
        }
    }

    impl<'de> Deserialize<'de> for DummySetting {
        fn deserialize<D: Deserializer<'de>>(
            deserializer: D,
        ) -> std::result::Result<Self, D::Error> {
            let value = serde_json::Value::deserialize(deserializer)?;
            let schema_url = value
                .get("$schema")
                .and_then(|v| v.as_str())
                .unwrap_or_default();
            let version = parse_url_version(schema_url).unwrap_or(SchemaVersion::new(0, 0, 0));
            let name = value
                .get("name")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string();
            Ok(DummySetting::new(
                ConfigSchema::new("dummy", version),
                &name,
            ))
        }
    }

    fn parse_url_version(url: &str) -> Option<SchemaVersion> {
        let rest = url.strip_prefix(crate::schema::SCHEMA_URL_PREFIX)?;
        let version_str = rest.rsplit('/').next()?;
        SchemaVersion::parse(version_str)
    }

    #[test]
    fn test_dummy_roundtrip() {
        let dir = temp_dir("roundtrip");
        let file =
            SettingFile::<DummySetting>::new(dir.join("dummy.json"), "dummy", expected(), || {
                DummySetting::new(expected(), "default")
            });

        let mut value = DummySetting::new(expected(), "hello");
        file.save(&mut value).unwrap();

        let loaded = file.load(None);
        assert!(matches!(loaded.access, SettingFileAccess::ReadWrite));
        assert_eq!(loaded.value.name, "hello");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_missing_file_uses_default() {
        let dir = temp_dir("missing");
        let file =
            SettingFile::<DummySetting>::new(dir.join("dummy.json"), "dummy", expected(), || {
                DummySetting::new(expected(), "default")
            });
        let loaded = file.load(None);
        assert!(matches!(loaded.access, SettingFileAccess::ReadWrite));
        assert_eq!(loaded.value.name, "default");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_malformed_json_backs_up_on_next_save() {
        let dir = temp_dir("malformed");
        let path = dir.join("dummy.json");
        std::fs::write(&path, "{ not json !!").unwrap();
        let file = SettingFile::<DummySetting>::new(path.clone(), "dummy", expected(), || {
            DummySetting::new(expected(), "default")
        });

        let loaded = file.load(None);
        assert!(matches!(loaded.access, SettingFileAccess::ReadWrite));
        assert!(loaded.value.is_backup_on_next_save());

        // Saving backs up the malformed file first.
        let mut v = loaded.value;
        file.save(&mut v).unwrap();
        assert!(path.with_file_name("dummy.json.1").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_older_schema_upgraded_to_expected() {
        let dir = temp_dir("older");
        let path = dir.join("dummy.json");
        let older = ConfigSchema::new("dummy", SchemaVersion::new(1, 1, 0));
        let mut value = DummySetting::new(older, "old");
        let file = SettingFile::<DummySetting>::new(path.clone(), "dummy", expected(), || {
            DummySetting::new(expected(), "default")
        });
        file.save(&mut value).unwrap();

        let loaded = file.load(None);
        assert!(matches!(loaded.access, SettingFileAccess::ReadWrite));
        assert_eq!(
            loaded.value.schema,
            expected(),
            "older schema must be upgraded"
        );
        assert_eq!(loaded.value.name, "old");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_newer_minor_is_read_only() {
        let dir = temp_dir("readonly");
        let path = dir.join("dummy.json");
        let newer = ConfigSchema::new("dummy", SchemaVersion::new(1, 3, 0));
        let mut value = DummySetting::new(newer.clone(), "new");
        let file = SettingFile::<DummySetting>::new(path.clone(), "dummy", expected(), || {
            DummySetting::new(expected(), "default")
        });
        file.save(&mut value).unwrap();

        let loaded = file.load(None);
        assert!(matches!(loaded.access, SettingFileAccess::ReadOnly));
        assert_eq!(loaded.value.schema, newer, "newer marker must be preserved");
        assert!(!loaded.value.is_savable());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_unreadable_foreign_schema_falls_back() {
        let dir = temp_dir("foreign");
        let path = dir.join("dummy.json");
        std::fs::write(
            &path,
            r#"{"$schema": "https://hmcl.huangyuhui.net/json/schema/other/1.0.0", "name": "x"}"#,
        )
        .unwrap();
        let file = SettingFile::<DummySetting>::new(path.clone(), "dummy", expected(), || {
            DummySetting::new(expected(), "default")
        });
        let loaded = file.load(None);
        assert!(matches!(loaded.access, SettingFileAccess::Unreadable));
        assert_eq!(loaded.value.name, "default");
        let _ = std::fs::remove_dir_all(&dir);
    }

    fn temp_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "rmccc-setting-test-{}-{}-{}",
            tag,
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }
}
