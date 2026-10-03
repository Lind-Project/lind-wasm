#!/usr/bin/env python3
"""Derives Gate 6's staged-OpenBLAS-testing report from raw run logs --
issue #22's OpenBLAS inference-to-runtime integration, Gate 6: "staged
real OpenBLAS testing".

Takes the Gate 4 manifest (the authoritative list of which symbols were
actually generated, and whether each came from the static or LLM track)
together with the raw captured output of each real run
build_gate6_openblas_report.sh produces, and turns them into a single
machine-readable report: per-run RESULTS counts, Gate 5's own
execution_oracle.classify() verdict for each interposed run, the combined
set of real symbols actually dispatched (via execution_oracle.parse_call_
counts, not hand-written regexes), and which generated symbols were never
reached. This is the mechanical derivation local-notes/active/gate6-
staged-real-openblas-testing.md's own prose report is generated FROM, not
a second, independent source of truth -- re-running the driver script and
this report builder must reproduce the same numbers from the same inputs.
"""
import argparse
import json
import os
import re
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import execution_oracle as oracle  # noqa: E402

_RESULTS_RE = re.compile(r"RESULTS: (\d+) tests \((\d+) ok, (\d+) failed, (\d+) skipped\) ran in (\d+) ms")


def parse_results_line(output):
    """Returns {"total":..,"ok":..,"failed":..,"skipped":..,"ms":..} from
    the first `RESULTS: ...` line in `output`, or None if absent (the
    suite crashed/trapped before ctest_main finished -- absence is itself
    meaningful evidence, not a parse failure)."""
    m = _RESULTS_RE.search(output)
    if not m:
        return None
    total, ok, failed, skipped, ms = (int(g) for g in m.groups())
    return {"total": total, "ok": ok, "failed": failed, "skipped": skipped, "ms": ms}


def load_run(path):
    """Reads one raw captured-output log plus its sidecar `<path>.exit`
    (the process exit code, written by the driver script) into a single
    dict. Returns None if `path` itself doesn't exist (an optional run
    the driver skipped)."""
    if not path or not os.path.isfile(path):
        return None
    with open(path) as fh:
        output = fh.read()
    exit_code = None
    exit_path = path + ".exit"
    if os.path.isfile(exit_path):
        with open(exit_path) as fh:
            exit_code = int(fh.read().strip())
    results = parse_results_line(output)
    return {
        "log_path": os.path.relpath(path),
        "exit_code": exit_code,
        "results": results,
        "call_counts": oracle.parse_call_counts(output),
        "ptr_sizes": oracle.parse_ptr_sizes(output),
    }


def classify_run(run, required_symbols):
    """Gate 5's own taxonomy, applied at whole-run granularity: the
    harness here can only observe "did this suite's RESULTS line show
    zero failures" and "did the process trap before producing one", not
    a per-test interposition verdict -- classify() still earns its keep
    by making those two structured facts (not regex-guessed ones) decide
    the verdict, and by requiring real dispatch evidence (any one of
    `required_symbols` with call_counts > 0) before ever calling a run
    PASS_INTERPOSED."""
    if run is None:
        return None
    trapped = run["results"] is None
    numeric_ok = (not trapped) and run["results"]["failed"] == 0
    return oracle.classify(
        required_symbols=required_symbols,
        numeric_ok=numeric_ok,
        trapped=trapped,
        call_counts=run["call_counts"],
    )


def build_report(manifest_path, runs):
    """`runs` maps a fixed set of run names (see main()'s --*-log flags)
    to raw log paths; missing/None entries are simply omitted from the
    report. Returns the full report dict."""
    with open(manifest_path) as fh:
        manifest = json.load(fh)
    symbols_by_name = {s["symbol"]: s for s in manifest["symbols"]}
    v2_symbols = sorted(manifest["v2_generated_symbols"])

    loaded = {name: load_run(path) for name, path in runs.items()}

    combined_counts = {}
    for name, run in loaded.items():
        if run is None:
            continue
        for sym, count in run["call_counts"].items():
            combined_counts[sym] = combined_counts.get(sym, 0) + count

    reached = sorted(set(combined_counts) & set(v2_symbols))
    never_reached = sorted(set(v2_symbols) - set(combined_counts))
    unexpected = sorted(set(combined_counts) - set(v2_symbols))
    static_reached = sorted(s for s in reached if symbols_by_name.get(s, {}).get("source") == "static")
    llm_reached = sorted(s for s in reached if symbols_by_name.get(s, {}).get("source") == "llm")

    report = {
        "manifest_path": os.path.relpath(manifest_path),
        "manifest_totals": manifest["totals"],
        "runs": {},
        "combined": {
            "v2_generated_total": len(v2_symbols),
            "reached": reached,
            "reached_count": len(reached),
            "never_reached": never_reached,
            "never_reached_count": len(never_reached),
            "unexpected_symbols_not_in_manifest": unexpected,
            "static_derived_reached": static_reached,
            "llm_derived_reached": llm_reached,
            "call_counts": combined_counts,
        },
    }
    for name, run in loaded.items():
        if run is None:
            report["runs"][name] = None
            continue
        entry = {
            "log_path": run["log_path"],
            "exit_code": run["exit_code"],
            "results": run["results"],
            "unique_symbols_observed": sorted(run["call_counts"]),
            "call_counts": run["call_counts"],
        }
        if name.startswith("strict_") or name.startswith("mixed_"):
            entry["classification"] = classify_run(run, v2_symbols)
        report["runs"][name] = entry
    return report


def main():
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--manifest", required=True, help="Gate 4 manifest.json path")
    ap.add_argument("--out", required=True, help="Where to write the JSON report")
    for name in (
        "baseline-utest", "baseline-utest-ext",
        "strict-utest", "strict-utest-ext",
        "mixed-utest", "mixed-utest-ext",
    ):
        ap.add_argument(f"--{name}-log", default=None,
                         help=f"Raw captured output for the {name.replace('-', ' ')} run (optional)")
    args = ap.parse_args()

    runs = {
        "baseline_utest": args.baseline_utest_log,
        "baseline_utest_ext": args.baseline_utest_ext_log,
        "strict_utest": args.strict_utest_log,
        "strict_utest_ext": args.strict_utest_ext_log,
        "mixed_utest": args.mixed_utest_log,
        "mixed_utest_ext": args.mixed_utest_ext_log,
    }
    report = build_report(args.manifest, runs)
    with open(args.out, "w") as fh:
        json.dump(report, fh, indent=2, sort_keys=True)
        fh.write("\n")

    print(f"[gate6_report] wrote {args.out}")
    print(f"[gate6_report] combined: {report['combined']['reached_count']}/"
          f"{report['combined']['v2_generated_total']} V2-generated symbols observed "
          f"({len(report['combined']['static_derived_reached'])} static, "
          f"{len(report['combined']['llm_derived_reached'])} llm)")
    for name, entry in report["runs"].items():
        if entry is None:
            continue
        res = entry["results"]
        res_str = (f"{res['ok']}/{res['total']} ok, {res['failed']} failed, {res['skipped']} skipped"
                   if res else "no RESULTS line (trapped/crashed)")
        cls = entry.get("classification")
        cls_str = f" [{cls}]" if cls else ""
        print(f"[gate6_report]   {name}: {res_str}{cls_str}")


if __name__ == "__main__":
    main()
