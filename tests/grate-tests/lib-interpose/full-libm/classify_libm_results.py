#!/usr/bin/env python3
"""Classify run_libm_tests.sh results into the categories used in LIBM_INTERPOSITION.md:
CLEAN / FLAG_ONLY (numerically correct, only FP-exception-flag/errno checks differ) /
REAL_FAIL (genuine bug) / CRASH. Optionally diffs two results dirs (e.g. baseline vs grate).

--detail additionally splits FLAG_ONLY into FLAG_ONLY_EXCEPTION / FLAG_ONLY_ERRNO /
FLAG_ONLY_BOTH, based on which kind(s) of flag-only "Failure:" line appear in the file
(a file with a genuine numeric failure still classifies as REAL_FAIL regardless of
what else it also fails on -- this only subdivides files that are otherwise pure
FLAG_ONLY).

Usage:
  python3 classify_libm_results.py generated/results
  python3 classify_libm_results.py generated/results generated/results_libm_full_grate
  python3 classify_libm_results.py generated/results --detail
"""
import re
import sys
import glob
from collections import Counter


def is_flagonly(line):
    return bool(re.search(r'Exception "[^"]+" (not set|set)', line)) or "errno" in line.lower()


def line_kind(line):
    """Classify a single 'Failure: ...' line: 'exception', 'errno', 'both'
    (line mentions both), or 'real' (neither -- a genuine numeric mismatch)."""
    has_exc = bool(re.search(r'Exception "[^"]+" (not set|set)', line))
    has_errno = "errno" in line.lower()
    if has_exc and has_errno:
        return "both"
    if has_exc:
        return "exception"
    if has_errno:
        return "errno"
    return "real"


def classify(path):
    txt = open(path).read()
    if "All tests passed successfully" in txt:
        return "CLEAN"
    if re.search(r"\d+ errors? occurred", txt):
        fails = re.findall(r"^Failure: .*$", txt, re.M)
        real = [fl for fl in fails if not is_flagonly(fl)]
        return "FLAG_ONLY" if not real else "REAL_FAIL"
    return "CRASH"


def classify_detailed(path):
    """Same as classify(), but splits FLAG_ONLY by which failure-line kind(s)
    (exception-flag, errno, or a mix of both) appear in the file."""
    txt = open(path).read()
    if "All tests passed successfully" in txt:
        return "CLEAN"
    if re.search(r"\d+ errors? occurred", txt):
        fails = re.findall(r"^Failure: .*$", txt, re.M)
        kinds = set(line_kind(fl) for fl in fails)
        if "real" in kinds:
            return "REAL_FAIL"
        if kinds == {"exception"}:
            return "FLAG_ONLY_EXCEPTION"
        if kinds == {"errno"}:
            return "FLAG_ONLY_ERRNO"
        return "FLAG_ONLY_BOTH"  # {"both"}, or a mix of {"exception","errno"}
    return "CRASH"


def load(d, detail=False):
    fn = classify_detailed if detail else classify
    out = {}
    for f in sorted(glob.glob(f"{d}/*.result")):
        name = f.split("/")[-1].replace(".result", "")
        out[name] = fn(f)
    return out


def main():
    args = [a for a in sys.argv[1:] if not a.startswith("--")]
    detail = "--detail" in sys.argv
    if not args:
        print(__doc__)
        sys.exit(1)
    a = load(args[0], detail)
    print(f"=== {args[0]} ({len(a)} tests) ===")
    print(Counter(a.values()))

    if len(args) > 1:
        b = load(args[1], detail)
        print(f"\n=== {args[1]} ({len(b)} tests) ===")
        print(Counter(b.values()))
        changed = [(n, a.get(n), b.get(n)) for n in sorted(set(a) | set(b)) if a.get(n) != b.get(n)]
        print(f"\n=== diff: {len(changed)} changed ===")
        for n, x, y in changed:
            print(f"  {n}: {x} -> {y}")


if __name__ == "__main__":
    main()
