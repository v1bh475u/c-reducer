# API Reference

## Core Types

### Pipeline

Main entry point for program reduction.

```rust
use slicer_core::{Pipeline, CoverageData};

let pipeline = Pipeline::new(slicer_passes::all_passes(), 100)
    .with_total_timeout(60);

let coverage: Option<&CoverageData> = None;

let result = pipeline.reduce(&source, coverage, &mut |reduced| {
    oracle.validate(reduced).unwrap_or(false)
});
```

`with_total_timeout(seconds)` sets a wall-clock deadline across all iterations; the pipeline checks it at the iteration, pass, and candidate levels and stops when exceeded.

### ReductionPass Trait

```rust
pub trait ReductionPass: Send + Sync {
    fn apply(&self, source: &str, coverage: Option<&CoverageData>) -> Vec<Candidate>;
}
```

### CoverageData

Execution count data from gcov, used by coverage-aware passes.

```rust
pub struct CoverageData {
    pub line_hits: HashMap<u32, u64>,
    pub function_hits: HashMap<String, u64>,
}

impl CoverageData {
    pub fn is_line_executed(&self, line: u32) -> bool;
}
```

### Candidate

Represents a potential reduction.

```rust
pub struct Candidate {
    pub range: Range<usize>,
    pub replacement: String,
}

impl Candidate {
    pub fn removal(range: Range<usize>) -> Self;
    pub fn new(range: Range<usize>, replacement: impl Into<String>) -> Self;
    pub fn apply(&self, source: &str) -> Option<String>;
}
```

### Oracle

Validates that reductions preserve behavior.

```rust
use slicer_validator::{Oracle, OracleConfig, CompilerConfig, TimeoutConfig};

let config = OracleConfig {
    compiler: CompilerConfig::default(),
    timeout: TimeoutConfig::new(5),
    check_coverage: true,
    expected_stdout: None,
    expected_stderr: None,
    expected_exit_code: None,
};

let mut oracle = Oracle::new(config)?;
oracle.initialize(&original_source)?;

if oracle.validate(&reduced_source)? {
    // Reduction preserves: compilation, stdout, stderr, exit code, and coverage
}
```

#### OracleConfig

```rust
pub struct OracleConfig {
    pub compiler: CompilerConfig,
    pub timeout: TimeoutConfig,
    pub check_coverage: bool,
    pub expected_stdout: Option<String>,
    pub expected_stderr: Option<String>,
    pub expected_exit_code: Option<i32>,
}
```

### Compiler

GCC-based compiler wrapper.

```rust
use slicer_validator::{Compiler, CompilerConfig};

let config = CompilerConfig::default()
    .with_flag("-O2")
    .with_flags(["-std=c11", "-pedantic"])
    .with_include("/usr/local/include")
    .with_coverage();

let mut compiler = Compiler::with_config(config)?;
let result = compiler.compile(&source)?;

if result.success {
    println!("Binary: {:?}", result.binary_path);
}
```

### Executor

Runs compiled binaries with timeout.

```rust
use slicer_validator::{Executor, TimeoutConfig};

let executor = Executor::new(TimeoutConfig::new(5));
let result = executor.execute(&binary_path)?;

if result.success() {
    println!("stdout: {}", result.stdout);
    println!("stderr: {}", result.stderr);
}
```

### Coverage

gcov-based coverage analysis.

```rust
use slicer_validator::{CoverageAnalyzer, CoverageReport, missing_coverage};

let analyzer = CoverageAnalyzer::new(&working_dir)?;
let report = analyzer.collect(&source_file)?;

println!("Coverage: {:.1}%", report.coverage_percentage());
println!("Line 10 executed: {}", report.is_line_executed(10));

let missing = missing_coverage(&original_report, &reduced_report);
```

### Cycles

CPU cycle measurement and padding via `perf stat`.

```rust
use slicer_validator::cycles::{measure_cycles, pad_for_cycles};

if let Some(cycles) = measure_cycles(&binary_path) {
    println!("CPU cycles: {}", cycles);
}

if let Some(padded) = pad_for_cycles(&reduced_source, original_cycles, reduced_cycles, 5.0) {
    // padded source has a busy-loop in main()
}
```

### Parser

libclang-based C parser.

```rust
use slicer_parser::CParser;

let parser = CParser::new()?;
let parser = parser
    .with_args(["-DFOO=1"])
    .with_includes(["/usr/include"]);

let unit = parser.parse(&source)?;

for func in unit.functions() {
    println!("Function: {} ({:?})", func.name, func.range);
    println!("  Returns: {} (ptr={})", func.return_type.name, func.return_type.is_pointer);
    for param in &func.parameters {
        println!("  Param: {} : {}", param.name, param.type_info.name);
    }
}

for decl in unit.declarations() {
    println!("Declaration: {} ({:?})", decl.name, decl.kind);
}

for stmt in unit.statements() {
    println!("Statement: {:?} at {:?}", stmt.kind, stmt.range);
}

for field in unit.struct_fields() {
    println!("Field: {}.{} : {}", field.struct_name, field.name, field.type_info.name);
}

println!("Header ends at byte: {}", unit.header_end());
println!("Parse errors: {}", unit.has_errors());
println!("Includes: {}", unit.includes().len());
println!("Typedefs: {}", unit.typedefs().len());
println!("Called functions: {:?}", unit.function_calls());
```

#### StructField

```rust
pub struct StructField {
    pub struct_name: String,
    pub name: String,
    pub type_info: TypeInfo,
    pub range: ByteRange,
}
```

## CLI Usage

```bash
# Basic reduction
slicer -i input.c

# With explicit output
slicer -i input.c -o output.c

# With options
slicer -i input.c --iterations 200 --timeout 10

# Wall-clock deadline for entire reduction
slicer -i input.c --total-timeout 60

# Disable coverage checking
slicer -i input.c --no-coverage

# Add compiler flags
slicer -i input.c -f "-O2" -f "-std=c11"
```

### CLI Options

| Option | Short | Description |
|--------|-------|-------------|
| `--input` | `-i` | Input C source file (required) |
| `--output` | `-o` | Output file (default: `<input>.reduced.c`) |
| `--iterations` | | Max reduction iterations (default: 100) |
| `--timeout` | `-t` | Per-execution timeout in seconds (default: 5) |
| `--total-timeout` | | Wall-clock deadline in seconds (default: 0 = disabled) |
| `--no-coverage` | | Disable coverage-based validation |
| `--flag` | `-f` | Extra compiler flag (repeatable) |

### Output

The reduced file is written to `<input>.reduced.c` (or the path given by `--output`). The CLI prints size and CPU cycle reduction statistics.

## Available Passes

```rust
use slicer_passes::all_passes;

// Returns Vec<Box<dyn ReductionPass>> containing (in order):
//  1. DeadFunctionPass      — removes uncalled functions
//  2. DeadCodePass          — removes unexecuted code (requires coverage)
//  3. UnusedVariablePass    — removes unused variables (clangd -Wunused-variable)
//  4. ArgumentRemovalPass   — removes unused parameters (clangd -Wunused-parameter)
//  5. GlobalRemovalPass     — removes unused global declarations
//  6. StructMemberRemovalPass — removes unused struct fields
//  7. StatementMergePass    — merges consecutive statements
//  8. TypedefPass           — removes typedef declarations
//  9. EnumStructRemovalPass — removes unused enum/struct types
// 10. HeaderRemovalPass     — removes unused #includes (clangd unused-includes)
```

## Error Handling

Uses `thiserror` for typed errors and `anyhow` in the CLI:

```rust
use slicer_validator::{ValidationError, ValidationResult};

// ValidationError variants:
// - CompilationFailed { message, stderr, exit_code }
// - Timeout { timeout_secs: f64 }
// - CoverageFailed(String)
// - Io(std::io::Error)
// - CompilerNotFound(String)

use slicer_parser::{ParseError, ParseResult};

// ParseError variants:
// - Clang(String)
// - TranslationUnit(String)
// - Io(std::io::Error)
```
