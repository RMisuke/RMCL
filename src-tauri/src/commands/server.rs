//! 主界面服务器信息卡的 SLP IPC。
//!
//! 网络 I/O 是同步 TCP（见 `rmccc_core::server_status`），这里放到 blocking
//! 线程执行，避免占用 tokio 的 async worker。

use rmccc_core::server_status::{query_server_status, ServerStatus};
use rmccc_error::IpcError;

#[tauri::command]
#[tracing::instrument(fields(address = %address))]
pub async fn server_status(address: String) -> Result<ServerStatus, IpcError> {
    let result = tokio::task::spawn_blocking(move || query_server_status(&address))
        .await
        .map_err(|e| IpcError {
            code: rmccc_error::ErrorCode::InternalError,
            message: format!("服务器查询线程失败：{e}"),
            retryable: true,
        })?;
    result.map_err(Into::into)
}
