use slicer_core::context::CoverageData;
use slicer_core::pass::{Candidate, ReductionPass};
use slicer_parser::{ByteRange, CParser, Declaration, DeclarationKind, StatementKind};

struct VarDecl {
    type_name: String,
    var_name: String,
    initializer: Option<String>,
}

#[derive(Debug, Default)]
pub struct StatementMergePass;

impl StatementMergePass {
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
            if value.is_empty() {
                None
            } else {
                Some(value.to_string())
            }
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
            if matches!(
                before,
                b'+' | b'-' | b'*' | b'/' | b'%' | b'&' | b'|' | b'^' | b'!' | b'<' | b'>'
            ) {
                return None;
            }
        }
        if trimmed.as_bytes().get(eq_pos + 1) == Some(&b'=') {
            return None;
        }

        let var_name = trimmed[..eq_pos].trim();
        if var_name.is_empty()
            || !var_name
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'_')
        {
            return None;
        }

        let value = trimmed[eq_pos + 1..].trim().strip_suffix(';')?;
        let value = value.trim();
        if value.is_empty() {
            return None;
        }

        Some((var_name.to_string(), value.to_string()))
    }

    fn extract_expr_body(text: &str) -> Option<&str> {
        let body = text.trim().strip_suffix(';')?;
        let body = body.trim();
        if body.is_empty() {
            None
        } else {
            Some(body)
        }
    }
}

impl ReductionPass for StatementMergePass {
    fn apply(&self, source: &str, _coverage: Option<&CoverageData>) -> Vec<Candidate> {
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
                        candidates.push(Candidate::new(range.to_range(), merged));
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
                                    candidates.push(Candidate::new(range.to_range(), merged));
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
                while j < stmts.len() && matches!(stmts[j].kind, StatementKind::Expression) {
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
                        candidates.push(Candidate::new(range.to_range(), merged));
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

    #[test]
    fn test_merge_same_type_declarations() {
        let source = "\nint main() {\n    int x;\n    int y;\n    return 0;\n}\n";
        let pass = StatementMergePass;
        let candidates = pass.apply(source, None);
        let c = candidates
            .iter()
            .find(|c| c.replacement.contains("int x, y;"))
            .expect("should find merge candidate");
        assert!(c.replacement.contains("int x, y;"));
    }

    #[test]
    fn test_merge_many_expressions() {
        let source = "\nint main() {\n    int a, b, c, d;\n    a = 1;\n    b = 2;\n    c = 3;\n    d = 4;\n    return 0;\n}\n";
        let pass = StatementMergePass;
        let candidates = pass.apply(source, None);
        let c = candidates
            .iter()
            .find(|c| c.replacement.contains("a = 1, b = 2, c = 3, d = 4;"))
            .expect("should merge all 4 expressions");
        assert!(c.replacement.contains("a = 1, b = 2, c = 3, d = 4;"));
    }

    #[test]
    fn test_parse_assignment_rejects_compound() {
        assert!(StatementMergePass::parse_assignment("x += 1;").is_none());
        assert!(StatementMergePass::parse_assignment("x == 1;").is_none());
    }
}
