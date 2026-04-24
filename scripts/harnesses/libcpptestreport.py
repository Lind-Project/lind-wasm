#!/usr/bin/env python3
"""Libc++ header / toolchain integration harness for the unified E2E report.

Compiles a small C++ smoke test with `lind_compile_cpp --compile-only`, runs it
via `lind_run`, and checks stdout for a canonical success line.
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
LIND_TOOL_PATH = REPO_ROOT / "scripts"
LINDFS_ROOT = Path(os.environ.get("LINDFS_ROOT", REPO_ROOT / "lindfs")).resolve()
DEFAULT_CPP_REL = Path("tests/unit-tests/cpp/hello.cpp")
JSON_OUTPUT = "libcpp.json"
HTML_OUTPUT = "libcpp_report.html"

# Must match the success line printed by tests/unit-tests/cpp/hello.cpp
EXPECTED_STDOUT_LINE = "LIBCPP_SORT_OK 1 2 3"
DEFAULT_RUN_TIMEOUT_SEC = 30


def get_empty_result() -> dict[str, Any]:
    return {
        "total_test_cases": 0,
        "number_of_success": 0,
        "success": [],
        "number_of_failures": 0,
        "failures": [],
        "number_of_compile_failures": 0,
        "compile_failures": [],
        "number_of_runtime_failures": 0,
        "runtime_failures": [],
        "number_of_output_mismatch": 0,
        "output_mismatch_failures": [],
        "number_of_timeout_failures": 0,
        "timeout_failures": [],
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
    elif error_type == "Runtime_Failure":
        result["number_of_runtime_failures"] += 1
        result["runtime_failures"].append(test_name)
    elif error_type == "Output_mismatch":
        result["number_of_output_mismatch"] += 1
        result["output_mismatch_failures"].append(test_name)
    elif error_type == "Timeout":
        result["number_of_timeout_failures"] += 1
        result["timeout_failures"].append(test_name)
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


def stdout_has_expected_line(text: str, expected: str = EXPECTED_STDOUT_LINE) -> bool:
    for line in text.splitlines():
        if line.strip() == expected:
            return True
    return False


def run_wasm_with_lind(wasm_basename: str, timeout_sec: int) -> tuple[Any, str]:
    """Run wasm via scripts/lind_run; return (returncode, combined_output) or ("timeout", msg)."""
    run_cmd = [str(LIND_TOOL_PATH / "lind_run"), wasm_basename]
    try:
        proc = subprocess.run(
            run_cmd,
            capture_output=True,
            text=True,
            cwd=str(REPO_ROOT),
            timeout=timeout_sec,
        )
    except subprocess.TimeoutExpired:
        return "timeout", f"Timed out after {timeout_sec}s"
    except OSError as exc:
        return "error", f"Exception running lind_run: {exc}"
    out = proc.stdout or ""
    err = proc.stderr or ""
    combined = out + (("\n" + err) if err else "")
    return proc.returncode, combined


def cleanup_wasm_artifacts(wasm_path: Path) -> None:
    """Remove wasm from source tree and lindfs copy left by lind_compile_cpp."""
    try:
        wasm_path.unlink(missing_ok=True)
    except OSError as exc:
        logger.debug("Could not remove %s: %s", wasm_path, exc)
    lindfs_copy = LINDFS_ROOT / wasm_path.name
    try:
        lindfs_copy.unlink(missing_ok=True)
    except OSError as exc:
        logger.debug("Could not remove %s: %s", lindfs_copy, exc)


def run_libcpp_integration(result: dict[str, Any], source: Path, timeout_sec: int) -> None:
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
    wasm_basename = wasm_path.name
    try:
        wasm_path.unlink(missing_ok=True)
    except OSError as exc:
        logger.warning("Could not remove prior wasm %s: %s", wasm_path, exc)

    rc, compile_out = run_compile(source)
    if rc != 0:
        add_test_result(result, rel_name, "Failure", "Compile_Failure", f"exit={rc}\n{compile_out}")
        return

    if not wasm_path.is_file():
        add_test_result(
            result,
            rel_name,
            "Failure",
            "Compile_Failure",
            f"Compiler exited 0 but wasm missing: {wasm_path}\n{compile_out}",
        )
        return

    run_rc, run_out = run_wasm_with_lind(wasm_basename, timeout_sec)

    try:
        wasm_disp = str(wasm_path.relative_to(REPO_ROOT))
    except ValueError:
        wasm_disp = str(wasm_path)

    try:
        if run_rc == "timeout":
            body = (
                f"=== compile (ok) ===\n{compile_out.strip()}\n\n"
                f"=== run ===\n{run_out}"
            )
            add_test_result(result, rel_name, "Failure", "Timeout", body.strip())
            return

        if isinstance(run_rc, str):
            body = (
                f"=== compile (ok) ===\n{compile_out.strip()}\n\n"
                f"=== run ===\n{run_out}"
            )
            add_test_result(result, rel_name, "Failure", "Runtime_Failure", body.strip())
            return

        if run_rc != 0:
            body = (
                f"=== compile (ok) ===\n{compile_out.strip()}\n\n"
                f"=== run (exit {run_rc}) ===\n{run_out.strip()}"
            )
            add_test_result(result, rel_name, "Failure", "Runtime_Failure", body.strip())
            return

        if not stdout_has_expected_line(run_out):
            body = (
                f"=== compile (ok) ===\n{compile_out.strip()}\n\n"
                f"=== run (exit 0) ===\n"
                f"Expected a line exactly: {EXPECTED_STDOUT_LINE!r}\n"
                f"--- stdout/stderr ---\n{run_out.strip()}"
            )
            add_test_result(result, rel_name, "Failure", "Output_mismatch", body.strip())
            return

        ok_msg = (
            f"=== compile ===\n{compile_out.strip()}\n\n"
            f"=== run (exit 0) ===\n{run_out.strip()}\n\n"
            f"Verified: {EXPECTED_STDOUT_LINE!r} present in output.\n"
            f"Artifact: {wasm_disp}"
        ).strip()
        add_test_result(result, rel_name, "Success", None, ok_msg)
    finally:
        cleanup_wasm_artifacts(wasm_path)


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
<p>Compiles with <code>lind_compile_cpp --compile-only</code>, runs via <code>lind_run</code>,
and checks for <code>{expected}</code> on a line of stdout.</p>
<table border="1" cellspacing="0" cellpadding="6">
<tr><th>Metric</th><th>Value</th></tr>
<tr><td>Total</td><td>{total}</td></tr>
<tr><td>Success</td><td>{success}</td></tr>
<tr><td>Failures</td><td>{failures}</td></tr>
<tr><td>Compile failures</td><td>{compile_failures}</td></tr>
<tr><td>Runtime failures</td><td>{runtime_failures}</td></tr>
<tr><td>Output mismatch</td><td>{output_mismatch}</td></tr>
<tr><td>Timeouts</td><td>{timeouts}</td></tr>
</table>
<h2>Cases</h2>
<table border="1" cellspacing="0" cellpadding="6">
<tr><th>Test</th><th>Status</th><th>Error type</th><th>Output</th></tr>
{rows}
</table>
</body></html>
""".format(
        expected=html.escape(EXPECTED_STDOUT_LINE),
        total=result.get("total_test_cases", 0),
        success=result.get("number_of_success", 0),
        failures=result.get("number_of_failures", 0),
        compile_failures=result.get("number_of_compile_failures", 0),
        runtime_failures=result.get("number_of_runtime_failures", 0),
        output_mismatch=result.get("number_of_output_mismatch", 0),
        timeouts=result.get("number_of_timeout_failures", 0),
        rows="\n".join(rows) if rows else "<tr><td colspan='4'><em>No cases</em></td></tr>",
    )


def parse_arguments(argv: list[str] | None) -> argparse.Namespace:
    p = argparse.ArgumentParser(description="Libc++ integration compile + run smoke test")
    p.add_argument("--output", default=JSON_OUTPUT, help="JSON report path")
    p.add_argument("--report", default=HTML_OUTPUT, help="HTML report path")
    p.add_argument("--timeout", type=int, default=DEFAULT_RUN_TIMEOUT_SEC, help="lind_run timeout (seconds)")
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
    run_libcpp_integration(result, default_source_path(), timeout_sec=args.timeout)

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
