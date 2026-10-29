//! Core error types and result alias for the Atalho domain model.

/// Core domain error enumeration representing all anticipated failure conditions across the workspace.
#[derive(Debug, thiserror::Error)]
pub enum AppError {
    /// Error encountered during discovery or indexing of applications and commands.
    #[error("Indexing failed: {0}")]
    IndexingFailed(String),

    /// Error encountered while executing search matching or relevance calculation.
    #[error("Match failed: {0}")]
    MatchFailed(String),

    /// Error encountered while launching or executing an application or command.
    #[error("Execution failed: {0}")]
    ExecutionFailed(String),

    /// Standard I/O failure encountered during storage, filesystem, or process operations.
    #[error("I/O failed: {0}")]
    IoFailed(#[from] std::io::Error),

    /// Operating-system or platform-specific failure.
    #[error("Platform failed: {0}")]
    PlatformFailed(String),
}

impl PartialEq for AppError {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::IndexingFailed(a), Self::IndexingFailed(b))
            | (Self::MatchFailed(a), Self::MatchFailed(b))
            | (Self::ExecutionFailed(a), Self::ExecutionFailed(b))
            | (Self::PlatformFailed(a), Self::PlatformFailed(b)) => a == b,
            (Self::IoFailed(a), Self::IoFailed(b)) => {
                a.kind() == b.kind() && a.to_string() == b.to_string()
            }
            _ => false,
        }
    }
}

impl AppError {
    /// Creates an [`AppError::IndexingFailed`] with the provided detail message.
    #[must_use]
    pub fn indexing_failed(message: impl Into<String>) -> Self {
        Self::IndexingFailed(message.into())
    }

    /// Creates an [`AppError::MatchFailed`] with the provided detail message.
    #[must_use]
    pub fn match_failed(message: impl Into<String>) -> Self {
        Self::MatchFailed(message.into())
    }

    /// Creates an [`AppError::ExecutionFailed`] with the provided detail message.
    #[must_use]
    pub fn execution_failed(message: impl Into<String>) -> Self {
        Self::ExecutionFailed(message.into())
    }

    /// Creates an [`AppError::IoFailed`] from a custom error message.
    #[must_use]
    pub fn io_failed(message: impl Into<String>) -> Self {
        Self::IoFailed(std::io::Error::other(message.into()))
    }

    /// Creates an [`AppError::PlatformFailed`] with the provided detail message.
    #[must_use]
    pub fn platform_failed(message: impl Into<String>) -> Self {
        Self::PlatformFailed(message.into())
    }

    /// Creates an [`AppError::PlatformFailed`] indicating unsupported platform operation.
    #[must_use]
    pub fn unsupported_platform() -> Self {
        Self::PlatformFailed("Operation not supported on this platform".to_string())
    }
}

/// Convenience alias for [`std::result::Result`] specialized with [`AppError`].
pub type Result<T> = std::result::Result<T, AppError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_formatting() {
        let err = AppError::indexing_failed("disk scan timeout");
        assert_eq!(err.to_string(), "Indexing failed: disk scan timeout");

        let err = AppError::match_failed("invalid query syntax");
        assert_eq!(err.to_string(), "Match failed: invalid query syntax");

        let err = AppError::execution_failed("process permission denied");
        assert_eq!(
            err.to_string(),
            "Execution failed: process permission denied"
        );

        let err = AppError::platform_failed("registry key missing");
        assert_eq!(err.to_string(), "Platform failed: registry key missing");

        let io_err = AppError::io_failed("file not found");
        assert_eq!(io_err.to_string(), "I/O failed: file not found");
    }

    #[test]
    fn test_error_partial_eq() {
        let err1 = AppError::indexing_failed("timeout");
        let err2 = AppError::indexing_failed("timeout");
        let err3 = AppError::match_failed("timeout");

        assert_eq!(err1, err2);
        assert_ne!(err1, err3);
    }
}
