#!/usr/bin/env python3
"""Unified Python test runner for Lind test harnesses.

Behavior:
- Discovers harness modules in scripts/harnesses/.
- Executes each module exposing run_harness(...).
- Provides a shared subprocess echo helper that harnesses can reuse.
- Writes each harness JSON payload to reports/<harness>.json (or module override).
- Writes optional HTML payloads when provided by a harness.
"""

from __future__ import annotations

import argparse
import importlib
import inspect
import json
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


def write_outputs(result: dict[str, Any], reports_dir: Path) -> list[Path]:
    written: list[Path] = []
    harness_name = str(result.get("name", "harness"))

    json_filename = str(result.get("json_filename", f"{harness_name}.json"))
    json_path = reports_dir / json_filename
    json_path.write_text(json.dumps(result["report"], indent=2), encoding="utf-8")
    written.append(json_path)

    html_payload = result.get("html")
    if html_payload is not None:
        html_filename = str(result.get("html_filename", f"{harness_name}.html"))
        html_path = reports_dir / html_filename
        html_path.write_text(str(html_payload), encoding="utf-8")
        written.append(html_path)

    return written


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

    for module_name in harness_modules:
        print(f"Running harness: {module_name}")
        result = run_harness(module_name, passthrough_args)
        for output_path in write_outputs(result, reports_dir):
            print(f"Wrote {output_path}")


if __name__ == "__main__":
    main()