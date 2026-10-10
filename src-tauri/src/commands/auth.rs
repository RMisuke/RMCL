//! Auth IPC commands — backed by `CoreContext` account repository.
//!
//! Phase 1: all credential storage goes through `AccountRepository`
//! (plaintext `accounts.json` metadata + encrypted private data). IPC handlers
//! never touch encryption or storage paths directly.
//! Phase 2: the token lifecycle (advance-refresh window, profile UUID
//! consistency, `CredentialExpired` re-authorization) lives inside the
//! `Account` implementations; login goes through `AccountFactory::create`.

use crate::state::AppState;
use chrono::{Duration, Utc};
use rmccc_auth::{AccountMetadata, AuthSession, CreateAccountContext, REFRESH_WINDOW_SECS};
use rmccc_error::IpcError;
use tauri::Emitter;

/// Whether the session is at/after the advance-refresh window (or already
/// expired), i.e. worth a network refresh before use. Avoids re-writing the
/// store on every poll when the token is still valid.
fn needs_refresh(session: &AuthSession) -> bool {
    match session.expires_at {
        Some(expires) => Utc::now() + Duration::seconds(REFRESH_WINDOW_SECS) >= expires,
        None => false,
    }
}

/// Try to refresh an expired Microsoft session using the repository.
///
/// P2：token 生命周期由 `Account` 内部承担；刷新失败（`CredentialExpired` /
/// `WrongAccount` / 网络错误）不再吞掉，原样传播以便前端引导用户重新登录。
async fn try_refresh_if_needed(
    state: &AppState,
    session: &AuthSession,
) -> Result<AuthSession, IpcError> {
    let core = state.core.read().await;
    Ok(core.refresh_session_if_needed(session).await?)
}

#[tauri::command]
#[tracing::instrument(skip(state), fields(request_id = %uuid::Uuid::new_v4()))]
pub async fn auth_microsoft_login(
    state: tauri::State<'_, AppState>,
) -> Result<AuthSession, IpcError> {
    // 注入 progress 回调：设备码到达后经 `AUTH_DEVICE_CODE` 事件推送给前端。
    let app_handle = state.app_handle.clone();
    let progress: rmccc_auth::ProgressCallback = Box::new(move |p| {
        if let rmccc_auth::AuthProgress::DeviceCode {
            user_code,
            verification_uri,
        } = p
        {
            let _ = app_handle.emit(
                crate::events::AUTH_DEVICE_CODE,
                serde_json::json!({
                    "user_code": user_code,
                    "verification_uri": verification_uri,
                }),
            );

            // 集成 tauri-plugin-opener：自动在系统默认浏览器打开授权页。
            if let Err(e) = tauri_plugin_opener::open_url(&verification_uri, None::<&str>) {
                tracing::warn!(
                    uri = %verification_uri,
                    error = ?e,
                    "Failed to auto-open verification URI; user must open it manually"
                );
            } else {
                tracing::info!(user_code = %user_code, "Auto-opened verification URI in browser");
            }
        }
    });

    // P2：经 MicrosoftAccountFactory.create 登录（统一走 Account 抽象）。
    let session = {
        let core = state.core.read().await;
        let account = core
            .account_repo
            .factory("microsoft")?
            .create(CreateAccountContext {
                additional_data: None,
                progress: Some(progress),
            })
            .await?;
        core.account_repo.save_account(account.as_ref())?;
        account.to_session()
    };

    Ok(session)
}

#[tauri::command]
#[tracing::instrument(skip(state))]
pub async fn auth_offline_login(
    state: tauri::State<'_, AppState>,
    username: String,
) -> Result<AuthSession, IpcError> {
    if username.trim().is_empty() {
        return Err(IpcError {
            code: rmccc_error::ErrorCode::ValidationError,
            message: "Username cannot be empty".to_string(),
            retryable: false,
        });
    }
    if username.len() > 32 {
        return Err(IpcError {
            code: rmccc_error::ErrorCode::ValidationError,
            message: format!(
                "Username too long (max 32 characters, got {})",
                username.len()
            ),
            retryable: false,
        });
    }

    // P2：经 OfflineAccountFactory.create 登录（离线 UUID 对齐 HMCL）。
    let session = {
        let core = state.core.read().await;
        let account = core
            .account_repo
            .factory("offline")?
            .create(CreateAccountContext {
                additional_data: Some(serde_json::json!({ "username": username.trim() })),
                progress: None,
            })
            .await?;
        core.account_repo.save_account(account.as_ref())?;
        account.to_session()
    };

    Ok(session)
}

#[tauri::command]
#[tracing::instrument(skip(state))]
pub async fn auth_logout(state: tauri::State<'_, AppState>) -> Result<(), IpcError> {
    let core = state.core.read().await;
    if let Some(active) = core.account_repo.load_active()? {
        core.account_repo.remove(&active.account_id)?;
    }
    Ok(())
}

#[tauri::command]
#[tracing::instrument(skip(state))]
pub async fn auth_get_session(
    state: tauri::State<'_, AppState>,
) -> Result<Option<AuthSession>, IpcError> {
    let core = state.core.read().await;
    let session = core.account_repo.load_active()?;

    // 接近/已过期才静默刷新；凭证失效（CredentialExpired）等错误原样传播，
    // 前端据此引导用户重新登录。
    if let Some(ref s) = session {
        if needs_refresh(s) {
            return Ok(Some(try_refresh_if_needed(&state, s).await?));
        }
    }

    Ok(session)
}

/// Explicitly refresh the token (called from frontend periodically or on demand).
#[tauri::command]
#[tracing::instrument(skip(state))]
pub async fn auth_refresh_token(
    state: tauri::State<'_, AppState>,
) -> Result<Option<AuthSession>, IpcError> {
    let core = state.core.read().await;
    let session = core.account_repo.load_active()?;

    match session {
        Some(s) => {
            if needs_refresh(&s) {
                let refreshed = try_refresh_if_needed(&state, &s).await?;
                Ok(Some(refreshed))
            } else {
                Ok(Some(s))
            }
        }
        None => Ok(None),
    }
}

/// 列出所有已保存账号的公开元数据（明文，不含 token）。
#[tauri::command]
#[tracing::instrument(skip(state))]
pub async fn auth_list_accounts(
    state: tauri::State<'_, AppState>,
) -> Result<Vec<AccountMetadata>, IpcError> {
    let core = state.core.read().await;
    Ok(core.account_repo.list_metadata()?)
}

/// 切换 active 账号（按稳定 AccountID）。
#[tauri::command]
#[tracing::instrument(skip(state))]
pub async fn auth_switch_account(
    state: tauri::State<'_, AppState>,
    account_id: String,
) -> Result<AuthSession, IpcError> {
    let core = state.core.read().await;
    core.account_repo.select(&account_id)?;
    core.account_repo
        .load(&account_id)?
        .ok_or_else(|| IpcError {
            code: rmccc_error::ErrorCode::Unknown,
            message: format!("Account not found after switch: {account_id}"),
            retryable: false,
        })
}

/// 移除指定账号（按稳定 AccountID）。
#[tauri::command]
#[tracing::instrument(skip(state))]
pub async fn auth_remove_account(
    state: tauri::State<'_, AppState>,
    account_id: String,
) -> Result<(), IpcError> {
    let core = state.core.read().await;
    core.account_repo.remove(&account_id)?;
    Ok(())
}
