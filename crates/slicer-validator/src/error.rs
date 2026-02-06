//! Error types for the validator crate.

use thiserror::Error;

/// Result type for validation operations.
pub type ValidationResult<T> = Result<T, ValidationError>;

/// Errors that can occur during validation.
#[derive(Debug, Error)]
pub enum ValidationError {
    /// Compilation failed.
    #[error("compilation failed: {message}")]
    CompilationFailed {
        message: String,
        stderr: String,
        exit_code: Option<i32>,
    },

    /// Execution failed.
    #[error("execution failed: {message}")]
    ExecutionFailed {
        message: String,
        stderr: String,
        exit_code: Option<i32>,
    },

    /// Execution timed out.
    #[error("execution timed out after {timeout_secs} seconds")]
    Timeout { timeout_secs: f64 },

    /// Coverage collection failed.
    #[error("coverage collection failed: {0}")]
    CoverageFailed(String),

    /// Output mismatch.
    #[error("output mismatch: expected {expected:?}, got {actual:?}")]
    OutputMismatch { expected: String, actual: String },

    /// Exit code mismatch.
    #[error("exit code mismatch: expected {expected}, got {actual}")]
    ExitCodeMismatch { expected: i32, actual: i32 },

    /// IO error.
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    /// Compiler not found.
    #[error("compiler not found: {0}")]
    CompilerNotFound(String),

    /// Invalid configuration.
    #[error("invalid configuration: {0}")]
    InvalidConfig(String),
}

impl ValidationError {
    pub fn compilation_failed(
        message: impl Into<String>,
        stderr: impl Into<String>,
        exit_code: Option<i32>,
    ) -> Self {
        Self::CompilationFailed {
            message: message.into(),
            stderr: stderr.into(),
            exit_code,
        }
    }

    pub fn execution_failed(
        message: impl Into<String>,
        stderr: impl Into<String>,
        exit_code: Option<i32>,
    ) -> Self {
        Self::ExecutionFailed {
            message: message.into(),
            stderr: stderr.into(),
            exit_code,
        }
    }

    pub fn timeout(timeout_secs: f64) -> Self {
        Self::Timeout { timeout_secs }
    }

    pub fn output_mismatch(expected: impl Into<String>, actual: impl Into<String>) -> Self {
        Self::OutputMismatch {
            expected: expected.into(),
            actual: actual.into(),
        }
    }

    pub fn is_timeout(&self) -> bool {
        matches!(self, Self::Timeout { .. })
    }

    pub fn is_compilation_error(&self) -> bool {
        matches!(self, Self::CompilationFailed { .. })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_display() {
        let err = ValidationError::timeout(5.0);
        assert_eq!(err.to_string(), "execution timed out after 5 seconds");
        assert!(err.is_timeout());

        let err = ValidationError::compilation_failed("syntax error", "error: ...", Some(1));
        assert!(err.is_compilation_error());
    }
}
