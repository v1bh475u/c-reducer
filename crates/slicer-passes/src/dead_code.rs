//! Dead Code Removal Pass
//!
//! This pass uses coverage data to remove code that was never executed:
//! 1. Individual statements on unexecuted lines
//! 2. Contiguous blocks of dead statements

use slicer_core::context::Context;
use slicer_core::pass::{Candidate, ReductionPass};
use slicer_parser::{CParser, StatementKind};
use tracing::trace;

use crate::util::extend_to_line;

#[derive(Debug, Default)]
pub struct DeadCodePass;

impl DeadCodePass {
    pub fn new() -> Self {
        Self
    }
}

impl ReductionPass for DeadCodePass {
    fn name(&self) -> &'static str {
        "dead_code"
    }

    fn priority(&self) -> u32 {
        90
    }

    fn apply(&self, source: &str, context: &Context) -> Vec<Candidate> {
        let coverage = match &context.coverage {
            Some(cov) => cov,
            None => return Vec::new(),
        };

        let parser = CParser::default();
        let unit = match parser.parse(source) {
            Ok(u) => u,
            Err(_) => return Vec::new(),
        };

        let header_end = unit.header_end();
        let mut candidates = Vec::new();
        let mut dead_ranges: Vec<(usize, usize)> = Vec::new();

        for stmt in unit.statements() {
            if stmt.range.start < header_end {
                continue;
            }

            if matches!(stmt.kind, StatementKind::Compound) {
                continue;
            }

            let start_line = byte_offset_to_line(source, stmt.range.start);
            let end_line = byte_offset_to_line(source, stmt.range.end);

            let has_any_executed = (start_line..=end_line)
                .any(|line| coverage.is_line_executed(line as u32));

            if has_any_executed {
                continue;
            }

            // Only consider lines that are actually in the coverage map (executable)
            let has_executable = (start_line..=end_line)
                .any(|line| coverage.line_hits.contains_key(&(line as u32)));

            if !has_executable {
                continue;
            }

            let extended = extend_to_line(source, stmt.range);
            dead_ranges.push((extended.start, extended.end));

            candidates.push(
                Candidate::removal(extended.to_range())
                    .with_description(format!("remove dead code at line {}", start_line)),
            );
            trace!("dead statement at line {} (0 hits)", start_line);
        }

        // Merge contiguous dead ranges into block-removal candidates
        dead_ranges.sort_by_key(|r| r.0);
        let mut i = 0;
        while i < dead_ranges.len() {
            let block_start = dead_ranges[i].0;
            let mut block_end = dead_ranges[i].1;
            let mut j = i + 1;

            while j < dead_ranges.len() {
                let gap = &source[block_end..dead_ranges[j].0];
                if gap.trim().is_empty() {
                    block_end = dead_ranges[j].1;
                    j += 1;
                } else {
                    break;
                }
            }

            if j - i > 1 {
                candidates.push(
                    Candidate::removal(block_start..block_end).with_description(format!(
                        "remove dead block ({} statements)",
                        j - i
                    )),
                );
            }

            i = j;
        }

        candidates
    }
}

fn byte_offset_to_line(source: &str, offset: usize) -> usize {
    source[..offset.min(source.len())]
        .chars()
        .filter(|&c| c == '\n')
        .count()
        + 1
}

#[cfg(test)]
mod tests {
    use super::*;
    use slicer_core::context::CoverageData;
    use std::collections::HashMap;

    fn context_with_coverage(dead_lines: &[u32], live_lines: &[u32]) -> Context {
        let mut line_hits = HashMap::new();
        for &line in dead_lines {
            line_hits.insert(line, 0);
        }
        for &line in live_lines {
            line_hits.insert(line, 1);
        }
        let coverage = CoverageData {
            line_hits,
            function_hits: HashMap::new(),
        };
        Context::new(Box::new(slicer_core::AlwaysValidOracle)).with_coverage(coverage)
    }

    fn empty_context() -> Context {
        Context::new(Box::new(slicer_core::AlwaysValidOracle))
    }

    #[test]
    fn test_remove_dead_statement() {
        let source = "\
int main() {
    int x = 1;
    int y = 2;
    return x;
}
";
        // Line 2: int x = 1;  -> executed
        // Line 3: int y = 2;  -> dead
        // Line 4: return x;   -> executed
        let ctx = context_with_coverage(&[3], &[2, 4]);
        let pass = DeadCodePass::new();
        let candidates = pass.apply(source, &ctx);

        assert!(!candidates.is_empty());

        let has_dead_removal = candidates.iter().any(|c| {
            c.apply(source)
                .map(|r| !r.contains("int y"))
                .unwrap_or(false)
        });
        assert!(has_dead_removal);
    }

    #[test]
    fn test_no_candidates_without_coverage() {
        let source = "int main() {\n    int x = 1;\n    return x;\n}\n";
        let ctx = empty_context();
        let pass = DeadCodePass::new();
        let candidates = pass.apply(source, &ctx);
        assert!(candidates.is_empty());
    }

    #[test]
    fn test_no_candidates_all_live() {
        let source = "int main() {\n    int x = 1;\n    return x;\n}\n";
        let ctx = context_with_coverage(&[], &[2, 3]);
        let pass = DeadCodePass::new();
        let candidates = pass.apply(source, &ctx);
        assert!(candidates.is_empty());
    }

    #[test]
    fn test_dead_block_removal() {
        let source = "\
int main() {
    int x = 1;
    int a = 10;
    int b = 20;
    int c = 30;
    return x;
}
";
        // Lines 3,4,5 are all dead -> should get a block candidate
        let ctx = context_with_coverage(&[3, 4, 5], &[2, 6]);
        let pass = DeadCodePass::new();
        let candidates = pass.apply(source, &ctx);

        let block_candidates: Vec<_> = candidates
            .iter()
            .filter(|c| c.description.contains("block"))
            .collect();
        assert!(
            !block_candidates.is_empty(),
            "should generate block removal candidate for contiguous dead statements"
        );
    }
}
