# C Program Slicer

A C program reducer written in Rust. Minimizes C source code while preserving specified behavior.

## Features

- **libclang-based parsing**: Full C language support including macros and typedefs
- **Six reduction passes**: Dead function/code removal, statement/include/typedef removal, expression simplification
- **Validation**: Ensures reduced code compiles and produces expected output
- **Configurable**: TOML-based configuration

## Prerequisites

- **Rust 1.70+**
- **LLVM/Clang 14+** (for libclang)
- **GCC** (for compilation/validation)

```bash
# Ubuntu/Debian
sudo apt install llvm-14 libclang-14-dev clang-14 gcc

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
# Generate config
slicer init -o slicer.toml

# Run reduction
slicer reduce -i program.c -o reduced.c --policy aggressive

# With verbose output
slicer reduce -i program.c -o reduced.c -v
```

## CLI Commands

| Command | Description |
|---------|-------------|
| `slicer reduce` | Run reduction passes on a C program |
| `slicer validate` | Check if reduced file matches original behavior |
| `slicer parse` | Parse a C file and display AST info |
| `slicer passes` | List available reduction passes |
| `slicer init` | Generate a configuration file |

## Reduction Passes

| Pass | Priority | Description |
|------|----------|-------------|
| `dead_function` | 100 | Removes uncalled functions |
| `dead_code` | 90 | Removes dead code (post-return, unreachable branches) |
| `statement` | 70 | Removes individual statements |
| `include` | 60 | Removes unnecessary #include directives |
| `typedef` | 50 | Removes unused typedef declarations |
| `expression` | 30 | Simplifies expressions |

Passes run serially in priority order (highest first). Each generates candidates that are validated before being applied.

## Project Structure

```
crates/
├── slicer-cli/       # Command-line interface
├── slicer-core/      # Core types, traits (ReductionPass), and Pipeline
├── slicer-parser/    # libclang-based C parser
├── slicer-passes/    # Pass implementations
└── slicer-validator/ # Compilation and execution validation
```

## Configuration

```toml
[source]
file = "input.c"

[output]
file = "output.c"

[validation]
compiler = "gcc"
compiler_flags = ["-O2"]
timeout = 30

[passes]
enabled = ["all"]
max_iterations = 100
```

## Testing

```bash
# Tests must run single-threaded due to libclang constraints
cargo test --test-threads=1
```

## License

MIT License - see [LICENSE](LICENSE)
