import glob
import json
import sys

paths = sorted(glob.glob("reports/*.json"))
print("DEBUG: report paths =", paths)

def count_failures(node):
    if isinstance(node, list):
        return sum(count_failures(x) for x in node)
    if not isinstance(node, dict):
        return 0

    direct = node.get("number_of_failures")
    try:
        direct_val = int(direct) if direct is not None else None
    except (TypeError, ValueError):
        direct_val = None

    nested = sum(count_failures(v) for v in node.values())
    return nested if direct_val is None else max(direct_val, nested)

total = 1 if not paths else 0

for path in paths:
    print(f"\n===== DEBUG REPORT: {path} =====")
    with open(path, encoding="utf-8") as f:
        data = json.load(f)

    if isinstance(data, dict):
        print("top-level keys =", list(data.keys()))
        print("top-level number_of_failures =", data.get("number_of_failures"))
        print("top-level error =", data.get("error"))
        print(
            "top-level results type =",
            type(data.get("results")).__name__ if "results" in data else "MISSING",
        )
    else:
        print("root type =", type(data).__name__)

    failures = count_failures(data)
    print("computed failures =", failures)
    total += failures

print(f"\nDEBUG: total_failures={total}")
sys.exit(1 if total else 0)
