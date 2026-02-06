//! Typedef Pass
//!
//! This pass removes typedef declarations that may not be needed.

use slicer_core::pass::{Candidate, ReductionPass};
use slicer_parser::ByteRange;
use tracing::trace;

use crate::util::extend_to_line;

#[derive(Debug, Default)]
pub struct TypedefPass;

impl TypedefPass {
    pub fn new() -> Self {
        Self
    }
}

impl ReductionPass for TypedefPass {
    fn name(&self) -> &'static str {
        "typedef"
    }

    fn priority(&self) -> u32 {
        50
    }

    fn apply(&self, source: &str, _context: &slicer_core::context::Context) -> Vec<Candidate> {
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

                let typedef_text = &source[line_start..typedef_end];
                if let Some(name) = extract_typedef_name(typedef_text) {
                    let count = source.matches(&name).count();

                    trace!("typedef '{}' appears {} times", name, count);

                    candidates.push(
                        Candidate::removal(extended.to_range())
                            .with_description(format!("remove typedef '{}'", name)),
                    );
                } else {
                    candidates.push(
                        Candidate::removal(extended.to_range())
                            .with_description("remove typedef".to_string()),
                    );
                }
            }
        }

        candidates
    }
}

fn extract_typedef_name(typedef: &str) -> Option<String> {
    let trimmed = typedef.trim().trim_end_matches(';').trim();
    let words: Vec<&str> = trimmed.split_whitespace().collect();
    if words.len() >= 2 {
        let last = words.last()?;
        let name: String = last
            .chars()
            .filter(|c| c.is_alphanumeric() || *c == '_')
            .collect();
        if !name.is_empty() && name != "typedef" {
            return Some(name);
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx() -> slicer_core::context::Context {
        slicer_core::Context::new(Box::new(slicer_core::AlwaysValidOracle))
    }

    #[test]
    fn test_remove_unused_typedef() {
        let source = r#"typedef int MyInt;

int main() {
    int x = 0;
    return x;
}
"#;

        let pass = TypedefPass::new();
        let candidates = pass.apply(source, &ctx());

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
        let source = r#"typedef int MyInt;

int main() {
    MyInt x = 0;
    return x;
}
"#;

        let pass = TypedefPass::new();
        let candidates = pass.apply(source, &ctx());

        // Should still generate a candidate (oracle will validate)
        assert!(!candidates.is_empty());
    }

    #[test]
    fn test_extract_typedef_name() {
        assert_eq!(
            extract_typedef_name("typedef int MyInt;"),
            Some("MyInt".to_string())
        );
        assert_eq!(
            extract_typedef_name("typedef unsigned long ULong;"),
            Some("ULong".to_string())
        );
    }
}
