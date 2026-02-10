use slicer_core::context::CoverageData;
use slicer_core::pass::{Candidate, ReductionPass};
use slicer_parser::{CParser, DeclarationKind};

use crate::util::extend_to_line;

#[derive(Debug, Default)]
pub struct EnumStructRemovalPass;

impl ReductionPass for EnumStructRemovalPass {
    fn apply(&self, source: &str, _coverage: Option<&CoverageData>) -> Vec<Candidate> {
        let parser = CParser::default();
        let unit = match parser.parse(source) {
            Ok(u) => u,
            Err(_) => return Vec::new(),
        };

        let mut candidates = Vec::new();

        for decl in unit.declarations() {
            let is_type_decl = matches!(
                decl.kind,
                DeclarationKind::Struct { .. } | DeclarationKind::Enum
            );

            if !is_type_decl || decl.name.is_empty() {
                continue;
            }

            let count = count_identifier(source, &decl.name);
            if count <= 1 {
                let extended = extend_to_line(source, decl.range);
                let end = find_semicolon_end(source, extended.end);
                let range = slicer_parser::ByteRange::new(extended.start, end);
                let fully_extended = extend_to_line(source, range);
                candidates.push(Candidate::removal(fully_extended.to_range()));
            }
        }

        candidates
    }
}

fn count_identifier(source: &str, name: &str) -> usize {
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

fn find_semicolon_end(source: &str, from: usize) -> usize {
    let rest = &source[from..];
    for (i, b) in rest.bytes().enumerate() {
        if b == b';' {
            return from + i + 1;
        }
        if b == b'\n' {
            break;
        }
    }
    from
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_remove_unused_struct() {
        let source =
            "struct Unused {\n    int x;\n    int y;\n};\n\nint main() {\n    return 0;\n}\n";
        let pass = EnumStructRemovalPass;
        let candidates = pass.apply(source, None);
        assert!(!candidates.is_empty(), "should find unused struct");
        let result = candidates[0].apply(source).unwrap();
        assert!(!result.contains("Unused"), "should remove unused struct");
        assert!(result.contains("main"));
    }

    #[test]
    fn test_remove_unused_enum() {
        let source = "enum Color { RED, GREEN, BLUE };\n\nint main() {\n    return 0;\n}\n";
        let pass = EnumStructRemovalPass;
        let candidates = pass.apply(source, None);
        assert!(!candidates.is_empty(), "should find unused enum");
    }

    #[test]
    fn test_keep_used_struct() {
        let source = "struct Point {\n    int x;\n    int y;\n};\n\nint main() {\n    struct Point p;\n    p.x = 1;\n    return p.x;\n}\n";
        let pass = EnumStructRemovalPass;
        let candidates = pass.apply(source, None);
        let removes_point = candidates.iter().any(|c| {
            c.apply(source)
                .map(|r| !r.contains("struct Point"))
                .unwrap_or(false)
        });
        assert!(!removes_point, "should not remove used struct");
    }

    #[test]
    fn test_keep_used_enum() {
        let source =
            "enum Dir { UP, DOWN };\n\nint main() {\n    enum Dir d = UP;\n    return d;\n}\n";
        let pass = EnumStructRemovalPass;
        let candidates = pass.apply(source, None);
        let removes_dir = candidates.iter().any(|c| {
            c.apply(source)
                .map(|r| !r.contains("enum Dir"))
                .unwrap_or(false)
        });
        assert!(!removes_dir, "should not remove used enum");
    }
}
