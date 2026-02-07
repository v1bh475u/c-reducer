use slicer_core::context::CoverageData;
use slicer_core::pass::{Candidate, ReductionPass};
use slicer_parser::ByteRange;

use crate::util::extend_to_line;

#[derive(Debug, Default)]
pub struct TypedefPass;

impl ReductionPass for TypedefPass {
    fn apply(&self, source: &str, _coverage: Option<&CoverageData>) -> Vec<Candidate> {
        let mut candidates = Vec::new();

        for (line_num, line) in source.lines().enumerate() {
            let trimmed = line.trim();

            if trimmed.starts_with("typedef ") {
                let line_start: usize = source
                    .lines()
                    .take(line_num)
                    .map(|l| l.len() + 1)
                    .sum();

                let typedef_end = if let Some(semi_pos) = source[line_start..].find(';') {
                    line_start + semi_pos + 1
                } else {
                    line_start + line.len()
                };

                let range = ByteRange::new(line_start, typedef_end);
                let extended = extend_to_line(source, range);
                candidates.push(Candidate::removal(extended.to_range()));
            }
        }

        candidates
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_remove_typedef() {
        let source = "typedef int MyInt;\n\nint main() {\n    int x = 0;\n    return x;\n}\n";
        let pass = TypedefPass;
        let candidates = pass.apply(source, None);
        assert!(!candidates.is_empty());
        let has_removal = candidates.iter().any(|c| {
            c.apply(source)
                .map(|r| !r.contains("typedef"))
                .unwrap_or(false)
        });
        assert!(has_removal);
    }

    #[test]
    fn test_typedef_candidate_for_used() {
        let source = "typedef int MyInt;\n\nint main() {\n    MyInt x = 0;\n    return x;\n}\n";
        let pass = TypedefPass;
        let candidates = pass.apply(source, None);
        assert!(!candidates.is_empty());
    }
}
