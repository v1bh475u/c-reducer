use slicer_core::context::CoverageData;
use slicer_core::pass::{Candidate, ReductionPass};
use slicer_parser::{CParser, StatementKind};

use crate::clangd;
use crate::util::{extend_to_line, LineIndex};

#[derive(Debug, Default)]
pub struct UnusedVariablePass;

fn extract_var_name(message: &str) -> Option<String> {
    let start = message.find('\'')?;
    let end = message[start + 1..].find('\'')?;
    Some(message[start + 1..start + 1 + end].to_string())
}

fn is_simple_assignment_to(text: &str, var_name: &str) -> bool {
    let trimmed = text.trim().trim_end_matches(';').trim();
    if let Some(eq_pos) = trimmed.find('=') {
        if eq_pos == 0 {
            return false;
        }
        let before_eq = trimmed.as_bytes()[eq_pos - 1];
        if matches!(
            before_eq,
            b'+' | b'-' | b'*' | b'/' | b'%' | b'&' | b'|' | b'^' | b'!' | b'<' | b'>'
        ) {
            return false;
        }
        if trimmed.as_bytes().get(eq_pos + 1) == Some(&b'=') {
            return false;
        }
        let lhs = trimmed[..eq_pos].trim();
        lhs == var_name
    } else {
        false
    }
}

impl ReductionPass for UnusedVariablePass {
    fn apply(&self, source: &str, _coverage: Option<&CoverageData>) -> Vec<Candidate> {
        let diags = clangd::get_diagnostics(source, &["-Wall", "-Wextra"]);

        let unused_var_lines: std::collections::HashSet<usize> = diags
            .iter()
            .filter(|d| d.code == "-Wunused-variable" || d.code == "-Wunused-but-set-variable")
            .map(|d| d.line)
            .collect();

        let set_but_unused_names: std::collections::HashSet<String> = diags
            .iter()
            .filter(|d| d.code == "-Wunused-but-set-variable")
            .filter_map(|d| extract_var_name(&d.message))
            .collect();

        if unused_var_lines.is_empty() {
            return Vec::new();
        }

        let parser = CParser::default();
        let unit = match parser.parse(source) {
            Ok(u) => u,
            Err(_) => return Vec::new(),
        };

        let mut candidates = Vec::new();
        let line_index = LineIndex::new(source);

        for stmt in unit.statements() {
            let stmt_start_line = line_index.line_of(stmt.range.start) - 1; // clangd uses 0-based lines

            match stmt.kind {
                StatementKind::Declaration => {
                    if unused_var_lines.contains(&stmt_start_line) {
                        let extended = extend_to_line(source, stmt.range);
                        candidates.push(Candidate::removal(extended.to_range()));
                    }
                },
                StatementKind::Expression if !set_but_unused_names.is_empty() => {
                    if let Some(text) = stmt.range.extract(source) {
                        for var_name in &set_but_unused_names {
                            if is_simple_assignment_to(text, var_name) {
                                let extended = extend_to_line(source, stmt.range);
                                candidates.push(Candidate::removal(extended.to_range()));
                                break;
                            }
                        }
                    }
                },
                _ => {},
            }
        }

        candidates
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_remove_unused_variable() {
        let source = "int main() {\n    int x = 1;\n    int y = 2;\n    return x;\n}\n";
        let pass = UnusedVariablePass;
        let candidates = pass.apply(source, None);
        assert!(!candidates.is_empty(), "should find unused variable y");
        let has_y_removal = candidates.iter().any(|c| {
            c.apply(source)
                .map(|r| !r.contains("int y") && r.contains("int x"))
                .unwrap_or(false)
        });
        assert!(has_y_removal, "should remove y but keep x");
    }

    #[test]
    fn test_no_candidates_all_used() {
        let source = "#include <stdio.h>\nint main() {\n    int x = 1;\n    printf(\"%d\", x);\n    return 0;\n}\n";
        let pass = UnusedVariablePass;
        let candidates = pass.apply(source, None);
        let removes_x = candidates.iter().any(|c| {
            c.apply(source)
                .map(|r| !r.contains("int x"))
                .unwrap_or(false)
        });
        assert!(!removes_x, "should not remove used variable x");
    }

    #[test]
    fn test_remove_set_but_unused() {
        let source = "int main() {\n    int x = 1;\n    int y = 2;\n    y = 3;\n    return x;\n}\n";
        let pass = UnusedVariablePass;
        let candidates = pass.apply(source, None);
        let has_y_decl_removal = candidates.iter().any(|c| {
            c.apply(source)
                .map(|r| !r.contains("int y"))
                .unwrap_or(false)
        });
        assert!(
            has_y_decl_removal,
            "should remove set-but-unused variable y declaration"
        );
        let has_y_assign_removal = candidates.iter().any(|c| {
            c.apply(source)
                .map(|r| !r.contains("y = 3"))
                .unwrap_or(false)
        });
        assert!(
            has_y_assign_removal,
            "should remove assignment to set-but-unused variable y"
        );
    }

    #[test]
    fn test_extract_var_name() {
        assert_eq!(
            extract_var_name("Variable 'foo' set but not used"),
            Some("foo".to_string())
        );
        assert_eq!(
            extract_var_name("Unused variable 'bar'"),
            Some("bar".to_string())
        );
        assert_eq!(extract_var_name("no quotes here"), None);
    }

    #[test]
    fn test_is_simple_assignment() {
        assert!(is_simple_assignment_to("x = 99;", "x"));
        assert!(is_simple_assignment_to("  x = 99;  ", "x"));
        assert!(!is_simple_assignment_to("x += 1;", "x"));
        assert!(!is_simple_assignment_to("x == 1;", "x"));
        assert!(!is_simple_assignment_to("y = 99;", "x"));
    }
}
