# API Reference

## Core Types

### Pipeline

Main entry point for program reduction.

```rust
use slicer_core::{Pipeline, PipelineConfig, Policy};

let config = PipelineConfig::builder()
    .policy(Policy::Aggressive)
    .max_iterations(100)
    .total_timeout_secs(60)
    .build();

let mut pipeline = Pipeline::new(config)?;

// Register passes
for pass in slicer_passes::all_passes() {
    pipeline.register_pass(pass);
}

// Run reduction
let result = pipeline.reduce(&source, &mut context)?;
println!("Reduced: {} -> {} lines", result.original_lines, result.final_lines);
```

### ReductionPass Trait

```rust
pub trait ReductionPass: Send + Sync {
    fn name(&self) -> &'static str;
    fn priority(&self) -> u32;  // Higher = runs first
    fn apply(&self, source: &str) -> Vec<Candidate>;
}
```

### Candidate

Represents a potential reduction.

```rust
pub struct Candidate {
    pub description: String,
    pub range: Range<usize>,
    pub replacement: String,
    pub confidence: f64,
}

impl Candidate {
    pub fn removal(range: Range<usize>) -> Self;
    pub fn replace(desc: &str, range: Range<usize>, replacement: &str) -> Self;
    pub fn apply(&self, source: &str) -> Option<String>;
}
```

### Oracle

Validates that reductions preserve behavior.

```rust
use slicer_validator::{Oracle, OracleConfig};

let oracle = Oracle::from_source(
    &source,
    OracleConfig {
        compiler: "gcc".into(),
        flags: vec!["-O0", "-w"],
        timeout: Duration::from_secs(5),
    }
)?;

if oracle.validate(&reduced_source)? {
    // Reduction is valid
}
```

### Parser

libclang-based C parser.

```rust
use slicer_parser::CParser;

let parser = CParser::default();
let unit = parser.parse(&source)?;

for func in unit.functions() {
    println!("Function: {} at {:?}", func.name, func.range);
}
```

## CLI Usage

```bash
# Reduce a program
slicer reduce -i input.c -o output.c

# With options
slicer reduce -i input.c -o output.c \
    --policy aggressive \
    --max-iterations 100 \
    --timeout 5 \
    --total-timeout 60 \
    --passes dead_function,dead_code,statement

# Validate equivalence
slicer validate --original input.c --reduced output.c

# Parse and inspect
slicer parse -i input.c --functions --includes

# List passes
slicer passes

# Generate config
slicer init -o slicer.toml
```

### CLI Options

| Option | Description |
|--------|-------------|
| `-i, --input` | Input C source file |
| `-o, --output` | Output file |
| `-c, --config` | TOML config file |
| `-p, --policy` | `aggressive` or `conservative` |
| `--max-iterations` | Max reduction iterations (default: 100) |
| `--timeout` | Per-validation timeout in seconds (default: 5) |
| `--total-timeout` | Total timeout in seconds (default: 60) |
| `--passes` | Comma-separated pass list or `all` |
| `-v` | Verbose output (-vv for debug, -vvv for trace) |

## Configuration File

```toml
# slicer.toml

[source]
file = "input.c"

[output]
file = "output.c"

[validation]
compiler = "gcc"
compiler_flags = ["-O0", "-w"]
timeout = 5

[passes]
enabled = ["all"]  # Or specific: ["dead_function", "statement"]
max_iterations = 100
```

## Available Passes

```rust
use slicer_passes::all_passes;

// Returns Vec<Box<dyn ReductionPass>> with:
// - DeadFunctionPass (priority 100)
// - DeadCodePass (priority 90)
// - StatementPass (priority 70)
// - IncludePass (priority 60)
// - TypedefPass (priority 50)
// - ExpressionSimplifyPass (priority 30)
```

## Error Handling

Uses `anyhow::Result` for error propagation:

```rust
use anyhow::{Context, Result};

fn reduce_file(path: &Path) -> Result<String> {
    let source = std::fs::read_to_string(path)
        .context("Failed to read source file")?;
    
    let result = pipeline.reduce(&source, &mut ctx)?;
    Ok(result.final_source)
}
```
