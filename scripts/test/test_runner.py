#!/usr/bin/env python3
"""Unified Python test runner for Lind test harnesses.

Behavior:
- Discovers harness modules in scripts/test/harnesses/.
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
import os
import re
import shutil
import subprocess
from pathlib import Path
from typing import Any

DEFAULT_REPORTS_DIR = Path("reports")
HARNESS_PACKAGE = "harnesses"
HARNESS_DIR = Path(__file__).resolve().parent / HARNESS_PACKAGE

UNIT_TEST_CATEGORIES: dict[str, tuple[str, ...]] = {
    "math": (
        "math_tests",
    ),
    "filesystem": (
        "file_tests",
    ),
    "memory": (
        "memory_tests",
    ),
    "process": (
        "process_tests",
    ),
    "signals": (
        "signal_tests",
    ),
    "networking": (
        "networking_tests",
    ),
    "dynamic-linking": (
        "dylink_tests",
    ),
}

UNIT_TEST_CATEGORY_ORDER: tuple[str, ...] = tuple(UNIT_TEST_CATEGORIES)


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
        "--category",
        action="append",
        choices=UNIT_TEST_CATEGORY_ORDER,
        help=(
            "Run one or more unit-test categories. "
            "Repeat the option to select multiple categories."
        ),
    )
    parser.add_argument(
        "--no-staged",
        action="store_true",
        help=(
            "Run selected unit-test categories together instead of "
            "progressively stopping after the first failing category."
        ),
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


def normalize_args(parsed: argparse.Namespace | tuple[argparse.Namespace, list[str]] | list[Any]) -> argparse.Namespace:
    """Normalize parser outputs across accidental parse_* variants.

    Some broken merges/edits may return parse_known_args()-style outputs
    (namespace, extras) or list-wrapped namespace objects. Normalize to an
    argparse.Namespace so downstream code can rely on .export_report, etc.
    """
    if isinstance(parsed, argparse.Namespace):
        return parsed

    if isinstance(parsed, tuple) and parsed and isinstance(parsed[0], argparse.Namespace):
        return parsed[0]

    if isinstance(parsed, list) and parsed and isinstance(parsed[0], argparse.Namespace):
        return parsed[0]

    raise TypeError(f"Unexpected parse_args() return type: {type(parsed)!r}")


def ordered_categories(requested: list[str] | None) -> list[str]:
    """Return selected categories in canonical staged execution order."""
    if not requested:
        return list(UNIT_TEST_CATEGORY_ORDER)

    requested_set = set(requested)
    return [
        category
        for category in UNIT_TEST_CATEGORY_ORDER
        if category in requested_set
    ]


def category_folders(categories: list[str]) -> list[str]:
    """Expand category names into unit-test folder names."""
    return [
        folder
        for category in categories
        for folder in UNIT_TEST_CATEGORIES[category]
    ]


def remaining_categories(
    categories: list[str],
    completed: list[str],
    failed: str | None,
) -> list[str]:
    """Return categories skipped after staged execution stops."""
    return [
        category
        for category in categories
        if category not in completed and category != failed
    ]


def discover_harness_modules(selected: set[str] | None = None) -> list[str]:
    modules: list[str] = []
    for path in sorted(HARNESS_DIR.glob("*.py")):
        name = path.stem
        if name == "__init__" or name.startswith("_") or name == "libcpptestreport":
            continue
        if selected and name not in selected:
            continue
        modules.append(name)
    return modules


def _is_compact_test_progress_line(line: str) -> bool:
    """Return True for compact test-progress lines that should be passed through unchanged."""
    stripped = line.strip()

    if not stripped:
        return True

    if re.fullmatch(r"Running \d+ tests", stripped):
        return True

    if re.fullmatch(r"[.XS]+", stripped):
        return True

    if re.fullmatch(r"\d+ passed, \d+ failed(, \d+ skipped)?", stripped):
        return True

    if stripped == "Failures:":
        return True

    return False


def execute_with_echo(command: list[str], cwd: Path, prefix: str) -> tuple[int, str]:
    """Run command and stream output.

    Compact test-progress lines are passed through unchanged so targets such as
    `make test` show the same concise progress output as the underlying harness.
    Other lines keep a harness prefix for context.
    """
    output_lines: list[str] = []
    env = os.environ.copy()
    env.setdefault("LIND_DEBUG_PANIC", "panic-and-exit")
    env.setdefault("LIND_LOG_OUTPUT", "none")

    proc = subprocess.Popen(
        command,
        cwd=cwd,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        text=True,
        bufsize=1,
        env=env,
    )

    assert proc.stdout is not None
    for line in proc.stdout:
        if prefix == "wasmtestreport" or _is_compact_test_progress_line(line):
            print(line, end="")
        else:
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


def report_failure_count(report: dict[str, Any]) -> int:
    """Count failed test cases across all sections of a harness report."""
    failure_count = 0

    for section in report.values():
        if not isinstance(section, dict):
            continue

        test_cases = section.get("test_cases")
        if isinstance(test_cases, dict):
            for test_case in test_cases.values():
                if not isinstance(test_case, dict):
                    continue

                status = str(test_case.get("status", "")).lower()
                if status and status not in {"success", "skipped"}:
                    failure_count += 1
            continue

        number_of_failures = section.get("number_of_failures", 0)
        if isinstance(number_of_failures, int):
            failure_count += number_of_failures

    return failure_count


def report_has_failures(report: dict[str, Any]) -> bool:
    """Return True when a harness report contains failed test cases."""
    return report_failure_count(report) > 0



def build_wasm_category_summary(
    category_results: list[dict[str, Any]],
    failed_category: str | None,
    skipped_categories: list[str],
) -> dict[str, Any]:
    """Build the legacy wasm.json report for category-based runs."""
    categories: dict[str, Any] = {}
    completed_categories: list[str] = []

    for result in category_results:
        result_name = str(result.get("name", ""))
        category = result_name.removeprefix("wasm-")
        categories[category] = result["report"]

        if category != failed_category:
            completed_categories.append(category)

    return {
        "number_of_failures": sum(
            report_failure_count(result["report"])
            for result in category_results
        ),
        "completed_categories": completed_categories,
        "failed_category": failed_category,
        "skipped_categories": skipped_categories,
        "categories": categories,
    }


def run_wasm_categories(
    categories: list[str],
    passthrough_args: list[str],
    staged: bool = True,
) -> tuple[list[dict[str, Any]], str | None, list[str]]:
    """Run selected WASM unit-test categories.

    In staged mode, categories run in canonical order and execution stops
    after the first failing category.
    """
    if not staged:
        harness_args = [
            *passthrough_args,
            "--allow-pre-compiled",
            "--skip-libcpp",
            "--skip",
            "static_tests",
            "--run",
            *category_folders(categories),
        ]
        result = run_harness("wasmtestreport", harness_args)
        result["name"] = "wasm-selected-categories"
        result["json_filename"] = "wasm-selected-categories.json"
        result["html_filename"] = "wasm-selected-categories.html"
        failed = "combined" if report_has_failures(result["report"]) else None
        return [result], failed, []

    results: list[dict[str, Any]] = []
    completed: list[str] = []
    failed: str | None = None

    for category in categories:
        print(f"Running category: {category}")

        harness_args = [
            *passthrough_args,
            "--allow-pre-compiled",
            "--skip-libcpp",
            "--skip",
            "static_tests",
            "--run",
            *UNIT_TEST_CATEGORIES[category],
        ]

        try:
            result = run_harness("wasmtestreport", harness_args)
        except RuntimeError as error:
            failed = category
            print(f"Category failed: {category}")
            print(error)
            break

        result["name"] = f"wasm-{category}"
        result["json_filename"] = f"wasm-{category}.json"
        result["html_filename"] = f"wasm-{category}.html"

        results.append(result)

        if report_has_failures(result["report"]):
            failed = category
            print(f"Category failed: {category}")
            break

        completed.append(category)
        print(f"Category passed: {category}")

    skipped = remaining_categories(categories, completed, failed)

    print("Category summary:")
    print(f"  Completed: {', '.join(completed) if completed else 'none'}")
    print(f"  Failed: {failed or 'none'}")
    print(f"  Skipped: {', '.join(skipped) if skipped else 'none'}")

    return results, failed, skipped


def write_outputs(result: dict[str, Any], reports_dir: Path) -> dict[str, Any]:
    harness_name = str(result.get("name", "harness"))

    json_filename = str(result.get("json_filename", f"{harness_name}.json"))
    json_path = reports_dir / json_filename
    json_path.write_text(json.dumps(result["report"], indent=2), encoding="utf-8")

    html_payload = result.get("html")
    html_path: Path | None = None
    # Only write a separate HTML file when html_filename is explicitly provided by
    # the harness. Harnesses that omit html_filename still contribute their HTML
    # content to the combined report.html via the in-memory html_payload field.
    if html_payload is not None and "html_filename" in result:
        html_path = reports_dir / str(result["html_filename"])
        html_path.write_text(str(html_payload), encoding="utf-8")

    return {
        "name": harness_name,
        "json_path": json_path,
        "html_path": html_path,
        "html": html_payload,
        "report": result["report"],
    }


def extract_html_body(raw_html: str) -> str:
    # Greedy body: use the last </body> so literal </body> inside <pre> cannot truncate.
    match = re.search(r"(?is)<\s*body\b[^>]*>(.*)</\s*body\s*>", raw_html)
    return match.group(1) if match else raw_html


def generate_combined_report(harness_outputs: list[dict[str, Any]], reports_dir: Path) -> Path:
    sections: list[str] = []
    for output in harness_outputs:
        name = output["name"]
        json_path: Path = output["json_path"]
        html_payload: str | None = output.get("html")
        html_path: Path | None = output["html_path"]

        if html_payload is not None:
            body = extract_html_body(html_payload)
        elif html_path and html_path.exists():
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
    cli_args = normalize_args(parse_args())
    reports_dir = cli_args.reports_dir
    reports_dir.mkdir(parents=True, exist_ok=True)

    passthrough_args = cli_args.harness_args
    if passthrough_args and passthrough_args[0] == "--":
        passthrough_args = passthrough_args[1:]

    selected = set(cli_args.harnesses) if cli_args.harnesses else None
    harness_modules = discover_harness_modules(selected=selected)
    if not harness_modules:
        raise RuntimeError("No harness modules found to run.")

    print(f"Discovered harnesses: {', '.join(harness_modules)}")

    harness_outputs: list[dict[str, Any]] = []
    failed_category: str | None = None

    for module_name in harness_modules:
        if module_name == "wasmtestreport":
            categories = ordered_categories(cli_args.category)
            staged = not cli_args.no_staged

            print(
                "Running unit-test categories: "
                f"{', '.join(categories)} "
                f"({'staged' if staged else 'combined'})"
            )

            category_results, failed_category, skipped_categories = run_wasm_categories(
                categories,
                passthrough_args,
                staged=staged,
            )

            for result in category_results:
                output_info = write_outputs(result, reports_dir)
                harness_outputs.append(output_info)

                print(f"Wrote {output_info['json_path']}")
                if output_info["html_path"] is not None:
                    print(f"Wrote {output_info['html_path']}")

            wasm_summary = build_wasm_category_summary(
                category_results,
                failed_category,
                skipped_categories,
            )
            wasm_json_path = reports_dir / "wasm.json"
            wasm_json_path.write_text(
                json.dumps(wasm_summary, indent=2),
                encoding="utf-8",
            )
            print(f"Wrote {wasm_json_path}")

            continue

        print(f"Running harness: {module_name}")
        result = run_harness(module_name, list(passthrough_args))

        output_info = write_outputs(result, reports_dir)
        harness_outputs.append(output_info)

        print(f"Wrote {output_info['json_path']}")
        if output_info["html_path"] is not None:
            print(f"Wrote {output_info['html_path']}")

    combined_path = generate_combined_report(harness_outputs, reports_dir)
    print(f"Wrote {combined_path}")

    if cli_args.export_report:
        export_path = cli_args.export_report
        export_path.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(combined_path, export_path)
        print(f"Exported combined report to {export_path}")

    if failed_category is not None:
        raise SystemExit(
            f"Unit-test category '{failed_category}' failed; "
            "higher-level categories were skipped."
        )


if __name__ == "__main__":
    main()
