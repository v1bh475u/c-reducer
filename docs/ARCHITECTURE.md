# Architecture

## Overview

A C program reducer written in Rust. Uses libclang for parsing and validates reductions by compiling and running the modified source.

## Crate Structure

```
crates/
├── slicer-core/        # Core types and pipeline
│   ├── config.rs       # PipelineConfig
│   ├── context.rs      # Reduction context
│   ├── pass.rs         # ReductionPass trait, Candidate
│   ├── pipeline.rs     # Pipeline orchestrator
│   └── result.rs       # ReductionResult
│
├── slicer-parser/      # C parsing with libclang
│   ├── ast.rs          # AST node types
│   ├── lib.rs          # CParser, ParsedUnit
│   ├── span.rs         # ByteRange utilities
│   └── error.rs        # ParseError
│
├── slicer-passes/      # Pass implementations
│   ├── dead_function.rs  # Priority 100
│   ├── dead_code.rs      # Priority 90
│   ├── statement.rs      # Priority 70
│   ├── include.rs        # Priority 60
│   ├── typedef.rs        # Priority 50
│   ├── expression.rs     # Priority 30
│   └── util.rs           # Shared utilities
│
├── slicer-validator/   # Validation
│   ├── compiler.rs     # Compiler abstraction
│   ├── executor.rs     # Program execution
│   ├── oracle.rs       # Validation oracle
│   └── coverage.rs     # gcov integration
│
└── slicer-cli/         # CLI
    ├── main.rs         # Entry point, clap setup
    └── commands.rs     # Command implementations
```

## Key Components

### Pipeline

Runs passes serially in priority order (higher priority first) until convergence:

```rust
pub struct Pipeline {
    config: PipelineConfig,
    passes: Vec<Box<dyn ReductionPass>>,
}

impl Pipeline {
    pub fn reduce(&self, source: &str, ctx: &mut Context) -> Result<ReductionResult> {
        let mut current = source.to_string();
        
        loop {
            let mut made_progress = false;
            
            for pass in &self.passes {  // Sorted by priority descending
                let candidates = pass.apply(&current);
                
                for candidate in candidates {
                    if let Some(reduced) = candidate.apply(&current) {
                        if oracle.validate(&reduced)? {
                            current = reduced;
                            made_progress = true;
                        }
                    }
                }
            }
            
            if !made_progress { break; }
        }
        
        Ok(ReductionResult::new(source, &current))
    }
}
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

Represents a potential reduction (removal or replacement):

```rust
pub struct Candidate {
    pub description: String,
    pub range: Range<usize>,
    pub replacement: String,
    pub confidence: f64,
}

impl Candidate {
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

Uses thread-local Clang instance due to libclang's single-instance constraint:

```rust
thread_local! {
    static CLANG: Clang = Clang::new().expect("Failed to initialize libclang");
}

pub struct CParser;

impl CParser {
    pub fn parse(&self, source: &str) -> Result<ParsedUnit, ParseError> {
        CLANG.with(|clang| { /* parse using clang */ })
    }
}
```

**Note**: Tests must run with `--test-threads=1`.

### Oracle

Validates that reduced code preserves behavior:

```rust
pub struct Oracle {
    compiler: Compiler,
    executor: Executor,
    expected_output: String,
    timeout: Duration,
}

impl Oracle {
    pub fn validate(&self, source: &str) -> Result<bool> {
        let binary = self.compiler.compile(source)?;
        let output = self.executor.run(&binary, self.timeout)?;
        Ok(output == self.expected_output)
    }
}
```

## Data Flow

```
Source File
    │
    ▼
CParser.parse() ──► ParsedUnit (functions, typedefs, includes, statements)
    │
    ▼
For each pass (by priority):
    │
    ├── pass.apply(source) ──► Vec<Candidate>
    │
    └── For each candidate:
            │
            ├── candidate.apply(source) ──► modified source
            ├── oracle.validate(modified) ──► compile & run
            │
            └── If valid: source = modified
    │
    ▼
Reduced Source
```

## Configuration

TOML-based configuration:

```toml
[validation]
compiler = "gcc"
compiler_flags = ["-O2"]
timeout = 30

[passes]
enabled = ["dead_function", "dead_code", "statement"]
max_iterations = 100
```
