//! 配置与状态域：启动器设置、运行时状态、用户偏好、配置导入导出。
//!
//! `impl CliContext` 的一个分片（P2-1：把 1,450 行的 God class 按域拆开）。
//! 方法名 / 签名与拆分前完全一致，命令层无需改动。

use super::*;

impl CliContext {
    /// Read the launcher settings; `key` restricts to one field.
    pub async fn config_get(&self, key: Option<&str>) -> Result<Value> {
        let core = self.core.read().await;
        let v = serde_json::to_value(&core.config)?;
        match key {
            Some(k) => Ok(v.get(k).cloned().unwrap_or(Value::Null)),
            None => Ok(v),
        }
    }

    /// Structured settings update (whitelist + type/range validation).
    /// Returns the updated config as JSON.
    pub async fn config_set(&self, field: &str, value: &str) -> Result<Value> {
        let mut core = self.core.write().await;
        match field {
            "data_dir" => core.config.data_dir = Some(PathBuf::from(value)),
            "game_root_dir" => core.config.game_root_dir = Some(PathBuf::from(value)),
            "default_java_path" => core.config.default_java_path = parse_optional_str(value)?,
            "default_min_memory_mb" => {
                core.config.default_min_memory_mb = parse_bounded_u32(value, 128, 65536)?
            }
            "default_max_memory_mb" => {
                core.config.default_max_memory_mb = parse_bounded_u32(value, 256, 65536)?
            }
            "download_concurrency" => {
                core.config.download_concurrency = parse_bounded_u32(value, 1, 64)?
            }
            "mirror_url" => core.config.mirror_url = parse_optional_str(value)?,
            "event_throttle_ms" => core.config.event_throttle_ms = parse_u64_field(value)?,
            other => {
                return Err(RmcccError::Other(format!(
                    "Unknown settings field '{other}' (see `rmcli config set --help`)"
                )))
            }
        }
        core.config.save(&core.data_dir)?;
        Ok(serde_json::to_value(&core.config)?)
    }

    /// Resolved data dir + game root (for `config path`).
    pub async fn config_paths(&self) -> Result<Value> {
        let core = self.core.read().await;
        Ok(json!({
            "data_dir": core.data_dir.display().to_string(),
            "resolved_data_dir": core.config.resolved_data_dir().display().to_string(),
            "game_root_dir": core.config.resolved_game_root().display().to_string(),
            "default_java_path": core.config.default_java_path,
        }))
    }

    pub async fn state_get(&self, key: Option<&str>) -> Result<Value> {
        let core = self.core.read().await;
        let mut v = serde_json::to_value(&core.launcher_state)?;
        let selected_key = {
            let service = core
                .instances
                .lock()
                .map_err(|_| RmcccError::Other("instance service lock poisoned".to_string()))?;
            service.selected_instance().map(str::to_string)
        };
        if let Some(object) = v.as_object_mut() {
            object.insert("selected_key".to_string(), json!(selected_key));
        }
        match key {
            Some(k) => Ok(v.get(k).cloned().unwrap_or(Value::Null)),
            None => Ok(v),
        }
    }

    pub async fn state_set(&self, field: &str, value: &str) -> Result<Value> {
        let mut core = self.core.write().await;
        match field {
            "selected_key" | "selected_instance" => {
                let key = parse_optional_str(value)?;
                {
                    let mut service = core.instances.lock().map_err(|_| {
                        RmcccError::Other("instance service lock poisoned".to_string())
                    })?;
                    service.select_instance(key.as_deref())?;
                }
                core.launcher_state.selected_profile_id = key;
            }
            "selected_profile_id" => {
                core.launcher_state.selected_profile_id = parse_optional_str(value)?
            }
            other => {
                return Err(RmcccError::Other(format!(
                    "Unknown state field '{other}' (only 'selected_profile_id')"
                )))
            }
        }
        core.launcher_state.save(&core.data_dir)?;
        Ok(serde_json::to_value(&core.launcher_state)?)
    }

    pub async fn user_get(&self, key: Option<&str>) -> Result<Value> {
        let settings = rmccc_config::UserSettings::load(&self.data_dir)?;
        let v = serde_json::to_value(&settings)?;
        match key {
            Some(k) => Ok(v.get(k).cloned().unwrap_or(Value::Null)),
            None => Ok(v),
        }
    }

    pub async fn user_set(&self, field: &str, value: &str) -> Result<Value> {
        let mut settings = rmccc_config::UserSettings::load(&self.data_dir)?;
        match field {
            "locale" => settings.locale = parse_optional_str(value)?,
            "check_updates" => settings.check_updates = parse_bool(value)?,
            other => {
                return Err(RmcccError::Other(format!(
                    "Unknown user-settings field '{other}' (locale | check_updates)"
                )))
            }
        }
        settings.save(&self.data_dir)?;
        Ok(serde_json::to_value(&settings)?)
    }
}
