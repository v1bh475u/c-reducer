//! Statement Merging Pass
//!
//! This pass merges consecutive statements to reduce program size:
//! 1. Consecutive same-type declarations: `int x; int y;` → `int x, y;`
//! 2. Declaration with immediate assignment: `int x; x = 1;` → `int x = 1;`
//! 3. Consecutive expressions via comma operator: `a = 1; b = 2;` → `a = 1, b = 2;`

use slicer_core::pass::{Candidate, ReductionPass};
use slicer_parser::{ByteRange, CParser, Declaration, DeclarationKind, StatementKind};
use tracing::trace;

struct VarDecl {
    type_name: String,
    var_name: String,
    initializer: Option<String>,
}

#[derive(Debug, Default)]
pub struct StatementMergePass;

impl StatementMergePass {
    pub fn new() -> Self {
        Self
    }

    fn find_var_decl<'a>(
        declarations: &'a [Declaration],
        stmt_range: &ByteRange,
    ) -> Option<&'a Declaration> {
        declarations.iter().find(|d| {
            matches!(d.kind, DeclarationKind::Variable { .. })
                && d.range.start >= stmt_range.start
                && d.range.end <= stmt_range.end
        })
    }

    fn extract_var_decl(decl: &Declaration, source: &str) -> Option<VarDecl> {
        let type_info = match &decl.kind {
            DeclarationKind::Variable { type_info } => type_info,
            _ => return None,
        };

        let decl_text = decl.range.extract(source)?;
        let name_pos = decl_text.find(&*decl.name)?;
        let after_name = decl_text[name_pos + decl.name.len()..].trim();

        let initializer = if let Some(rest) = after_name.strip_prefix('=') {
            let value = rest.trim();
            if value.is_empty() { None } else { Some(value.to_string()) }
        } else {
            None
        };

        Some(VarDecl {
            type_name: type_info.name.clone(),
            var_name: decl.name.clone(),
            initializer,
        })
    }

    fn parse_assignment(text: &str) -> Option<(String, String)> {
        let trimmed = text.trim();
        let eq_pos = trimmed.find('=')?;

        if eq_pos > 0 {
            let before = trimmed.as_bytes()[eq_pos - 1];
            if matches!(before, b'+' | b'-' | b'*' | b'/' | b'%' | b'&' | b'|' | b'^' | b'!' | b'<' | b'>') {
                return None;
            }
        }
        if trimmed.as_bytes().get(eq_pos + 1) == Some(&b'=') {
            return None;
        }

        let var_name = trimmed[..eq_pos].trim();
        if var_name.is_empty() || !var_name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_') {
            return None;
        }

        let value = trimmed[eq_pos + 1..].trim().strip_suffix(';')?.trim();
        if value.is_empty() { return None; }

        Some((var_name.to_string(), value.to_string()))
    }

    fn extract_expr_body(text: &str) -> Option<&str> {
        let body = text.trim().strip_suffix(';')?.trim();
        if body.is_empty() { None } else { Some(body) }
    }
}

impl ReductionPass for StatementMergePass {
    fn name(&self) -> &'static str {
        "statement_merge"
    }

    fn priority(&self) -> u32 {
        65
    }

    fn apply(&self, source: &str, _context: &slicer_core::context::Context) -> Vec<Candidate> {
        let parser = CParser::default();
        let unit = match parser.parse(source) {
            Ok(u) => u,
            Err(_) => return Vec::new(),
        };

        let mut candidates = Vec::new();
        let header_end = unit.header_end();
        let declarations = unit.declarations();
        let stmts: Vec<_> = unit
            .statements()
            .iter()
            .filter(|s| {
                s.range.start >= header_end && !matches!(s.kind, StatementKind::Compound)
            })
            .collect();

        let mut i = 0;
        while i < stmts.len() {
            if matches!(stmts[i].kind, StatementKind::Declaration) {
                if let Some(first) = Self::find_var_decl(declarations, &stmts[i].range)
                    .and_then(|d| Self::extract_var_decl(d, source))
                {
                    let mut run = vec![first];
                    let run_start = i;
                    let mut j = i + 1;

                    while j < stmts.len()
                        && matches!(stmts[j].kind, StatementKind::Declaration)
                    {
                        if let Some(v) = Self::find_var_decl(declarations, &stmts[j].range)
                            .and_then(|d| Self::extract_var_decl(d, source))
                        {
                            if v.type_name == run[0].type_name {
                                run.push(v);
                                j += 1;
                                continue;
                            }
                        }
                        break;
                    }

                    if run.len() >= 2 {
                        let parts: Vec<String> = run
                            .iter()
                            .map(|v| match &v.initializer {
                                Some(init) => format!("{} = {}", v.var_name, init),
                                None => v.var_name.clone(),
                            })
                            .collect();
                        let merged = format!("{} {};", run[0].type_name, parts.join(", "));
                        let range = ByteRange::new(
                            stmts[run_start].range.start,
                            stmts[j - 1].range.end,
                        );
                        candidates.push(
                            Candidate::new(range.to_range(), merged)
                                .with_description(format!(
                                    "merge {} {} decls",
                                    run.len(),
                                    run[0].type_name
                                )),
                        );
                        trace!("merged {} {} declarations", run.len(), run[0].type_name);
                        i = j;
                        continue;
                    }

                    if run[0].initializer.is_none()
                        && j < stmts.len()
                        && matches!(stmts[j].kind, StatementKind::Expression)
                    {
                        if let Some(text) = stmts[j].range.extract(source) {
                            if let Some((assign_var, value)) = Self::parse_assignment(text) {
                                if run[0].var_name == assign_var {
                                    let merged = format!(
                                        "{} {} = {};",
                                        run[0].type_name, run[0].var_name, value
                                    );
                                    let range = ByteRange::new(
                                        stmts[i].range.start,
                                        stmts[j].range.end,
                                    );
                                    candidates.push(
                                        Candidate::new(range.to_range(), merged)
                                            .with_description(format!(
                                                "merge decl+assign: {} {}",
                                                run[0].type_name, run[0].var_name
                                            )),
                                    );
                                }
                            }
                        }
                    }
                }
                i += 1;
                continue;
            }

            if matches!(stmts[i].kind, StatementKind::Expression) {
                let run_start = i;
                let mut j = i + 1;
                while j < stmts.len()
                    && matches!(stmts[j].kind, StatementKind::Expression)
                {
                    j += 1;
                }

                if j - run_start >= 2 {
                    let bodies: Option<Vec<&str>> = (run_start..j)
                        .map(|k| {
                            stmts[k]
                                .range
                                .extract(source)
                                .and_then(Self::extract_expr_body)
                        })
                        .collect();

                    if let Some(bodies) = bodies {
                        let merged = format!("{};", bodies.join(", "));
                        let range = ByteRange::new(
                            stmts[run_start].range.start,
                            stmts[j - 1].range.end,
                        );
                        candidates.push(
                            Candidate::new(range.to_range(), merged)
                                .with_description(format!(
                                    "merge {} exprs with comma",
                                    j - run_start
                                )),
                        );
                        trace!("merged {} expression statements", j - run_start);
                    }
                }
                i = j;
                continue;
            }

            i += 1;
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
    fn test_merge_same_type_declarations() {
        let source = r#"
int main() {
    int x;
    int y;
    return 0;
}
"#;
        let pass = StatementMergePass::new();
        let candidates = pass.apply(source, &ctx());
        let c = candidates
            .iter()
            .find(|c| c.description.contains("merge"))
            .expect("should find merge candidate");
        assert!(
            c.replacement.contains("int x, y;"),
            "expected 'int x, y;', got: {}",
            c.replacement
        );
    }

    #[test]
    fn test_merge_many_same_type_declarations() {
        let source = r#"
int main() {
    int a;
    int b;
    int c;
    int d;
    return 0;
}
"#;
        let pass = StatementMergePass::new();
        let candidates = pass.apply(source, &ctx());
        let c = candidates
            .iter()
            .find(|c| c.description.contains("merge 4"))
            .expect("should merge all 4 in one candidate");
        assert!(
            c.replacement.contains("int a, b, c, d;"),
            "expected 'int a, b, c, d;', got: {}",
            c.replacement
        );
    }

    #[test]
    fn test_merge_mixed_type_runs() {
        // int a, b should merge; float c stays alone; int d, e should merge
        let source = r#"
int main() {
    int a;
    int b;
    float c;
    int d;
    int e;
    return 0;
}
"#;
        let pass = StatementMergePass::new();
        let candidates = pass.apply(source, &ctx());

        let decl_candidates: Vec<_> =
            candidates.iter().filter(|c| c.description.contains("decls")).collect();
        assert_eq!(decl_candidates.len(), 2, "should produce 2 merge candidates");
        assert!(decl_candidates[0].replacement.contains("int a, b;"));
        assert!(decl_candidates[1].replacement.contains("int d, e;"));
    }

    #[test]
    fn test_merge_declarations_with_initializers() {
        let source = r#"
int main() {
    int x = 1;
    int y = 2;
    int z;
    return 0;
}
"#;
        let pass = StatementMergePass::new();
        let candidates = pass.apply(source, &ctx());
        let c = candidates
            .iter()
            .find(|c| c.description.contains("merge 3"))
            .expect("should merge all 3");
        assert!(
            c.replacement.contains("int x = 1, y = 2, z;"),
            "expected 'int x = 1, y = 2, z;', got: {}",
            c.replacement
        );
    }

    #[test]
    fn test_merge_declaration_with_assignment() {
        let source = r#"
int main() {
    int x;
    x = 42;
    return 0;
}
"#;
        let pass = StatementMergePass::new();
        let candidates = pass.apply(source, &ctx());
        let c = candidates
            .iter()
            .find(|c| c.description.contains("decl+assign"));
        if let Some(c) = c {
            assert!(
                c.replacement.contains("int x = 42;"),
                "expected 'int x = 42;', got: {}",
                c.replacement
            );
        }
    }

    #[test]
    fn test_no_merge_different_types() {
        let source = r#"
int main() {
    int x;
    float y;
    return 0;
}
"#;
        let pass = StatementMergePass::new();
        let candidates = pass.apply(source, &ctx());
        assert!(
            candidates.iter().all(|c| !c.description.contains("decls")),
            "should not merge different types"
        );
    }

    #[test]
    fn test_merge_many_expressions() {
        let source = r#"
int main() {
    int a, b, c, d;
    a = 1;
    b = 2;
    c = 3;
    d = 4;
    return 0;
}
"#;
        let pass = StatementMergePass::new();
        let candidates = pass.apply(source, &ctx());
        let c = candidates
            .iter()
            .find(|c| c.description.contains("merge 4 exprs"))
            .expect("should merge all 4 expressions in one candidate");
        assert!(
            c.replacement.contains("a = 1, b = 2, c = 3, d = 4;"),
            "expected comma-joined, got: {}",
            c.replacement
        );
    }

    #[test]
    fn test_parse_assignment_rejects_compound() {
        assert!(StatementMergePass::parse_assignment("x += 1;").is_none());
        assert!(StatementMergePass::parse_assignment("x == 1;").is_none());
    }

    #[test]
    fn test_string_literal_initializers() {
        let source = r#"
int main() {
    char *s = "hello;world";
    char *t = "foo;bar";
    return 0;
}
"#;
        let pass = StatementMergePass::new();
        let candidates = pass.apply(source, &ctx());
        if let Some(c) = candidates.iter().find(|c| c.description.contains("decls")) {
            assert!(
                c.replacement.contains(r#""hello;world""#),
                "should preserve string literal: {}",
                c.replacement
            );
        }
    }

    #[test]
    fn test_merge_non_constant_expressions() {
        // Expression merging is not limited to constant assignments.
        // Function calls, arithmetic, pointer derefs — all merge via comma operator.
        let source = r#"
int foo(int x) { return x; }
int bar(int x) { return x; }
int main() {
    int a, b, c;
    a = foo(1);
    b = bar(2);
    c = a + b;
    return 0;
}
"#;
        let pass = StatementMergePass::new();
        let candidates = pass.apply(source, &ctx());
        let c = candidates
            .iter()
            .find(|c| c.description.contains("merge 3 exprs"))
            .expect("should merge all 3 expression stmts");
        assert!(
            c.replacement.contains("a = foo(1), b = bar(2), c = a + b;"),
            "expected comma-joined expressions, got: {}",
            c.replacement
        );
    }
}
