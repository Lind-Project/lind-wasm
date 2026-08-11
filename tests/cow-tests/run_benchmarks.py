#!/usr/bin/env python3
"""
Standalone runner for tests/cow-tests/benchmarks/*.c.

Reuses tests/benchmarks/bench.c (gettimens/emit_result) the same way
scripts/test/benchrunner.py does, but stays self-contained to
tests/cow-tests since benchrunner.py hardcodes its benchmark directory to
tests/benchmarks. Each benchmark prints tab-delimited rows
(<test>\t<param>\t<loops>\t<avg_ns>, see tests/benchmarks/README.md); this
script parses them and prints a table, and can optionally write CSV for
later comparison against a future COW-enabled run.

Usage:
  ./run_benchmarks.py                          # run every benchmark, print a table
  ./run_benchmarks.py --out results.csv        # also write CSV
  ./run_benchmarks.py cow_fork_exec            # run only the named benchmark(s)
"""
import argparse
import csv
import subprocess
import sys
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parents[2]
LIND_TOOL_PATH = REPO_ROOT / "scripts" / "bin"
BENCH_DIR = Path(__file__).resolve().parent / "benchmarks"
SHARED_BENCH_C = REPO_ROOT / "tests" / "benchmarks" / "bench.c"
LINDFS_ROOT = REPO_ROOT / "lindfs"
DEFAULT_TIMEOUT = 120


def run(cmd, cwd=None, timeout=None):
    try:
        return subprocess.run(cmd, cwd=cwd, timeout=timeout,
                               stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
    except subprocess.TimeoutExpired:
        return None


def compile_wasm(src: Path, static: bool = False):
    cmd = [str(LIND_TOOL_PATH / "lind_compile")]
    if static:
        cmd.append("-s")
    cmd += [str(src), str(SHARED_BENCH_C)]
    proc = run(cmd)
    if proc is None or proc.returncode != 0:
        print(f"  [wasm compile FAILED]\n{proc.stderr if proc else 'timeout'}")
        return None
    cwasm = src.with_suffix(".cwasm")
    if not cwasm.exists():
        print(f"  [wasm compile FAILED] expected artifact not found: {cwasm}")
        return None
    return cwasm


def run_wasm(cwasm: Path, timeout):
    proc = run([str(LIND_TOOL_PATH / "lind_run"), cwasm.name], timeout=timeout)
    if proc is None:
        return None
    if proc.returncode != 0:
        print(f"  [wasm run FAILED, exit {proc.returncode}]\n{proc.stdout}{proc.stderr}")
        return None
    return proc.stdout


def parse_rows(output: str, source_binary: str, build_mode: str):
    rows = []
    for line in output.splitlines():
        parts = line.split("\t")
        if len(parts) != 4:
            continue
        test, param, loops, avg_ns = parts
        try:
            rows.append({
                "binary": source_binary,
                "build_mode": build_mode,
                "test": test,
                "param_mib": int(param),
                "loops": int(loops),
                "avg_ns": int(avg_ns),
            })
        except ValueError:
            continue
    return rows


def cleanup(paths):
    for p in paths:
        try:
            if p and p.exists():
                p.unlink()
        except OSError:
            pass


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("benchmarks", nargs="*", help="benchmark stems to run (default: all)")
    parser.add_argument("--out", help="write results as CSV to this path")
    parser.add_argument("--timeout", type=int, default=DEFAULT_TIMEOUT)
    parser.add_argument("--static", action="store_true",
                         help="compile with lind_compile -s (static/non-dylink build) instead of the default dynamic/dylink build")
    args = parser.parse_args()

    all_sources = sorted(p for p in BENCH_DIR.glob("*.c"))
    if args.benchmarks:
        wanted = set(args.benchmarks)
        all_sources = [s for s in all_sources if s.stem in wanted]
        missing = wanted - {s.stem for s in all_sources}
        if missing:
            print(f"Unknown benchmark stem(s): {sorted(missing)}")
            sys.exit(2)

    if not all_sources:
        print(f"No benchmarks found under {BENCH_DIR}")
        sys.exit(1)

    all_rows = []
    for src in all_sources:
        print(f"--- {src.stem} ---")
        cwasm = compile_wasm(src, static=args.static)
        if cwasm is None:
            continue
        output = run_wasm(cwasm, args.timeout)
        lindfs_copy = LINDFS_ROOT / cwasm.name
        cleanup([cwasm, lindfs_copy])
        if output is None:
            continue
        rows = parse_rows(output, src.stem, "static" if args.static else "dynamic")
        all_rows.extend(rows)
        for r in rows:
            print(f"  {r['test']:<20} {r['param_mib']:>6} MiB  {r['avg_ns']:>12} ns/op  ({r['loops']} loops)")

    if not all_rows:
        print("No benchmark rows produced.")
        sys.exit(1)

    print(f"\n{len(all_rows)} result rows from {len(all_sources)} benchmark(s)")

    if args.out:
        with open(args.out, "w", newline="") as f:
            writer = csv.DictWriter(f, fieldnames=["binary", "build_mode", "test", "param_mib", "loops", "avg_ns"])
            writer.writeheader()
            writer.writerows(all_rows)
        print(f"Wrote {args.out}")


if __name__ == "__main__":
    main()
