use thiserror::Error;

pub type ValidationResult<T> = Result<T, ValidationError>;

#[derive(Debug, Error)]
pub enum ValidationError {
    #[error("compilation failed: {message}")]
    CompilationFailed {
        message: String,
        stderr: String,
        exit_code: Option<i32>,
    },

    #[error("execution timed out after {timeout_secs} seconds")]
    Timeout { timeout_secs: f64 },

    #[error("coverage collection failed: {0}")]
    CoverageFailed(String),

    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    #[error("compiler not found: {0}")]
    CompilerNotFound(String),
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
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_display() {
        let err = ValidationError::compilation_failed("syntax error", "error: ...", Some(1));
        assert!(err.to_string().contains("compilation failed"));
    }
}
