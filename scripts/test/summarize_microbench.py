#!/usr/bin/env python3
"""Summarize repeated run_microbench.sh outputs.

For every benchmark and every counter (and DERIVED line), prints the median, min and max of
net/call across the given runs, plus the spread (max - min). A large spread means a run was
disturbed; rerun or look at that run's file.

Usage: summarize_microbench.py [--csv FILE] run-1.txt run-2.txt ...
"""
import argparse
import csv
import re
import statistics
import sys

SEP = re.compile(r"^=+\s*$")
SECTION = re.compile(r"^-+\s+(.*?)\s+-+\s*$")
COUNTER = re.compile(
    r"^(\S*::\S+)\s+(\d+)\s+[\d.]+\s+\S+\s+[\d.]+\s+\S+\s+(-?[\d.]+)\s+(\S+)\s*$"
)
DERIVED = re.compile(r"^(.*\S)\s{2,}(-?[\d.]+)\s+(\S+)\s*$")
SYSCALL_ROW = re.compile(r"^(.+?)\s{2,}(cage|grate|fwd)\s+\d+\s+[\d.]+\s+\S+\s+[\d.]+%\s+(.*)$")
# Columns of the PER SYSCALL table after "share" (lind-boot perf.rs SYSCALL_COLUMNS).
SYSCALL_COLS = ["entry", "3i", "3i lookup", "rawposix", "3i call", "grate", "grate wasm"]
OVERHEAD = re.compile(r"^timer overhead per measurement: ([\d.]+) (\S+)")

# The lind-perf report picks a unit per value (ns below 1,000 ns, then us, ms, s), so one row can
# switch units between runs. Convert to ns before comparing; print back in a readable unit.
TO_NS = {"ns": 1.0, "us": 1e3, "\u00b5s": 1e3, "ms": 1e6, "s": 1e9}


def to_base(value, unit):
    """(value, unit) -> (value in ns, "ns"), or unchanged for units like cycles."""
    if unit in TO_NS:
        return value * TO_NS[unit], "ns"
    return value, unit


def fmt(v, unit):
    """Format a value in base unit `unit` the way the lind-perf report does."""
    if unit != "ns":
        return f"{v:9.1f}", unit
    a = abs(v)
    if a < 1e3:
        return f"{v:9.1f}", "ns"
    if a < 1e6:
        return f"{v / 1e3:9.3f}", "us"
    if a < 1e9:
        return f"{v / 1e6:9.3f}", "ms"
    return f"{v / 1e9:9.3f}", "s"


def parse(path):
    """Return {benchmark: {name: (value, unit, kind)}} for one run's output."""
    lines = open(path, errors="replace").read().splitlines()
    out = {}
    bench = None
    section = ""
    i = 0
    while i < len(lines):
        line = lines[i]
        if SEP.match(line) and i + 2 < len(lines) and SEP.match(lines[i + 2]):
            bench = lines[i + 1].strip()
            out.setdefault(bench, {})
            section = ""
            i += 3
            continue
        i += 1
        if bench is None:
            continue
        m = SECTION.match(line)
        if m:
            section = m.group(1)
            continue
        m = OVERHEAD.match(line)
        if m:
            out[bench]["timer overhead"] = (*to_base(float(m.group(1)), m.group(2)), "overhead")
            continue
        if section.startswith("DERIVED"):
            m = DERIVED.match(line)
            if m:
                out[bench][m.group(1)] = (*to_base(float(m.group(2)), m.group(3)), "derived")
            continue
        if section.startswith("PER SYSCALL"):
            m = SYSCALL_ROW.match(line)
            if m:
                toks = m.group(3).split()
                vals = []
                while toks:
                    t = toks.pop(0)
                    if t == "-":
                        vals.append(None)
                    else:
                        vals.append((float(t), toks.pop(0) if toks else ""))
                for col, v in zip(SYSCALL_COLS, vals):
                    if v is not None:
                        name = f"{m.group(1)} [{m.group(2)}] {col}"
                        out[bench][name] = (*to_base(v[0], v[1]), "per-syscall")
            continue
        m = COUNTER.match(line)
        if m:
            out[bench][m.group(1)] = (*to_base(float(m.group(3)), m.group(4)), "counter")
    return out


def main():
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("runs", nargs="+")
    ap.add_argument("--csv", help="also write the summary as CSV")
    args = ap.parse_args()

    runs = [parse(p) for p in args.runs]
    benches, names, values = [], {}, {}
    for run in runs:
        for b, rows in run.items():
            if b not in names:
                benches.append(b)
                names[b] = []
            for n, (v, unit, kind) in rows.items():
                if n not in names[b]:
                    names[b].append(n)
                values.setdefault((b, n), []).append((v, unit, kind))

    print(f"lind-perf summary: {len(runs)} runs; net per call; median [min .. max] spread")
    print("(values converted to ns before comparing; the CSV is in ns, or cycles with --perf=tsc)")
    table = []
    for b in benches:
        print()
        print("=" * 100)
        print(f" {b}")
        print("=" * 100)
        for n in names[b]:
            vals = values[(b, n)]
            xs = [v for v, _, _ in vals]
            unit, kind = vals[0][1], vals[0][2]
            med, lo, hi = statistics.median(xs), min(xs), max(xs)
            label = n if kind == "counter" else f"{n} ({kind})"
            # all four numbers in the median's unit, so they compare at a glance
            m_s, u = fmt(med, unit)
            scale = 1.0 if unit != "ns" else {"ns": 1.0, "us": 1e3, "ms": 1e6, "s": 1e9}[u]
            d = 1 if u in ("ns", unit) and unit != "ns" or u == "ns" else 3
            print(
                f"{label[:60]:<60} {m_s} [{lo / scale:8.{d}f} .. {hi / scale:8.{d}f}] "
                f"{(hi - lo) / scale:7.{d}f} {u}"
                + ("" if len(xs) == len(runs) else f"   (in {len(xs)}/{len(runs)} runs)")
            )
            table.append([b, n, kind, unit, len(xs), f"{med:.1f}", f"{lo:.1f}", f"{hi:.1f}"])

    if args.csv:
        with open(args.csv, "w", newline="") as f:
            w = csv.writer(f)
            w.writerow(["benchmark", "name", "kind", "unit", "runs", "median", "min", "max"])
            w.writerows(table)
    return 0


if __name__ == "__main__":
    sys.exit(main())
