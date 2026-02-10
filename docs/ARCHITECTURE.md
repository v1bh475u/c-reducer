# Architecture

## Overview

A C program reducer written in Rust. Uses libclang for parsing, clangd for LSP-based diagnostics, and validates reductions by compiling, running, and optionally comparing coverage of the modified source.

## Crate Structure

```
crates/
├── slicer-core/          # Core types and pipeline (zero external deps)
│   ├── context.rs        # CoverageData
│   ├── pass.rs           # ReductionPass trait, Candidate
│   └── pipeline.rs       # Pipeline orchestrator (with total_timeout support)
│
├── slicer-parser/        # C parsing with libclang
│   ├── ast.rs            # CParser, ParsedUnit, Function, Declaration, Statement, StructField
│   ├── error.rs          # ParseError
│   └── span.rs           # ByteRange utilities
│
├── slicer-passes/        # Pass implementations
│   ├── clangd.rs         # clangd LSP diagnostic client
│   ├── dead_function.rs  # Remove uncalled functions
│   ├── dead_code.rs      # Remove unexecuted code (coverage-based)
│   ├── unused_variable.rs# Remove unused variables (clangd-based)
│   ├── argument_removal.rs # Remove unused params + update call sites (clangd-based)
│   ├── global_removal.rs # Remove unreferenced global variables
│   ├── struct_member_removal.rs # Remove unaccessed struct fields
│   ├── statement_merge.rs# Merge consecutive statements
│   ├── typedef.rs        # Remove typedef declarations
│   ├── enum_struct_removal.rs # Remove unused enum/struct declarations
│   ├── header_removal.rs # Remove unused includes (clangd-based)
│   └── util.rs           # LineIndex, line-extension utilities
│
├── slicer-validator/     # Validation
│   ├── compiler.rs       # GCC compiler wrapper
│   ├── executor.rs       # Binary execution with timeout
│   ├── oracle.rs         # Validation oracle (compile + run + compare)
│   ├── coverage.rs       # gcov coverage analysis
│   ├── cycles.rs         # perf stat CPU cycle measurement
│   └── error.rs          # ValidationError types
│
└── slicer-cli/           # CLI (single-file binary)
    └── main.rs           # Entry point, clap setup, reduction flow
```

## Key Components

### Pipeline

Runs all passes sequentially up to `max_iterations` times until convergence. Supports an optional `total_timeout` that halts reduction after a wall-clock duration.

```rust
pub struct Pipeline {
    passes: Vec<Box<dyn ReductionPass>>,
    max_iterations: u32,
    total_timeout: Option<Duration>,
}

impl Pipeline {
    pub fn new(passes: Vec<Box<dyn ReductionPass>>, max_iterations: u32) -> Self;
    pub fn with_total_timeout(self, seconds: u64) -> Self;

    pub fn reduce(
        &self,
        source: &str,
        coverage: Option<&CoverageData>,
        oracle: &mut dyn FnMut(&str) -> bool,
    ) -> String;
}
```

### ReductionPass Trait

```rust
pub trait ReductionPass: Send + Sync {
    fn apply(&self, source: &str, coverage: Option<&CoverageData>) -> Vec<Candidate>;
}
```

Passes receive optional `CoverageData` (line/function execution counts) and return a list of `Candidate` reductions to try.

### Candidate

Represents a potential reduction (removal or replacement):

```rust
pub struct Candidate {
    pub range: Range<usize>,
    pub replacement: String,
}

impl Candidate {
    pub fn removal(range: Range<usize>) -> Self;
    pub fn new(range: Range<usize>, replacement: impl Into<String>) -> Self;

    pub fn apply(&self, source: &str) -> Option<String> {
        let mut result = String::new();
        result.push_str(&source[..self.range.start]);
        result.push_str(&self.replacement);
        result.push_str(&source[self.range.end..]);
        Some(result)
    }
}
```

### Parser (libclang)

Uses a thread-local Clang instance. Writes source to a temp file, parses with `-std=c11 -ferror-limit=0`, and recursively visits the AST.

```rust
pub struct CParser {
    temp_dir: TempDir,
    args: Vec<String>,
}

impl CParser {
    pub fn new() -> ParseResult<Self>;
    pub fn with_args(self, args: impl IntoIterator<Item = impl Into<String>>) -> Self;
    pub fn with_includes(self, paths: impl IntoIterator<Item = impl AsRef<Path>>) -> Self;
    pub fn parse(&self, source: &str) -> ParseResult<ParsedUnit>;
}
```

`ParsedUnit` provides access to functions, declarations, function calls, includes, typedefs, statements, struct fields, and the `header_end` byte offset (where the first function definition begins).

**Note**: Tests must run with `--test-threads=1` due to libclang's single-instance constraint.

### Oracle

Validates that reduced code preserves behavior through a multi-stage pipeline:

```rust
pub struct Oracle {
    config: OracleConfig,
    compiler: Compiler,
    executor: Executor,
    parser: CParser,
    original_coverage: Option<CoverageReport>,
}

impl Oracle {
    pub fn initialize(&mut self, original_source: &str) -> ValidationResult<()>;

    pub fn validate(&mut self, reduced_source: &str) -> ValidationResult<bool> {
        // 1. Parse with CParser → reject if has_errors()
        // 2. Compile with GCC → reject if fails
        // 3. Execute with timeout → reject if timeout/incomplete
        // 4. Compare stdout, stderr, exit_code against expected
        // 5. If check_coverage: compile with --coverage, run, collect
        //    gcov, compare with missing_coverage → reject if any
        //    originally-executed lines are missing
    }
}
```

### clangd LSP Integration

Some passes leverage clangd's diagnostic engine for precise analysis:

```rust
// crates/slicer-passes/src/clangd.rs
pub fn get_diagnostics(source: &str, extra_flags: &[&str]) -> Vec<Diagnostic>;
pub fn lines_with_code(source: &str, code: &str, extra_flags: &[&str]) -> HashSet<usize>;
```

Spawns a clangd subprocess, sends LSP `initialize` + `textDocument/didOpen`, waits for `publishDiagnostics` notifications, and parses them. Used by `UnusedVariablePass`, `ArgumentRemovalPass`, and `HeaderRemovalPass`.

## Data Flow

```
Source File
    │
    ▼
Oracle.initialize() ──► Compile & run original, capture expected output
    │                    Optionally collect coverage via gcov
    ▼
CoverageData (optional line/function execution counts)
    │
    ▼
Pipeline.reduce() loop (up to max_iterations):
    │
    For each pass:
    │
    ├── pass.apply(source, coverage) ──► Vec<Candidate>
    │   (may use CParser, clangd, or text-based scanning)
    │
    └── For each candidate (sorted back-to-front):
            │
            ├── candidate.apply(source) ──► modified source
            ├── oracle.validate(modified) ──► parse → compile → run → compare
            │
            └── If valid: source = modified
    │
    ▼
Reduced Source
    │
    ▼
(Optional) Cycle measurement via perf stat + padding
```

## CPU Cycle Preservation

After reduction, the CLI optionally measures CPU cycles using `perf stat -e cpu-cycles` and can insert a busy-loop into `main()` to match the original program's cycle count within a configurable tolerance.
