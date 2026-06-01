use crate::context::CoverageData;
use std::ops::Range;

#[derive(Debug, Clone)]
pub struct Candidate {
    pub range: Range<usize>,
    pub replacement: String,
}

impl Candidate {
    pub fn removal(range: Range<usize>) -> Self {
        Self {
            range,
            replacement: String::new(),
        }
    }

    pub fn new(range: Range<usize>, replacement: impl Into<String>) -> Self {
        Self {
            range,
            replacement: replacement.into(),
        }
    }

    pub fn apply(&self, source: &str) -> Option<String> {
        if self.range.start > source.len() || self.range.end > source.len() {
            return None;
        }
        if self.range.start > self.range.end {
            return None;
        }
        if !source.is_char_boundary(self.range.start) || !source.is_char_boundary(self.range.end) {
            return None;
        }
        let mut result = String::with_capacity(source.len());
        result.push_str(&source[..self.range.start]);
        result.push_str(&self.replacement);
        result.push_str(&source[self.range.end..]);
        Some(result)
    }
}

pub trait ReductionPass: Send + Sync {
    fn apply(&self, source: &str, coverage: Option<&CoverageData>) -> Vec<Candidate>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_candidate_remove() {
        let source = "int x = 0; int y = 1;";
        let candidate = Candidate::removal(0..11);
        let result = candidate.apply(source).unwrap();
        assert_eq!(result, "int y = 1;");
    }

    #[test]
    fn test_candidate_replace() {
        let source = "x + 0";
        let candidate = Candidate::new(0..5, "x");
        let result = candidate.apply(source).unwrap();
        assert_eq!(result, "x");
    }

    #[test]
    fn test_candidate_out_of_bounds() {
        let source = "short";
        let candidate = Candidate::removal(0..100);
        assert!(candidate.apply(source).is_none());
    }
}
