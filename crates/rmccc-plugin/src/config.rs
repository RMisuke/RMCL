//! 插件配置：`plugin-data/<id>/config.json` + `config.schema.json` 子集校验（指南 §7.4/§12.4）。
//!
//! Phase 2 不引入完整 JSON Schema 引擎，而是实现一个**严格子集**：
//! 不支持的 keyword 在安装阶段就报错，绝不静默忽略。支持的关键字见
//! [`SUPPORTED_KEYWORDS`]。

use crate::capability::plugin_error;
use crate::storage::{backup_file, now_rfc3339, read_json, write_json_atomic};
use rmccc_error::{PluginErrorCode, Result, RmcccError};
use rmccc_platform::PlatformFs;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::path::{Path, PathBuf};
use ts_rs::TS;

pub const SUPPORTED_KEYWORDS: &[&str] = &[
    "$schema",
    "$id",
    "title",
    "description",
    "default",
    "type",
    "properties",
    "required",
    "additionalProperties",
    "enum",
    "minimum",
    "maximum",
    "minLength",
    "maxLength",
    "items",
    "x-rmccc-schemaVersion",
];

pub const DEFAULT_CONFIG_SCHEMA_VERSION: u32 = 1;
const MAX_SCHEMA_BYTES: u64 = 256 * 1024;
const MAX_CONFIG_BYTES: u64 = 1024 * 1024;

/// 插件配置文档（`plugin-data/<id>/config.json`）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export_to = "types.ts")]
pub struct PluginConfig {
    pub schema_version: u32,
    #[ts(type = "unknown")]
    pub values: Value,
    #[serde(default)]
    pub updated_at: Option<String>,
}

impl PluginConfig {
    pub fn new(schema: &Value) -> Self {
        Self {
            schema_version: schema_version_of(schema),
            values: defaults(schema),
            updated_at: None,
        }
    }
}

/// 加载并校验 `config.schema.json`。缺失返回 `None`。
pub fn load_schema(path: &Path) -> Result<Option<Value>> {
    let meta = match std::fs::metadata(path) {
        Ok(meta) => meta,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e.into()),
    };
    if meta.len() > MAX_SCHEMA_BYTES {
        return Err(config_error(format!(
            "config.schema.json 过大（上限 {MAX_SCHEMA_BYTES} 字节）"
        )));
    }
    let bytes = std::fs::read(path)?;
    let schema: Value = serde_json::from_slice(&bytes)
        .map_err(|e| config_error(format!("config.schema.json 不是合法 JSON：{e}")))?;
    validate_schema_document(&schema)?;
    Ok(Some(schema))
}

/// 校验 schema 本身：必须是对象，关键字必须属于受支持子集。
pub fn validate_schema_document(schema: &Value) -> Result<()> {
    let obj = schema
        .as_object()
        .ok_or_else(|| config_error("config.schema.json 顶层必须是对象".to_string()))?;
    for key in obj.keys() {
        if !SUPPORTED_KEYWORDS.contains(&key.as_str()) {
            return Err(config_error(format!(
                "config.schema.json 使用了不支持的 keyword：{key}"
            )));
        }
    }
    validate_schema_node(schema, "$")?;
    Ok(())
}

fn validate_schema_node(schema: &Value, path: &str) -> Result<()> {
    let obj = schema
        .as_object()
        .ok_or_else(|| config_error(format!("{path}: schema 必须是对象")))?;

    if let Some(ty) = obj.get("type") {
        validate_type(ty, path)?;
    }
    if let Some(props) = obj.get("properties") {
        let props = props
            .as_object()
            .ok_or_else(|| config_error(format!("{path}.properties 必须是对象")))?;
        for (name, sub) in props {
            validate_schema_node(sub, &format!("{path}.properties.{name}"))?;
        }
    }
    if let Some(required) = obj.get("required") {
        let required = required
            .as_array()
            .ok_or_else(|| config_error(format!("{path}.required 必须是数组")))?;
        for item in required {
            if !item.is_string() {
                return Err(config_error(format!("{path}.required 只能包含字符串")));
            }
        }
    }
    if let Some(additional) = obj.get("additionalProperties") {
        if !additional.is_boolean() {
            return Err(config_error(format!(
                "{path}.additionalProperties 只支持布尔值"
            )));
        }
    }
    if let Some(enum_values) = obj.get("enum") {
        if !enum_values.is_array() {
            return Err(config_error(format!("{path}.enum 必须是数组")));
        }
    }
    for key in ["minimum", "maximum"] {
        if let Some(v) = obj.get(key) {
            if !v.is_number() {
                return Err(config_error(format!("{path}.{key} 必须是数字")));
            }
        }
    }
    for key in ["minLength", "maxLength"] {
        if let Some(v) = obj.get(key) {
            if v.as_u64().is_none() {
                return Err(config_error(format!("{path}.{key} 必须是非负整数")));
            }
        }
    }
    if let Some(items) = obj.get("items") {
        if items.is_array() {
            return Err(config_error(format!("{path}.items 只支持单个 schema")));
        }
        validate_schema_node(items, &format!("{path}.items"))?;
    }
    if let Some(version) = obj.get("x-rmccc-schemaVersion") {
        if version.as_u64().unwrap_or(0) == 0 {
            return Err(config_error(format!(
                "{path}.x-rmccc-schemaVersion 必须是正整数"
            )));
        }
    }
    Ok(())
}

fn validate_type(ty: &Value, path: &str) -> Result<()> {
    const TYPES: &[&str] = &[
        "object", "array", "string", "integer", "number", "boolean", "null",
    ];
    let check = |t: &Value| -> bool { t.as_str().map(|s| TYPES.contains(&s)).unwrap_or(false) };
    match ty {
        Value::String(_) if check(ty) => Ok(()),
        Value::Array(items) if !items.is_empty() && items.iter().all(check) => Ok(()),
        _ => Err(config_error(format!("{path}.type 非法：{ty}"))),
    }
}

fn schema_version_of(schema: &Value) -> u32 {
    schema
        .get("x-rmccc-schemaVersion")
        .and_then(|v| v.as_u64())
        .map(|v| v as u32)
        .unwrap_or(DEFAULT_CONFIG_SCHEMA_VERSION)
}

/// 按 schema 生成默认值（`default` 优先，对象递归）。
pub fn defaults(schema: &Value) -> Value {
    if let Some(default) = schema.get("default") {
        return default.clone();
    }
    let Some(obj) = schema.as_object() else {
        return Value::Object(Map::new());
    };
    let ty = obj.get("type").and_then(|t| t.as_str());
    if ty == Some("object") || obj.contains_key("properties") {
        let mut out = Map::new();
        if let Some(props) = obj.get("properties").and_then(|p| p.as_object()) {
            for (name, sub) in props {
                if sub.get("default").is_some() {
                    out.insert(name.clone(), defaults(sub));
                }
            }
        }
        Value::Object(out)
    } else if ty == Some("array") {
        Value::Array(Vec::new())
    } else {
        Value::Object(Map::new())
    }
}

/// 校验 `values` 是否满足 schema 子集。
pub fn validate_values(schema: &Value, values: &Value) -> Result<()> {
    validate_value(schema, values, "$")
}

fn validate_value(schema: &Value, value: &Value, path: &str) -> Result<()> {
    let Some(obj) = schema.as_object() else {
        return Ok(());
    };

    if let Some(ty) = obj.get("type") {
        let ok = match ty {
            Value::String(t) => matches_type(t, value),
            Value::Array(types) => types
                .iter()
                .filter_map(|t| t.as_str())
                .any(|t| matches_type(t, value)),
            _ => false,
        };
        if !ok {
            return Err(config_error(format!(
                "{path}: 期望类型 {ty}，实际 {}",
                json_type(value)
            )));
        }
    }

    if let Some(enum_values) = obj.get("enum").and_then(|e| e.as_array()) {
        if !enum_values.iter().any(|candidate| candidate == value) {
            return Err(config_error(format!("{path}: 取值不在 enum 中")));
        }
    }

    match value {
        Value::Object(map) => {
            let props = obj.get("properties").and_then(|p| p.as_object());
            if let Some(required) = obj.get("required").and_then(|r| r.as_array()) {
                for key in required.iter().filter_map(|k| k.as_str()) {
                    if !map.contains_key(key) {
                        return Err(config_error(format!("{path}: 缺少必填字段 {key}")));
                    }
                }
            }
            let deny_extra = obj
                .get("additionalProperties")
                .and_then(|v| v.as_bool())
                .map(|v| !v)
                .unwrap_or(false);
            for (key, sub_value) in map {
                match props.and_then(|p| p.get(key)) {
                    Some(sub_schema) => {
                        validate_value(sub_schema, sub_value, &format!("{path}.{key}"))?
                    }
                    None if deny_extra => {
                        return Err(config_error(format!("{path}: 不允许未知字段 {key}")))
                    }
                    None => {}
                }
            }
        }
        Value::Array(items) => {
            if let Some(items_schema) = obj.get("items") {
                for (i, item) in items.iter().enumerate() {
                    validate_value(items_schema, item, &format!("{path}[{i}]"))?;
                }
            }
        }
        Value::String(s) => {
            let len = s.chars().count() as u64;
            if let Some(min) = obj.get("minLength").and_then(|v| v.as_u64()) {
                if len < min {
                    return Err(config_error(format!("{path}: 长度小于 {min}")));
                }
            }
            if let Some(max) = obj.get("maxLength").and_then(|v| v.as_u64()) {
                if len > max {
                    return Err(config_error(format!("{path}: 长度大于 {max}")));
                }
            }
        }
        Value::Number(n) => {
            if let Some(min) = obj.get("minimum").and_then(|v| v.as_f64()) {
                if n.as_f64().unwrap_or(f64::MIN) < min {
                    return Err(config_error(format!("{path}: 小于最小值 {min}")));
                }
            }
            if let Some(max) = obj.get("maximum").and_then(|v| v.as_f64()) {
                if n.as_f64().unwrap_or(f64::MAX) > max {
                    return Err(config_error(format!("{path}: 大于最大值 {max}")));
                }
            }
        }
        _ => {}
    }
    Ok(())
}

fn matches_type(ty: &str, value: &Value) -> bool {
    match ty {
        "object" => value.is_object(),
        "array" => value.is_array(),
        "string" => value.is_string(),
        "integer" => value.as_f64().map(|n| n.fract() == 0.0).unwrap_or(false),
        "number" => value.is_number(),
        "boolean" => value.is_boolean(),
        "null" => value.is_null(),
        _ => false,
    }
}

fn json_type(value: &Value) -> &'static str {
    match value {
        Value::Null => "null",
        Value::Bool(_) => "boolean",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    }
}

/// 加载配置；不存在则按 schema 初始化并落盘。
///
/// 版本处理：
/// - 存储版本 > schema 版本：**只读拒绝**，不覆盖新字段；
/// - 存储版本 < schema 版本：先按新 schema 校验，通过才升级版本号；
///   失败则**保留原文件**并返回错误（不写默认值、不覆盖用户配置）。
pub fn load_or_init(
    fs: &dyn PlatformFs,
    config_path: &Path,
    schema: &Value,
) -> Result<PluginConfig> {
    let schema_version = schema_version_of(schema);
    let existing: Option<PluginConfig> = match read_json(config_path) {
        Ok(v) => v,
        Err(e) => {
            backup_file(config_path)?;
            return Err(config_error(format!("插件配置损坏，已备份到 .bak：{e}")));
        }
    };

    match existing {
        None => {
            let config = PluginConfig::new(schema);
            write_config(fs, config_path, &config)?;
            Ok(config)
        }
        Some(mut config) => {
            if config.schema_version > schema_version {
                return Err(config_error(format!(
                    "配置 schemaVersion {} 来自更新版本的插件（当前支持 {}），按只读处理",
                    config.schema_version, schema_version
                )));
            }
            validate_values(schema, &config.values)?;
            if config.schema_version < schema_version {
                config.schema_version = schema_version;
                config.updated_at = Some(now_rfc3339());
                write_config(fs, config_path, &config)?;
            }
            Ok(config)
        }
    }
}

/// 校验并写入新配置（保留 schemaVersion）。
pub fn save_values(
    fs: &dyn PlatformFs,
    config_path: &Path,
    schema: &Value,
    values: Value,
) -> Result<PluginConfig> {
    validate_values(schema, &values)?;
    let config = PluginConfig {
        schema_version: schema_version_of(schema),
        values,
        updated_at: Some(now_rfc3339()),
    };
    write_config(fs, config_path, &config)?;
    Ok(config)
}

/// 显式重置为默认值（用户动作，不是错误恢复）。
pub fn reset_to_defaults(
    fs: &dyn PlatformFs,
    config_path: &Path,
    schema: &Value,
) -> Result<PluginConfig> {
    let config = PluginConfig::new(schema);
    write_config(fs, config_path, &config)?;
    Ok(config)
}

fn write_config(fs: &dyn PlatformFs, path: &Path, config: &PluginConfig) -> Result<()> {
    let bytes = serde_json::to_vec_pretty(config)?;
    if bytes.len() as u64 > MAX_CONFIG_BYTES {
        return Err(config_error(format!(
            "插件配置过大（上限 {MAX_CONFIG_BYTES} 字节）"
        )));
    }
    write_json_atomic(fs, path, config)
}

fn config_error(message: String) -> RmcccError {
    plugin_error(PluginErrorCode::ConfigInvalid, message)
}

/// 插件配置目录（`plugin-data/<id>/`）。
pub fn plugin_config_dir(data_dir: &Path, plugin_id: &str) -> PathBuf {
    rmccc_config::paths::plugin_data_dir(data_dir).join(plugin_id)
}

/// `plugin-data/<id>/config.json`。
pub fn plugin_config_path(data_dir: &Path, plugin_id: &str) -> PathBuf {
    plugin_config_dir(data_dir, plugin_id).join("config.json")
}

#[cfg(test)]
mod tests {
    use super::*;
    use rmccc_platform::Platform;

    const SCHEMA: &str = r#"{
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "type": "object",
        "additionalProperties": false,
        "required": ["server"],
        "properties": {
            "server": { "type": "string", "minLength": 1, "maxLength": 64, "default": "play.example.net" },
            "port": { "type": "integer", "minimum": 1, "maximum": 65535, "default": 25565 },
            "showStatus": { "type": "boolean", "default": true }
        }
    }"#;

    fn schema() -> Value {
        let s: Value = serde_json::from_str(SCHEMA).unwrap();
        validate_schema_document(&s).unwrap();
        s
    }

    fn temp_dir(tag: &str) -> (std::path::PathBuf, Platform) {
        let dir = std::env::temp_dir().join(format!(
            "rmccc-plugin-config-{tag}-{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        (dir, Platform::current())
    }

    #[test]
    fn defaults_follow_schema() {
        let d = defaults(&schema());
        assert_eq!(d["server"], "play.example.net");
        assert_eq!(d["port"], 25565);
        assert_eq!(d["showStatus"], true);
    }

    #[test]
    fn validates_values() {
        let s = schema();
        validate_values(&s, &defaults(&s)).unwrap();
        for bad in [
            serde_json::json!({}),
            serde_json::json!({ "server": "" }),
            serde_json::json!({ "server": "x", "port": 0 }),
            serde_json::json!({ "server": "x", "port": 70000 }),
            serde_json::json!({ "server": "x", "extra": 1 }),
        ] {
            assert!(validate_values(&s, &bad).is_err(), "should reject {bad}");
        }
    }

    #[test]
    fn rejects_unknown_keywords() {
        let s: Value = serde_json::from_str(r#"{"type":"object","oneOf":[]}"#).unwrap();
        assert!(validate_schema_document(&s).is_err());
    }

    #[test]
    fn initializes_and_reloads_config() {
        let (dir, platform) = temp_dir("init");
        let path = dir.join("plugin-data/dev.rmccc.demo/config.json");
        let cfg = load_or_init(platform.fs.as_ref(), &path, &schema()).unwrap();
        assert_eq!(cfg.values["port"], 25565);
        let loaded = load_or_init(platform.fs.as_ref(), &path, &schema()).unwrap();
        assert_eq!(loaded, cfg);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn migrates_older_schema_when_values_still_valid() {
        let (dir, platform) = temp_dir("migrate");
        let path = dir.join("config.json");
        std::fs::write(
            &path,
            r#"{"schemaVersion":1,"values":{"server":"mc.example.net","port":25565,"showStatus":true}}"#,
        )
        .unwrap();
        let mut s = schema();
        s["x-rmccc-schemaVersion"] = serde_json::json!(2);
        let cfg = load_or_init(platform.fs.as_ref(), &path, &s).unwrap();
        assert_eq!(cfg.schema_version, 2);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn refuses_incompatible_older_config_without_overwriting() {
        let (dir, platform) = temp_dir("incompatible");
        let path = dir.join("config.json");
        std::fs::write(
            &path,
            r#"{"schemaVersion":1,"values":{"server":"","port":25565,"showStatus":true}}"#,
        )
        .unwrap();
        let mut s = schema();
        s["x-rmccc-schemaVersion"] = serde_json::json!(2);
        assert!(load_or_init(platform.fs.as_ref(), &path, &s).is_err());
        let raw = std::fs::read_to_string(&path).unwrap();
        assert!(raw.contains("\"schemaVersion\":1"), "旧配置不能被覆盖");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn refuses_newer_config() {
        let (dir, platform) = temp_dir("newer");
        let path = dir.join("config.json");
        std::fs::write(
            &path,
            r#"{"schemaVersion":9,"values":{"server":"x","port":25565,"showStatus":true}}"#,
        )
        .unwrap();
        assert!(load_or_init(platform.fs.as_ref(), &path, &schema()).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn corrupted_config_is_backed_up() {
        let (dir, platform) = temp_dir("corrupt");
        let path = dir.join("config.json");
        std::fs::write(&path, b"not json").unwrap();
        assert!(load_or_init(platform.fs.as_ref(), &path, &schema()).is_err());
        assert!(!path.exists());
        let backups: Vec<_> = std::fs::read_dir(&dir)
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_name().to_string_lossy().contains(".bak-"))
            .collect();
        assert_eq!(backups.len(), 1);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
