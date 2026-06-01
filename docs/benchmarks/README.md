# Benchmarks

Benchmark reports are generated artifacts. Run the Python tooling on Linux to create a fresh report in `docs/benchmarks/latest/`:

```bash
python3 scripts/benchmark.py --fixtures tests/integration --out docs/benchmarks/latest --no-coverage
python3 scripts/plot_benchmarks.py docs/benchmarks/latest/results.json
python3 scripts/summarize_benchmarks.py docs/benchmarks/latest/results.json
```

The generated JSON intentionally records machine and tool details without absolute directory paths.
