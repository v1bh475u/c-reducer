//! Error types for the parser crate.

use thiserror::Error;

/// Result type for parsing operations.
pub type ParseResult<T> = Result<T, ParseError>;

/// Errors that can occur during parsing.
#[derive(Debug, Error)]
pub enum ParseError {
    /// Failed to parse the source code.
    #[error("parse error at {file}:{line}:{column}: {message}")]
    Syntax {
        file: String,
        line: u32,
        column: u32,
        message: String,
    },

    /// Invalid byte range specified.
    #[error("invalid byte range {start}..{end} (source length: {source_len})")]
    InvalidRange {
        start: usize,
        end: usize,
        source_len: usize,
    },

    /// Node not found.
    #[error("node not found: {0}")]
    NodeNotFound(String),

    /// Clang error.
    #[error("clang error: {0}")]
    Clang(String),

    /// Failed to create translation unit.
    #[error("failed to create translation unit: {0}")]
    TranslationUnit(String),

    /// IO error.
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}

impl ParseError {
    /// Create a syntax error at a specific location.
    pub fn syntax(
        file: impl Into<String>,
        line: u32,
        column: u32,
        message: impl Into<String>,
    ) -> Self {
        Self::Syntax {
            file: file.into(),
            line,
            column,
            message: message.into(),
        }
    }

    /// Create an invalid range error.
    pub fn invalid_range(start: usize, end: usize, source_len: usize) -> Self {
        Self::InvalidRange {
            start,
            end,
            source_len,
        }
    }

    /// Create a node not found error.
    pub fn node_not_found(description: impl Into<String>) -> Self {
        Self::NodeNotFound(description.into())
    }

    /// Create a clang error.
    pub fn clang(message: impl Into<String>) -> Self {
        Self::Clang(message.into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_display() {
        let err = ParseError::syntax("test.c", 10, 5, "unexpected token");
        assert_eq!(
            err.to_string(),
            "parse error at test.c:10:5: unexpected token"
        );

        let err = ParseError::invalid_range(100, 200, 50);
        assert_eq!(
            err.to_string(),
            "invalid byte range 100..200 (source length: 50)"
        );
    }
}
