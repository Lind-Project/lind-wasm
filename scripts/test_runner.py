#!/usr/bin/env python3
"""Unified Python test runner for Lind test harnesses.

Behavior:
- Discovers harness modules in scripts/harnesses/.
- Executes each module exposing run_harness(...).
- Provides a shared subprocess echo helper that harnesses can reuse.
- Writes each harness JSON payload to reports/<harness>.json (or module override).
- Writes optional HTML payloads when provided by a harness.
- Generates a combined reports/report.html that includes all executed harnesses.
"""

from __future__ import annotations

import argparse
import importlib
import inspect
import json
import re
import shutil
import subprocess
from pathlib import Path
from typing import Any

DEFAULT_REPORTS_DIR = Path("reports")
HARNESS_PACKAGE = "harnesses"
HARNESS_DIR = Path(__file__).resolve().parent / HARNESS_PACKAGE


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description="Unified test harness runner")
    parser.add_argument(
        "--reports-dir",
        type=Path,
        default=DEFAULT_REPORTS_DIR,
        help="Directory where consolidated reports are written.",
    )
    parser.add_argument(
        "--harness",
        action="append",
        dest="harnesses",
        help="Only run specific harness module name(s), e.g. --harness wasmtestreport",
    )
    parser.add_argument(
        "--export-report",
        type=Path,
        help="Optional path to copy combined reports/report.html for external export.",
    )
    parser.add_argument(
        "harness_args",
        nargs=argparse.REMAINDER,
        help=(
            "Arguments forwarded to harnesses that accept pass-through args. "
            "Prefix forwarded options with '--'. Example: test_runner.py -- --timeout 30"
        ),
    )
    return parser.parse_args()


def discover_harness_modules(selected: set[str] | None = None) -> list[str]:
    modules: list[str] = []
    for path in sorted(HARNESS_DIR.glob("*.py")):
        name = path.stem
        if name == "__init__" or name.startswith("_"):
            continue
        if selected and name not in selected:
            continue
        modules.append(name)
    return modules


def execute_with_echo(command: list[str], cwd: Path, prefix: str) -> tuple[int, str]:
    """Run command and stream output lines with a prefix.

    Returns:
        tuple(return_code, combined_output)
    """
    output_lines: list[str] = []
    proc = subprocess.Popen(
        command,
        cwd=cwd,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        text=True,
        bufsize=1,
    )

    assert proc.stdout is not None
    for line in proc.stdout:
        print(f"[{prefix}] {line}", end="")
        output_lines.append(line)

    proc.wait()
    return proc.returncode, "".join(output_lines)


def run_harness(module_name: str, forward_args: list[str]) -> dict[str, Any]:
    module = importlib.import_module(f"{HARNESS_PACKAGE}.{module_name}")
    runner = getattr(module, "run_harness", None)
    if runner is None or not callable(runner):
        raise RuntimeError(f"Harness module '{module_name}' does not define callable run_harness(...)")

    kwargs: dict[str, Any] = {"forward_args": forward_args}
    signature = inspect.signature(runner)
    if "execute_with_echo" in signature.parameters:
        kwargs["execute_with_echo"] = execute_with_echo

    result = runner(**kwargs)
    if not isinstance(result, dict):
        raise RuntimeError(f"Harness module '{module_name}' returned non-dict result")

    if "report" not in result:
        raise RuntimeError(f"Harness module '{module_name}' result must include 'report'")

    return result


def write_outputs(result: dict[str, Any], reports_dir: Path) -> dict[str, Any]:
    harness_name = str(result.get("name", "harness"))

    json_filename = str(result.get("json_filename", f"{harness_name}.json"))
    json_path = reports_dir / json_filename
    json_path.write_text(json.dumps(result["report"], indent=2), encoding="utf-8")

    html_path: Path | None = None
    html_payload = result.get("html")
    if html_payload is not None:
        html_filename = str(result.get("html_filename", f"{harness_name}.html"))
        html_path = reports_dir / html_filename
        html_path.write_text(str(html_payload), encoding="utf-8")

    return {
        "name": harness_name,
        "json_path": json_path,
        "html_path": html_path,
        "report": result["report"],
    }


def extract_html_body(raw_html: str) -> str:
    match = re.search(r"(?is)<\s*body\b[^>]*>(.*?)</\s*body\s*>", raw_html)
    return match.group(1) if match else raw_html


def generate_combined_report(harness_outputs: list[dict[str, Any]], reports_dir: Path) -> Path:
    sections: list[str] = []
    for output in harness_outputs:
        name = output["name"]
        json_path: Path = output["json_path"]
        html_path: Path | None = output["html_path"]

        if html_path and html_path.exists():
            body = extract_html_body(html_path.read_text(encoding="utf-8"))
        else:
            body = (
                "<p>No harness HTML report was provided. "
                f"See JSON output at <code>{json_path.name}</code>.</p>"
            )

        sections.append(
            f"""
<section class=\"test-section\"> 
  <h2>{name} harness</h2>
  <div class=\"harness-content\">{body}</div>
</section>
"""
        )

    combined = f"""<!DOCTYPE html>
<html>
  <head>
    <meta charset=\"UTF-8\">
    <title>Unified Test Report</title>
    <style>
      body {{ font-family: Arial, sans-serif; margin: 20px; }}
      .test-section {{ margin: 24px 0; border: 2px solid #333; border-radius: 8px; padding: 16px; }}
      .test-section h2 {{ margin-top: 0; }}
    </style>
  </head>
  <body>
    <h1>Unified Test Report</h1>
    {''.join(sections)}
  </body>
</html>
"""

    combined_path = reports_dir / "report.html"
    combined_path.write_text(combined, encoding="utf-8")
    return combined_path


def main() -> None:
    args = parse_args()
    reports_dir = args.reports_dir
    reports_dir.mkdir(parents=True, exist_ok=True)

    passthrough_args = args.harness_args
    if passthrough_args and passthrough_args[0] == "--":
        passthrough_args = passthrough_args[1:]

    selected = set(args.harnesses) if args.harnesses else None
    harness_modules = discover_harness_modules(selected=selected)
    if not harness_modules:
        raise RuntimeError("No harness modules found to run.")

    print(f"Discovered harnesses: {', '.join(harness_modules)}")

    harness_outputs: list[dict[str, Any]] = []
    for module_name in harness_modules:
        print(f"Running harness: {module_name}")
        result = run_harness(module_name, passthrough_args)
        output_info = write_outputs(result, reports_dir)
        harness_outputs.append(output_info)

        print(f"Wrote {output_info['json_path']}")
        if output_info["html_path"] is not None:
            print(f"Wrote {output_info['html_path']}")

    combined_path = generate_combined_report(harness_outputs, reports_dir)
    print(f"Wrote {combined_path}")

    if args.export_report:
        export_path = args.export_report
        export_path.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(combined_path, export_path)
        print(f"Exported combined report to {export_path}")


if __name__ == "__main__":
    main()
