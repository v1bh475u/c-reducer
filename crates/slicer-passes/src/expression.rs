//! Expression Simplification Pass
//!
//! This pass simplifies expressions using libclang AST:
//! 1. Replace complex expressions with constants
//! 2. Simplify arithmetic
//! 3. Remove unnecessary parentheses

use slicer_core::pass::{Candidate, ReductionPass};
use slicer_parser::{CParser, ExpressionKind};
use tracing::trace;

#[derive(Debug, Default)]
pub struct ExpressionSimplifyPass;

impl ExpressionSimplifyPass {
    pub fn new() -> Self {
        Self
    }
}

impl ReductionPass for ExpressionSimplifyPass {
    fn name(&self) -> &'static str {
        "expression"
    }

    fn priority(&self) -> u32 {
        30
    }

    fn apply(&self, source: &str, _context: &slicer_core::context::Context) -> Vec<Candidate> {
        let parser = CParser::default();
        let unit = match parser.parse(source) {
            Ok(u) => u,
            Err(_) => return Vec::new(),
        };

        let mut candidates = Vec::new();
        let header_end = unit.header_end();

        for expr in unit.expressions() {
            if expr.range.start < header_end {
                continue;
            }

            let text = match expr.range.extract(source) {
                Some(t) => t,
                None => continue,
            };

            match &expr.kind {
                ExpressionKind::Binary { operator } => {
                    if operator == "=" {
                        continue;
                    }

                    let range = expr.range.to_range();
                    candidates.push(
                        Candidate::new(range.clone(), "0".to_string())
                            .with_description(format!("simplify binary '{}' to 0", text)),
                    );
                    candidates.push(
                        Candidate::new(range.clone(), "1".to_string())
                            .with_description(format!("simplify binary '{}' to 1", text)),
                    );

                    trace!("binary expression candidate: {}", text);
                }
                ExpressionKind::Paren => {
                    if text.len() > 2 {
                        let inner = &text[1..text.len() - 1];
                        if inner.chars().all(|c| c.is_alphanumeric() || c == '_') {
                            candidates.push(
                                Candidate::new(expr.range.to_range(), inner.to_string())
                                    .with_description(format!("remove parens: {}", text)),
                            );
                            trace!("paren simplification: {}", text);
                        }
                    }
                }
                ExpressionKind::Call { function, .. } => {
                    if function == "main" || function == "printf" {
                        continue;
                    }

                    candidates.push(
                        Candidate::new(expr.range.to_range(), "0".to_string())
                            .with_description(format!("simplify call '{}' to 0", function)),
                    );

                    trace!("call expression candidate: {}", function);
                }
                ExpressionKind::Unary { .. } => {
                    candidates.push(
                        Candidate::new(expr.range.to_range(), "0".to_string())
                            .with_description(format!("simplify unary '{}' to 0", text)),
                    );
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
    fn test_simplify_binary_to_constant() {
        let source = "int main() { return 1 + 2; }";

        let pass = ExpressionSimplifyPass::new();
        let candidates = pass.apply(source, &ctx());
        assert!(!candidates.is_empty());
    }

    #[test]
    fn test_simplify_parens() {
        // Use a valid program with x declared
        let source = "int main() { int x = 1; return (x); }";

        let pass = ExpressionSimplifyPass::new();
        let _ = pass.apply(source, &ctx());
    }
}
