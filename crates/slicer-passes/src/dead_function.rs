//! Dead Function Removal Pass
//!
//! This pass removes functions that are never called.
//! It identifies functions that:
//! 1. Are not `main`
//! 2. Are not called anywhere in the program
//! 3. Are not referenced (e.g., function pointers)

use slicer_core::pass::{Candidate, ReductionPass};
use slicer_parser::CParser;
use tracing::trace;

use crate::util::extend_to_line;

#[derive(Debug, Default)]
pub struct DeadFunctionPass;

impl DeadFunctionPass {
    pub fn new() -> Self {
        Self
    }
}

impl ReductionPass for DeadFunctionPass {
    fn name(&self) -> &'static str {
        "dead_function"
    }

    fn priority(&self) -> u32 {
        100
    }

    fn apply(&self, source: &str, _context: &slicer_core::context::Context) -> Vec<Candidate> {
        let parser = CParser::default();
        let unit = match parser.parse(source) {
            Ok(u) => u,
            Err(_) => return Vec::new(),
        };

        let mut candidates = Vec::new();
        let called_functions = unit.function_calls();
        let functions = unit.functions();

        for func in functions {
            if func.name == "main" {
                continue;
            }

            if called_functions.contains(&func.name) {
                continue;
            }

            // Skip functions referenced elsewhere (potential function pointers)
            let name_count = source.matches(&func.name).count();
            if name_count > 1 {
                trace!("skipping {} - appears {} times", func.name, name_count);
                continue;
            }

            let extended_range = extend_to_line(source, func.range);

            candidates.push(
                Candidate::removal(extended_range.to_range())
                    .with_description(format!("remove unused function '{}'", func.name)),
            );
            trace!("dead function candidate: {}", func.name);
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
    fn test_remove_uncalled_function() {
        let source = r#"
void unused() {
    int x = 1;
}

int main() {
    return 0;
}
"#;

        let pass = DeadFunctionPass::new();
        let candidates = pass.apply(source, &ctx());
        let result = candidates[0].apply(source).expect("should apply");
        assert!(!result.contains("unused"));
        assert!(result.contains("main"));
    }

    #[test]
    fn test_keep_called_function() {
        let source = r#"
int helper() {
    return 42;
}

int main() {
    return helper();
}
"#;

        let pass = DeadFunctionPass::new();
        let candidates = pass.apply(source, &ctx());

        // helper is called, so no candidates should remove it
        assert!(
            candidates.is_empty()
                || candidates.iter().all(|c| {
                    c.apply(source)
                        .map(|r| r.contains("helper"))
                        .unwrap_or(true)
                })
        );
    }

    #[test]
    fn test_never_remove_main() {
        let source = "int main() { return 0; }";

        let pass = DeadFunctionPass::new();
        let candidates = pass.apply(source, &ctx());
        assert!(candidates.is_empty());
    }
}
