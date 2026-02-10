use slicer_core::context::CoverageData;
use slicer_core::pass::{Candidate, ReductionPass};
use slicer_parser::CParser;

use crate::util::extend_to_line;

#[derive(Debug, Default)]
pub struct StructMemberRemovalPass;

impl ReductionPass for StructMemberRemovalPass {
    fn apply(&self, source: &str, _coverage: Option<&CoverageData>) -> Vec<Candidate> {
        let parser = CParser::default();
        let unit = match parser.parse(source) {
            Ok(u) => u,
            Err(_) => return Vec::new(),
        };

        let mut candidates = Vec::new();

        for field in unit.struct_fields() {
            if field.name.is_empty() {
                continue;
            }

            // Check for `.field_name` or `->field_name` access patterns
            let dot_access = format!(".{}", field.name);
            let arrow_access = format!("->{}", field.name);

            let has_dot = has_identifier_access(source, &dot_access, &field.name);
            let has_arrow = has_identifier_access(source, &arrow_access, &field.name);

            if !has_dot && !has_arrow {
                let end = find_field_end(source, field.range.end);
                let range_with_semi = slicer_parser::ByteRange::new(field.range.start, end);
                let extended = extend_to_line(source, range_with_semi);
                candidates.push(Candidate::removal(extended.to_range()));
            }
        }

        candidates
    }
}

fn has_identifier_access(source: &str, pattern: &str, _field_name: &str) -> bool {
    let mut search_from = 0;
    while let Some(pos) = source[search_from..].find(pattern) {
        let abs_pos = search_from + pos;
        let after_pos = abs_pos + pattern.len();
        // Check that the field name ends at a word boundary
        let after_ok = after_pos >= source.len() || {
            let b = source.as_bytes()[after_pos];
            !b.is_ascii_alphanumeric() && b != b'_'
        };
        if after_ok {
            return true;
        }
        search_from = abs_pos + 1;
    }
    false
}

fn find_field_end(source: &str, from: usize) -> usize {
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
    fn test_remove_unused_field() {
        let source = "\
struct Point {
    int x;
    int y;
    int unused_z;
};

int main() {
    struct Point p;
    p.x = 1;
    p.y = 2;
    return p.x + p.y;
}
";
        let pass = StructMemberRemovalPass;
        let candidates = pass.apply(source, None);
        let removes_z = candidates.iter().any(|c| {
            c.apply(source)
                .map(|r| !r.contains("unused_z") && r.contains("int x") && r.contains("int y"))
                .unwrap_or(false)
        });
        assert!(removes_z, "should remove unused field unused_z");
    }

    #[test]
    fn test_keep_used_fields() {
        let source = "\
struct Vec2 {
    int x;
    int y;
};

int main() {
    struct Vec2 v;
    v.x = 10;
    v.y = 20;
    return v.x + v.y;
}
";
        let pass = StructMemberRemovalPass;
        let candidates = pass.apply(source, None);
        assert!(
            candidates.is_empty(),
            "should not remove any used fields, got {} candidates",
            candidates.len()
        );
    }

    #[test]
    fn test_remove_arrow_accessed_field() {
        let source = "\
struct Node {
    int value;
    int unused_flag;
    struct Node *next;
};

int main() {
    struct Node n;
    n.value = 42;
    n.next = 0;
    return n.value;
}
";
        let pass = StructMemberRemovalPass;
        let candidates = pass.apply(source, None);
        let removes_flag = candidates.iter().any(|c| {
            c.apply(source)
                .map(|r| !r.contains("unused_flag"))
                .unwrap_or(false)
        });
        assert!(removes_flag, "should remove unused field unused_flag");
    }

    #[test]
    fn test_no_false_positive_substring() {
        // Field named "x" — make sure "xy" doesn't count as access to "x"
        let source = "\
struct S {
    int x;
    int xy;
};

int main() {
    struct S s;
    s.xy = 1;
    return s.xy;
}
";
        let pass = StructMemberRemovalPass;
        let candidates = pass.apply(source, None);
        let removes_x = candidates.iter().any(|c| {
            c.apply(source)
                .map(|r| !r.contains("int x;"))
                .unwrap_or(false)
        });
        assert!(
            removes_x,
            "should remove unused field x (xy access should not count)"
        );
        let removes_xy = candidates.iter().any(|c| {
            c.apply(source)
                .map(|r| !r.contains("int xy;"))
                .unwrap_or(false)
        });
        assert!(!removes_xy, "should NOT remove used field xy");
    }
}
