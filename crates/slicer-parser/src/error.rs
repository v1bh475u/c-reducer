use thiserror::Error;

pub type ParseResult<T> = Result<T, ParseError>;

#[derive(Debug, Error)]
pub enum ParseError {
    #[error("clang error: {0}")]
    Clang(String),

    #[error("failed to create translation unit: {0}")]
    TranslationUnit(String),

    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}

impl ParseError {
    pub fn clang(message: impl Into<String>) -> Self {
        Self::Clang(message.into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_display() {
        let err = ParseError::clang("failed to init");
        assert!(err.to_string().contains("clang error"));
    }
}
