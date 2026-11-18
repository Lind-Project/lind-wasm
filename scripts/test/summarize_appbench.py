#!/usr/bin/env python3
"""Summarize collect_appbench.sh results: median, min and max per workload and mode.

Values come from each run's output (lmbench latency in microseconds or bandwidth in MB/s) or
from the wall-clock time the script measured (ms). Runs that failed, or whose output has no
value, are left out and counted.

Usage: summarize_appbench.py [--csv FILE] RESULTS_DIR
"""
import argparse
import csv
import os
import re
import statistics
import sys

NUM = r"(-?\d+(?:\.\d+)?(?:[eE][-+]?\d+)?)"
US = re.compile(NUM + r"\s+microseconds")
MBPS = re.compile(NUM + r"\s+MB/sec")
ANY = re.compile(NUM)
UNITS = {"us": "us", "MBps": "MB/s", "wall": "ms"}
MODES = ["native", "lind", "grate"]


GRATE_TRAILER = re.compile(r"^\[testing-grate\]", re.M)


def value_of(metric, wall_ns, text):
    if metric == "wall":
        return wall_ns / 1e6
    # testing-grate prints its own result (e.g. "[testing-grate] result: Ok(\n    0,\n)") when
    # the cage exits; it's not the workload's output.
    m = GRATE_TRAILER.search(text)
    if m:
        text = text[: m.start()]
    pattern = US if metric == "us" else MBPS
    found = pattern.findall(text)
    if found:
        return float(found[-1])
    # lmbench tools such as lat_mmap and bw_file_rd print "<size> <value>"
    for line in reversed(text.strip().splitlines()):
        nums = ANY.findall(line)
        if nums and not line.lstrip().startswith("["):
            return float(nums[-1])
    return None


def main():
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("dir")
    ap.add_argument("--csv")
    args = ap.parse_args()

    order, metrics, vals, fails = [], {}, {}, {}
    with open(os.path.join(args.dir, "results.tsv")) as f:
        for row in csv.DictReader(f, delimiter="\t"):
            w, mode = row["workload"], row["mode"]
            if w not in metrics:
                order.append(w)
                metrics[w] = row["metric"]
            key = (w, mode)
            vals.setdefault(key, [])
            text = ""
            try:
                with open(os.path.join(args.dir, row["output"]), errors="replace") as fo:
                    text = fo.read()
            except OSError:
                pass
            v = value_of(row["metric"], int(row["wall_ns"]), text) if row["exit"] == "0" else None
            if v is None:
                fails[key] = fails.get(key, 0) + 1
            else:
                vals[key].append(v)

    def med(w, m):
        xs = vals.get((w, m)) or []
        return statistics.median(xs) if xs else None

    def fmt(x, width=10):
        if x is None:
            return f"{'-':>{width}}"
        return f"{x:>{width}.4g}" if abs(x) < 1000 else f"{x:>{width}.0f}"

    print("lind-perf app benchmarks: median per workload and mode (lower is better, except MB/s)")
    print("lind-native and grate-lind are differences of medians; x is the ratio.\n")
    head = f"{'workload':<20} {'unit':<5} {'native':>10} {'lind':>10} {'grate':>10}  {'lind-native':>12} {'x':>6}  {'grate-lind':>11} {'x':>6}"
    print(head)
    print("-" * len(head))
    table = []
    for w in order:
        unit = UNITS.get(metrics[w], metrics[w])
        n, l, g = med(w, "native"), med(w, "lind"), med(w, "grate")
        ln = (l - n) if (l is not None and n is not None) else None
        lnx = (l / n) if (l is not None and n) else None
        gl = (g - l) if (g is not None and l is not None) else None
        glx = (g / l) if (g is not None and l) else None
        print(
            f"{w:<20} {unit:<5} {fmt(n)} {fmt(l)} {fmt(g)}  {fmt(ln, 12)} {fmt(lnx, 6)}  {fmt(gl, 11)} {fmt(glx, 6)}"
        )
        table.append([w, unit, n, l, g, ln, lnx, gl, glx])

    print("\nSpread: median [min .. max] (runs used / failed)")
    for w in order:
        parts = []
        for m in MODES:
            xs = vals.get((w, m))
            if xs is None and (w, m) not in fails:
                continue
            xs = xs or []
            f = fails.get((w, m), 0)
            if xs:
                parts.append(f"{m} {statistics.median(xs):.4g} [{min(xs):.4g} .. {max(xs):.4g}] ({len(xs)}/{f})")
            else:
                parts.append(f"{m} - ({0}/{f})")
        print(f"  {w:<20} " + "; ".join(parts))

    if args.csv:
        with open(args.csv, "w", newline="") as f:
            cw = csv.writer(f)
            cw.writerow(["workload", "unit", "native", "lind", "grate", "lind_minus_native",
                         "lind_over_native", "grate_minus_lind", "grate_over_lind"])
            for r in table:
                cw.writerow([r[0], r[1]] + ["" if x is None else f"{x:.6g}" for x in r[2:]])
    return 0


if __name__ == "__main__":
    sys.exit(main())
