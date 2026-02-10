# C Program Slicer

A C program reducer written in Rust. Minimizes C source code while preserving specified behavior, including compilation, output, and optionally code coverage.

## Features

- **libclang-based parsing**: Full C language support including macros, typedefs, and complex declarations
- **clangd LSP integration**: Precise unused variable and unused include detection via clangd diagnostics
- **Six reduction passes**: Dead function/code removal, unused variable removal, statement merging, typedef removal, header removal
- **Coverage-aware reduction**: Uses gcov to ensure reduced code preserves execution coverage
- **CPU cycle measurement**: Optional `perf stat` integration to measure and preserve cycle counts
- **Validation oracle**: Multi-stage validation (parse → compile → run → compare output → compare coverage)

## Prerequisites

- **Rust 1.70+**
- **LLVM/Clang 14+** (for libclang)
- **clangd** (for unused variable and header detection)
- **GCC** (for compilation/validation)
- **gcov** (for coverage analysis, optional)
- **perf** (for cycle measurement, optional)

```bash
# Ubuntu/Debian
sudo apt install llvm-14 libclang-14-dev clang-14 clangd-14 gcc

# macOS
brew install llvm
```

## Installation

```bash
cargo build --release
# Binary at target/release/slicer
```

## Quick Start

```bash
# Basic reduction
slicer program.c

# With more iterations and longer timeout
slicer program.c --iterations 200 --timeout 10

# Without coverage checking (faster)
slicer program.c --no-coverage

# With extra compiler flags
slicer program.c -f "-O2" -f "-std=c11"
```

Output is written to `program.reduced.c` alongside the input file.

## CLI Options

| Option | Short | Default | Description |
|--------|-------|---------|-------------|
| `<source>` | | | Input C source file (positional, required) |
| `--iterations` | `-i` | 100 | Maximum reduction iterations |
| `--timeout` | `-t` | 5 | Per-execution timeout in seconds |
| `--no-coverage` | | false | Disable coverage-based validation |
| `--flag` | `-f` | | Extra compiler flag (repeatable) |

## Reduction Passes

| # | Pass | Description | Analysis Method |
|---|------|-------------|-----------------|
| 1 | `DeadFunctionPass` | Removes uncalled functions | libclang parser |
| 2 | `DeadCodePass` | Removes unexecuted code | gcov coverage |
| 3 | `UnusedVariablePass` | Removes unused variables | clangd LSP |
| 4 | `StatementMergePass` | Merges consecutive statements | libclang parser |
| 5 | `TypedefPass` | Removes typedef declarations | Text scan |
| 6 | `HeaderRemovalPass` | Removes unused `#include` directives | clangd LSP |

Passes run sequentially. Each generates candidates that are validated by the oracle before being applied. The pipeline iterates until no more reductions succeed.

## Project Structure

```
crates/
├── slicer-cli/         # CLI binary (single file)
├── slicer-core/        # Core types: ReductionPass trait, Candidate, Pipeline
├── slicer-parser/      # libclang-based C parser
├── slicer-passes/      # Pass implementations + clangd integration
└── slicer-validator/   # Oracle, compiler, executor, coverage, cycle measurement
```

## Testing

```bash
# Tests must run single-threaded due to libclang constraints
cargo test -- --test-threads=1
```

## Documentation

- [Architecture](docs/ARCHITECTURE.md) — System design, data flow, and key components
- [API Reference](docs/API.md) — Public types, traits, and usage examples
- [Reduction Passes](docs/PASSES.md) — Detailed pass descriptions and implementation notes

## License

MIT License — see [LICENSE](LICENSE)
