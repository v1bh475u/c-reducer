use slicer_core::context::CoverageData;
use slicer_core::pass::{Candidate, ReductionPass};
use slicer_parser::CParser;

use crate::util::extend_to_line;

#[derive(Debug, Default)]
pub struct DeadFunctionPass;

impl ReductionPass for DeadFunctionPass {
    fn apply(&self, source: &str, _coverage: Option<&CoverageData>) -> Vec<Candidate> {
        let parser = CParser::default();
        let unit = match parser.parse(source) {
            Ok(u) => u,
            Err(_) => return Vec::new(),
        };

        let mut candidates = Vec::new();
        let called_functions = unit.function_calls();

        for func in unit.functions() {
            if func.name == "main" {
                continue;
            }
            if called_functions.contains(&func.name) {
                continue;
            }
            let name_count = source.matches(&func.name).count();
            if name_count > 1 {
                continue;
            }
            let extended_range = extend_to_line(source, func.range);
            candidates.push(Candidate::removal(extended_range.to_range()));
        }

        candidates
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
        let pass = DeadFunctionPass;
        let candidates = pass.apply(source, None);
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
        let pass = DeadFunctionPass;
        let candidates = pass.apply(source, None);
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
        let pass = DeadFunctionPass;
        let candidates = pass.apply(source, None);
        assert!(candidates.is_empty());
    }
}
