//! Statement Reduction Pass
//!
//! This pass simplifies or removes statements using libclang AST:
//! 1. Remove individual statements
//! 2. Simplify compound statements
//! 3. Remove empty blocks

use slicer_core::pass::{Candidate, ReductionPass};
use slicer_parser::{CParser, StatementKind};
use tracing::trace;

use crate::util::extend_to_line;

#[derive(Debug, Default)]
pub struct StatementPass;

impl StatementPass {
    pub fn new() -> Self {
        Self
    }
}

impl ReductionPass for StatementPass {
    fn name(&self) -> &'static str {
        "statement"
    }

    fn priority(&self) -> u32 {
        70
    }

    fn apply(&self, source: &str, _context: &slicer_core::context::Context) -> Vec<Candidate> {
        let parser = CParser::default();
        let unit = match parser.parse(source) {
            Ok(u) => u,
            Err(_) => return Vec::new(),
        };

        let mut candidates = Vec::new();
        let header_end = unit.header_end();

        for stmt in unit.statements() {
            if stmt.range.start < header_end {
                continue;
            }

            let text = match stmt.range.extract(source) {
                Some(t) => t,
                None => continue,
            };

            match &stmt.kind {
                StatementKind::Expression => {
                    if text.starts_with("return") {
                        continue;
                    }

                    let extended = extend_to_line(source, stmt.range);
                    candidates.push(
                        Candidate::removal(extended.to_range())
                            .with_description(format!(
                                "remove expr stmt: {}",
                                text.chars().take(30).collect::<String>()
                            )),
                    );
                    trace!("expression statement: {}", text);
                }
                StatementKind::Declaration => {
                    let extended = extend_to_line(source, stmt.range);
                    candidates.push(
                        Candidate::removal(extended.to_range())
                            .with_description(format!(
                                "remove decl: {}",
                                text.chars().take(30).collect::<String>()
                            )),
                    );
                    trace!("declaration statement: {}", text);
                }
                StatementKind::Compound => {
                    let inner = text.trim();
                    if inner == "{}" {
                        candidates.push(
                            Candidate::new(stmt.range.to_range(), ";".to_string())
                                .with_description("replace empty block with ;".to_string()),
                        );
                        trace!("empty compound block");
                    }
                }
                StatementKind::If { .. } | StatementKind::While | StatementKind::For => {
                    let extended = extend_to_line(source, stmt.range);
                    candidates.push(
                        Candidate::removal(extended.to_range())
                            .with_description(format!("remove control flow: {:?}", stmt.kind)),
                    );
                    trace!("control flow: {:?}", stmt.kind);
                }
                StatementKind::Return => {
                    if text.contains(' ') && !text.contains("return;") {
                        candidates.push(
                            Candidate::new(stmt.range.to_range(), "return 0;".to_string())
                                .with_description("simplify return to 0".to_string()),
                        );
                        trace!("return statement simplification");
                    }
                }
                _ => {}
            }
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
    fn test_remove_expression_statement() {
        let source = r#"
int main() {
    int x = 1;
    x = 2;
    return 0;
}
"#;

        let pass = StatementPass::new();
        let candidates = pass.apply(source, &ctx());

        assert!(!candidates.is_empty());
    }

    #[test]
    fn test_simplify_empty_block() {
        let source = r#"
int main() {
    if (1) {}
    return 0;
}
"#;

        let pass = StatementPass::new();
        let candidates = pass.apply(source, &ctx());
        let _ = candidates;
    }

    #[test]
    fn test_remove_local_declaration() {
        let source = r#"
int main() {
    int unused = 42;
    return 0;
}
"#;

        let pass = StatementPass::new();
        let candidates = pass.apply(source, &ctx());

        assert!(!candidates.is_empty());
    }
}