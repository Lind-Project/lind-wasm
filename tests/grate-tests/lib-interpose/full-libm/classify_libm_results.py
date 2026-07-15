#!/usr/bin/env python3
"""Classify run_libm_tests.sh results into the categories used in LIBM_INTERPOSITION.md:
CLEAN / FLAG_ONLY (numerically correct, only FP-exception-flag/errno checks differ) /
REAL_FAIL (genuine bug) / CRASH. Optionally diffs two results dirs (e.g. baseline vs grate).

Usage:
  python3 classify_libm_results.py generated/results
  python3 classify_libm_results.py generated/results generated/results_libm_full_grate
"""
import re
import sys
import glob
from collections import Counter


def is_flagonly(line):
    return bool(re.search(r'Exception "[^"]+" (not set|set)', line)) or "errno" in line.lower()


def classify(path):
    txt = open(path).read()
    if "All tests passed successfully" in txt:
        return "CLEAN"
    if re.search(r"\d+ errors? occurred", txt):
        fails = re.findall(r"^Failure: .*$", txt, re.M)
        real = [fl for fl in fails if not is_flagonly(fl)]
        return "FLAG_ONLY" if not real else "REAL_FAIL"
    return "CRASH"


def load(d):
    out = {}
    for f in sorted(glob.glob(f"{d}/*.result")):
        name = f.split("/")[-1].replace(".result", "")
        out[name] = classify(f)
    return out


def main():
    if len(sys.argv) < 2:
        print(__doc__)
        sys.exit(1)
    a = load(sys.argv[1])
    print(f"=== {sys.argv[1]} ({len(a)} tests) ===")
    print(Counter(a.values()))

    if len(sys.argv) > 2:
        b = load(sys.argv[2])
        print(f"\n=== {sys.argv[2]} ({len(b)} tests) ===")
        print(Counter(b.values()))
        changed = [(n, a.get(n), b.get(n)) for n in sorted(set(a) | set(b)) if a.get(n) != b.get(n)]
        print(f"\n=== diff: {len(changed)} changed ===")
        for n, x, y in changed:
            print(f"  {n}: {x} -> {y}")


if __name__ == "__main__":
    main()
