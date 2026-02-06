//! Reduction pass trait and candidate types.

use std::ops::Range;

#[derive(Debug, Clone)]
pub struct Candidate {
    pub description: String,
    pub range: Range<usize>,
    pub replacement: String,
    pub confidence: f64,
}

impl Candidate {
    pub fn removal(range: impl Into<Range<usize>>) -> Self {
        Self {
            description: "removal".into(),
            range: range.into(),
            replacement: String::new(),
            confidence: 1.0,
        }
    }

    pub fn new(range: impl Into<Range<usize>>, replacement: impl Into<String>) -> Self {
        Self {
            description: "replacement".into(),
            range: range.into(),
            replacement: replacement.into(),
            confidence: 1.0,
        }
    }

    pub fn remove(description: impl Into<String>, range: Range<usize>) -> Self {
        Self {
            description: description.into(),
            range,
            replacement: String::new(),
            confidence: 1.0,
        }
    }

    pub fn replace(
        description: impl Into<String>,
        range: Range<usize>,
        replacement: impl Into<String>,
    ) -> Self {
        Self {
            description: description.into(),
            range,
            replacement: replacement.into(),
            confidence: 1.0,
        }
    }

    pub fn with_description(mut self, desc: impl Into<String>) -> Self {
        self.description = desc.into();
        self
    }

    pub fn with_confidence(mut self, confidence: f64) -> Self {
        self.confidence = confidence;
        self
    }

    /// Apply this candidate to the source.
    ///
    /// Returns None if the range is out of bounds.
    pub fn apply(&self, source: &str) -> Option<String> {
        // Bounds checking
        if self.range.start > source.len() || self.range.end > source.len() {
            return None;
        }
        if self.range.start > self.range.end {
            return None;
        }

        // Check for valid UTF-8 boundaries
        if !source.is_char_boundary(self.range.start) || !source.is_char_boundary(self.range.end) {
            return None;
        }

        let mut result = String::with_capacity(source.len());
        result.push_str(&source[..self.range.start]);
        result.push_str(&self.replacement);
        result.push_str(&source[self.range.end..]);
        Some(result)
    }

    pub fn reduction(&self) -> isize {
        (self.range.end - self.range.start) as isize - self.replacement.len() as isize
    }
}

/// Trait for reduction passes.
///
/// Each pass implements a specific reduction strategy. Passes run serially
/// in priority order (higher priority number = runs first).
pub trait ReductionPass: Send + Sync {
    /// Human-readable name of this pass.
    fn name(&self) -> &'static str;

    /// Priority (higher = runs first).
    ///
    /// Suggested ranges:
    /// - 90-100: Coarse passes (dead functions, whole blocks)
    /// - 60-80: Medium passes (statements, includes)
    /// - 30-50: Fine passes (expressions, typedefs)
    /// - 0-20: Very fine passes (arguments, operators)
    fn priority(&self) -> u32;

    /// Generate candidate reductions from the source.
    ///
    /// Returns a list of candidates that could potentially reduce the program.
    /// The caller is responsible for validating each candidate.
    /// Context provides optional coverage data and other shared state.
    fn apply(&self, source: &str, context: &crate::context::Context) -> Vec<Candidate>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_candidate_remove() {
        let source = "int x = 0; int y = 1;";
        let candidate = Candidate::remove("remove x", 0..11);
        let result = candidate.apply(source).unwrap();
        assert_eq!(result, "int y = 1;");
    }

    #[test]
    fn test_candidate_replace() {
        let source = "x + 0";
        let candidate = Candidate::replace("simplify", 0..5, "x");
        let result = candidate.apply(source).unwrap();
        assert_eq!(result, "x");
    }

    #[test]
    fn test_candidate_reduction() {
        let candidate = Candidate::remove("test", 0..10);
        assert_eq!(candidate.reduction(), 10);

        let candidate = Candidate::replace("test", 0..10, "abc");
        assert_eq!(candidate.reduction(), 7);
    }
}
