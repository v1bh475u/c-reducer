use slicer_core::context::CoverageData;
use slicer_core::pass::{Candidate, ReductionPass};
use slicer_parser::{CParser, StatementKind};

use crate::util::extend_to_line;

#[derive(Debug, Default)]
pub struct DeadCodePass;

impl ReductionPass for DeadCodePass {
    fn apply(&self, source: &str, coverage: Option<&CoverageData>) -> Vec<Candidate> {
        let coverage = match coverage {
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

            let has_any_executed =
                (start_line..=end_line).any(|line| coverage.is_line_executed(line as u32));
            if has_any_executed {
                continue;
            }

            let has_executable = (start_line..=end_line)
                .any(|line| coverage.line_hits.contains_key(&(line as u32)));
            if !has_executable {
                continue;
            }

            let extended = extend_to_line(source, stmt.range);
            dead_ranges.push((extended.start, extended.end));
            candidates.push(Candidate::removal(extended.to_range()));
        }

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
                candidates.push(Candidate::removal(block_start..block_end));
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
    use std::collections::HashMap;

    fn make_coverage(dead_lines: &[u32], live_lines: &[u32]) -> CoverageData {
        let mut line_hits = HashMap::new();
        for &line in dead_lines {
            line_hits.insert(line, 0);
        }
        for &line in live_lines {
            line_hits.insert(line, 1);
        }
        CoverageData {
            line_hits,
            function_hits: HashMap::new(),
        }
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
        let cov = make_coverage(&[3], &[2, 4]);
        let pass = DeadCodePass;
        let candidates = pass.apply(source, Some(&cov));
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
        let pass = DeadCodePass;
        let candidates = pass.apply(source, None);
        assert!(candidates.is_empty());
    }

    #[test]
    fn test_no_candidates_all_live() {
        let source = "int main() {\n    int x = 1;\n    return x;\n}\n";
        let cov = make_coverage(&[], &[2, 3]);
        let pass = DeadCodePass;
        let candidates = pass.apply(source, Some(&cov));
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
        let cov = make_coverage(&[3, 4, 5], &[2, 6]);
        let pass = DeadCodePass;
        let candidates = pass.apply(source, Some(&cov));
        let block_candidates: Vec<_> = candidates
            .iter()
            .filter(|c| {
                let range_size = c.range.end - c.range.start;
                range_size > 30
            })
            .collect();
        assert!(
            !block_candidates.is_empty(),
            "should generate block removal candidate for contiguous dead statements"
        );
    }
}
