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
import time
from pathlib import Path
from typing import Any

SCRIPT_DIR = Path(__file__).resolve().parent
REPO_ROOT = SCRIPT_DIR.parents[1]
LIND_WASM_BASE = Path(os.environ.get("LIND_WASM_BASE", REPO_ROOT)).resolve()
LINDFS_ROOT = Path(os.environ.get("LINDFS_ROOT", LIND_WASM_BASE / "lindfs")).resolve()
LIND_TOOL_PATH = LIND_WASM_BASE / "scripts"
CXX = os.environ.get("CXX", "c++")
DEFAULT_CPP_DIR = Path("tests/unit-tests/cpp")


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


def _timing_fields_for_case(
    native_compile_time_sec: float | None,
    wasm_compile_time_sec: float | None,
    native_run_time_sec: float | None,
    wasm_run_time_sec: float | None,
) -> dict[str, Any]:
    """Match wasmtestreport.add_test_result timing shape (lazy import avoids import cycle)."""
    from harnesses import wasmtestreport as wtr

    merged = wtr.build_timing_info(
        native_compile_time_sec=native_compile_time_sec,
        wasm_compile_time_sec=wasm_compile_time_sec,
        native_run_time_sec=native_run_time_sec,
        wasm_run_time_sec=wasm_run_time_sec,
    )
    native_total = None
    if merged["native_compile_time_sec"] is not None or merged["native_run_time_sec"] is not None:
        native_total = (merged["native_compile_time_sec"] or 0.0) + (merged["native_run_time_sec"] or 0.0)
    wasm_total = None
    if merged["wasm_compile_time_sec"] is not None or merged["wasm_run_time_sec"] is not None:
        wasm_total = (merged["wasm_compile_time_sec"] or 0.0) + (merged["wasm_run_time_sec"] or 0.0)
    return {
        "native_time": native_total,
        "wasm_time": wasm_total,
        "timing": {
            **merged,
            "native_time_sec": native_total,
            "wasm_time_sec": wasm_total,
        },
    }


def _add_test_result(
    bucket: dict[str, Any],
    test_name: str,
    status: str,
    error_type: str | None,
    output: str,
    logger: logging.Logger,
    *,
    native_compile_time_sec: float | None = None,
    wasm_compile_time_sec: float | None = None,
    native_run_time_sec: float | None = None,
    wasm_run_time_sec: float | None = None,
) -> None:
    bucket["total_test_cases"] += 1
    entry: dict[str, Any] = {
        "status": status,
        "error_type": error_type,
        "output": output,
    }
    if (
        native_compile_time_sec is not None
        or wasm_compile_time_sec is not None
        or native_run_time_sec is not None
        or wasm_run_time_sec is not None
    ):
        entry.update(
            _timing_fields_for_case(
                native_compile_time_sec,
                wasm_compile_time_sec,
                native_run_time_sec,
                wasm_run_time_sec,
            )
        )
    bucket["test_cases"][test_name] = entry
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


def _discover_source_paths() -> list[Path]:
    override = os.environ.get("LIBCPP_TEST_CPP")
    if override:
        p = Path(override).resolve()
        if p.is_dir():
            return sorted(p.glob("*.cpp"))
        return [p]
    return sorted((LIND_WASM_BASE / DEFAULT_CPP_DIR).glob("*.cpp"))


def _cwasm_path_for_source(source: Path) -> Path:
    return source.parent / f"{source.name}.cwasm"


def _run_lind_compile_cpp_full(source: Path) -> tuple[int, str, float]:
    cmd = [str(LIND_WASM_BASE / "scripts" / "lind_compile_cpp"), str(source)]
    t0 = time.perf_counter()
    try:
        proc = subprocess.run(cmd, capture_output=True, text=True, cwd=str(LIND_WASM_BASE))
    except OSError as exc:
        return 127, f"Exception running lind_compile_cpp: {exc}", time.perf_counter() - t0
    out = proc.stdout or ""
    err = proc.stderr or ""
    return proc.returncode, out + (("\n" + err) if out and err else err), time.perf_counter() - t0


def _run_native_compile_cpp(source: Path, output_binary: Path) -> tuple[int, str, float]:
    cmd = [CXX, "-std=c++17", str(source), "-o", str(output_binary)]
    t0 = time.perf_counter()
    try:
        proc = subprocess.run(cmd, capture_output=True, text=True, cwd=str(LIND_WASM_BASE))
    except OSError as exc:
        return 127, f"Exception running native C++ compiler ({CXX}): {exc}", time.perf_counter() - t0
    out = proc.stdout or ""
    err = proc.stderr or ""
    return proc.returncode, out + (("\n" + err) if out and err else err), time.perf_counter() - t0


def _run_wasm_with_lind(wasm_basename: str, timeout_sec: int) -> tuple[Any, str, float]:
    cmd = [str(LIND_TOOL_PATH / "lind_run"), wasm_basename]
    t0 = time.perf_counter()
    try:
        proc = subprocess.run(
            cmd,
            capture_output=True,
            text=True,
            cwd=str(LIND_WASM_BASE),
            timeout=timeout_sec,
        )
    except subprocess.TimeoutExpired:
        return "timeout", f"Timed out after {timeout_sec}s", time.perf_counter() - t0
    except OSError as exc:
        return "error", f"Exception running lind_run: {exc}", time.perf_counter() - t0

    out = proc.stdout or ""
    err = proc.stderr or ""
    return proc.returncode, out + (("\n" + err) if err else ""), time.perf_counter() - t0


def _run_native_binary(binary_path: Path, timeout_sec: int) -> tuple[Any, str, float]:
    t0 = time.perf_counter()
    try:
        proc = subprocess.run(
            [str(binary_path)],
            capture_output=True,
            text=True,
            cwd=str(binary_path.parent),
            timeout=timeout_sec,
        )
    except subprocess.TimeoutExpired:
        return "timeout", f"Timed out after {timeout_sec}s", time.perf_counter() - t0
    except OSError as exc:
        return "error", f"Exception running native binary: {exc}", time.perf_counter() - t0
    out = proc.stdout or ""
    err = proc.stderr or ""
    return proc.returncode, out + (("\n" + err) if err else ""), time.perf_counter() - t0


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


def _cleanup_native_artifact(native_path: Path, logger: logging.Logger) -> None:
    try:
        native_path.unlink(missing_ok=True)
    except OSError as exc:
        logger.debug("[libcpp] Could not remove native artifact %s: %s", native_path, exc)


def _run_single_libcpp_test(
    bucket: dict[str, Any],
    src: Path,
    timeout_sec: int,
    logger: logging.Logger,
) -> None:
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
    native_bin = src.parent / f"{src.stem}.native"

    native_compile_rc, native_compile_out, t_native_compile = _run_native_compile_cpp(src, native_bin)
    if native_compile_rc != 0:
        _add_test_result(
            bucket,
            rel_name,
            "Failure",
            "Compile_Failure",
            f"Native compile failed (exit={native_compile_rc})\n{native_compile_out}",
            logger,
            native_compile_time_sec=t_native_compile,
        )
        return

    rc, compile_out, t_wasm_compile = _run_lind_compile_cpp_full(src)
    if rc != 0:
        _add_test_result(
            bucket,
            rel_name,
            "Failure",
            "Compile_Failure",
            f"lind_compile_cpp failed (exit={rc})\n{compile_out}",
            logger,
            native_compile_time_sec=t_native_compile,
            wasm_compile_time_sec=t_wasm_compile,
        )
        _cleanup_native_artifact(native_bin, logger)
        return
    if not cwasm_path.is_file():
        _add_test_result(
            bucket,
            rel_name,
            "Failure",
            "Compile_Failure",
            f"lind_compile_cpp exited 0 but .cwasm missing: {cwasm_path}\n{compile_out}",
            logger,
            native_compile_time_sec=t_native_compile,
            wasm_compile_time_sec=t_wasm_compile,
        )
        _cleanup_native_artifact(native_bin, logger)
        return

    native_rc, native_out, t_native_run = _run_native_binary(native_bin, timeout_sec)
    run_rc, run_out, t_wasm_run = _run_wasm_with_lind(run_basename, timeout_sec)
    try:
        if native_rc == "timeout":
            _add_test_result(
                bucket,
                rel_name,
                "Failure",
                "Timeout",
                "Native execution timed out",
                logger,
                native_compile_time_sec=t_native_compile,
                wasm_compile_time_sec=t_wasm_compile,
                native_run_time_sec=t_native_run,
            )
            return
        if isinstance(native_rc, str):
            _add_test_result(
                bucket,
                rel_name,
                "Failure",
                "Runtime_Failure",
                f"Native execution failed\n{native_out}",
                logger,
                native_compile_time_sec=t_native_compile,
                wasm_compile_time_sec=t_wasm_compile,
                native_run_time_sec=t_native_run,
            )
            return
        if run_rc == "timeout":
            _add_test_result(
                bucket,
                rel_name,
                "Failure",
                "Timeout",
                run_out,
                logger,
                native_compile_time_sec=t_native_compile,
                wasm_compile_time_sec=t_wasm_compile,
                native_run_time_sec=t_native_run,
                wasm_run_time_sec=t_wasm_run,
            )
            return
        if isinstance(run_rc, str) or run_rc != 0:
            _add_test_result(
                bucket,
                rel_name,
                "Failure",
                "Runtime_Failure",
                run_out,
                logger,
                native_compile_time_sec=t_native_compile,
                wasm_compile_time_sec=t_wasm_compile,
                native_run_time_sec=t_native_run,
                wasm_run_time_sec=t_wasm_run,
            )
            return
        if run_rc != native_rc or run_out != native_out:
            mismatch = (
                f"Native exit={native_rc}\nWasm exit={run_rc}\n\n"
                "=== Native output ===\n"
                f"{native_out}\n\n"
                "=== Wasm output ===\n"
                f"{run_out}"
            )
            _add_test_result(
                bucket,
                rel_name,
                "Failure",
                "Output_mismatch",
                mismatch,
                logger,
                native_compile_time_sec=t_native_compile,
                wasm_compile_time_sec=t_wasm_compile,
                native_run_time_sec=t_native_run,
                wasm_run_time_sec=t_wasm_run,
            )
            return

        _add_test_result(
            bucket,
            rel_name,
            "Success",
            None,
            (
                f"Native/Wasm parity verified (exit={run_rc}).\n\n"
                f"{run_out}"
            ),
            logger,
            native_compile_time_sec=t_native_compile,
            wasm_compile_time_sec=t_wasm_compile,
            native_run_time_sec=t_native_run,
            wasm_run_time_sec=t_wasm_run,
        )
    finally:
        _cleanup_artifacts(wasm_path, cwasm_path, logger)
        _cleanup_native_artifact(native_bin, logger)


def run_libcpp_integration(
    bucket: dict[str, Any],
    source: Path | None,
    timeout_sec: int,
    logger: logging.Logger,
) -> None:
    sources = [source.resolve()] if source is not None else _discover_source_paths()
    if not sources:
        _add_test_result(
            bucket,
            "tests/unit-tests/cpp",
            "Failure",
            "Compile_Failure",
            "No .cpp tests discovered in tests/unit-tests/cpp",
            logger,
        )
        return
    for src in sources:
        _run_single_libcpp_test(bucket, src, timeout_sec, logger)


def generate_html_section(libcpp: dict[str, Any]) -> str:
    from harnesses import wasmtestreport as wtr

    rows: list[str] = []
    for test_name, test_result in sorted(libcpp.get("test_cases", {}).items()):
        status = test_result.get("status", "Unknown")
        error_type = test_result.get("error_type") or ""
        out = html.escape(str(test_result.get("output", "")))
        native_time, wasm_time = wtr.get_row_time_values(test_result)
        rows.append(
            "<tr>"
            f"<td>{html.escape(test_name)}</td>"
            f"<td>{html.escape(status)}</td>"
            f"<td>{html.escape(error_type)}</td>"
            f"<td>{wtr.format_time_cell(native_time)}</td>"
            f"<td>{wtr.format_time_cell(wasm_time)}</td>"
            f"<td><pre>{out}</pre></td>"
            "</tr>"
        )
    rows_html = "\n".join(rows) if rows else '<tr><td colspan="6"><em>No cases</em></td></tr>'
    return "\n".join(
        [
            '<div class="wasm-harness-subsection">',
            "<h2>Libc++ integration</h2>",
            "<p>Full <code>lind_compile_cpp</code> (wasm-opt + precompile), <code>lind_run</code> on the "
            "<code>.cwasm</code>, and verify native and wasm runs have identical exit codes and output. "
            "<strong>Native time</strong> / <strong>Wasm time</strong> are total wall time (native compile + "
            "native run vs <code>lind_compile_cpp</code> + <code>lind_run</code>), same convention as the "
            "deterministic/fail tables above.</p>",
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
