//! Include Pass
//!
//! This pass removes #include directives that may not be needed.

use slicer_core::pass::{Candidate, ReductionPass};
use slicer_parser::ByteRange;
use tracing::trace;

use crate::util::extend_to_line;

#[derive(Debug, Default)]
pub struct IncludePass;

impl IncludePass {
    pub fn new() -> Self {
        Self
    }
}

impl ReductionPass for IncludePass {
    fn name(&self) -> &'static str {
        "include"
    }

    fn priority(&self) -> u32 {
        60
    }

    fn apply(&self, source: &str, _context: &slicer_core::context::Context) -> Vec<Candidate> {
        let mut candidates = Vec::new();

        for (line_num, line) in source.lines().enumerate() {
            let trimmed = line.trim();

            if trimmed.starts_with("#include") {
                let line_start: usize = source
                    .lines()
                    .take(line_num)
                    .map(|l| l.len() + 1)
                    .sum();

                let line_end = line_start + line.len();

                let range = ByteRange::new(line_start, line_end);
                let extended = extend_to_line(source, range);

                trace!("include candidate: {}", trimmed);

                candidates.push(
                    Candidate::removal(extended.to_range())
                        .with_description(format!("remove {}", trimmed)),
                );
            }
        }

        candidates
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx() -> slicer_core::context::Context {
        slicer_core::Context::new(Box::new(slicer_core::AlwaysValidOracle))
    }

    #[test]
    fn test_remove_include() {
        let source = r#"#include <stdio.h>
#include <stdlib.h>

int main() {
    return 0;
}
"#;

        let pass = IncludePass::new();
        let candidates = pass.apply(source, &ctx());
        assert!(candidates.len() >= 2);

        // Each candidate should remove one include
        for candidate in &candidates {
            let result = candidate.apply(source).unwrap();
            // Either stdio.h or stdlib.h should be missing
            let removed_stdio = !result.contains("stdio.h");
            let removed_stdlib = !result.contains("stdlib.h");
            assert!(removed_stdio || removed_stdlib);
        }
    }

    #[test]
    fn test_no_includes() {
        let source = "int main() { return 0; }";

        let pass = IncludePass::new();
        let candidates = pass.apply(source, &ctx());
        assert!(candidates.is_empty());
    }
}
