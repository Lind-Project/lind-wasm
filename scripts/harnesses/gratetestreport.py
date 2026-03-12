#!/usr/bin/env python3
"""Grate test harness report generator.

A grate test is successful iff the grate run command exits with code 0.
Any non-zero exit code is recorded as a failure in the report.
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
import time
from dataclasses import dataclass
from pathlib import Path
from typing import Any, Callable

logger = logging.getLogger("gratetestreport")
logger.setLevel(logging.DEBUG)
ch = logging.StreamHandler()
ch.setFormatter(logging.Formatter("[%(levelname)s] %(message)s"))
logger.addHandler(ch)

DEFAULT_TIMEOUT = 30
JSON_OUTPUT = "grates.json"
HTML_OUTPUT = "grate_report.html"

SCRIPT_DIR = Path(__file__).resolve().parent
REPO_ROOT = SCRIPT_DIR.parents[1]
GRATE_TEST_BASE = REPO_ROOT / "tests" / "grate-tests"
LIND_TOOL_PATH = REPO_ROOT / "scripts"
LINDFS_ROOT = Path(os.environ.get("LINDFS_ROOT", REPO_ROOT / "lindfs")).resolve()

GRATE_CLANG = os.environ.get("GRATE_CLANG", "lind-clang")
GRATE_RUNNER = os.environ.get("GRATE_RUNNER", "lind-wasm")
SKIP_TESTS_FILE = "skip_test_cases.txt"

error_types = {
    "Compile_Failure": "Compile Failure",
    "Runtime_Failure": "Runtime Failure",
    "Timeout": "Timeout",
    "Missing_Pair": "Missing Grate/Cage Pair",
}


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
        "number_of_timeout_failures": 0,
        "timeout_failures": [],
        "number_of_missing_pair_failures": 0,
        "missing_pair_failures": [],
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
        logger.info("SUCCESS")
        return

    result["number_of_failures"] += 1
    result["failures"].append(test_name)
    if error_type == "Compile_Failure":
        result["number_of_compile_failures"] += 1
        result["compile_failures"].append(test_name)
    elif error_type == "Runtime_Failure":
        result["number_of_runtime_failures"] += 1
        result["runtime_failures"].append(test_name)
    elif error_type == "Timeout":
        result["number_of_timeout_failures"] += 1
        result["timeout_failures"].append(test_name)
    elif error_type == "Missing_Pair":
        result["number_of_missing_pair_failures"] += 1
        result["missing_pair_failures"].append(test_name)

    error_message = error_types.get(error_type or "", "Undefined Failure")
    logger.error(f"FAILURE: {error_message}")


def check_timeout(value: str) -> int:
    ivalue = int(value)
    if ivalue <= 0:
        raise argparse.ArgumentTypeError("Timeout should be an integer greater than 0")
    return ivalue


def parse_arguments(argv: list[str] | None = None) -> argparse.Namespace:
    parser = argparse.ArgumentParser(description="Run grate tests and generate a report")
    parser.add_argument("--skip", nargs="*", default=[], help="List of folders to skip")
    parser.add_argument("--run", nargs="*", default=[], help="List of folders to run")
    parser.add_argument("--timeout", type=check_timeout, default=DEFAULT_TIMEOUT, help="Timeout in seconds")
    parser.add_argument("--output", default=JSON_OUTPUT, help="Report JSON filename")
    parser.add_argument("--report", default=HTML_OUTPUT, help="Report HTML filename")
    parser.add_argument("--generate-html", action="store_true", help="Generate HTML report")
    parser.add_argument("--debug", action="store_true", help="Enable debug logging")
    parser.add_argument("--testfiles", type=Path, nargs="+", help="Specific grate files (*_grate.c) to run")
    parser.add_argument("--clean-results", action="store_true", help="Delete output files and exit")
    return parser.parse_args(argv)


@dataclass(frozen=True)
class GrateTestCase:
    name: str
    grate_source: Path
    cage_source: Path

    @property
    def grate_wasm(self) -> Path:
        return self.grate_source.with_suffix(".wasm")

    @property
    def cage_wasm(self) -> Path:
        return self.cage_source.with_suffix(".wasm")


def resolve_module_output(source_file: Path, cwd: Path) -> Path:
    """Resolve expected runtime module output across lind-clang modes.

    `lind-clang` (lind_compile) defaults to full mode and typically produces a
    precompiled `.cwasm` artifact. In some environments/tests a plain `.wasm`
    is executed instead. Support both and prefer the artifact that exists.
    """
    candidates = [
        source_file.with_suffix(".cwasm"),
        source_file.with_suffix(".wasm"),
        cwd / source_file.with_suffix(".cwasm").name,
        cwd / source_file.with_suffix(".wasm").name,
    ]

    for candidate in candidates:
        if candidate.exists():
            return candidate

    # Default to cwasm expectation for clearer error messages in full pipeline.
    return source_file.with_suffix(".cwasm")


def in_selected_folders(path: Path, run_folders: list[str], skip_folders: list[str]) -> bool:
    rel = path.resolve().relative_to(GRATE_TEST_BASE.resolve())
    rel_parts = rel.parts[:-1]

    if skip_folders and any(folder in rel_parts for folder in skip_folders):
        return False
    if run_folders:
        return any(folder in rel_parts for folder in run_folders)
    return True


def discover_tests(args: argparse.Namespace) -> tuple[list[GrateTestCase], list[tuple[str, str]]]:
    failures: list[tuple[str, str]] = []
    cases: list[GrateTestCase] = []

    if args.testfiles:
        grate_files = [Path(p).resolve() for p in args.testfiles]
    else:
        grate_files = sorted(GRATE_TEST_BASE.rglob("*_grate.c"))

    skip_test_cases = load_skip_test_cases()

    for grate_file in grate_files:
        if not in_selected_folders(grate_file, args.run, args.skip):
            continue

        cage_file = grate_file.with_name(grate_file.name.replace("_grate.c", ".c"))
        test_name = str(grate_file.relative_to(GRATE_TEST_BASE))

        if should_skip_test_case(grate_file, cage_file, skip_test_cases):
            logger.info(f"Skipping {test_name}")
            continue

        if not cage_file.exists():
            failures.append((test_name, f"Missing cage source for grate test: {cage_file}"))
            continue

        cases.append(GrateTestCase(name=test_name, grate_source=grate_file, cage_source=cage_file))

    return cases, failures


def load_skip_test_cases() -> set[str]:
    skip_test_cases: set[str] = set()
    skip_file = REPO_ROOT / SKIP_TESTS_FILE

    if not skip_file.exists():
        logger.debug(f"{SKIP_TESTS_FILE} not found")
        return skip_test_cases

    for line in skip_file.read_text(encoding="utf-8").splitlines():
        test_name = line.strip()
        if not test_name or test_name.startswith("#"):
            continue
        skip_test_cases.add(test_name)

    return skip_test_cases


def should_skip_test_case(grate_file: Path, cage_file: Path, skip_test_cases: set[str]) -> bool:
    if not skip_test_cases:
        return False

    grate_rel = str(grate_file.relative_to(GRATE_TEST_BASE))
    cage_rel = str(cage_file.relative_to(GRATE_TEST_BASE))

    candidate_names = {
        grate_rel,
        cage_rel,
        grate_file.name,
        cage_file.name,
    }

    return any(name in skip_test_cases for name in candidate_names)


def run_subprocess(cmd: list[str], timeout: int | None = None, cwd: Path | None = None) -> subprocess.CompletedProcess[str]:
    return subprocess.run(cmd, capture_output=True, text=True, timeout=timeout, cwd=cwd)


def compile_grate_test(test: GrateTestCase) -> tuple[bool, str]:
    grate_compile_cmd = [GRATE_CLANG, "--compile-grate", test.grate_source.name]
    cage_compile_cmd = [GRATE_CLANG, test.cage_source.name]

    try:
        grate_proc = run_subprocess(grate_compile_cmd, cwd=test.grate_source.parent)
    except Exception as exc:
        return False, f"Exception compiling grate source: {exc}"

    if grate_proc.returncode != 0:
        return False, (
            f"Grate compile failed (exit={grate_proc.returncode})\n"
            f"STDOUT:\n{grate_proc.stdout}\nSTDERR:\n{grate_proc.stderr}"
        )

    try:
        cage_proc = run_subprocess(cage_compile_cmd, cwd=test.cage_source.parent)
    except Exception as exc:
        return False, f"Exception compiling cage source: {exc}"

    if cage_proc.returncode != 0:
        return False, (
            f"Cage compile failed (exit={cage_proc.returncode})\n"
            f"STDOUT:\n{cage_proc.stdout}\nSTDERR:\n{cage_proc.stderr}"
        )

    grate_module = resolve_module_output(test.grate_source, test.grate_source.parent)
    cage_module = resolve_module_output(test.cage_source, test.cage_source.parent)

    if not grate_module.exists() or not cage_module.exists():
        return False, (
            "Compilation completed but expected wasm outputs were not found.\n"
            f"grate_module={grate_module} exists={grate_module.exists()}\n"
            f"cage_module={cage_module} exists={cage_module.exists()}"
        )

    return True, ""


def build_grate_run_cmd(grate_wasm: Path, cage_wasm: Path) -> list[str]:
    """Build grate run command with lind_run wrapper when available.

    Using scripts/lind_run matches wasm harness behavior and provides the
    same sudo escalation flow in environments that require privilege.
    """
    lind_run_wrapper = LIND_TOOL_PATH / "lind_run"

    grate_wasm = Path(grate_wasm)
    cage_wasm = Path(cage_wasm)

    # Respect explicit override first.
    if "GRATE_RUNNER" in os.environ:
        return [GRATE_RUNNER, str(grate_wasm.name), str(cage_wasm.name)]

    if lind_run_wrapper.is_file():
        return [str(lind_run_wrapper), str(grate_wasm.name), str(cage_wasm.name)]

    return [GRATE_RUNNER, str(grate_wasm.name), str(cage_wasm.name)]


def run_grate_test(test: GrateTestCase, timeout_sec: int) -> tuple[str, str, int | str]:
    grate_module = resolve_module_output(test.grate_source, test.grate_source.parent)
    cage_module = resolve_module_output(test.cage_source, test.cage_source.parent)

    # lind_run/lind-boot resolves inputs from lindfs. lind_compile copies the
    # build artifact into LINDFS_ROOT, so run from that location rather than
    # source-tree absolute paths.
    grate_runtime_module = (LINDFS_ROOT / grate_module.name).resolve()
    cage_runtime_module = (LINDFS_ROOT / cage_module.name).resolve()

    # Fall back to direct module paths when lindfs copy is unavailable.
    if not grate_runtime_module.exists():
        grate_runtime_module = grate_module.resolve()
    if not cage_runtime_module.exists():
        cage_runtime_module = cage_module.resolve()

    run_cwd = REPO_ROOT
    run_cmd = build_grate_run_cmd(grate_runtime_module, cage_runtime_module)

    try:
        proc = run_subprocess(run_cmd, timeout=timeout_sec, cwd=run_cwd)
    except subprocess.TimeoutExpired:
        return "Timeout", f"Timed Out (timeout: {timeout_sec}s)", "timeout"
    except Exception as exc:
        return "Runtime_Failure", f"Exception running grate test: {exc}", "exception"

    output = f"STDOUT:\n{proc.stdout}\nSTDERR:\n{proc.stderr}"
    if proc.returncode == 0:
        return "Success", output, 0

    return "Runtime_Failure", f"Exit code: {proc.returncode}\n{output}", proc.returncode


def generate_html_report(result: dict[str, Any]) -> str:
    rows: list[str] = []
    for test_name, test_result in sorted(result.get("test_cases", {}).items()):
        status = test_result.get("status", "Unknown")
        error_type = test_result.get("error_type") or ""
        output = html.escape(str(test_result.get("output", "")))
        rows.append(
            "<tr>"
            f"<td>{html.escape(test_name)}</td>"
            f"<td>{html.escape(status)}</td>"
            f"<td>{html.escape(error_type)}</td>"
            f"<td><pre>{output}</pre></td>"
            "</tr>"
        )

    return """<!DOCTYPE html>
<html><head><meta charset='UTF-8'><title>Grate Test Report</title></head>
<body>
<h1>Grate Test Report</h1>
<table border='1' cellspacing='0' cellpadding='6'>
<tr><th>Metric</th><th>Value</th></tr>
<tr><td>Total</td><td>{total}</td></tr>
<tr><td>Success</td><td>{success}</td></tr>
<tr><td>Failures</td><td>{failures}</td></tr>
<tr><td>Compile Failures</td><td>{compile_failures}</td></tr>
<tr><td>Runtime Failures</td><td>{runtime_failures}</td></tr>
<tr><td>Timeout Failures</td><td>{timeout_failures}</td></tr>
<tr><td>Missing Pair Failures</td><td>{missing_failures}</td></tr>
</table>
<h2>Cases</h2>
<table border='1' cellspacing='0' cellpadding='6'>
<tr><th>Test</th><th>Status</th><th>Error Type</th><th>Output</th></tr>
{rows}
</table>
</body></html>
""".format(
        total=result.get("total_test_cases", 0),
        success=result.get("number_of_success", 0),
        failures=result.get("number_of_failures", 0),
        compile_failures=result.get("number_of_compile_failures", 0),
        runtime_failures=result.get("number_of_runtime_failures", 0),
        timeout_failures=result.get("number_of_timeout_failures", 0),
        missing_failures=result.get("number_of_missing_pair_failures", 0),
        rows="\n".join(rows),
    )


def run_report(argv: list[str] | None = None) -> dict[str, Any]:
    args = parse_arguments(argv)
    if args.debug:
        logger.setLevel(logging.DEBUG)
    else:
        logger.setLevel(logging.INFO)

    output_json = str(Path(args.output).with_suffix(".json"))
    output_html = str(Path(args.report).with_suffix(".html"))

    if args.clean_results:
        for path in [output_json, output_html]:
            if os.path.isfile(path):
                os.remove(path)
        return get_empty_result()

    result = get_empty_result()

    tests_to_run, discovery_failures = discover_tests(args)
    for test_name, message in discovery_failures:
        add_test_result(result, test_name, "Failure", "Missing_Pair", message)

    if not tests_to_run:
        logger.warning("No grate tests found.")

    for idx, test in enumerate(tests_to_run, start=1):
        logger.info(f"[{idx}/{len(tests_to_run)}] {test.name}")
        compile_ok, compile_output = compile_grate_test(test)
        if not compile_ok:
            add_test_result(result, test.name, "Failure", "Compile_Failure", compile_output)
            continue

        status, output, _ = run_grate_test(test, args.timeout)
        if status == "Success":
            add_test_result(result, test.name, "Success", None, output)
        elif status == "Timeout":
            add_test_result(result, test.name, "Failure", "Timeout", output)
        else:
            add_test_result(result, test.name, "Failure", "Runtime_Failure", output)

    with open(output_json, "w", encoding="utf-8") as fp:
        json.dump(result, fp, indent=4)
    logger.info(f"'{os.path.abspath(output_json)}' generated.")

    if args.generate_html:
        report_html = generate_html_report(result)
        with open(output_html, "w", encoding="utf-8") as out:
            out.write(report_html)
        logger.info(f"'{os.path.abspath(output_html)}' generated.")

    return result


def run_harness(
    forward_args: list[str] | None = None,
    execute_with_echo: Callable[[list[str], Path, str], tuple[int, str]] | None = None,
) -> dict[str, Any]:
    args = ["python3", str(Path(__file__).resolve())]
    if forward_args:
        args.extend(forward_args)

    with tempfile.TemporaryDirectory(prefix="harness_gratetestreport_") as td:  # type: ignore[name-defined]
        tmp_path = Path(td)
        json_out = tmp_path / "grates.json"
        html_out = tmp_path / "grate_report.html"
        args.extend(["--output", str(json_out), "--report", str(html_out), "--generate-html"])

        if execute_with_echo is not None:
            return_code, combined_output = execute_with_echo(args, REPO_ROOT, "gratetestreport")
            if return_code != 0:
                raise RuntimeError(
                    "scripts/harnesses/gratetestreport.py failed "
                    f"with exit code {return_code}.\nCombined output:\n{combined_output}"
                )
        else:
            proc = subprocess.run(args, capture_output=True, text=True, cwd=REPO_ROOT)
            if proc.returncode != 0:
                raise RuntimeError(
                    "scripts/harnesses/gratetestreport.py failed "
                    f"with exit code {proc.returncode}.\nSTDOUT:\n{proc.stdout}\nSTDERR:\n{proc.stderr}"
                )

        report_data = json.loads(json_out.read_text(encoding="utf-8"))
        html_data = html_out.read_text(encoding="utf-8")

    return {
        "name": "grate",
        "json_filename": "grates.json",
        "html_filename": "grate_report.html",
        "report": report_data,
        "html": html_data,
    }


def main() -> None:
    run_report(sys.argv[1:])


if __name__ == "__main__":
    main()