# Reduction Passes

## Overview

Six passes implement the `ReductionPass` trait. Passes run sequentially in the order returned by `all_passes()`. The pipeline iterates all passes up to `max_iterations` times until no more progress is made.

Two passes (`UnusedVariablePass` and `HeaderRemovalPass`) use **clangd** LSP diagnostics for analysis. One pass (`DeadCodePass`) requires **coverage data** from gcov.

## Pass Summary

| # | Pass | Description | Requires |
|---|------|-------------|----------|
| 1 | `DeadFunctionPass` | Removes uncalled functions | Parser |
| 2 | `DeadCodePass` | Removes unexecuted code | Coverage data |
| 3 | `UnusedVariablePass` | Removes unused variables | clangd |
| 4 | `StatementMergePass` | Merges consecutive statements | Parser |
| 5 | `TypedefPass` | Removes typedef declarations | Text scan |
| 6 | `HeaderRemovalPass` | Removes unused `#include` directives | clangd |

## Pass Implementations

### DeadFunctionPass (#1)

Removes functions that are never called.

```rust
impl ReductionPass for DeadFunctionPass {
    fn apply(&self, source: &str, _coverage: Option<&CoverageData>) -> Vec<Candidate> {
        let unit = parser.parse(source);
        let called = unit.function_calls();

        unit.functions()
            .filter(|f| f.name != "main"
                && !called.contains(&f.name)
                && count_occurrences(source, &f.name) == 1)
            .map(|f| Candidate::removal(extend_to_line(source, &f.range).to_range()))
            .collect()
    }
}
```

Skips:
- `main` function
- Functions that appear in `function_calls()`
- Functions whose name appears more than once in source (possible function pointer references)

### DeadCodePass (#2)

Removes statements that were never executed according to gcov coverage data.

```rust
impl ReductionPass for DeadCodePass {
    fn apply(&self, source: &str, coverage: Option<&CoverageData>) -> Vec<Candidate> {
        // Returns empty if coverage is None
        let coverage = coverage?;
        let unit = parser.parse(source);

        // Find statements past header_end with 0 execution count
        // Merge contiguous dead ranges into block removals
    }
}
```

**Requires coverage data** — returns no candidates without it. Only considers statements after `header_end` (inside function bodies). Merges adjacent dead statements into single block-removal candidates for efficiency.

### UnusedVariablePass (#3)

Uses clangd LSP diagnostics to find and remove unused variables.

```rust
impl ReductionPass for UnusedVariablePass {
    fn apply(&self, source: &str, _coverage: Option<&CoverageData>) -> Vec<Candidate> {
        // Query clangd for -Wunused-variable and -Wunused-but-set-variable
        let unused_lines = lines_with_code(source, "-Wunused-variable", &[]);
        let unused_set_lines = lines_with_code(source, "-Wunused-but-set-variable", &[]);

        // For each flagged line:
        //   - If it's a declaration statement → remove the line
        //   - If it's a simple assignment expression → remove the line
    }
}
```

Leverages clangd's `-Wunused-variable` and `-Wunused-but-set-variable` diagnostic codes for precise detection. No false positives from clangd's semantic analysis.

### StatementMergePass (#4)

Merges consecutive statements to reduce code size.

```rust
impl ReductionPass for StatementMergePass {
    fn apply(&self, source: &str, _coverage: Option<&CoverageData>) -> Vec<Candidate> {
        let unit = parser.parse(source);
        let stmts = unit.statements();

        // Three merge strategies applied to consecutive statement pairs
    }
}
```

Transformations:
- **Same-type declarations**: `int x; int y;` → `int x, y;`
- **Declaration + assignment**: `int x; x = 42;` → `int x = 42;`
- **Consecutive expressions**: `a = 1; b = 2;` → `a = 1, b = 2;`

### TypedefPass (#5)

Removes `typedef` declarations.

```rust
impl ReductionPass for TypedefPass {
    fn apply(&self, source: &str, _coverage: Option<&CoverageData>) -> Vec<Candidate> {
        source.lines()
            .enumerate()
            .filter(|(_, line)| line.trim().starts_with("typedef "))
            .map(|(line_num, _)| Candidate::removal(line_range))
            .collect()
    }
}
```

Simple text-based scan. Generates removal candidates for every `typedef` line. The oracle validates whether each removal is safe (i.e., the typedef is actually unused).

### HeaderRemovalPass (#6)

Removes `#include` directives flagged as unused by clangd.

```rust
impl ReductionPass for HeaderRemovalPass {
    fn apply(&self, source: &str, _coverage: Option<&CoverageData>) -> Vec<Candidate> {
        // Query clangd for unused-includes diagnostics
        let unused_lines = lines_with_code(source, "unused-includes", &[]);

        // Generate removal candidates for #include lines on flagged lines
    }
}
```

Uses clangd's `unused-includes` diagnostic for precise detection. Only removes includes that clangd confirms are unused, then the oracle provides an additional safety check.

## Utility Modules

### util.rs

```rust
/// Extends a ByteRange to cover complete lines (including trailing newline).
pub fn extend_to_line(source: &str, range: &ByteRange) -> ByteRange;
```

### clangd.rs

Full LSP client for clangd diagnostics:

```rust
pub struct Diagnostic {
    pub line: usize,
    pub code: String,
    pub message: String,
}

/// Spawn clangd, open a source file, and collect all diagnostics.
pub fn get_diagnostics(source: &str, extra_flags: &[&str]) -> Vec<Diagnostic>;

/// Get line numbers that have a specific diagnostic code.
pub fn lines_with_code(source: &str, code: &str, extra_flags: &[&str]) -> HashSet<usize>;
```

## Adding a Custom Pass

```rust
use slicer_core::{Candidate, CoverageData, ReductionPass};

pub struct MyPass;

impl ReductionPass for MyPass {
    fn apply(&self, source: &str, coverage: Option<&CoverageData>) -> Vec<Candidate> {
        // Generate candidates
        vec![]
    }
}

// Add to pipeline
let mut passes = slicer_passes::all_passes();
passes.push(Box::new(MyPass));
let pipeline = Pipeline::new(passes, 100);
```
