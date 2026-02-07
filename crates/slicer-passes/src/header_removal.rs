use slicer_core::context::CoverageData;
use slicer_core::pass::{Candidate, ReductionPass};

use crate::clangd;
use crate::util::extend_to_line;

#[derive(Debug, Default)]
pub struct HeaderRemovalPass;

impl ReductionPass for HeaderRemovalPass {
    fn apply(&self, source: &str, _coverage: Option<&CoverageData>) -> Vec<Candidate> {
        let unused_lines = clangd::lines_with_code(source, "unused-includes", &[]);

        let mut candidates = Vec::new();
        let mut offset = 0;

        for (line_num, line) in source.lines().enumerate() {
            let trimmed = line.trim();
            let is_include =
                trimmed.starts_with("#include ") || trimmed.starts_with("#include\t");

            if is_include && unused_lines.contains(&line_num) {
                let line_end = offset + line.len();
                let range = slicer_parser::ByteRange::new(offset, line_end);
                let extended = extend_to_line(source, range);
                candidates.push(Candidate::removal(extended.to_range()));
            }
            offset += line.len() + 1;
        }

        candidates
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_find_unused_includes() {
        let unused = clangd::lines_with_code(
            "#include <stdio.h>\n#include <stdlib.h>\n#include <string.h>\n\nint main() {\n    printf(\"hi\");\n    return 0;\n}\n",
            "unused-includes",
            &[],
        );
        assert!(
            unused.contains(&1) || unused.contains(&2),
            "clangd should flag stdlib.h or string.h as unused, got: {:?}",
            unused
        );
        assert!(
            !unused.contains(&0),
            "stdio.h is used (printf), should not be flagged"
        );
    }

    #[test]
    fn test_only_removes_unused() {
        let source = "#include <stdio.h>\n#include <stdlib.h>\n\nint main() {\n    printf(\"hi\");\n    return 0;\n}\n";
        let pass = HeaderRemovalPass;
        let candidates = pass.apply(source, None);
        for c in &candidates {
            let result = c.apply(source).expect("should apply");
            assert!(
                result.contains("#include <stdio.h>"),
                "should not remove used stdio.h"
            );
        }
    }
}
