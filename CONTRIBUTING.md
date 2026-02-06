# Contributing to C Program Slicer

Thank you for your interest in contributing! This document provides guidelines and instructions for contributing.

## Getting Started

1. Fork the repository
2. Clone your fork: `git clone https://github.com/yourusername/c-program-slicer.git`
3. Create a branch: `git checkout -b feature/your-feature-name`
4. Make your changes
5. Run tests: `cargo test --test-threads=1`
6. Submit a pull request

## Development Setup

### Prerequisites

- Rust 1.70+ (install via [rustup](https://rustup.rs/))
- LLVM/Clang 14+ with libclang development files
- GCC with gcov for coverage tests

### Building

```bash
# Debug build
cargo build

# Release build
cargo build --release

# Run with cargo
cargo run -- reduce --config config.toml
```

### Testing

```bash
# Run all tests (single-threaded required due to libclang)
cargo test --test-threads=1

# Run specific crate tests
cargo test -p slicer-parser --test-threads=1
cargo test -p slicer-passes --test-threads=1

# Run with output
cargo test --test-threads=1 -- --nocapture
```

### Code Quality

```bash
# Format code (required before submitting PR)
cargo fmt

# Run linter
cargo clippy -- -D warnings

# Check for unused dependencies
cargo +nightly udeps
```

## Code Style

- Follow standard Rust conventions
- Use `rustfmt` for formatting
- Address all `clippy` warnings
- Write documentation for public APIs
- Add tests for new functionality

### Naming Conventions

- Use `snake_case` for functions, variables, and modules
- Use `PascalCase` for types and traits
- Use `SCREAMING_SNAKE_CASE` for constants
- Prefix private helper functions with `_` if needed

### Documentation

- Document all public items with `///` doc comments
- Include examples in documentation where helpful
- Update relevant markdown docs when adding features

## Project Structure

```
crates/
├── slicer-cli/        # CLI interface - handles argument parsing and user interaction
├── slicer-core/       # Core types and traits - Pass, Candidate, Pipeline, Config
├── slicer-parser/     # C parser using libclang - AST extraction and source manipulation
├── slicer-passes/     # Reduction passes - implement the Pass trait
└── slicer-validator/  # Validation logic - compile, run, compare output
```

## Adding a New Reduction Pass

1. Create a new file in `crates/slicer-passes/src/`
2. Implement the `Pass` trait from `slicer-core`
3. Register in `crates/slicer-passes/src/lib.rs`
4. Add tests
5. Document in `docs/PASSES.md`

Example:

```rust
use slicer_core::{Candidate, Config, Context, Pass, ReductionResult};
use slicer_parser::ParsedUnit;

pub struct MyNewPass;

impl Pass for MyNewPass {
    fn name(&self) -> &'static str {
        "my_new_pass"
    }

    fn description(&self) -> &'static str {
        "Description of what this pass does"
    }

    fn priority(&self) -> u32 {
        75 // Higher runs first
    }

    fn generate_candidates(
        &self,
        unit: &ParsedUnit,
        _context: &Context,
        _config: &Config,
    ) -> Vec<Candidate> {
        // Generate removal candidates
        vec![]
    }
}
```

## Commit Messages

Follow conventional commits:

- `feat:` New feature
- `fix:` Bug fix
- `docs:` Documentation changes
- `test:` Adding or updating tests
- `refactor:` Code refactoring
- `perf:` Performance improvements
- `chore:` Maintenance tasks

Example: `feat(passes): add struct field removal pass`

## Pull Request Process

1. Ensure tests pass: `cargo test --test-threads=1`
2. Ensure code is formatted: `cargo fmt`
3. Ensure no clippy warnings: `cargo clippy`
4. Update documentation if needed
5. Add tests for new functionality
6. Write a clear PR description

## Reporting Issues

When reporting bugs, please include:

- Rust version (`rustc --version`)
- OS and version
- LLVM/Clang version
- Minimal reproduction case
- Expected vs actual behavior

## Questions?

Open an issue for questions or discussions about the project.
