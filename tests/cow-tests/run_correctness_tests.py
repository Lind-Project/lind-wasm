#!/usr/bin/env python3
"""
Standalone runner for tests/cow-tests/correctness/*.c.

These tests validate fork() data-isolation/inheritance invariants that
must hold regardless of whether fork's memory handling is implemented via
eager copy (today) or COW (see /cow-design.md, /cow-implementation-plan.md).
They are deliberately kept out of tests/unit-tests/ (a separate,
dedicated home for the COW effort, see tests/cow-tests/README.md) so this
script reuses the same lind_compile/lind_run tools the main harness uses,
without depending on wasmtestreport.py's hardcoded tests/unit-tests/
discovery root.

Usage:
  ./run_correctness_tests.py                 # run every test
  ./run_correctness_tests.py m1_write_child_first m2_recursive_chain
                                              # run only the named tests (by stem)
  ./run_correctness_tests.py --timeout 60     # override the default per-test timeout
"""
import argparse
import subprocess
import sys
import time
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parents[2]
LIND_TOOL_PATH = REPO_ROOT / "scripts" / "bin"
TEST_DIR = Path(__file__).resolve().parent / "correctness"
LINDFS_ROOT = REPO_ROOT / "lindfs"
DEFAULT_TIMEOUT = 30
CC = "cc"


def run(cmd, cwd=None, timeout=None):
    try:
        return subprocess.run(cmd, cwd=cwd, timeout=timeout,
                               stdout=subprocess.PIPE, stderr=subprocess.PIPE,
                               text=True)
    except subprocess.TimeoutExpired:
        return None


def compile_native(src: Path, out: Path) -> bool:
    proc = run([CC, "-O0", "-g", "-pthread", str(src), "-o", str(out)])
    if proc is None or proc.returncode != 0:
        print(f"  [native compile FAILED]\n{proc.stderr if proc else 'timeout'}")
        return False
    return True


def compile_wasm(src: Path) -> Path | None:
    proc = run([str(LIND_TOOL_PATH / "lind_compile"), str(src)])
    if proc is None or proc.returncode != 0:
        print(f"  [wasm compile FAILED]\n{proc.stderr if proc else 'timeout'}")
        return None
    cwasm = src.with_suffix(".cwasm")
    if not cwasm.exists():
        print(f"  [wasm compile FAILED] expected artifact not found: {cwasm}")
        return None
    return cwasm


def run_native(binary: Path, timeout) -> tuple[int, str]:
    proc = run([str(binary)], timeout=timeout)
    if proc is None:
        return ("timeout", "")
    return (proc.returncode, proc.stdout + proc.stderr)


def run_wasm(cwasm: Path, timeout) -> tuple[int, str]:
    proc = run([str(LIND_TOOL_PATH / "lind_run"), cwasm.name], timeout=timeout)
    if proc is None:
        return ("timeout", "")
    return (proc.returncode, proc.stdout + proc.stderr)


def cleanup(paths):
    for p in paths:
        try:
            if p and p.exists():
                p.unlink()
        except OSError:
            pass


def run_one(src: Path, timeout: int) -> bool:
    name = src.stem
    print(f"--- {name} ---")

    native_bin = src.with_suffix(".native.out")
    ok = compile_native(src, native_bin)
    if not ok:
        return False

    native_rc, native_out = run_native(native_bin, timeout)
    cwasm = compile_wasm(src)
    if cwasm is None:
        cleanup([native_bin])
        return False

    wasm_rc, wasm_out = run_wasm(cwasm, timeout)

    lindfs_copy = LINDFS_ROOT / cwasm.name
    cleanup([native_bin, cwasm, lindfs_copy])

    if native_rc != 0:
        print(f"  [FAIL] native exited {native_rc} (expected 0 -- test itself is broken, not COW-related)")
        print(f"  native output:\n{native_out}")
        return False

    if wasm_rc != 0:
        print(f"  [FAIL] wasm exited {wasm_rc} (expected 0)")
        print(f"  wasm output:\n{wasm_out}")
        return False

    if native_out.strip() != wasm_out.strip():
        print("  [FAIL] native/wasm output mismatch")
        print(f"  native:\n{native_out}\n  wasm:\n{wasm_out}")
        return False

    print("  [PASS]")
    return True


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("tests", nargs="*", help="test stems to run (default: all)")
    parser.add_argument("--timeout", type=int, default=DEFAULT_TIMEOUT)
    args = parser.parse_args()

    all_tests = sorted(TEST_DIR.glob("*.c"))
    if args.tests:
        wanted = set(args.tests)
        all_tests = [t for t in all_tests if t.stem in wanted]
        missing = wanted - {t.stem for t in all_tests}
        if missing:
            print(f"Unknown test stem(s): {sorted(missing)}")
            sys.exit(2)

    if not all_tests:
        print(f"No tests found under {TEST_DIR}")
        sys.exit(1)

    results = {}
    start = time.time()
    for src in all_tests:
        results[src.stem] = run_one(src, args.timeout)

    elapsed = time.time() - start
    passed = sum(1 for v in results.values() if v)
    total = len(results)
    print(f"\n{passed}/{total} passed in {elapsed:.1f}s")
    if passed != total:
        print("FAILED: " + ", ".join(k for k, v in results.items() if not v))
        sys.exit(1)


if __name__ == "__main__":
    main()
