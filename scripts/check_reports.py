import glob
import json
import sys
import argparse

# Parse optional debug flag from CLI
# When enabled, prints detailed report inspection info
parser = argparse.ArgumentParser()
parser.add_argument("--debug", action="store_true")
args = parser.parse_args()

DEBUG = args.debug


def dprint(*args, **kwargs):
    """
    Debug print helper.
    Only prints when DEBUG flag is enabled to avoid polluting CI output.
    """
    if DEBUG:
        print(*args, **kwargs)


# Collect all JSON reports under reports/
paths = sorted(glob.glob("reports/*.json"))
dprint("DEBUG: report paths =", paths)


def count_failures(node):
    """
    Recursively count failures in a JSON report.

    Rules:
    - If a node has 'number_of_failures', treat it as authoritative
      but still compare with nested counts (take max for safety)
    - Otherwise, aggregate failures from children
    - Handles mixed nested structures (dict/list)

    This makes the checker robust to different report schemas.
    """
    if isinstance(node, list):
        return sum(count_failures(x) for x in node)

    if not isinstance(node, dict):
        return 0

    # Direct failure count (if present)
    direct = node.get("number_of_failures")
    try:
        direct_val = int(direct) if direct is not None else None
    except (TypeError, ValueError):
        direct_val = None

    # Recursively count nested failures
    nested = sum(count_failures(v) for v in node.values())

    # Prefer explicit count but guard against under-reporting
    return nested if direct_val is None else max(direct_val, nested)


# If no reports exist, treat as failure (fail closed)
total = 1 if not paths else 0

for path in paths:
    dprint(f"\n===== DEBUG REPORT: {path} =====")

    with open(path, encoding="utf-8") as f:
        data = json.load(f)

    # Print top-level structure for debugging schema issues
    if isinstance(data, dict):
        dprint("top-level keys =", list(data.keys()))
        dprint("top-level number_of_failures =", data.get("number_of_failures"))
        dprint("top-level error =", data.get("error"))
        dprint(
            "top-level results type =",
            type(data.get("results")).__name__ if "results" in data else "MISSING",
        )
    else:
        dprint("root type =", type(data).__name__)

    failures = count_failures(data)
    dprint("computed failures =", failures)

    total += failures


# Final aggregated result
dprint(f"\nDEBUG: total_failures={total}")

# Exit code:
# 0 -> success (no failures)
# 1 -> failure detected
sys.exit(1 if total else 0)
