#!/usr/bin/env python3
"""Run slicer fixture benchmarks and write path-safe JSON/CSV results."""

from __future__ import annotations

import argparse
import csv
import hashlib
import json
import os
import platform
import shutil
import subprocess
import sys
import time
from datetime import datetime, timezone
from pathlib import Path
from typing import Any


def repo_root() -> Path:
    return Path(__file__).resolve().parents[1]


def run_command(args: list[str], timeout: int | None = None) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        args,
        cwd=repo_root(),
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        timeout=timeout,
        check=False,
    )


def command_version(command: str, args: list[str] | None = None) -> str | None:
    if shutil.which(command) is None:
        return None
    version_args = [command] + (args or ["--version"])
    try:
        proc = run_command(version_args, timeout=10)
    except (OSError, subprocess.TimeoutExpired):
        return None
    output = (proc.stdout or proc.stderr).strip().splitlines()
    return output[0].strip() if output else None


def linux_memory_total() -> str | None:
    meminfo = Path("/proc/meminfo")
    if not meminfo.exists():
        return None
    for line in meminfo.read_text(encoding="utf-8", errors="replace").splitlines():
        if line.startswith("MemTotal:"):
            parts = line.split()
            if len(parts) >= 2 and parts[1].isdigit():
                gib = int(parts[1]) / 1024 / 1024
                return f"{gib:.1f} GiB"
    return None


def linux_cpu_model() -> str | None:
    cpuinfo = Path("/proc/cpuinfo")
    if not cpuinfo.exists():
        return None
    for line in cpuinfo.read_text(encoding="utf-8", errors="replace").splitlines():
        if line.lower().startswith("model name"):
            return line.split(":", 1)[1].strip()
    return None


def machine_metadata() -> dict[str, Any]:
    return {
        "captured_at_utc": datetime.now(timezone.utc).isoformat(timespec="seconds"),
        "os": platform.system(),
        "os_release": platform.release(),
        "kernel": platform.version(),
        "architecture": platform.machine(),
        "cpu_model": linux_cpu_model() or platform.processor() or None,
        "cpu_count_logical": os.cpu_count(),
        "memory_total": linux_memory_total(),
        "tools": {
            "rustc": command_version("rustc"),
            "cargo": command_version("cargo"),
            "clang": command_version("clang"),
            "clangd": command_version("clangd"),
            "gcc": command_version("gcc"),
            "csmith": command_version("csmith"),
            "perf": command_version("perf", ["--version"]),
        },
    }


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def count_lines(path: Path) -> int:
    text = path.read_text(encoding="utf-8", errors="replace")
    return len(text.splitlines())


def compile_and_run(source: Path, binary: Path, timeout: int) -> dict[str, Any]:
    compile_proc = run_command(["gcc", "-w", "-O0", str(source), "-o", str(binary)], timeout=timeout)
    if compile_proc.returncode != 0:
        return {"compiled": False, "ran": False, "exit_code": None, "stdout_sha256": None, "stderr_sha256": None}

    try:
        run_proc = run_command([str(binary)], timeout=timeout)
    except subprocess.TimeoutExpired:
        return {"compiled": True, "ran": False, "exit_code": "timeout", "stdout_sha256": None, "stderr_sha256": None}

    return {
        "compiled": True,
        "ran": True,
        "exit_code": run_proc.returncode,
        "stdout_sha256": hashlib.sha256(run_proc.stdout.encode("utf-8", errors="replace")).hexdigest(),
        "stderr_sha256": hashlib.sha256(run_proc.stderr.encode("utf-8", errors="replace")).hexdigest(),
    }


def benchmark_fixture(fixture: Path, slicer: Path, out_dir: Path, args: argparse.Namespace) -> dict[str, Any]:
    name = fixture.stem
    reduced_dir = out_dir / "reduced"
    bin_dir = out_dir / "bin"
    reduced_dir.mkdir(parents=True, exist_ok=True)
    bin_dir.mkdir(parents=True, exist_ok=True)

    reduced = reduced_dir / f"{name}.reduced.c"
    command = [
        str(slicer),
        "-i",
        str(fixture),
        "-o",
        str(reduced),
        "--iterations",
        str(args.iterations),
        "--timeout",
        str(args.timeout),
        "--total-timeout",
        str(args.total_timeout),
    ]
    if args.no_coverage:
        command.append("--no-coverage")
    for flag in args.flag:
        command.extend(["-f", flag])

    start = time.perf_counter()
    try:
        proc = run_command(command, timeout=max(args.total_timeout + args.timeout + 15, args.timeout + 15))
        elapsed = time.perf_counter() - start
    except subprocess.TimeoutExpired:
        elapsed = time.perf_counter() - start
        proc = None

    input_bytes = fixture.stat().st_size
    input_lines = count_lines(fixture)
    ok = proc is not None and proc.returncode == 0 and reduced.exists()

    result: dict[str, Any] = {
        "case": name,
        "input_artifact": fixture.name,
        "output_artifact": f"reduced/{reduced.name}",
        "status": "pass" if ok else "fail",
        "exit_code": proc.returncode if proc is not None else "timeout",
        "duration_seconds": round(elapsed, 4),
        "input_bytes": input_bytes,
        "output_bytes": reduced.stat().st_size if reduced.exists() else None,
        "input_lines": input_lines,
        "output_lines": count_lines(reduced) if reduced.exists() else None,
        "input_sha256": sha256_file(fixture),
        "output_sha256": sha256_file(reduced) if reduced.exists() else None,
        "behavior_preserved": None,
    }

    if result["output_bytes"] is not None and input_bytes:
        result["byte_reduction_pct"] = round((1 - result["output_bytes"] / input_bytes) * 100, 2)
    else:
        result["byte_reduction_pct"] = None

    if result["output_lines"] is not None and input_lines:
        result["line_reduction_pct"] = round((1 - result["output_lines"] / input_lines) * 100, 2)
    else:
        result["line_reduction_pct"] = None

    if ok and args.verify_compile and shutil.which("gcc") is not None:
        original_run = compile_and_run(fixture, bin_dir / f"{name}.orig", args.timeout)
        reduced_run = compile_and_run(reduced, bin_dir / f"{name}.reduced", args.timeout)
        result["original_run"] = original_run
        result["reduced_run"] = reduced_run
        result["behavior_preserved"] = (
            original_run.get("compiled")
            and reduced_run.get("compiled")
            and original_run.get("ran")
            and reduced_run.get("ran")
            and original_run.get("exit_code") == reduced_run.get("exit_code")
            and original_run.get("stdout_sha256") == reduced_run.get("stdout_sha256")
            and original_run.get("stderr_sha256") == reduced_run.get("stderr_sha256")
        )

    return result


def write_csv(results: list[dict[str, Any]], csv_path: Path) -> None:
    fields = [
        "case",
        "status",
        "duration_seconds",
        "input_lines",
        "output_lines",
        "line_reduction_pct",
        "input_bytes",
        "output_bytes",
        "byte_reduction_pct",
        "behavior_preserved",
    ]
    with csv_path.open("w", newline="", encoding="utf-8") as handle:
        writer = csv.DictWriter(handle, fieldnames=fields)
        writer.writeheader()
        for row in results:
            writer.writerow({field: row.get(field) for field in fields})


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--fixtures", default="tests/integration", help="Directory containing .c fixture files")
    parser.add_argument("--out", default="docs/benchmarks/latest", help="Output directory for benchmark artifacts")
    parser.add_argument("--binary", help="Path to an existing slicer binary")
    parser.add_argument("--skip-build", action="store_true", help="Do not run cargo build --release before benchmarking")
    parser.add_argument("--iterations", type=int, default=100, help="Reducer iterations per fixture")
    parser.add_argument("--timeout", type=int, default=5, help="Per-command timeout in seconds")
    parser.add_argument("--total-timeout", type=int, default=60, help="Reducer total timeout per fixture in seconds")
    parser.add_argument("--no-coverage", action="store_true", help="Pass --no-coverage to slicer")
    parser.add_argument("--no-verify-compile", dest="verify_compile", action="store_false", help="Skip GCC behavior checks")
    parser.add_argument("-f", "--flag", action="append", default=[], help="Extra compiler flag passed to slicer")
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    root = repo_root()
    out_dir = (root / args.out).resolve() if not Path(args.out).is_absolute() else Path(args.out)
    out_dir.mkdir(parents=True, exist_ok=True)

    if not args.skip_build:
        build = run_command(["cargo", "build", "--release"], timeout=600)
        if build.returncode != 0:
            print("cargo build --release failed", file=sys.stderr)
            return build.returncode

    slicer = Path(args.binary) if args.binary else root / "target" / "release" / ("slicer.exe" if os.name == "nt" else "slicer")
    if not slicer.exists():
        print("slicer binary was not found; run cargo build --release or pass --binary", file=sys.stderr)
        return 2

    fixtures_dir = (root / args.fixtures).resolve() if not Path(args.fixtures).is_absolute() else Path(args.fixtures)
    fixtures = sorted(fixtures_dir.glob("*.c"))
    if not fixtures:
        print("no .c fixtures found", file=sys.stderr)
        return 2

    results = [benchmark_fixture(fixture, slicer, out_dir, args) for fixture in fixtures]
    passed = sum(1 for result in results if result["status"] == "pass")
    failed = len(results) - passed

    payload = {
        "schema_version": 1,
        "summary": {
            "total": len(results),
            "passed": passed,
            "failed": failed,
            "coverage_enabled": not args.no_coverage,
            "verify_compile": args.verify_compile,
        },
        "machine": machine_metadata(),
        "parameters": {
            "iterations": args.iterations,
            "timeout": args.timeout,
            "total_timeout": args.total_timeout,
            "extra_flags_count": len(args.flag),
        },
        "results": results,
    }

    results_json = out_dir / "results.json"
    results_json.write_text(json.dumps(payload, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    write_csv(results, out_dir / "results.csv")
    print(f"Wrote {len(results)} benchmark results to {results_json}")
    return 0 if failed == 0 else 1


if __name__ == "__main__":
    raise SystemExit(main())
