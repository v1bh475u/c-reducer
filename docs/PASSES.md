# Reduction Passes

## Overview

Seven passes implement the `ReductionPass` trait. Passes run serially in priority order (higher priority first).

## Pass Summary

| Pass | Priority | Description |
|------|----------|-------------|
| `dead_function` | 100 | Removes uncalled functions |
| `dead_code` | 90 | Removes dead code (post-return, unreachable) |
| `statement` | 70 | Removes individual statements |
| `statement_merge` | 65 | Merges consecutive statements |
| `include` | 60 | Removes #include directives |
| `typedef` | 50 | Removes unused typedefs |
| `expression` | 30 | Simplifies expressions |

## Pass Implementations

### DeadFunctionPass (Priority 100)

Removes functions that are never called.

```rust
impl ReductionPass for DeadFunctionPass {
    fn name(&self) -> &'static str { "dead_function" }
    fn priority(&self) -> u32 { 100 }

    fn apply(&self, source: &str) -> Vec<Candidate> {
        let unit = parser.parse(source)?;
        let called = unit.function_calls();
        
        unit.functions()
            .filter(|f| f.name != "main" && !called.contains(&f.name))
            .map(|f| Candidate::removal(f.range)
                .with_description(format!("remove unused function '{}'", f.name)))
            .collect()
    }
}
```

Skips:
- `main` function
- Functions that are called
- Functions referenced elsewhere (possible function pointers)

### DeadCodePass (Priority 90)

Removes unreachable code:
- Code after return statements
- Unreachable branches (`if(0)`, always-false conditions)

```rust
impl ReductionPass for DeadCodePass {
    fn name(&self) -> &'static str { "dead_code" }
    fn priority(&self) -> u32 { 90 }

    fn apply(&self, source: &str) -> Vec<Candidate> {
        let mut candidates = Vec::new();
        candidates.extend(self.find_post_return_code(source));
        candidates.extend(self.find_unreachable_branches(source));
        candidates
    }
}
```

### StatementPass (Priority 70)

Removes individual statements.

```rust
impl ReductionPass for StatementPass {
    fn name(&self) -> &'static str { "statement" }
    fn priority(&self) -> u32 { 70 }

    fn apply(&self, source: &str) -> Vec<Candidate> {
        let unit = parser.parse(source)?;
        
        unit.statements()
            .filter(|s| !s.text.starts_with("return"))
            .map(|s| Candidate::removal(s.range)
                .with_description("remove statement"))
            .collect()
    }
}
```

Skips:
- Return statements
- Statements in header region

### StatementMergePass (Priority 65)

Merges consecutive statements to reduce code size:

```rust
impl ReductionPass for StatementMergePass {
    fn name(&self) -> &'static str { "statement_merge" }
    fn priority(&self) -> u32 { 65 }

    fn apply(&self, source: &str) -> Vec<Candidate> {
        // Merge consecutive declarations of same type:
        // int x; int y; → int x, y;
        
        // Merge declaration with immediate assignment:
        // int x; x = 1; → int x = 1;
    }
}
```

Transformations:
- `int x; int y;` → `int x, y;`
- `int x = 1; int y = 2;` → `int x = 1, y = 2;`
- `int x; x = 42;` → `int x = 42;`

### IncludePass (Priority 60)

Removes `#include` directives.

```rust
impl ReductionPass for IncludePass {
    fn name(&self) -> &'static str { "include" }
    fn priority(&self) -> u32 { 60 }

    fn apply(&self, source: &str) -> Vec<Candidate> {
        source.lines()
            .enumerate()
            .filter(|(_, line)| line.trim().starts_with("#include"))
            .map(|(line_num, line)| {
                let range = calculate_line_range(source, line_num);
                Candidate::removal(range)
                    .with_description(format!("remove {}", line.trim()))
            })
            .collect()
    }
}
```

### TypedefPass (Priority 50)

Removes unused typedef declarations.

```rust
impl ReductionPass for TypedefPass {
    fn name(&self) -> &'static str { "typedef" }
    fn priority(&self) -> u32 { 50 }

    fn apply(&self, source: &str) -> Vec<Candidate> {
        source.lines()
            .enumerate()
            .filter(|(_, line)| line.trim().starts_with("typedef "))
            .filter_map(|(line_num, _)| {
                let name = extract_typedef_name(line)?;
                // Only remove if name appears just once (in the typedef itself)
                if source.matches(&name).count() == 1 {
                    Some(Candidate::removal(line_range)
                        .with_description(format!("remove unused typedef '{}'", name)))
                } else {
                    None
                }
            })
            .collect()
    }
}
```

### ExpressionSimplifyPass (Priority 30)

Simplifies expressions by replacing with constants.

```rust
impl ReductionPass for ExpressionSimplifyPass {
    fn name(&self) -> &'static str { "expression" }
    fn priority(&self) -> u32 { 30 }

    fn apply(&self, source: &str) -> Vec<Candidate> {
        let unit = parser.parse(source)?;
        let mut candidates = Vec::new();
        
        for expr in unit.expressions() {
            // Try replacing with 0 or 1
            candidates.push(Candidate::replace("simplify to 0", expr.range, "0"));
            candidates.push(Candidate::replace("simplify to 1", expr.range, "1"));
        }
        
        candidates
    }
}
```

## Adding a Custom Pass

```rust
use slicer_core::pass::{Candidate, ReductionPass};

pub struct MyPass;

impl ReductionPass for MyPass {
    fn name(&self) -> &'static str { "my_pass" }
    fn priority(&self) -> u32 { 40 }

    fn apply(&self, source: &str) -> Vec<Candidate> {
        // Generate candidates
        vec![]
    }
}

// Register with pipeline
pipeline.register_pass(Box::new(MyPass));
```

## Configuration

Enable/disable passes in config:

```toml
[passes]
enabled = ["dead_function", "dead_code", "statement"]
# Or use "all" for all passes
enabled = ["all"]
```
