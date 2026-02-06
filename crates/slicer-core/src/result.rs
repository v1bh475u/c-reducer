//! Reduction result.

use std::time::Duration;

#[derive(Debug, Clone)]
pub struct PassStats {
    pub name: String,
    pub lines_removed: u32,
    pub duration: Duration,
    pub candidates_tried: u32,
    pub candidates_valid: u32,
}

#[derive(Debug, Clone)]
pub struct ReductionResult {
    pub original_source: String,
    pub final_source: String,
    pub original_lines: u32,
    pub final_lines: u32,
    pub reduction_percent: f64,
    pub iterations: u32,
    pub converged: bool,
    pub pass_stats: Vec<PassStats>,
    pub total_duration: Duration,
}

impl ReductionResult {
    pub fn new(original: &str, final_source: &str) -> Self {
        let original_lines = count_lines(original);
        let final_lines = count_lines(final_source);
        let reduction_percent = if original_lines > 0 {
            (1.0 - final_lines as f64 / original_lines as f64) * 100.0
        } else {
            0.0
        };

        Self {
            original_source: original.to_string(),
            final_source: final_source.to_string(),
            original_lines,
            final_lines,
            reduction_percent,
            iterations: 0,
            converged: false,
            pass_stats: Vec::new(),
            total_duration: Duration::ZERO,
        }
    }

    pub fn with_iterations(mut self, iterations: u32, converged: bool) -> Self {
        self.iterations = iterations;
        self.converged = converged;
        self
    }

    pub fn with_pass_stats(mut self, stats: Vec<PassStats>) -> Self {
        self.pass_stats = stats;
        self
    }

    pub fn with_duration(mut self, duration: Duration) -> Self {
        self.total_duration = duration;
        self
    }

    pub fn summary(&self) -> String {
        format!(
            "Reduced {} → {} lines ({:.1}% reduction) in {:?}",
            self.original_lines, self.final_lines, self.reduction_percent, self.total_duration
        )
    }
}

/// Count non-empty lines in source.
fn count_lines(source: &str) -> u32 {
    source.lines().filter(|l| !l.trim().is_empty()).count() as u32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_reduction_result() {
        let original = "line1\nline2\nline3\nline4\nline5\n";
        let reduced = "line1\nline3\n";

        let result = ReductionResult::new(original, reduced);
        assert_eq!(result.original_lines, 5);
        assert_eq!(result.final_lines, 2);
        assert!((result.reduction_percent - 60.0).abs() < 0.1);
    }
}
