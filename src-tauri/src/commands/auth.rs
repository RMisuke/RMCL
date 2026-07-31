//! Auth IPC commands — with automatic token refresh.

use crate::state::AppState;
use rmccc_auth::{create_offline_session, AuthProvider, AuthSession};
use rmccc_error::IpcError;

/// Try to refresh an expired Microsoft session using the refresh token.
async fn try_refresh_if_needed(
    state: &AppState,
    session: &AuthSession,
) -> Result<Option<AuthSession>, IpcError> {
    // Only refresh Microsoft sessions with a refresh token
    if session.provider != "microsoft" || session.refresh_token.is_none() {
        return Ok(None);
    }

    // Check if expired
    let expired = session
        .expires_at
        .map(|e| chrono::Utc::now() >= e)
        .unwrap_or(false);

    if !expired {
        return Ok(None);
    }

    tracing::info!("Token expired, attempting refresh for {}", session.username);

    let provider = rmccc_auth::MicrosoftAuthProvider::new(state.http_client.clone());
    match provider.refresh(session).await {
        Ok(new_session) => {
            // Save refreshed session
            let data_dir = {
                let core = state.core.read().await;
                core.config.resolved_data_dir()
            };
            let auth_path = data_dir.join("auth.json");
            let vault = state.vault.lock().await;
            vault.save_session(&new_session, &auth_path)?;

            tracing::info!("Token refreshed successfully for {}", new_session.username);
            Ok(Some(new_session))
        }
        Err(e) => {
            tracing::warn!("Token refresh failed: {e}");
            Ok(None)
        }
    }
}

#[tauri::command]
#[tracing::instrument(skip(state))]
pub async fn auth_microsoft_login(
    state: tauri::State<'_, AppState>,
) -> Result<AuthSession, IpcError> {
    let provider = rmccc_auth::MicrosoftAuthProvider::new(state.http_client.clone());
    let session = provider.authenticate(None).await?;
    // Encrypt and save to disk
    let data_dir = {
        let core = state.core.read().await;
        core.config.resolved_data_dir()
    };
    let auth_path = data_dir.join("auth.json");
    let vault = state.vault.lock().await;
    vault.save_session(&session, &auth_path)?;

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
            code: rmccc_error::ErrorCode::AuthFailed,
            message: "Username cannot be empty".to_string(),
            retryable: false,
        });
    }

    let session = create_offline_session(username.trim());

    // Encrypt and save to disk
    let data_dir = {
        let core = state.core.read().await;
        core.config.resolved_data_dir()
    };
    let auth_path = data_dir.join("auth.json");
    let vault = state.vault.lock().await;
    vault.save_session(&session, &auth_path)?;

    Ok(session)
}

#[tauri::command]
#[tracing::instrument(skip(state))]
pub async fn auth_logout(
    state: tauri::State<'_, AppState>,
) -> Result<(), IpcError> {
    let data_dir = {
        let core = state.core.read().await;
        core.config.resolved_data_dir()
    };
    let auth_path = data_dir.join("auth.json");
    let vault = state.vault.lock().await;
    vault.clear_session(&auth_path)?;
    Ok(())
}

#[tauri::command]
#[tracing::instrument(skip(state))]
pub async fn auth_get_session(
    state: tauri::State<'_, AppState>,
) -> Result<Option<AuthSession>, IpcError> {
    let data_dir = {
        let core = state.core.read().await;
        core.config.resolved_data_dir()
    };
    let auth_path = data_dir.join("auth.json");
    let vault = state.vault.lock().await;
    let session = vault.load_session(&auth_path)?;

    // Auto-refresh if expired
    if let Some(ref s) = session {
        if let Some(refreshed) = try_refresh_if_needed(&state, s).await? {
            return Ok(Some(refreshed));
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
    let data_dir = {
        let core = state.core.read().await;
        core.config.resolved_data_dir()
    };
    let auth_path = data_dir.join("auth.json");
    let vault = state.vault.lock().await;
    let session = vault.load_session(&auth_path)?;

    match session {
        Some(s) => {
            if let Some(refreshed) = try_refresh_if_needed(&state, &s).await? {
                Ok(Some(refreshed))
            } else {
                Ok(Some(s))
            }
        }
        None => Ok(None),
    }
}
