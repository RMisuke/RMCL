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
use serde::Serialize;
use tauri::Emitter;
use ts_rs::TS;

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
///
/// A1：**绝不嵌套读锁**。调用方已释放自己的读守卫后才进入这里；本函数
/// 自己的读锁只覆盖一次 Arc 克隆（微秒级），网络刷新全程不持 core 锁——
/// 此前外层读锁 + 排队写者 + 内层 `read()` 在 tokio 公平 RwLock 下会死锁。
async fn try_refresh_if_needed(
    state: &AppState,
    session: &AuthSession,
) -> Result<AuthSession, IpcError> {
    let account_repo = {
        let core = state.core.read().await;
        core.account_repo.clone()
    };
    Ok(rmccc_core::refresh_session(&account_repo, session).await?)
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
    // A2：设备码流要等用户在浏览器完成授权（分钟级）——短锁克隆 Arc 后
    // 立即释放读锁，登录全程不持 core 锁。
    let account_repo = {
        let core = state.core.read().await;
        core.account_repo.clone()
    };
    let session = {
        let account = account_repo
            .factory("microsoft")?
            .create(CreateAccountContext {
                additional_data: None,
                progress: Some(progress),
            })
            .await?;
        account_repo.save_account(account.as_ref())?;
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
    // A2 同款：不持 core 读锁跨 await。
    let account_repo = {
        let core = state.core.read().await;
        core.account_repo.clone()
    };
    let session = {
        let account = account_repo
            .factory("offline")?
            .create(CreateAccountContext {
                additional_data: Some(serde_json::json!({ "username": username.trim() })),
                progress: None,
            })
            .await?;
        account_repo.save_account(account.as_ref())?;
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

/// Fetch the profile's skin texture URL from Mojang's session server.
///
/// WKWebView 的 fetch 会被 CORS 拦截，crafatar 等镜像又不可靠（HTTP 500 → 默认
/// Alex），所以由后端代理解码：sessionserver → textures 属性 → base64 → SKIN.url。
/// 无皮肤/网络失败返回 None，前端回退首字母或 Steve。
#[tauri::command]
#[tracing::instrument]
pub async fn auth_skin_url(uuid: String) -> Result<Option<SkinTextures>, IpcError> {
    let undashed = uuid.replace('-', "").to_lowercase();
    if undashed.len() != 32 || !undashed.chars().all(|c| c.is_ascii_hexdigit()) {
        return Ok(None);
    }
    let client = reqwest::Client::new();
    let resp = client
        .get(format!(
            "https://sessionserver.mojang.com/session/minecraft/profile/{undashed}"
        ))
        .timeout(std::time::Duration::from_secs(8))
        .send()
        .await;
    let resp = match resp {
        Ok(r) if r.status().is_success() => r,
        Ok(r) => {
            tracing::warn!(status = %r.status(), "sessionserver skin lookup failed");
            return Ok(None);
        }
        Err(e) => {
            tracing::warn!(error = %e, "sessionserver skin lookup unreachable");
            return Ok(None);
        }
    };
    let body: serde_json::Value = match resp.json().await {
        Ok(v) => v,
        Err(_) => return Ok(None),
    };
    let textures = body["properties"]
        .as_array()
        .and_then(|props| {
            props
                .iter()
                .find(|p| p["name"] == "textures")
                .and_then(|p| p["value"].as_str())
        })
        .and_then(|raw| {
            use base64::Engine as _;
            base64::engine::general_purpose::STANDARD
                .decode(raw)
                .ok()
                .and_then(|bytes| serde_json::from_slice::<serde_json::Value>(&bytes).ok())
        });
    let pick = |key: &str| {
        textures
            .as_ref()
            .and_then(|t| t["textures"][key]["url"].as_str())
            .map(|u| u.replacen("http://", "https://", 1))
    };
    let skin = pick("SKIN");
    let cape = pick("CAPE");
    if skin.is_none() && cape.is_none() {
        return Ok(None);
    }
    Ok(Some(SkinTextures { skin, cape }))
}

/// sessionserver 纹理信息：皮肤 + 披风（3D 预览共用一次请求）。
#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export_to = "types.ts")]
pub struct SkinTextures {
    pub skin: Option<String>,
    pub cape: Option<String>,
}

#[tauri::command]
#[tracing::instrument(skip(state))]
pub async fn auth_get_session(
    state: tauri::State<'_, AppState>,
) -> Result<Option<AuthSession>, IpcError> {
    let session = {
        let core = state.core.read().await;
        core.account_repo.load_active()?
    };

    // 接近/已过期才静默刷新；凭证失效（CredentialExpired）等错误原样传播，
    // 前端据此引导用户重新登录。守卫已释放——try_refresh 自己取短锁（A1）。
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
    let session = {
        let core = state.core.read().await;
        core.account_repo.load_active()?
    };

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
