# C Program Slicer

A C program reducer written in Rust. Minimizes C source code while preserving specified behavior, including compilation, output, and optionally code coverage.

## Features

- **libclang-based parsing**: Full C language support including macros, typedefs, structs, enums, and complex declarations
- **clangd LSP integration**: Precise unused variable, unused parameter, and unused include detection via clangd diagnostics
- **Ten reduction passes**: Dead function/code removal, unused variable/parameter/global/struct-field removal, statement merging, typedef removal, enum/struct removal, header removal
- **Coverage-aware reduction**: Uses gcov to ensure reduced code preserves execution coverage
- **CPU cycle measurement**: Optional `perf stat` integration to measure and preserve cycle counts
- **Validation oracle**: Multi-stage validation (parse → compile → run → compare output → compare coverage)
- **Total timeout**: Time-bounded reduction for CI/automated workflows

## Prerequisites

- **Rust 1.70+**
- **LLVM/Clang 14+** (for libclang)
- **clangd** (for unused variable, parameter, and header detection)
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
slicer -i program.c

# With explicit output path
slicer -i program.c -o reduced.c

# With more iterations and longer timeout
slicer -i program.c --iterations 200 --timeout 10

# Without coverage checking (faster)
slicer -i program.c --no-coverage

# With total timeout for CI (stop after 60s)
slicer -i program.c --no-coverage --total-timeout 60

# With extra compiler flags
slicer -i program.c -f "-O2" -f "-std=c11"
```

Output defaults to `program.reduced.c` alongside the input file.

## CLI Options

| Option | Short | Default | Description |
|--------|-------|---------|-------------|
| `--input` | `-i` | | Input C source file (required) |
| `--output` | `-o` | `<input>.reduced.c` | Output file path |
| `--iterations` | | 100 | Maximum reduction iterations |
| `--timeout` | `-t` | 5 | Per-execution timeout in seconds |
| `--total-timeout` | | 0 (disabled) | Total wall-clock timeout for entire reduction |
| `--no-coverage` | | false | Disable coverage-based validation |
| `--flag` | `-f` | | Extra compiler flag (repeatable) |

## Reduction Passes

| # | Pass | Description | Analysis Method |
|---|------|-------------|-----------------|
| 1 | `DeadFunctionPass` | Removes uncalled functions | libclang parser |
| 2 | `DeadCodePass` | Removes unexecuted code | gcov coverage |
| 3 | `UnusedVariablePass` | Removes unused variables | clangd LSP |
| 4 | `ArgumentRemovalPass` | Removes unused function parameters + updates call sites | clangd LSP + parser |
| 5 | `GlobalRemovalPass` | Removes unreferenced global variables | Identifier counting |
| 6 | `StructMemberRemovalPass` | Removes unaccessed struct fields | Access pattern search |
| 7 | `StatementMergePass` | Merges consecutive statements | libclang parser |
| 8 | `TypedefPass` | Removes typedef declarations | Text scan |
| 9 | `EnumStructRemovalPass` | Removes unused enum/struct type declarations | Identifier counting |
| 10 | `HeaderRemovalPass` | Removes unused `#include` directives | clangd LSP |

Passes run sequentially. Each generates candidates that are validated by the oracle before being applied. The pipeline iterates until no more reductions succeed or the total timeout is reached.

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
# Unit tests (single-threaded due to libclang constraints)
cargo test -- --test-threads=1

# CSmith integration tests (requires csmith, perf, bc)
bash tests/run_csmith_tests.sh 10 2
```

## Documentation

- [Architecture](docs/ARCHITECTURE.md) — System design, data flow, and key components
- [API Reference](docs/API.md) — Public types, traits, and usage examples
- [Reduction Passes](docs/PASSES.md) — Detailed pass descriptions and implementation notes

## License

MIT License — see [LICENSE](LICENSE)
