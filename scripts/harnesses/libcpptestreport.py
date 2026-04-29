#!/usr/bin/env python3
"""Shared libc++ smoke helpers used by wasmtestreport.

This module intentionally exposes helper functions only; it is invoked by
`scripts/harnesses/wasmtestreport.py` so libc++ results remain embedded in the
single wasm harness JSON/HTML output.
"""

from __future__ import annotations

import html
import logging
import os
import subprocess
from pathlib import Path
from typing import Any

SCRIPT_DIR = Path(__file__).resolve().parent
REPO_ROOT = SCRIPT_DIR.parents[1]
LIND_WASM_BASE = Path(os.environ.get("LIND_WASM_BASE", REPO_ROOT)).resolve()
LINDFS_ROOT = Path(os.environ.get("LINDFS_ROOT", LIND_WASM_BASE / "lindfs")).resolve()
LIND_TOOL_PATH = LIND_WASM_BASE / "scripts"

EXPECTED_STDOUT_LINE = "LIBCPP_SORT_OK 1 2 3"
DEFAULT_SOURCE_REL = Path("tests/unit-tests/cpp/hello.cpp")


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


def _add_test_result(
    bucket: dict[str, Any],
    test_name: str,
    status: str,
    error_type: str | None,
    output: str,
    logger: logging.Logger,
) -> None:
    bucket["total_test_cases"] += 1
    bucket["test_cases"][test_name] = {
        "status": status,
        "error_type": error_type,
        "output": output,
    }
    if status == "Success":
        bucket["number_of_success"] += 1
        bucket["success"].append(test_name)
        logger.info("[libcpp] SUCCESS: %s", test_name)
        return

    bucket["number_of_failures"] += 1
    bucket["failures"].append(test_name)
    if error_type == "Compile_Failure":
        bucket["number_of_compile_failures"] += 1
        bucket["compile_failures"].append(test_name)
    elif error_type == "Runtime_Failure":
        bucket["number_of_runtime_failures"] += 1
        bucket["runtime_failures"].append(test_name)
    elif error_type == "Output_mismatch":
        bucket["number_of_output_mismatch"] += 1
        bucket["output_mismatch_failures"].append(test_name)
    elif error_type == "Timeout":
        bucket["number_of_timeout_failures"] += 1
        bucket["timeout_failures"].append(test_name)
    logger.error("[libcpp] FAILURE: %s — %s", test_name, error_type or "unknown")


def _default_source_path() -> Path:
    override = os.environ.get("LIBCPP_TEST_CPP")
    if override:
        return Path(override).resolve()
    return (LIND_WASM_BASE / DEFAULT_SOURCE_REL).resolve()


def _cwasm_path_for_source(source: Path) -> Path:
    return source.parent / f"{source.name}.cwasm"


def _run_lind_compile_cpp_full(source: Path) -> tuple[int, str]:
    cmd = [str(LIND_WASM_BASE / "scripts" / "lind_compile_cpp"), str(source)]
    try:
        proc = subprocess.run(cmd, capture_output=True, text=True, cwd=str(LIND_WASM_BASE))
    except OSError as exc:
        return 127, f"Exception running lind_compile_cpp: {exc}"
    out = proc.stdout or ""
    err = proc.stderr or ""
    return proc.returncode, out + (("\n" + err) if out and err else err)


def _stdout_has_expected_line(text: str) -> bool:
    for line in text.splitlines():
        if line.strip() == EXPECTED_STDOUT_LINE:
            return True
    return False


def _run_wasm_with_lind(wasm_basename: str, timeout_sec: int) -> tuple[Any, str]:
    cmd = [str(LIND_TOOL_PATH / "lind_run"), wasm_basename]
    try:
        proc = subprocess.run(
            cmd,
            capture_output=True,
            text=True,
            cwd=str(LIND_WASM_BASE),
            timeout=timeout_sec,
        )
    except subprocess.TimeoutExpired:
        return "timeout", f"Timed out after {timeout_sec}s"
    except OSError as exc:
        return "error", f"Exception running lind_run: {exc}"

    out = proc.stdout or ""
    err = proc.stderr or ""
    return proc.returncode, out + (("\n" + err) if err else "")


def _cleanup_artifacts(wasm_path: Path, cwasm_path: Path, logger: logging.Logger) -> None:
    for path in (wasm_path, cwasm_path):
        try:
            path.unlink(missing_ok=True)
        except OSError as exc:
            logger.debug("[libcpp] Could not remove %s: %s", path, exc)
        try:
            (LINDFS_ROOT / path.name).unlink(missing_ok=True)
        except OSError as exc:
            logger.debug("[libcpp] Could not remove %s: %s", LINDFS_ROOT / path.name, exc)


def run_libcpp_integration(
    bucket: dict[str, Any],
    source: Path | None,
    timeout_sec: int,
    logger: logging.Logger,
) -> None:
    src = source if source is not None else _default_source_path()
    try:
        rel_name = str(src.relative_to(LIND_WASM_BASE))
    except ValueError:
        rel_name = str(src)

    if not src.is_file():
        _add_test_result(bucket, rel_name, "Failure", "Compile_Failure", f"Source file not found: {src}", logger)
        return

    wasm_path = src.parent / f"{src.name}.wasm"
    cwasm_path = _cwasm_path_for_source(src)
    run_basename = cwasm_path.name

    rc, compile_out = _run_lind_compile_cpp_full(src)
    if rc != 0:
        _add_test_result(bucket, rel_name, "Failure", "Compile_Failure", f"exit={rc}\n{compile_out}", logger)
        return
    if not cwasm_path.is_file():
        _add_test_result(
            bucket,
            rel_name,
            "Failure",
            "Compile_Failure",
            f"lind_compile_cpp exited 0 but .cwasm missing: {cwasm_path}\n{compile_out}",
            logger,
        )
        return

    run_rc, run_out = _run_wasm_with_lind(run_basename, timeout_sec)
    try:
        if run_rc == "timeout":
            _add_test_result(bucket, rel_name, "Failure", "Timeout", run_out, logger)
            return
        if isinstance(run_rc, str) or run_rc != 0:
            _add_test_result(bucket, rel_name, "Failure", "Runtime_Failure", run_out, logger)
            return
        if not _stdout_has_expected_line(run_out):
            _add_test_result(bucket, rel_name, "Failure", "Output_mismatch", run_out, logger)
            return
        _add_test_result(bucket, rel_name, "Success", None, run_out, logger)
    finally:
        _cleanup_artifacts(wasm_path, cwasm_path, logger)


def generate_html_section(libcpp: dict[str, Any]) -> str:
    rows: list[str] = []
    for test_name, test_result in sorted(libcpp.get("test_cases", {}).items()):
        status = test_result.get("status", "Unknown")
        error_type = test_result.get("error_type") or ""
        out = html.escape(str(test_result.get("output", "")))
        rows.append(
            "<tr>"
            f"<td>{html.escape(test_name)}</td>"
            f"<td>{html.escape(status)}</td>"
            f"<td>{html.escape(error_type)}</td>"
            "<td>N/A</td><td>N/A</td>"
            f"<td><pre>{out}</pre></td>"
            "</tr>"
        )
    rows_html = "\n".join(rows) if rows else '<tr><td colspan="6"><em>No cases</em></td></tr>'
    exp = html.escape(EXPECTED_STDOUT_LINE)
    return "\n".join(
        [
            '<div class="wasm-harness-subsection">',
            "<h2>Libc++ integration</h2>",
            "<p>Full <code>lind_compile_cpp</code> (wasm-opt + precompile), <code>lind_run</code> on the "
            f"<code>.cwasm</code>, stdout must contain a line exactly <code>{exp}</code>.</p>",
            "<h3>Summary</h3>",
            '<table class="summary-table">',
            "<tr><th>Metric</th><th>Value</th></tr>",
            f'<tr><td>Total</td><td>{libcpp.get("total_test_cases", 0)}</td></tr>',
            f'<tr><td>Success</td><td>{libcpp.get("number_of_success", 0)}</td></tr>',
            f'<tr><td>Failures</td><td>{libcpp.get("number_of_failures", 0)}</td></tr>',
            f'<tr><td>Compile failures</td><td>{libcpp.get("number_of_compile_failures", 0)}</td></tr>',
            f'<tr><td>Runtime failures</td><td>{libcpp.get("number_of_runtime_failures", 0)}</td></tr>',
            f'<tr><td>Output mismatch</td><td>{libcpp.get("number_of_output_mismatch", 0)}</td></tr>',
            f'<tr><td>Timeouts</td><td>{libcpp.get("number_of_timeout_failures", 0)}</td></tr>',
            "</table>",
            "<h3>Cases</h3>",
            '<table class="test-results-table">',
            "<tr><th>Test</th><th>Status</th><th>Error type</th><th>Native time</th><th>Wasm time</th><th>Output</th></tr>",
            rows_html,
            "</table>",
            "</div>",
        ]
    )
