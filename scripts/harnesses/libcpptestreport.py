#!/usr/bin/env python3
"""Libc++ header / toolchain integration harness for the unified E2E report.

Runs `scripts/lind_compile_cpp --compile-only` on a small C++ smoke test.
Expects a full sysroot at `build/sysroot` (including libc++ merge), same as
Dockerfile `libcpp-test` / E2E `test` stage setup.
"""

from __future__ import annotations

import argparse
import html
import json
import logging
import os
import subprocess
import sys
import tempfile
from pathlib import Path
from typing import Any, Callable

logger = logging.getLogger("libcpptestreport")
logger.setLevel(logging.INFO)
_ch = logging.StreamHandler()
_ch.setFormatter(logging.Formatter("[%(levelname)s] %(message)s"))
logger.addHandler(_ch)

SCRIPT_DIR = Path(__file__).resolve().parent
REPO_ROOT = SCRIPT_DIR.parents[1]
DEFAULT_CPP_REL = Path("tests/unit-tests/cpp/hello.cpp")
JSON_OUTPUT = "libcpp.json"
HTML_OUTPUT = "libcpp_report.html"


def get_empty_result() -> dict[str, Any]:
    return {
        "total_test_cases": 0,
        "number_of_success": 0,
        "success": [],
        "number_of_failures": 0,
        "failures": [],
        "number_of_compile_failures": 0,
        "compile_failures": [],
        "test_cases": {},
    }


def add_test_result(result: dict[str, Any], test_name: str, status: str, error_type: str | None, output: str) -> None:
    result["total_test_cases"] += 1
    result["test_cases"][test_name] = {
        "status": status,
        "error_type": error_type,
        "output": output,
    }
    if status == "Success":
        result["number_of_success"] += 1
        result["success"].append(test_name)
        logger.info("SUCCESS: %s", test_name)
        return
    result["number_of_failures"] += 1
    result["failures"].append(test_name)
    if error_type == "Compile_Failure":
        result["number_of_compile_failures"] += 1
        result["compile_failures"].append(test_name)
    logger.error("FAILURE: %s — %s", test_name, error_type or "unknown")


def default_source_path() -> Path:
    override = os.environ.get("LIBCPP_TEST_CPP")
    if override:
        return Path(override).resolve()
    return (REPO_ROOT / DEFAULT_CPP_REL).resolve()


def run_compile(source: Path) -> tuple[int, str]:
    script = REPO_ROOT / "scripts" / "lind_compile_cpp"
    cmd = [str(script), "--compile-only", str(source)]
    try:
        proc = subprocess.run(
            cmd,
            capture_output=True,
            text=True,
            cwd=str(REPO_ROOT),
        )
    except OSError as exc:
        return 127, f"Exception running lind_compile_cpp: {exc}"
    combined = ""
    if proc.stdout:
        combined += proc.stdout
    if proc.stderr:
        if combined:
            combined += "\n"
        combined += proc.stderr
    return proc.returncode, combined


def run_libcpp_integration(result: dict[str, Any], source: Path) -> None:
    try:
        rel_name = str(source.relative_to(REPO_ROOT))
    except ValueError:
        rel_name = str(source)

    if not source.is_file():
        add_test_result(
            result,
            rel_name,
            "Failure",
            "Compile_Failure",
            f"Source file not found: {source}",
        )
        return

    wasm_path = source.parent / f"{source.name}.wasm"
    try:
        wasm_path.unlink(missing_ok=True)
    except OSError as exc:
        logger.warning("Could not remove prior wasm %s: %s", wasm_path, exc)

    rc, output = run_compile(source)
    if rc != 0:
        add_test_result(result, rel_name, "Failure", "Compile_Failure", f"exit={rc}\n{output}")
        return

    if not wasm_path.is_file():
        add_test_result(
            result,
            rel_name,
            "Failure",
            "Compile_Failure",
            f"Compiler exited 0 but wasm missing: {wasm_path}\n{output}",
        )
        return

    try:
        wasm_disp = str(wasm_path.relative_to(REPO_ROOT))
    except ValueError:
        wasm_disp = str(wasm_path)
    ok_msg = f"OK: {wasm_disp}\n{output}".strip()
    add_test_result(result, rel_name, "Success", None, ok_msg)


def generate_html_report(result: dict[str, Any]) -> str:
    rows: list[str] = []
    for test_name, test_result in sorted(result.get("test_cases", {}).items()):
        status = test_result.get("status", "Unknown")
        error_type = test_result.get("error_type") or ""
        out = html.escape(str(test_result.get("output", "")))
        rows.append(
            "<tr>"
            f"<td>{html.escape(test_name)}</td>"
            f"<td>{html.escape(status)}</td>"
            f"<td>{html.escape(error_type)}</td>"
            f"<td><pre>{out}</pre></td>"
            "</tr>"
        )

    return """<!DOCTYPE html>
<html><head><meta charset="UTF-8"><title>Libc++ integration report</title></head>
<body>
<h1>Libc++ integration report</h1>
<p>smoke test: <code>lind_compile_cpp --compile-only</code> on test file(s) in ./tests/unit-tests/cpp/... into .wasm.</p>
<table border="1" cellspacing="0" cellpadding="6">
<tr><th>Metric</th><th>Value</th></tr>
<tr><td>Total</td><td>{total}</td></tr>
<tr><td>Success</td><td>{success}</td></tr>
<tr><td>Failures</td><td>{failures}</td></tr>
<tr><td>Compile failures</td><td>{compile_failures}</td></tr>
</table>
<h2>Cases</h2>
<table border="1" cellspacing="0" cellpadding="6">
<tr><th>Test</th><th>Status</th><th>Error type</th><th>Output</th></tr>
{rows}
</table>
</body></html>
""".format(
        total=result.get("total_test_cases", 0),
        success=result.get("number_of_success", 0),
        failures=result.get("number_of_failures", 0),
        compile_failures=result.get("number_of_compile_failures", 0),
        rows="\n".join(rows) if rows else "<tr><td colspan='4'><em>No cases</em></td></tr>",
    )


def parse_arguments(argv: list[str] | None) -> argparse.Namespace:
    p = argparse.ArgumentParser(description="Libc++ integration smoke compile")
    p.add_argument("--output", default=JSON_OUTPUT, help="JSON report path")
    p.add_argument("--report", default=HTML_OUTPUT, help="HTML report path")
    p.add_argument("--clean-results", action="store_true", help="Delete outputs and exit")
    return p.parse_args(argv)


def run_report(argv: list[str] | None) -> dict[str, Any]:
    args = parse_arguments(argv)
    output_json = str(Path(args.output).with_suffix(".json"))
    output_html = str(Path(args.report).with_suffix(".html"))

    if args.clean_results:
        for path in (output_json, output_html):
            if os.path.isfile(path):
                os.remove(path)
        return get_empty_result()

    result = get_empty_result()
    run_libcpp_integration(result, default_source_path())

    Path(output_json).write_text(json.dumps(result, indent=2), encoding="utf-8")
    Path(output_html).write_text(generate_html_report(result), encoding="utf-8")
    logger.info("Wrote %s and %s", output_json, output_html)
    return result


def run_harness(
    forward_args: list[str] | None = None,
    execute_with_echo: Callable[[list[str], Path, str], tuple[int, str]] | None = None,
) -> dict[str, Any]:
    args = ["python3", str(Path(__file__).resolve())]
    if forward_args:
        args.extend(forward_args)

    with tempfile.TemporaryDirectory(prefix="harness_libcpptestreport_") as tmp:
        tmp_path = Path(tmp)
        json_out = tmp_path / "libcpp.json"
        html_out = tmp_path / "libcpp_report.html"
        args.extend(["--output", str(json_out), "--report", str(html_out)])

        if execute_with_echo is not None:
            rc, combined = execute_with_echo(args, REPO_ROOT, "libcpptestreport")
            if rc != 0:
                raise RuntimeError(
                    "libcpptestreport failed "
                    f"with exit code {rc}.\nCombined output:\n{combined}"
                )
        else:
            proc = subprocess.run(args, capture_output=True, text=True, cwd=str(REPO_ROOT))
            if proc.returncode != 0:
                raise RuntimeError(
                    "libcpptestreport failed "
                    f"with exit code {proc.returncode}.\nSTDOUT:\n{proc.stdout}\nSTDERR:\n{proc.stderr}"
                )

        report_data = json.loads(json_out.read_text(encoding="utf-8"))
        html_data = html_out.read_text(encoding="utf-8")

    return {
        "name": "libcpp",
        "json_filename": "libcpp.json",
        "html_filename": "libcpp_report.html",
        "report": report_data,
        "html": html_data,
    }


def main() -> None:
    run_report(sys.argv[1:])


if __name__ == "__main__":
    main()
