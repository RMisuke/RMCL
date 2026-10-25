//! Unified error types for the RMCCC project.
//!
//! This crate is a leaf dependency — all other crates can depend on it,
//! but it depends on no other internal crate.

use serde::{Deserialize, Serialize};
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

    // ── Validation / platform ──
    /// 输入校验失败（非法 URL、空名称、越界数值等）。
    #[error("Validation failed: {0}")]
    Validation(String),

    /// 当前平台未实现该能力（平台适配层 Windows/Linux 后端为 deferred）。
    ///
    /// 平台适配层 trait 必须返回本错误而不是 panic 或静默失败
    /// （指南 §5.7.1 第 4 条）。
    #[error("Unsupported on this platform: {0}")]
    UnsupportedPlatform(String),

    // ── Plugin ──
    /// 插件系统错误：Manifest/包/权限/状态机/配置。
    #[error("Plugin error [{code:?}]: {message}")]
    Plugin {
        code: PluginErrorCode,
        message: String,
    },

    // ── Generic ──
    #[error("{0}")]
    Other(String),
}

/// 插件系统的细分错误码（IPC 会映射到 [`ErrorCode`] 的对应变体）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[derive(TS)]
#[ts(export_to = "types.ts")]
pub enum PluginErrorCode {
    /// Manifest 字段/语义非法。
    ManifestInvalid,
    /// `.rmcpkg` 包结构/路径/大小非法。
    PackageInvalid,
    /// 能力未授权或作用域不符。
    PermissionDenied,
    /// 新权限尚未审查授予，插件不能启用。
    PermissionPending,
    /// 插件 ID 冲突或保留命名空间。
    IdConflict,
    /// 必需依赖缺失/版本不满足。
    DependencyMissing,
    /// 依赖图存在环。
    DependencyCycle,
    /// 生命周期状态机拒绝该转移。
    StateConflict,
    /// 插件配置/schema 非法。
    ConfigInvalid,
    /// 插件未安装。
    NotFound,
    /// 包内容与预期不一致（完整性）。
    Integrity,
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
    /// 当前平台未实现该能力（deferred 后端）。
    UnsupportedPlatform,
    // ── Plugin ──
    PluginManifestInvalid,
    PluginPackageInvalid,
    PluginPermissionDenied,
    PluginPermissionPending,
    PluginIdConflict,
    PluginDependencyMissing,
    PluginDependencyCycle,
    PluginStateConflict,
    PluginConfigInvalid,
    PluginNotFound,
    PluginIntegrity,
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
            RmcccError::AuthFailed(_) => ErrorCode::AuthFailed,
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
            RmcccError::Validation(_) => ErrorCode::ValidationError,
            RmcccError::UnsupportedPlatform(_) => ErrorCode::UnsupportedPlatform,
            RmcccError::Plugin { code, .. } => match code {
                PluginErrorCode::ManifestInvalid => ErrorCode::PluginManifestInvalid,
                PluginErrorCode::PackageInvalid => ErrorCode::PluginPackageInvalid,
                PluginErrorCode::PermissionDenied => ErrorCode::PluginPermissionDenied,
                PluginErrorCode::PermissionPending => ErrorCode::PluginPermissionPending,
                PluginErrorCode::IdConflict => ErrorCode::PluginIdConflict,
                PluginErrorCode::DependencyMissing => ErrorCode::PluginDependencyMissing,
                PluginErrorCode::DependencyCycle => ErrorCode::PluginDependencyCycle,
                PluginErrorCode::StateConflict => ErrorCode::PluginStateConflict,
                PluginErrorCode::ConfigInvalid => ErrorCode::PluginConfigInvalid,
                PluginErrorCode::NotFound => ErrorCode::PluginNotFound,
                PluginErrorCode::Integrity => ErrorCode::PluginIntegrity,
            },
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
