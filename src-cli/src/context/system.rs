//! 系统域：诊断信息、日志读取与清理、目录探测。
//!
//! `impl CliContext` 的一个分片（P2-1：把 1,450 行的 God class 按域拆开）。
//! 方法名 / 签名与拆分前完全一致，命令层无需改动。

use super::*;

impl CliContext {
    /// Overall launcher status snapshot (for `system info`).
    pub async fn system_info(&self) -> Result<Value> {
        let core = self.core.read().await;
        let active_account = core.account_repo.load_active()?;
        let (instances, local_versions, active_instance) = {
            let mut service = core
                .instances
                .lock()
                .map_err(|_| RmcccError::Other("instance service lock poisoned".to_string()))?;
            let all = service.list(None, false, true)?;
            let selected = service.selected_instance().map(str::to_string);
            (all.len(), all.len(), selected)
        };
        let running = self.running_games.len();
        let java = core.platform.java.detect_all().unwrap_or_default();

        Ok(json!({
            "data_dir": core.data_dir.display().to_string(),
            "game_root_dir": core.config.resolved_game_root().display().to_string(),
            "environment": std::env::var("RMCCC_ENV").unwrap_or_else(|_| "prod".to_string()),
            "active_account": active_account.map(|a| json!({
                "provider": a.provider,
                "username": a.username,
                "uuid": a.uuid,
                "expires_at": a.expires_at,
            })),
            "active_instance": active_instance,
            "instances": instances,
            "local_versions": local_versions,
            "running_games": running,
            "java_installations": java.len(),
            "version": env!("CARGO_PKG_VERSION"),
        }))
    }
}
