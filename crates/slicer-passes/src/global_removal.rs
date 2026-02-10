use slicer_core::context::CoverageData;
use slicer_core::pass::{Candidate, ReductionPass};
use slicer_parser::{CParser, DeclarationKind};

use crate::util::extend_to_line;

#[derive(Debug, Default)]
pub struct GlobalRemovalPass;

impl ReductionPass for GlobalRemovalPass {
    fn apply(&self, source: &str, _coverage: Option<&CoverageData>) -> Vec<Candidate> {
        let parser = CParser::default();
        let unit = match parser.parse(source) {
            Ok(u) => u,
            Err(_) => return Vec::new(),
        };

        let header_end = unit.header_end();
        let mut candidates = Vec::new();

        for decl in unit.declarations() {
            let is_global_var = matches!(decl.kind, DeclarationKind::Variable { .. })
                && decl.range.start < header_end;

            if !is_global_var {
                continue;
            }

            if decl.name.is_empty() {
                continue;
            }

            let count = count_identifier_occurrences(source, &decl.name);
            if count <= 1 {
                let extended = extend_to_line(source, decl.range);
                candidates.push(Candidate::removal(extended.to_range()));
            }
        }

        candidates
    }
}

fn count_identifier_occurrences(source: &str, name: &str) -> usize {
    let mut count = 0;
    let mut search_from = 0;
    while let Some(pos) = source[search_from..].find(name) {
        let abs_pos = search_from + pos;
        let before_ok = abs_pos == 0 || {
            let b = source.as_bytes()[abs_pos - 1];
            !b.is_ascii_alphanumeric() && b != b'_'
        };
        let after_pos = abs_pos + name.len();
        let after_ok = after_pos >= source.len() || {
            let b = source.as_bytes()[after_pos];
            !b.is_ascii_alphanumeric() && b != b'_'
        };
        if before_ok && after_ok {
            count += 1;
        }
        search_from = abs_pos + 1;
    }
    count
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_remove_unused_global() {
        let source = "int unused_global = 42;\n\nint main() {\n    return 0;\n}\n";
        let pass = GlobalRemovalPass;
        let candidates = pass.apply(source, None);
        assert!(!candidates.is_empty(), "should find unused global");
        let removed = candidates[0].apply(source).unwrap();
        assert!(!removed.contains("unused_global"));
        assert!(removed.contains("main"));
    }

    #[test]
    fn test_keep_used_global() {
        let source = "int used_global = 42;\n\nint main() {\n    return used_global;\n}\n";
        let pass = GlobalRemovalPass;
        let candidates = pass.apply(source, None);
        // used_global appears twice (decl + use), so should NOT be removed
        let removes_it = candidates.iter().any(|c| {
            c.apply(source)
                .map(|r| !r.contains("used_global"))
                .unwrap_or(false)
        });
        assert!(!removes_it, "should not remove used global variable");
    }

    #[test]
    fn test_multiple_unused_globals() {
        let source = "int a = 1;\nint b = 2;\nint c = 3;\n\nint main() {\n    return a;\n}\n";
        let pass = GlobalRemovalPass;
        let candidates = pass.apply(source, None);
        // b and c are unused
        assert!(
            candidates.len() >= 2,
            "should find at least 2 unused globals, got {}",
            candidates.len()
        );
    }

    #[test]
    fn test_count_identifier_occurrences() {
        assert_eq!(count_identifier_occurrences("int x = 1; return x;", "x"), 2);
        assert_eq!(count_identifier_occurrences("int xyz = 1;", "x"), 0);
        assert_eq!(count_identifier_occurrences("int x = 1;", "x"), 1);
        assert_eq!(count_identifier_occurrences("x + x + x", "x"), 3);
    }
}
