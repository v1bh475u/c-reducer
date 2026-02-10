# Reduction Passes

## Overview

Ten passes implement the `ReductionPass` trait. Passes run sequentially in the order returned by `all_passes()`. The pipeline iterates all passes up to `max_iterations` times (or until `total_timeout` is reached) until no more progress is made.

Three passes (`UnusedVariablePass`, `ArgumentRemovalPass`, and `HeaderRemovalPass`) use **clangd** LSP diagnostics for analysis. One pass (`DeadCodePass`) requires **coverage data** from gcov. The remaining passes use libclang parsing or text-based identifier counting.

## Pass Summary

| # | Pass | Description | Requires |
|---|------|-------------|----------|
| 1 | `DeadFunctionPass` | Removes uncalled functions | Parser |
| 2 | `DeadCodePass` | Removes unexecuted code | Coverage data |
| 3 | `UnusedVariablePass` | Removes unused variables | clangd |
| 4 | `ArgumentRemovalPass` | Removes unused params + updates call sites | clangd + Parser |
| 5 | `GlobalRemovalPass` | Removes unreferenced global variables | Identifier counting |
| 6 | `StructMemberRemovalPass` | Removes unaccessed struct fields | Access pattern search |
| 7 | `StatementMergePass` | Merges consecutive statements | Parser |
| 8 | `TypedefPass` | Removes typedef declarations | Text scan |
| 9 | `EnumStructRemovalPass` | Removes unused enum/struct declarations | Identifier counting |
| 10 | `HeaderRemovalPass` | Removes unused `#include` directives | clangd |

## Pass Implementations

### DeadFunctionPass (#1)

Removes functions that are never called.

Skips:
- `main` function
- Functions that appear in `function_calls()`
- Functions whose name appears more than once in source (possible function pointer references)

### DeadCodePass (#2)

Removes statements that were never executed according to gcov coverage data.

Requires coverage data — returns no candidates without it. Only considers statements after `header_end` (inside function bodies). Uses `LineIndex` for O(log N) byte-offset-to-line lookups. Merges adjacent dead statements into single block-removal candidates.

### UnusedVariablePass (#3)

Uses clangd LSP diagnostics to find and remove unused variables.

Leverages clangd's `-Wunused-variable` and `-Wunused-but-set-variable` diagnostic codes. For set-but-unused variables, also removes assignment statements to those variables.

### ArgumentRemovalPass (#4)

Uses clangd's `-Wunused-parameter` diagnostic to identify unused function parameters, then removes them from both the function signature and all call sites.

For each unused parameter:
1. Removes the parameter from the function definition's parameter list
2. Finds all call sites using text search with word-boundary checks
3. Removes the corresponding argument from each call
4. Emits a single whole-source rewrite candidate

Skips `main` and functions without definitions.

### GlobalRemovalPass (#5)

Removes global variable declarations that are never referenced.

Counts whole-word identifier occurrences across the entire source. If a global variable's name appears exactly once (its declaration), it's unused. Only targets `VarDecl` nodes before `header_end`.

### StructMemberRemovalPass (#6)

Removes struct fields that are never accessed.

For each field in each struct, searches the source for `.field_name` and `->field_name` access patterns with word-boundary checking to avoid substring matches. If neither pattern is found, the field is removed.

### StatementMergePass (#7)

Merges consecutive statements to reduce code size.

Transformations:
- **Same-type declarations**: `int x; int y;` → `int x, y;`
- **Declaration + assignment**: `int x; x = 42;` → `int x = 42;`
- **Consecutive expressions**: `a = 1; b = 2;` → `a = 1, b = 2;`

### TypedefPass (#8)

Removes `typedef` declarations.

Simple text-based scan. Generates removal candidates for every `typedef` line. The oracle validates whether each removal is safe.

### EnumStructRemovalPass (#9)

Removes unused `enum` and `struct` type declarations.

Counts whole-word identifier occurrences of the type name. If it appears only once (the declaration itself), generates a removal candidate. Handles trailing semicolons that may not be included in the parser range.

### HeaderRemovalPass (#10)

Removes `#include` directives flagged as unused by clangd.

Uses clangd's `unused-includes` diagnostic for precise detection. Only removes includes that clangd confirms are unused, then the oracle provides an additional safety check.

## Utility Modules

### util.rs

```rust
pub struct LineIndex { ... }

impl LineIndex {
    pub fn new(source: &str) -> Self;       // O(N) construction
    pub fn line_of(&self, offset: usize) -> usize;  // O(log N) lookup, 1-based
}

pub fn extend_to_line(source: &str, range: ByteRange) -> ByteRange;
```

### clangd.rs

LSP client for clangd diagnostics:

```rust
pub struct Diagnostic {
    pub line: usize,
    pub code: String,
    pub message: String,
}

pub fn get_diagnostics(source: &str, extra_flags: &[&str]) -> Vec<Diagnostic>;
pub fn lines_with_code(source: &str, code: &str, extra_flags: &[&str]) -> HashSet<usize>;
```

## Adding a Custom Pass

```rust
use slicer_core::{Candidate, CoverageData, ReductionPass};

pub struct MyPass;

impl ReductionPass for MyPass {
    fn apply(&self, source: &str, coverage: Option<&CoverageData>) -> Vec<Candidate> {
        vec![]
    }
}

let mut passes = slicer_passes::all_passes();
passes.push(Box::new(MyPass));
let pipeline = Pipeline::new(passes, 100);
```
