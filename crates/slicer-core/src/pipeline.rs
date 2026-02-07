use crate::context::CoverageData;
use crate::pass::ReductionPass;

pub struct Pipeline {
    passes: Vec<Box<dyn ReductionPass>>,
    max_iterations: u32,
}

impl Pipeline {
    pub fn new(passes: Vec<Box<dyn ReductionPass>>, max_iterations: u32) -> Self {
        Self { passes, max_iterations }
    }

    pub fn reduce(
        &self,
        source: &str,
        coverage: Option<&CoverageData>,
        oracle: &mut dyn FnMut(&str) -> bool,
    ) -> String {
        let mut current = source.to_string();

        for _ in 0..self.max_iterations {
            let prev = current.clone();

            for pass in &self.passes {
                let mut candidates = pass.apply(&current, coverage);
                candidates.sort_by(|a, b| b.range.start.cmp(&a.range.start));

                for candidate in &candidates {
                    if let Some(reduced) = candidate.apply(&current) {
                        if oracle(&reduced) {
                            current = reduced;
                        }
                    }
                }
            }

            if current == prev {
                break;
            }
        }

        current
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pass::Candidate;

    struct RemoveFirstLinePass;

    impl ReductionPass for RemoveFirstLinePass {
        fn apply(&self, source: &str, _coverage: Option<&CoverageData>) -> Vec<Candidate> {
            let lines: Vec<_> = source.lines().collect();
            if lines.len() > 1 {
                let first_line_end = lines[0].len() + 1;
                vec![Candidate::removal(0..first_line_end.min(source.len()))]
            } else {
                Vec::new()
            }
        }
    }

    #[test]
    fn test_pipeline_reduces() {
        let pipeline = Pipeline::new(vec![Box::new(RemoveFirstLinePass)], 10);
        let source = "line1\nline2\nline3\n";
        let result = pipeline.reduce(source, None, &mut |_| true);
        assert_eq!(result, "line3\n");
    }

    #[test]
    fn test_pipeline_oracle_rejects() {
        let pipeline = Pipeline::new(vec![Box::new(RemoveFirstLinePass)], 10);
        let source = "line1\nline2\n";
        let result = pipeline.reduce(source, None, &mut |_| false);
        assert_eq!(result, source);
    }
}
