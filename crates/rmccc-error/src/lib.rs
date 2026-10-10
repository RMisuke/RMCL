//! Unified error types for the RMCCC project.
//!
//! This crate is a leaf dependency — all other crates can depend on it,
//! but it depends on no other internal crate.

use serde::Serialize;
use thiserror::Error;
use ts_rs::TS;

/// Central error type used across all RMCCC crates.
#[derive(Debug, Error)]
pub enum RmcccError {
    // ── Network ──
    #[error("Network request failed: {0}")]
    Network(String),

    #[error("HTTP {status}: {url}")]
    HttpStatus { status: u16, url: String },

    #[error("Connection timeout after {0}s")]
    Timeout(u64),

    // ── Auth ──
    #[error("Authentication failed: {0}")]
    AuthFailed(String),

    /// The stored credentials are permanently invalid and a full
    /// re-authorization is required (e.g. the Microsoft refresh token was
    /// revoked). Aligned with HMCL's `CredentialsExpiredException`; callers
    /// should route the user through `Account::log_in_when_credentials_expired`.
    #[error("Credentials expired, re-authentication required")]
    CredentialExpired,

    /// After refreshing, the refreshed credentials belong to a different
    /// Minecraft profile than the one stored. Aligned with HMCL's
    /// `OAuthAccount.WrongAccountException`; the token silently moved to
    /// another account and must not be used for the stored profile.
    #[error(
        "Refreshed credentials belong to a different profile (expected {expected}, got {actual})"
    )]
    WrongAccount { expected: String, actual: String },

    #[error("Token expired and refresh failed")]
    TokenExpired,

    #[allow(dead_code)]
    #[error("Device code auth pending: visit {uri} and enter code {user_code}")]
    DeviceCodePending { uri: String, user_code: String },

    // ── Download ──
    #[error("Download failed: {url} — {reason}")]
    DownloadFailed { url: String, reason: String },

    #[error("SHA1 mismatch for {path}: expected {expected}, got {actual}")]
    HashMismatch {
        path: String,
        expected: String,
        actual: String,
    },

    #[error("Insufficient disk space: need {needed} bytes, have {available}")]
    InsufficientDiskSpace { needed: u64, available: u64 },

    // ── Java ──
    #[error("Java not found")]
    JavaNotFound,

    #[error("Java version {found} too old, need {required}+")]
    JavaVersionMismatch { found: u32, required: u32 },

    // ── Launch ──
    #[error("Game process exited with code {0}")]
    GameCrashed(i32),

    #[error("Profile not found: {0}")]
    ProfileNotFound(String),

    // ── Mod ──
    #[error("Mod dependency missing: {0}")]
    MissingDependency(String),

    #[error("Mod version conflict: {mod_name} requires {constraint}")]
    ModVersionConflict {
        mod_name: String,
        constraint: String,
    },

    // ── IO / System ──
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("JSON parse error: {0}")]
    Json(#[from] serde_json::Error),

    // ── Generic ──
    #[error("{0}")]
    Other(String),
}

impl RmcccError {
    /// Whether this error is worth retrying.
    pub fn is_retryable(&self) -> bool {
        match self {
            RmcccError::Network(_) | RmcccError::Timeout(_) => true,
            RmcccError::HttpStatus { status, .. } => *status >= 500,
            _ => false,
        }
    }
}

/// Error code enum for IPC boundary — serialized to frontend.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
#[derive(TS)]
#[ts(export_to = "types.ts")]
pub enum ErrorCode {
    NetworkError,
    AuthFailed,
    /// Credentials permanently invalid — the user must re-authenticate.
    CredentialExpired,
    /// Refreshed credentials belong to a different Minecraft profile.
    WrongAccount,
    TokenExpired,
    DownloadFailed,
    HashMismatch,
    InsufficientDiskSpace,
    JavaNotFound,
    JavaVersionMismatch,
    GameCrashed,
    ProfileNotFound,
    MissingDependency,
    ModVersionConflict,
    IoError,
    AlreadyRunning,
    /// Internal error (should not happen in normal operation).
    InternalError,
    /// Input validation failed (e.g. name too long).
    ValidationError,
    /// Resource not found (version/mod not installed).
    NotFound,
    Unknown,
}

/// IPC-safe error that crosses the Tauri boundary.
#[derive(Debug, Serialize, TS)]
#[ts(export_to = "types.ts")]
pub struct IpcError {
    pub code: ErrorCode,
    pub message: String,
    pub retryable: bool,
}

impl From<RmcccError> for IpcError {
    fn from(err: RmcccError) -> Self {
        let retryable = err.is_retryable();
        let code = match &err {
            RmcccError::Network(_) | RmcccError::Timeout(_) | RmcccError::HttpStatus { .. } => {
                ErrorCode::NetworkError
            }
            RmcccError::AuthFailed(_) | RmcccError::DeviceCodePending { .. } => {
                ErrorCode::AuthFailed
            }
            RmcccError::CredentialExpired => ErrorCode::CredentialExpired,
            RmcccError::WrongAccount { .. } => ErrorCode::WrongAccount,
            RmcccError::TokenExpired => ErrorCode::TokenExpired,
            RmcccError::DownloadFailed { .. } => ErrorCode::DownloadFailed,
            RmcccError::HashMismatch { .. } => ErrorCode::HashMismatch,
            RmcccError::InsufficientDiskSpace { .. } => ErrorCode::InsufficientDiskSpace,
            RmcccError::JavaNotFound => ErrorCode::JavaNotFound,
            RmcccError::JavaVersionMismatch { .. } => ErrorCode::JavaVersionMismatch,
            RmcccError::GameCrashed(_) => ErrorCode::GameCrashed,
            RmcccError::ProfileNotFound(_) => ErrorCode::ProfileNotFound,
            RmcccError::MissingDependency(_) => ErrorCode::MissingDependency,
            RmcccError::ModVersionConflict { .. } => ErrorCode::ModVersionConflict,
            RmcccError::Io(_) | RmcccError::Json(_) => ErrorCode::IoError,
            _ => ErrorCode::Unknown,
        };
        IpcError {
            code,
            message: err.to_string(),
            retryable,
        }
    }
}

/// Direct conversion from low-level errors for cleaner IPC command code.
impl From<std::io::Error> for IpcError {
    fn from(e: std::io::Error) -> Self {
        IpcError::from(RmcccError::from(e))
    }
}

impl From<serde_json::Error> for IpcError {
    fn from(e: serde_json::Error) -> Self {
        IpcError::from(RmcccError::from(e))
    }
}

/// Convenience alias used throughout all crates.
pub type Result<T> = std::result::Result<T, RmcccError>;
