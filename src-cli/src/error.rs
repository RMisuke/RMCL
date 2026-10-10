//! Unified CLI error model.
//!
//! The CLI reuses the backend [`ErrorCode`] vocabulary so that the machine
//! readable `code` field is identical to the Tauri IPC boundary — one error
//! model across CLI and frontend. Each error additionally carries a stable
//! process **exit code** so shell scripts can branch on failure classes.

use rmccc_error::ErrorCode;
use serde::Serialize;

/// Result alias for CLI command handlers.
pub type CliResult<T> = std::result::Result<T, CliError>;

/// A CLI-level error with a stable machine-readable code and an exit code.
#[derive(Debug, Clone, Serialize)]
pub struct CliError {
    pub code: ErrorCode,
    pub message: String,
    pub retryable: bool,
    /// Process exit code to return to the shell.
    pub exit_code: i32,
}

impl CliError {
    /// Build an error from a backend error code with a human-readable message.
    pub fn new(code: ErrorCode, message: impl Into<String>) -> Self {
        let retryable = matches!(
            &code,
            ErrorCode::NetworkError | ErrorCode::TokenExpired | ErrorCode::DownloadFailed
        );
        Self {
            exit_code: exit_code_for(&code),
            code,
            message: message.into(),
            retryable,
        }
    }

    /// The process exit code for this error.
    pub fn exit_code(&self) -> i32 {
        self.exit_code
    }
}

impl From<rmccc_error::RmcccError> for CliError {
    fn from(e: rmccc_error::RmcccError) -> Self {
        use rmccc_error::RmcccError as E;
        let retryable = e.is_retryable();
        // Refine the backend taxonomy for the CLI: the backend only exposes a
        // generic `Other` bucket, but the CLI needs finer classes (validation /
        // not-found / already-running) so scripts can branch on exit codes.
        let code = match &e {
            E::Network(_) | E::Timeout(_) | E::HttpStatus { .. } => ErrorCode::NetworkError,
            E::AuthFailed(_) | E::DeviceCodePending { .. } => ErrorCode::AuthFailed,
            E::CredentialExpired => ErrorCode::CredentialExpired,
            E::WrongAccount { .. } => ErrorCode::WrongAccount,
            E::TokenExpired => ErrorCode::TokenExpired,
            E::DownloadFailed { .. } => ErrorCode::DownloadFailed,
            E::HashMismatch { .. } => ErrorCode::HashMismatch,
            E::InsufficientDiskSpace { .. } => ErrorCode::InsufficientDiskSpace,
            E::JavaNotFound => ErrorCode::JavaNotFound,
            E::JavaVersionMismatch { .. } => ErrorCode::JavaVersionMismatch,
            E::GameCrashed(_) => ErrorCode::GameCrashed,
            E::ProfileNotFound(_) => ErrorCode::ProfileNotFound,
            E::MissingDependency(_) => ErrorCode::MissingDependency,
            E::ModVersionConflict { .. } => ErrorCode::ModVersionConflict,
            E::Io(_) | E::Json(_) => ErrorCode::IoError,
            E::Other(msg) => cli_classify(msg),
        };
        Self {
            exit_code: exit_code_for(&code),
            code,
            message: e.to_string(),
            retryable,
        }
    }
}

/// Classify a generic backend `Other` error message into a CLI error code.
///
/// The backend's `RmcccError::Other` is a catch-all; the CLI adapter refines
/// it into stable classes based on the message shape so exit codes stay
/// meaningful. Unknown/internal messages fall back to [`ErrorCode::Unknown`].
fn cli_classify(message: &str) -> ErrorCode {
    let lower = message.to_lowercase();
    if lower.contains("already running") {
        return ErrorCode::AlreadyRunning;
    }
    if lower.contains("not found")
        || lower.contains("not installed")
        || lower.contains("does not exist")
        || lower.contains("no version")
    {
        return ErrorCode::NotFound;
    }
    if lower.contains("unknown")
        || lower.contains("not a valid")
        || lower.contains("cannot be empty")
        || lower.contains("too long")
        || lower.contains("must be")
        || lower.contains("requires")
        || lower.contains("invalid")
        || lower.contains("expected")
    {
        return ErrorCode::ValidationError;
    }
    ErrorCode::Unknown
}

impl From<std::io::Error> for CliError {
    fn from(e: std::io::Error) -> Self {
        rmccc_error::RmcccError::from(e).into()
    }
}

impl From<serde_json::Error> for CliError {
    fn from(e: serde_json::Error) -> Self {
        rmccc_error::RmcccError::from(e).into()
    }
}

impl std::fmt::Display for CliError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "[{:?}] {}", self.code, self.message)
    }
}

impl std::error::Error for CliError {}

/// Map a backend error code to a stable process exit code.
///
/// Exit code contract (documented in `docs/CLI.md`):
/// - 0   success
/// - 1   unknown / internal
/// - 2   CLI usage / parse error (also used by clap)
/// - 3   validation error (bad argument value / business pre-check)
/// - 4   resource / profile / version / mod not found
/// - 5   authentication / credential errors
/// - 6   network / download errors
/// - 7   Java errors
/// - 8   I/O / system errors
/// - 9   already running
/// - 10  business conflict (missing dependency / version conflict)
/// - 130 interrupted by SIGINT
pub fn exit_code_for(code: &ErrorCode) -> i32 {
    match code {
        ErrorCode::NetworkError
        | ErrorCode::DownloadFailed
        | ErrorCode::HashMismatch
        | ErrorCode::InsufficientDiskSpace => 6,
        ErrorCode::AuthFailed
        | ErrorCode::CredentialExpired
        | ErrorCode::WrongAccount
        | ErrorCode::TokenExpired => 5,
        ErrorCode::JavaNotFound | ErrorCode::JavaVersionMismatch => 7,
        ErrorCode::GameCrashed => 1,
        ErrorCode::ProfileNotFound | ErrorCode::NotFound => 4,
        ErrorCode::MissingDependency | ErrorCode::ModVersionConflict => 10,
        ErrorCode::IoError => 8,
        ErrorCode::AlreadyRunning => 9,
        ErrorCode::ValidationError => 3,
        ErrorCode::InternalError | ErrorCode::Unknown => 1,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exit_codes_are_stable() {
        assert_eq!(exit_code_for(&ErrorCode::NetworkError), 6);
        assert_eq!(exit_code_for(&ErrorCode::DownloadFailed), 6);
        assert_eq!(exit_code_for(&ErrorCode::HashMismatch), 6);
        assert_eq!(exit_code_for(&ErrorCode::InsufficientDiskSpace), 6);
        assert_eq!(exit_code_for(&ErrorCode::AuthFailed), 5);
        assert_eq!(exit_code_for(&ErrorCode::CredentialExpired), 5);
        assert_eq!(exit_code_for(&ErrorCode::WrongAccount), 5);
        assert_eq!(exit_code_for(&ErrorCode::TokenExpired), 5);
        assert_eq!(exit_code_for(&ErrorCode::JavaNotFound), 7);
        assert_eq!(exit_code_for(&ErrorCode::JavaVersionMismatch), 7);
        assert_eq!(exit_code_for(&ErrorCode::ProfileNotFound), 4);
        assert_eq!(exit_code_for(&ErrorCode::NotFound), 4);
        assert_eq!(exit_code_for(&ErrorCode::IoError), 8);
        assert_eq!(exit_code_for(&ErrorCode::ValidationError), 3);
        assert_eq!(exit_code_for(&ErrorCode::AlreadyRunning), 9);
        assert_eq!(exit_code_for(&ErrorCode::MissingDependency), 10);
        assert_eq!(exit_code_for(&ErrorCode::ModVersionConflict), 10);
        assert_eq!(exit_code_for(&ErrorCode::Unknown), 1);
        assert_eq!(exit_code_for(&ErrorCode::InternalError), 1);
    }

    #[test]
    fn rmccc_error_maps_to_cli_error() {
        let e = CliError::from(rmccc_error::RmcccError::ProfileNotFound("p1".into()));
        assert_eq!(e.code, ErrorCode::ProfileNotFound);
        assert_eq!(e.exit_code, 4);
        assert!(!e.retryable);

        let e = CliError::from(rmccc_error::RmcccError::Network("boom".into()));
        assert_eq!(e.code, ErrorCode::NetworkError);
        assert_eq!(e.exit_code, 6);
        assert!(e.retryable);
    }
}
