//! Microsoft OAuth2 device code flow for Minecraft authentication.
//!
//! Flow (aligned with JMCCC reference implementation):
//! 1. Get device code from Microsoft (scope: XboxLive.signin offline_access)
//! 2. Poll for MS token (grant_type: device_code)
//! 3a. Get Xbox Live user token (TokenType: JWT)
//! 3b. Get XSTS token for Minecraft relying party
//! 4. Exchange XSTS for Minecraft access token
//! 5. Fetch Minecraft profile (username + UUID)

use chrono::{Duration, Utc};
use rmccc_error::{RmcccError, Result};
use serde::Deserialize;
use std::time::Duration as StdDuration;

use crate::types::{AuthProgress, AuthProvider, AuthSession, ProgressCallback};

// ── Constants ──

/// RMCCC Microsoft OAuth2 client ID.
pub const CLIENT_ID: &str = "d51b460a-0b8a-4696-af4d-690f7ba7f5b6";

/// Scope for device code request — must be exactly this.
const DEVICE_CODE_SCOPE: &str = "XboxLive.signin offline_access";

const DEVICE_CODE_URL: &str =
    "https://login.microsoftonline.com/consumers/oauth2/v2.0/devicecode";
const TOKEN_URL: &str =
    "https://login.microsoftonline.com/consumers/oauth2/v2.0/token";

const XBL_AUTH_URL: &str = "https://user.auth.xboxlive.com/user/authenticate";
const XSTS_AUTH_URL: &str = "https://xsts.auth.xboxlive.com/xsts/authorize";
const MC_AUTH_URL: &str =
    "https://api.minecraftservices.com/authentication/login_with_xbox";
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
            tracing::error!(status = %status, body = %body, "Device code request failed");
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
                    return Err(RmcccError::AuthFailed(
                        "User declined authorization".into(),
                    ));
                }
                "expired_token" => {
                    return Err(RmcccError::TokenExpired);
                }
                other => {
                    tracing::error!(error = %other, "Unexpected token error");
                    return Err(RmcccError::AuthFailed(format!(
                        "Token error: {other}"
                    )));
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

    async fn authenticate(
        &self,
        progress: Option<&ProgressCallback>,
    ) -> Result<AuthSession> {
        tracing::info!("Starting Microsoft OAuth2 device code flow");

        // Step 1: Get device code
        let device = self.request_device_code().await?;

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
        let ms_token = self
            .poll_for_token(
                &device.device_code,
                device.interval,
                device.expires_in,
                progress,
            )
            .await?;

        tracing::info!("Microsoft token acquired, exchanging for Xbox Live...");

        // Step 3a: Authenticate with Xbox Live
        let xbl = self.authenticate_xbl(&ms_token.access_token).await?;

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
        let xsts = self.authenticate_xsts(&xbl.token).await?;

        // Step 4: Exchange for Minecraft token
        let mc_auth = self
            .authenticate_minecraft(&xsts.token, &user_hash)
            .await?;

        // Step 5: Fetch Minecraft profile
        let profile = self.fetch_profile(&mc_auth.access_token).await?;

        if let Some(cb) = progress {
            cb(AuthProgress::Success);
        }

        tracing::info!(
            username = %profile.name,
            uuid = %profile.id,
            "Microsoft auth complete"
        );

        Ok(AuthSession {
            provider: "microsoft".into(),
            access_token: mc_auth.access_token,
            username: profile.name,
            uuid: profile.id,
            refresh_token: ms_token.refresh_token,
            expires_at: Some(
                Utc::now() + Duration::seconds(mc_auth.expires_in as i64),
            ),
        })
    }

    async fn refresh(&self, session: &AuthSession) -> Result<AuthSession> {
        let refresh_token = session
            .refresh_token
            .as_ref()
            .ok_or_else(|| RmcccError::TokenExpired)?;

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
        let mc_auth = self
            .authenticate_minecraft(&xsts.token, &user_hash)
            .await?;
        let profile = self.fetch_profile(&mc_auth.access_token).await?;

        tracing::info!(username = %profile.name, "Token refreshed");

        Ok(AuthSession {
            provider: "microsoft".into(),
            access_token: mc_auth.access_token,
            username: profile.name,
            uuid: profile.id,
            refresh_token: ms_token
                .refresh_token
                .or_else(|| session.refresh_token.clone()),
            expires_at: Some(
                Utc::now() + Duration::seconds(mc_auth.expires_in as i64),
            ),
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
