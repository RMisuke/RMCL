//! Microsoft OAuth2 device code flow for Minecraft authentication.
//!
//! Flow (aligned with JMCCC reference implementation):
//! 1. Get device code from Microsoft (scope: XboxLive.signin offline_access)
//! 2. Poll for MS token (grant_type: device_code)
//! 3. Xbox Live: get user token (3a, JWT) and XSTS token (3b, Minecraft relying party)
//! 4. Exchange XSTS for Minecraft access token
//! 5. Fetch Minecraft profile (username + UUID)

use chrono::{Duration, Utc};
use rmccc_error::{Result, RmcccError};
use serde::Deserialize;
use std::sync::{Arc, Mutex};
use std::time::Duration as StdDuration;
use uuid::Uuid;

use crate::account::Account;
use crate::account_id::AccountID;
use crate::auth_info::{user_type, AuthInfo};
use crate::factory::{AccountFactory, CreateAccountContext};
use crate::types::{AuthProgress, AuthProvider, AuthSession, ProgressCallback};

// ── Constants ──

/// RMCCC Microsoft OAuth2 client ID.
pub const CLIENT_ID: &str = "d51b460a-0b8a-4696-af4d-690f7ba7f5b6";

/// Scope for device code request — must be exactly this.
const DEVICE_CODE_SCOPE: &str = "XboxLive.signin offline_access";

const DEVICE_CODE_URL: &str = "https://login.microsoftonline.com/consumers/oauth2/v2.0/devicecode";
const TOKEN_URL: &str = "https://login.microsoftonline.com/consumers/oauth2/v2.0/token";

const XBL_AUTH_URL: &str = "https://user.auth.xboxlive.com/user/authenticate";
const XSTS_AUTH_URL: &str = "https://xsts.auth.xboxlive.com/xsts/authorize";
const MC_AUTH_URL: &str = "https://api.minecraftservices.com/authentication/login_with_xbox";
const MC_PROFILE_URL: &str = "https://api.minecraftservices.com/minecraft/profile";

/// Minecraft relying party for XSTS.
const MC_RELYING_PARTY: &str = "rp://api.minecraftservices.com/";

// ── Provider ──

/// Microsoft authentication provider.
pub struct MicrosoftAuthProvider {
    client: reqwest::Client,
    client_id: String,
}

impl MicrosoftAuthProvider {
    pub fn new(client: reqwest::Client) -> Self {
        Self {
            client,
            client_id: CLIENT_ID.to_string(),
        }
    }
}

// ── Step 1: Device code ──

impl MicrosoftAuthProvider {
    /// Request a device code from Microsoft.
    ///
    /// The `scope` parameter MUST be `XboxLive.signin offline_access`.
    /// Using other scopes (like `service::user.auth.xboxlive.com::MBI_SSL`)
    /// causes HTTP 400 because that scope is only valid at the token endpoint.
    async fn request_device_code(&self) -> Result<DeviceCodeResponse> {
        let params = [
            ("client_id", self.client_id.as_str()),
            ("scope", DEVICE_CODE_SCOPE),
        ];

        let resp = self
            .client
            .post(DEVICE_CODE_URL)
            .form(&params)
            .send()
            .await
            .map_err(|e| RmcccError::Network(e.to_string()))?;

        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            // P3-10：body 限长 + 脱敏，避免完整响应体（可能含 token/密钥）写入日志。
            let redacted = redact_body(&body, 300);
            tracing::error!(status = %status, body = %redacted, "Device code request failed");
            return Err(RmcccError::HttpStatus {
                status: status.as_u16(),
                url: DEVICE_CODE_URL.to_string(),
            });
        }

        resp.json::<DeviceCodeResponse>()
            .await
            .map_err(|e| RmcccError::Network(e.to_string()))
    }
}

// ── Step 2: Poll for MS token ──

impl MicrosoftAuthProvider {
    /// Poll until the user completes browser auth or the device code expires.
    async fn poll_for_token(
        &self,
        device_code: &str,
        interval: u64,
        expires_in: u64,
        progress: Option<&ProgressCallback>,
    ) -> Result<MsTokenResponse> {
        let deadline = Utc::now() + Duration::seconds(expires_in as i64);
        let poll_interval = StdDuration::from_secs(interval.max(5));

        let params = [
            ("grant_type", "urn:ietf:params:oauth:grant-type:device_code"),
            ("client_id", self.client_id.as_str()),
            ("device_code", device_code),
        ];

        loop {
            tokio::time::sleep(poll_interval).await;

            if Utc::now() > deadline {
                return Err(RmcccError::Timeout(expires_in));
            }

            if let Some(cb) = progress {
                cb(AuthProgress::Polling);
            }

            let resp = self
                .client
                .post(TOKEN_URL)
                .form(&params)
                .send()
                .await
                .map_err(|e| RmcccError::Network(e.to_string()))?;

            let status = resp.status();
            if status.is_success() {
                return resp
                    .json::<MsTokenResponse>()
                    .await
                    .map_err(|e| RmcccError::Network(e.to_string()));
            }

            let body: serde_json::Value = resp
                .json()
                .await
                .map_err(|e| RmcccError::Network(e.to_string()))?;

            let error = body["error"].as_str().unwrap_or("");
            match error {
                "authorization_pending" | "slow_down" => continue,
                "authorization_declined" => {
                    return Err(RmcccError::AuthFailed("User declined authorization".into()));
                }
                "expired_token" => {
                    return Err(RmcccError::TokenExpired);
                }
                other => {
                    tracing::error!(error = %other, "Unexpected token error");
                    return Err(RmcccError::AuthFailed(format!("Token error: {other}")));
                }
            }
        }
    }
}

// ── Step 3a: Xbox Live user token ──

impl MicrosoftAuthProvider {
    /// Authenticate with Xbox Live to get a user token.
    ///
    /// CRITICAL: Must include `"TokenType": "JWT"` in the request body.
    async fn authenticate_xbl(&self, ms_token: &str) -> Result<XblAuthResponse> {
        let body = serde_json::json!({
            "Properties": {
                "AuthMethod": "RPS",
                "SiteName": "user.auth.xboxlive.com",
                "RpsTicket": format!("d={ms_token}")
            },
            "RelyingParty": "http://auth.xboxlive.com",
            "TokenType": "JWT"
        });

        let resp = self
            .client
            .post(XBL_AUTH_URL)
            .json(&body)
            .send()
            .await
            .map_err(|e| RmcccError::Network(e.to_string()))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            tracing::error!(status = %status, body = %body, "XBL auth failed");
            return Err(RmcccError::AuthFailed(format!(
                "XBL auth failed: HTTP {status}"
            )));
        }

        resp.json::<XblAuthResponse>()
            .await
            .map_err(|e| RmcccError::Network(e.to_string()))
    }
}

// ── Step 3b: XSTS token for Minecraft ──

impl MicrosoftAuthProvider {
    /// Exchange XBL token for an XSTS token authorized for Minecraft.
    async fn authenticate_xsts(&self, xbl_token: &str) -> Result<XblAuthResponse> {
        let body = serde_json::json!({
            "Properties": {
                "SandboxId": "RETAIL",
                "UserTokens": [xbl_token]
            },
            "RelyingParty": MC_RELYING_PARTY,
            "TokenType": "JWT"
        });

        let resp = self
            .client
            .post(XSTS_AUTH_URL)
            .json(&body)
            .send()
            .await
            .map_err(|e| RmcccError::Network(e.to_string()))?;

        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            tracing::error!(status = %status, body = %body, "XSTS auth failed");

            if status.as_u16() == 401 {
                return Err(RmcccError::AuthFailed(
                    "XSTS authorization failed — account may not have an Xbox profile".into(),
                ));
            }
            return Err(RmcccError::AuthFailed(format!(
                "XSTS auth failed: HTTP {status}"
            )));
        }

        resp.json::<XblAuthResponse>()
            .await
            .map_err(|e| RmcccError::Network(e.to_string()))
    }
}

// ── Step 4: Minecraft access token ──

impl MicrosoftAuthProvider {
    /// Exchange XSTS token for a Minecraft access token.
    async fn authenticate_minecraft(
        &self,
        xsts_token: &str,
        user_hash: &str,
    ) -> Result<McAuthResponse> {
        let body = serde_json::json!({
            "identityToken": format!("XBL3.0 x={user_hash};{xsts_token}")
        });

        let resp = self
            .client
            .post(MC_AUTH_URL)
            .json(&body)
            .send()
            .await
            .map_err(|e| RmcccError::Network(e.to_string()))?;

        if !resp.status().is_success() {
            let status = resp.status();
            return Err(RmcccError::AuthFailed(format!(
                "Minecraft auth failed: HTTP {status}"
            )));
        }

        resp.json::<McAuthResponse>()
            .await
            .map_err(|e| RmcccError::Network(e.to_string()))
    }
}

// ── Step 5: Minecraft profile ──

impl MicrosoftAuthProvider {
    /// Fetch the Minecraft profile (username + UUID).
    async fn fetch_profile(&self, mc_token: &str) -> Result<McProfileResponse> {
        let resp = self
            .client
            .get(MC_PROFILE_URL)
            .header("Authorization", format!("Bearer {mc_token}"))
            .send()
            .await
            .map_err(|e| RmcccError::Network(e.to_string()))?;

        if !resp.status().is_success() {
            let status = resp.status();
            return Err(RmcccError::AuthFailed(format!(
                "Profile fetch failed: HTTP {status}"
            )));
        }

        resp.json::<McProfileResponse>()
            .await
            .map_err(|e| RmcccError::Network(e.to_string()))
    }
}

// ── AuthProvider trait implementation ──

#[async_trait::async_trait]
impl AuthProvider for MicrosoftAuthProvider {
    fn provider_id(&self) -> &str {
        "microsoft"
    }

    async fn authenticate(&self, progress: Option<&ProgressCallback>) -> Result<AuthSession> {
        // P3-04：device code 过期自动重新请求（限次），避免用户重新点登录。
        const MAX_ATTEMPTS: u32 = 3;
        let mut attempt = 0u32;
        loop {
            match authenticate_once(self, progress).await {
                Err(RmcccError::TokenExpired) if attempt < MAX_ATTEMPTS => {
                    attempt += 1;
                    tracing::warn!(
                        attempt = attempt,
                        max = MAX_ATTEMPTS,
                        "Device code expired, retrying with a fresh code"
                    );
                    // 短等待后重试（避免连续请求被限流）
                    tokio::time::sleep(StdDuration::from_secs(1)).await;
                }
                other => return other,
            }
        }
    }

    async fn refresh(&self, session: &AuthSession) -> Result<AuthSession> {
        let refresh_token = session
            .refresh_token
            .as_ref()
            .ok_or(RmcccError::TokenExpired)?;

        tracing::info!("Refreshing Microsoft token...");

        // Refresh token params — do NOT pass scope for refresh
        let params = [
            ("grant_type", "refresh_token"),
            ("client_id", self.client_id.as_str()),
            ("refresh_token", refresh_token.as_str()),
        ];

        let resp = self
            .client
            .post(TOKEN_URL)
            .form(&params)
            .send()
            .await
            .map_err(|e| RmcccError::Network(e.to_string()))?;

        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            tracing::error!(status = %status, body = %body, "Token refresh failed");
            return Err(RmcccError::TokenExpired);
        }

        let ms_token: MsTokenResponse = resp
            .json()
            .await
            .map_err(|e| RmcccError::Network(e.to_string()))?;

        // Re-run the Xbox → XSTS → MC token chain
        let xbl = self.authenticate_xbl(&ms_token.access_token).await?;
        let user_hash = xbl
            .display_claims
            .as_ref()
            .and_then(|dc| dc.xui.as_ref())
            .and_then(|xui| xui.first())
            .map(|x| x.uhs.clone())
            .unwrap_or_default();

        let xsts = self.authenticate_xsts(&xbl.token).await?;
        let mc_auth = self.authenticate_minecraft(&xsts.token, &user_hash).await?;
        let profile = self.fetch_profile(&mc_auth.access_token).await?;

        tracing::info!(username = %profile.name, "Token refreshed");

        Ok(AuthSession {
            // 刷新不改变账户记录 ID：稳定标识必须跨刷新保持不变。
            account_id: session.account_id.clone(),
            provider: "microsoft".into(),
            access_token: mc_auth.access_token,
            username: profile.name,
            uuid: profile.id,
            refresh_token: ms_token
                .refresh_token
                .or_else(|| session.refresh_token.clone()),
            expires_at: Some(Utc::now() + Duration::seconds(mc_auth.expires_in as i64)),
        })
    }

    async fn validate(&self, session: &AuthSession) -> bool {
        if let Some(expires) = session.expires_at {
            if Utc::now() >= expires {
                return false;
            }
        }
        let resp = self
            .client
            .get(MC_PROFILE_URL)
            .header("Authorization", format!("Bearer {}", session.access_token))
            .send()
            .await;
        resp.map(|r| r.status().is_success()).unwrap_or(false)
    }
}

/// 单次 device code 流程（P3-04 拆分，供 `authenticate` 重试循环调用）。
async fn authenticate_once(
    provider: &MicrosoftAuthProvider,
    progress: Option<&ProgressCallback>,
) -> Result<AuthSession> {
    tracing::info!("Starting Microsoft OAuth2 device code flow");

    // Step 1: Get device code
    let device = provider.request_device_code().await?;

    tracing::info!(
        user_code = %device.user_code,
        verification_uri = %device.verification_uri,
        "Device code received"
    );

    if let Some(cb) = progress {
        cb(AuthProgress::DeviceCode {
            user_code: device.user_code.clone(),
            verification_uri: device.verification_uri.clone(),
        });
    }

    // Step 2: Poll for MS token
    let ms_token = provider
        .poll_for_token(
            &device.device_code,
            device.interval,
            device.expires_in,
            progress,
        )
        .await?;

    tracing::info!("Microsoft token acquired, exchanging for Xbox Live...");

    // Step 3a: Authenticate with Xbox Live
    let xbl = provider.authenticate_xbl(&ms_token.access_token).await?;

    // Extract user hash from DisplayClaims
    let user_hash = xbl
        .display_claims
        .as_ref()
        .and_then(|dc| dc.xui.as_ref())
        .and_then(|xui| xui.first())
        .map(|x| x.uhs.clone())
        .unwrap_or_default();

    if user_hash.is_empty() {
        return Err(RmcccError::AuthFailed(
            "XBL response missing user hash (DisplayClaims.xui[0].uhs)".into(),
        ));
    }

    // Step 3b: Get XSTS token for Minecraft
    let xsts = provider.authenticate_xsts(&xbl.token).await?;

    // Step 4: Exchange for Minecraft token
    let mc_auth = provider
        .authenticate_minecraft(&xsts.token, &user_hash)
        .await?;

    // Step 5: Fetch Minecraft profile
    let profile = provider.fetch_profile(&mc_auth.access_token).await?;

    if let Some(cb) = progress {
        cb(AuthProgress::Success);
    }

    tracing::info!(
        username = %profile.name,
        uuid = %profile.id,
        "Microsoft auth complete"
    );

    Ok(AuthSession {
        // 新登录生成新的稳定账户记录 ID（Phase 1 起作为账户存储主键）。
        account_id: crate::account_id::AccountID::generate().to_string(),
        provider: "microsoft".into(),
        access_token: mc_auth.access_token,
        username: profile.name,
        uuid: profile.id,
        refresh_token: ms_token.refresh_token,
        expires_at: Some(Utc::now() + Duration::seconds(mc_auth.expires_in as i64)),
    })
}

/// P3-10：对可能含敏感字段的错误响应 body 脱敏并限长。
/// 对常见 token/密钥 JSON 键的值替换为 `[REDACTED]`，再截断到 `max_len`。
fn redact_body(body: &str, max_len: usize) -> String {
    const SENSITIVE_KEYS: &[&str] = &[
        "access_token",
        "refresh_token",
        "id_token",
        "client_secret",
        "password",
        "secret",
    ];

    let mut out = body.to_string();
    for key in SENSITIVE_KEYS {
        // 匹配 "key": "value" / "key"="value" / key: value 的字符串值
        let needles = [
            format!("\"{key}\": \""),
            format!("\"{key}\" : \""),
            format!("\"{key}\"=\""),
        ];
        for needle in &needles {
            let mut rest = out.as_str();
            let mut built = String::with_capacity(out.len());
            while let Some(pos) = rest.find(needle.as_str()) {
                built.push_str(&rest[..pos + needle.len()]);
                rest = &rest[pos + needle.len()..];
                // 跳过值直到下一个未转义引号
                if let Some(end) = rest.find('"') {
                    built.push_str("[REDACTED]");
                    rest = &rest[end..];
                } else {
                    break;
                }
            }
            built.push_str(rest);
            out = built;
        }
    }

    if out.chars().count() > max_len {
        let truncated: String = out.chars().take(max_len).collect();
        format!("{truncated}…[truncated]")
    } else {
        out
    }
}

// ── Response types ──

#[derive(Debug, Deserialize)]
struct DeviceCodeResponse {
    user_code: String,
    device_code: String,
    verification_uri: String,
    expires_in: u64,
    interval: u64,
}

#[derive(Debug, Deserialize)]
struct MsTokenResponse {
    access_token: String,
    #[serde(default)]
    refresh_token: Option<String>,
    #[allow(dead_code)]
    expires_in: u64,
}

/// Xbox Live / XSTS authenticate response.
/// Both XBL and XSTS endpoints return the same structure.
#[derive(Debug, Deserialize)]
struct XblAuthResponse {
    #[serde(rename = "Token")]
    token: String,
    #[serde(rename = "DisplayClaims")]
    display_claims: Option<DisplayClaims>,
}

#[derive(Debug, Deserialize)]
struct DisplayClaims {
    xui: Option<Vec<XuiClaim>>,
}

#[derive(Debug, Deserialize)]
struct XuiClaim {
    uhs: String,
}

#[derive(Debug, Deserialize)]
struct McAuthResponse {
    access_token: String,
    expires_in: u64,
}

#[derive(Debug, Deserialize)]
struct McProfileResponse {
    id: String,
    name: String,
}

// ── Account lifecycle (P2) ──

/// Advance-refresh window: refresh the token this many seconds before it
/// actually expires so a launch never races against a just-expired token.
/// Mirrors HMCL's proactive refresh behaviour.
pub const REFRESH_WINDOW_SECS: i64 = 600;

/// Microsoft account — mirrors HMCL's `MicrosoftAccount` (an `OAuthAccount`).
///
/// Owns its credentials and refreshes them silently on [`Account::log_in`],
/// verifying the refreshed profile still matches the stored one. When the
/// refresh token is permanently invalid it returns
/// [`RmcccError::CredentialExpired`] so callers can route the user through
/// [`Account::log_in_when_credentials_expired`].
pub struct MicrosoftAccount {
    account_id: AccountID,
    provider: Arc<dyn AuthProvider>,
    session: Mutex<AuthSession>,
}

impl MicrosoftAccount {
    /// Creates an account from a freshly authenticated or restored session.
    pub fn new(provider: Arc<dyn AuthProvider>, session: AuthSession) -> Self {
        let account_id =
            AccountID::parse(&session.account_id).unwrap_or_else(|_| AccountID::generate());
        Self {
            account_id,
            provider,
            session: Mutex::new(session),
        }
    }

    /// Builds an [`AuthInfo`] from the current session (Microsoft → `msa`).
    fn auth_info_from(&self, session: &AuthSession) -> AuthInfo {
        AuthInfo::from_session(session)
    }
}

#[async_trait::async_trait]
impl Account for MicrosoftAccount {
    fn account_id(&self) -> &AccountID {
        &self.account_id
    }

    fn provider(&self) -> &str {
        "microsoft"
    }

    fn profile_name(&self) -> String {
        let session = self.session.lock().unwrap();
        session.username.clone()
    }

    fn profile_id(&self) -> String {
        let session = self.session.lock().unwrap();
        session.uuid.clone()
    }

    async fn log_in(&self) -> Result<AuthInfo> {
        let session = self.session.lock().unwrap().clone();

        // Advance-refresh window: if the token is still far from expiring, use
        // it as-is without any network round-trip.
        if let Some(expires) = session.expires_at {
            if Utc::now() + Duration::seconds(REFRESH_WINDOW_SECS) < expires {
                tracing::debug!("Token still valid, skipping refresh");
                return Ok(self.auth_info_from(&session));
            }
        }

        tracing::info!(
            username = %session.username,
            "Microsoft token near/after expiry, refreshing"
        );
        let refreshed = match self.provider.refresh(&session).await {
            Ok(s) => s,
            // 刷新失败（refresh token 失效）→ 凭证彻底失效，需重新授权。
            Err(RmcccError::TokenExpired) => return Err(RmcccError::CredentialExpired),
            Err(e) => return Err(e),
        };

        // 刷新后校验 profile uuid 一致（对齐 OAuthAccount.WrongAccountException）。
        if !refreshed.uuid.eq_ignore_ascii_case(&session.uuid) {
            tracing::error!(
                expected = %session.uuid,
                actual = %refreshed.uuid,
                "Token refresh silently switched to a different profile"
            );
            return Err(RmcccError::WrongAccount {
                expected: session.uuid,
                actual: refreshed.uuid,
            });
        }

        *self.session.lock().unwrap() = refreshed.clone();
        Ok(self.auth_info_from(&refreshed))
    }

    async fn log_in_when_credentials_expired(
        &self,
        progress: Option<&ProgressCallback>,
    ) -> Result<AuthInfo> {
        // 完整重新授权（device code），更新为新的会话；账户记录 ID 保持不变。
        let mut new_session = self.provider.authenticate(progress).await?;
        new_session.account_id = self.account_id.to_string();
        *self.session.lock().unwrap() = new_session.clone();
        tracing::info!(
            username = %new_session.username,
            "Re-authorized Microsoft account"
        );
        Ok(self.auth_info_from(&new_session))
    }

    fn play_offline(&self) -> AuthInfo {
        let session = self.session.lock().unwrap().clone();
        AuthInfo::new(
            session.username,
            session.uuid,
            Uuid::new_v4().to_string(),
            user_type::MSA,
            "{}",
        )
    }

    fn write_metadata(&self) -> serde_json::Value {
        let session = self.session.lock().unwrap().clone();
        serde_json::json!({
            "type": "microsoft",
            "account_id": self.account_id.to_string(),
            "profile_name": &session.username,
            "profile_id": &session.uuid,
        })
    }

    fn write_private_data(&self) -> serde_json::Value {
        let session = self.session.lock().unwrap().clone();
        serde_json::json!({
            "access_token": &session.access_token,
            "refresh_token": &session.refresh_token,
            "expires_at": &session.expires_at,
        })
    }

    fn to_session(&self) -> AuthSession {
        self.session.lock().unwrap().clone()
    }
}

/// Microsoft account factory — mirrors HMCL's `MicrosoftAccountFactory`.
pub struct MicrosoftAccountFactory {
    provider: Arc<dyn AuthProvider>,
}

impl MicrosoftAccountFactory {
    /// Creates a factory backed by the shared Microsoft auth provider.
    pub fn new(provider: Arc<dyn AuthProvider>) -> Self {
        Self { provider }
    }
}

#[async_trait::async_trait]
impl AccountFactory for MicrosoftAccountFactory {
    fn login_type(&self) -> &'static str {
        "microsoft"
    }

    async fn create(&self, ctx: CreateAccountContext) -> Result<Box<dyn Account>> {
        let session = self.provider.authenticate(ctx.progress.as_ref()).await?;
        Ok(Box::new(MicrosoftAccount::new(
            self.provider.clone(),
            session,
        )))
    }

    fn from_storage(
        &self,
        metadata: &serde_json::Value,
        private: Option<&serde_json::Value>,
    ) -> Result<Box<dyn Account>> {
        let account_id = metadata
            .get("account_id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| RmcccError::Other("microsoft metadata missing account_id".into()))?;
        let account_id = AccountID::parse(account_id)
            .map_err(|e| RmcccError::Other(format!("invalid microsoft account_id: {e:?}")))?;
        let username = metadata
            .get("profile_name")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let uuid = metadata
            .get("profile_id")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        let private = private
            .ok_or_else(|| RmcccError::Other("microsoft account private data missing".into()))?;
        let access_token = private
            .get("access_token")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let refresh_token = private
            .get("refresh_token")
            .and_then(|v| v.as_str())
            .map(String::from);
        let expires_at = private
            .get("expires_at")
            .and_then(|v| v.as_str())
            .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
            .map(|dt| dt.with_timezone(&Utc));

        let session = AuthSession {
            account_id: account_id.to_string(),
            provider: "microsoft".into(),
            access_token,
            username,
            uuid,
            refresh_token,
            expires_at,
        };
        Ok(Box::new(MicrosoftAccount::new(
            self.provider.clone(),
            session,
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::AuthProgress;
    use chrono::DateTime;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{Arc, Mutex};

    /// 验证 `ProgressCallback` 协议：`DeviceCode` 变体能被回调触发（P0-01）。
    ///
    /// MS 端点为硬编码 URL，无法在无网络单测中走通完整 `authenticate` 流程，
    /// 故此处直接验证回调协议可用；真实触发依赖人工 GUI/CLI 验证。
    #[test]
    fn test_device_code_progress_callback_contract() {
        let captured: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));
        let captured_clone = captured.clone();

        let cb: ProgressCallback = Box::new(move |p| {
            if let AuthProgress::DeviceCode { user_code, .. } = p {
                *captured_clone.lock().unwrap() = Some(user_code);
            }
        });

        cb(AuthProgress::DeviceCode {
            user_code: "ABCDEF".into(),
            verification_uri: "https://microsoft.com/devicelogin".into(),
        });

        assert_eq!(captured.lock().unwrap().as_deref(), Some("ABCDEF"));
    }

    // ── P2: token lifecycle ──

    /// A mock `AuthProvider` that returns a scripted refresh/authenticate
    /// result, letting us simulate token expiry, successful refresh, wrong
    /// account after refresh, and refresh failure without any network access.
    struct MockAuthProvider {
        /// `Some(session)` => refresh succeeds; `None` => refresh fails with
        /// `TokenExpired`.
        refresh_session: Mutex<Option<AuthSession>>,
        authenticate_session: Mutex<AuthSession>,
        refresh_calls: AtomicUsize,
        authenticate_calls: AtomicUsize,
    }

    impl MockAuthProvider {
        fn new(refresh_session: Option<AuthSession>, authenticate_session: AuthSession) -> Self {
            Self {
                refresh_session: Mutex::new(refresh_session),
                authenticate_session: Mutex::new(authenticate_session),
                refresh_calls: AtomicUsize::new(0),
                authenticate_calls: AtomicUsize::new(0),
            }
        }

        fn refresh_calls(&self) -> usize {
            self.refresh_calls.load(Ordering::SeqCst)
        }
    }

    #[async_trait::async_trait]
    impl AuthProvider for MockAuthProvider {
        fn provider_id(&self) -> &str {
            "microsoft"
        }

        async fn authenticate(&self, _progress: Option<&ProgressCallback>) -> Result<AuthSession> {
            self.authenticate_calls.fetch_add(1, Ordering::SeqCst);
            Ok(self.authenticate_session.lock().unwrap().clone())
        }

        async fn refresh(&self, _session: &AuthSession) -> Result<AuthSession> {
            self.refresh_calls.fetch_add(1, Ordering::SeqCst);
            match self.refresh_session.lock().unwrap().as_ref() {
                Some(s) => Ok(s.clone()),
                None => Err(RmcccError::TokenExpired),
            }
        }

        async fn validate(&self, _session: &AuthSession) -> bool {
            true
        }
    }

    fn ms_session(
        account_id: &str,
        username: &str,
        uuid: &str,
        expires_at: Option<DateTime<Utc>>,
        token: &str,
    ) -> AuthSession {
        AuthSession {
            account_id: account_id.into(),
            provider: "microsoft".into(),
            access_token: token.into(),
            username: username.into(),
            uuid: uuid.into(),
            refresh_token: Some("rt".into()),
            expires_at,
        }
    }

    #[test]
    fn test_ms_log_in_skips_refresh_when_fresh() {
        let runtime = tokio::runtime::Runtime::new().unwrap();
        let id = AccountID::generate();
        let provider = Arc::new(MockAuthProvider::new(
            Some(ms_session(
                &id.to_string(),
                "Other",
                "uuid-other",
                Some(Utc::now() + Duration::hours(1)),
                "new-tok",
            )),
            ms_session(&id.to_string(), "x", "x", None, "x"),
        ));
        let account = MicrosoftAccount::new(
            provider.clone(),
            ms_session(
                &id.to_string(),
                "Steve",
                "uuid-abc",
                Some(Utc::now() + Duration::hours(2)),
                "tok",
            ),
        );
        let info = runtime.block_on(account.log_in()).unwrap();
        // 未过提前窗口 → 不触发网络刷新。
        assert_eq!(info.username, "Steve");
        assert_eq!(provider.refresh_calls(), 0);
    }

    #[test]
    fn test_ms_log_in_refreshes_when_expired_and_updates_session() {
        let runtime = tokio::runtime::Runtime::new().unwrap();
        let id = AccountID::generate();
        let provider = Arc::new(MockAuthProvider::new(
            Some(ms_session(
                &id.to_string(),
                "Steve",
                "uuid-abc",
                Some(Utc::now() + Duration::hours(1)),
                "new-tok",
            )),
            ms_session(&id.to_string(), "x", "x", None, "x"),
        ));
        let account = MicrosoftAccount::new(
            provider.clone(),
            ms_session(
                &id.to_string(),
                "Steve",
                "uuid-abc",
                Some(Utc::now() - Duration::minutes(5)),
                "old-tok",
            ),
        );
        let info = runtime.block_on(account.log_in()).unwrap();
        assert_eq!(info.username, "Steve");
        assert_eq!(provider.refresh_calls(), 1);
        // 内部 session 已更新为新 token。
        assert_eq!(account.to_session().access_token, "new-tok");
    }

    #[test]
    fn test_ms_log_in_wrong_account_after_refresh() {
        let runtime = tokio::runtime::Runtime::new().unwrap();
        let id = AccountID::generate();
        let provider = Arc::new(MockAuthProvider::new(
            Some(ms_session(
                &id.to_string(),
                "Other",
                "uuid-other",
                Some(Utc::now() + Duration::hours(1)),
                "new-tok",
            )),
            ms_session(&id.to_string(), "x", "x", None, "x"),
        ));
        let account = MicrosoftAccount::new(
            provider.clone(),
            ms_session(
                &id.to_string(),
                "Steve",
                "uuid-abc",
                Some(Utc::now() - Duration::minutes(5)),
                "old-tok",
            ),
        );
        let err = runtime.block_on(account.log_in()).unwrap_err();
        match err {
            RmcccError::WrongAccount { expected, actual } => {
                assert_eq!(expected, "uuid-abc");
                assert_eq!(actual, "uuid-other");
            }
            other => panic!("expected WrongAccount, got {other:?}"),
        }
    }

    #[test]
    fn test_ms_log_in_credential_expired_when_refresh_fails() {
        let runtime = tokio::runtime::Runtime::new().unwrap();
        let id = AccountID::generate();
        let provider = Arc::new(MockAuthProvider::new(
            None,
            ms_session(&id.to_string(), "x", "x", None, "x"),
        ));
        let account = MicrosoftAccount::new(
            provider.clone(),
            ms_session(
                &id.to_string(),
                "Steve",
                "uuid-abc",
                Some(Utc::now() - Duration::minutes(5)),
                "old-tok",
            ),
        );
        let err = runtime.block_on(account.log_in()).unwrap_err();
        assert!(
            matches!(err, RmcccError::CredentialExpired),
            "expected CredentialExpired, got {err:?}"
        );
    }

    #[test]
    fn test_ms_log_in_when_credentials_expired_keeps_account_id() {
        let runtime = tokio::runtime::Runtime::new().unwrap();
        let id = AccountID::generate();
        let provider = Arc::new(MockAuthProvider::new(
            None,
            ms_session(
                &id.to_string(),
                "Reborn",
                "uuid-reborn",
                Some(Utc::now() + Duration::hours(1)),
                "re-tok",
            ),
        ));
        let account = MicrosoftAccount::new(
            provider.clone(),
            ms_session(
                &id.to_string(),
                "Steve",
                "uuid-abc",
                Some(Utc::now() - Duration::minutes(5)),
                "old-tok",
            ),
        );
        let info = runtime
            .block_on(account.log_in_when_credentials_expired(None))
            .unwrap();
        assert_eq!(info.username, "Reborn");
        // 重新授权不改变账户记录 ID。
        assert_eq!(account.account_id(), &id);
        assert_eq!(account.to_session().account_id, id.to_string());
    }

    #[test]
    fn test_ms_play_offline_uses_msa_random_token() {
        let id = AccountID::generate();
        let provider = Arc::new(MockAuthProvider::new(
            None,
            ms_session(&id.to_string(), "x", "x", None, "x"),
        ));
        let account = MicrosoftAccount::new(
            provider.clone(),
            ms_session(&id.to_string(), "Steve", "uuid-abc", None, "tok"),
        );
        let info = account.play_offline();
        assert_eq!(info.user_type, user_type::MSA);
        assert_ne!(info.access_token, "tok", "offline token must be random");
    }

    #[test]
    fn test_ms_account_factory_roundtrip() {
        let id = AccountID::generate();
        let provider = Arc::new(MockAuthProvider::new(
            None,
            ms_session(&id.to_string(), "x", "x", None, "x"),
        ));
        let factory = MicrosoftAccountFactory::new(provider.clone());
        let account = MicrosoftAccount::new(
            provider.clone(),
            ms_session(
                &id.to_string(),
                "Steve",
                "uuid-abc",
                Some(Utc::now() + Duration::hours(1)),
                "tok",
            ),
        );
        let meta = account.write_metadata();
        let private = account.write_private_data();
        let restored = factory
            .from_storage(&meta, Some(&private))
            .expect("from_storage must restore a microsoft account");
        assert_eq!(restored.account_id(), &id);
        assert_eq!(restored.profile_name(), "Steve");
        assert_eq!(restored.profile_id(), "uuid-abc");
        assert_eq!(restored.to_session().access_token, "tok");
        assert_eq!(restored.to_session().refresh_token.as_deref(), Some("rt"));
    }
}
