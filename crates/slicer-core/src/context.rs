use std::collections::HashMap;

#[derive(Debug, Clone, Default)]
pub struct CoverageData {
    pub line_hits: HashMap<u32, u64>,
    pub function_hits: HashMap<String, u64>,
}

impl CoverageData {
    pub fn is_line_executed(&self, line: u32) -> bool {
        self.line_hits.get(&line).copied().unwrap_or(0) > 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_coverage_data() {
        let mut cov = CoverageData::default();
        cov.line_hits.insert(1, 5);
        cov.line_hits.insert(2, 0);
        assert!(cov.is_line_executed(1));
        assert!(!cov.is_line_executed(2));
        assert!(!cov.is_line_executed(3));
    }
}
