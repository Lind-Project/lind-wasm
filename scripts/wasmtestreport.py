#!/usr/bin/env python3

# Usage
#   "./wasmtestreport" to run with default settings(5 second timeout and all tests inside unit-tests folder)
#   "./wasmtestreport.py --skip-folders config_tests file_tests" to skip the test cases in those folders
#   "./wasmtestreport.py --run-folders config_tests file_tests" to run test cases in folder1 and folder2 only
#   "./wasmtestreport.py --timeout 10" to run with a timeout of 10 seconds 
#   "./wasmtestreport.py --output newresult" to change the output file(The new file will be newresult.json)
#   "./wasmtestreport.py --generate-html" to generate the html file
#   The arguments can be stacked eg: "./wasmtestreport.py --generate-html --skip-folders config_tests file_tests --timeout 10"
#
#   "./wasmtestreport.py --pre-test-only" to copy the testfiles to lind fs root(does not run tests)
#   "./wasmtestreport.py --clean-testfiles" to delete the testfiles from lind fs root(does not run tests)
#   NOTE: without the last two testfiles arguments, we will always copy the test cases and then run the tests
#!/usr/bin/env python3

import json
import os
import subprocess
from pathlib import Path
import argparse
import shutil
import time
from contextlib import contextmanager

DEFAULT_TIMEOUT = 5  # seconds
DEBUG_MODE = False

JSON_OUTPUT = "results.json"
HTML_OUTPUT = "report.html"

SKIP_FOLDERS: list[str] = []
RUN_FOLDERS: list[str] = []

LIND_WASM_BASE = os.environ.get("LIND_WASM_BASE", "/home/lind/lind-wasm")
LIND_FS_ROOT = os.environ.get("LIND_FS_ROOT", "/home/lind/lind-wasm/src/RawPOSIX/tmp")

<<<<<<< HEAD:wasmtestreport.py
LINDTOOL_PATH = os.path.join(LIND_WASM_BASE, "lindtool.sh")
=======
LIND_TOOL_PATH = Path(f"{LIND_WASM_BASE}/scripts")
>>>>>>> main:scripts/wasmtestreport.py
TEST_FILE_BASE = Path(f"{LIND_WASM_BASE}/tests/unit-tests")
TESTFILES_SRC = Path(f"{LIND_WASM_BASE}/tests/testfiles")
TESTFILES_DST = Path(f"{LIND_FS_ROOT}/testfiles")
DETERMINISTIC_PARENT_NAME = "deterministic"
NON_DETERMINISTIC_PARENT_NAME = "non-deterministic"
EXPECTED_DIRECTORY = Path("./expected")
SKIP_TESTS_FILE = "skip_test_cases.txt"

error_types = {
    "Failure_native_compiling": "Compilation Failure Native",
    "Failure_native_running": "Runtime Failure Native",
    "Native_Segmentation_Fault": "Segmentation Fault Native",
    "Native_Timeout": "Timeout During Native",
    "Lind_wasm_compiling": "Lind Wasm Compile Failure",
    "Lind_wasm_runtime": "Lind Wasm Runtime Failure",
    "Lind_wasm_Segmentation_Fault": "Lind Wasm Segmentation Failure",
    "Lind_wasm_Timeout": "Timeout During Lind Wasm run",
    "Unknown_Failure": "Unknown Failure",
    "Output_mismatch": "GCC and Wasm Output mismatch",
}

# ----------------------------------------------------------------------
# Function: timer
#
# Purpose:
#  Measure elapsed wall‑clock time for any code block.
#
# Variables:
#   Output: Yields a simple namespace with an ``elapsed`` attribute (float seconds).
# ----------------------------------------------------------------------
@contextmanager
def timer():
    start = time.perf_counter()
    result = type("Timer", (), {})()  # anonymous object
    try:
        yield result
    finally:
        result.elapsed = time.perf_counter() - start

# ----------------------------------------------------------------------
# Function: get_empty_result
#
# Purpose:
#   Creates and returns a dictionary to store results about 
#   test outcomes (e.g., successes, failures, timeouts).
#
# Variables:
# - Input: None
# - Output: Returns a dictionary with various counters and lists for test results.
#
# Note:
#   This is used for initializing a empty "results" dictionary for storing test stats.
# ----------------------------------------------------------------------
def get_empty_result():
    result = {
        "total_test_cases": 0,
        "success_count": 0,
        "success": [],
        "failure_count": 0,
        "failures": [],
        "test_cases": {},
    }
    for err in error_types:
        result[f"{err}_count"] = 0
        result[err] = []
    return result


# ----------------------------------------------------------------------
# Function: add_test_result
#
# Purpose:
#   Updates the given results dictionary for a test case with its outcome.
#   Handles counters for successes/failures/timeouts/segfaults.
#
# Variables:
# - Input: 
#    result (dict): The results structure to update.
#    file_path (str): The path (or name) of the test file.
#    status (str): "Success" or "Failure" for the test result.
#    error_type (str or None): The type of error if failure occurred.
#    output (str): Any relevant output from the test.
# - Output: Modifies 'result' variable, no return
#
# ----------------------------------------------------------------------
def add_test_result(result, file_path, status, error_type, output):
    result["total_test_cases"] += 1
    result["test_cases"].setdefault(file_path, {})
    result["test_cases"][file_path].update(
        {
            "status": status,
            "error_type": error_type,
            "output": output,
        }
    )

    if status.lower() == "success":
        result["success_count"] += 1
        result["success"].append(file_path)
        print("SUCCESS")
    else:
        result["failure_count"] += 1
        result["failures"].append(file_path)
        err_key = error_type if error_type in error_types else "Unknown_Failure"
        result[f"{err_key}_count"] += 1
        result[err_key].append(file_path)
        print(f"FAILURE: {error_types.get(err_key, 'Undefined Failure')}")

# ----------------------------------------------------------------------
# Function: native_compile
#
# Purpose:
#   Compiles a C source file with GCC and measures compile time.
#
# Variables:
# - Input: source_file (Path) – path to the .c file
# - Output: tuple (return_code, stdout+stderr, elapsed_time(float))
# ----------------------------------------------------------------------

def native_compile(source_file):
    cmd = ["gcc", str(source_file), "-o", str(source_file.with_suffix(".o"))]
    if DEBUG_MODE:
        print("Native compile cmd:", cmd)
    with timer() as t:
        proc = subprocess.run(cmd, capture_output=True, text=True)
    return proc.returncode, proc.stdout + proc.stderr, t.elapsed

# ----------------------------------------------------------------------
# Function: native_run
#
# Purpose:
#   Executes the native binary produced by ``native_compile``.
#
# Variables:
# - Input:
#     exec_path (Path) – path to the native binary (.o)
#     timeout  (int)  – seconds before killing
# - Output: tuple(return_code | "timeout", stdout+stderr, elapsed_time(float))
# ----------------------------------------------------------------------

def native_run(exec_path, timeout):
    cmd = [str(exec_path)]
    if DEBUG_MODE:
        print("Native run cmd:", cmd)
    try:
        with timer() as t:
            proc = subprocess.run(cmd, capture_output=True, text=True, timeout=timeout)
        return proc.returncode, proc.stdout + proc.stderr, t.elapsed
    except subprocess.TimeoutExpired:
        return "timeout", "Timed out", timeout

# ----------------------------------------------------------------------
# Function: compile_c_to_wasm
#
# Purpose:
#   Given a path to a .c file, calls `lind_compile` to compile it into wasm.
#
# Variables:
# - Input: source_file - path to the .c file.
# - Output: (wasm_file, error_message).
#       If compilation succeeds, returns paths to the wasm and empty string for error_message.
#       On failure, returns (None, <error_message>).
#
# Exceptions:
#   Catches and returns exceptions as error strings
#
# Note:
#   Dependancy on the script `lind_compile`.
# ----------------------------------------------------------------------
<<<<<<< HEAD:wasmtestreport.py
=======
def compile_c_to_wasm(source_file):
    source_file = Path(source_file).resolve()
    testcase = str(source_file.with_suffix(''))
    compile_cmd = [os.path.join(LIND_TOOL_PATH, "lind_compile"), source_file]
    if DEBUG_MODE:
        print("Running command:", compile_cmd)
        if os.path.isfile(os.path.join(LIND_TOOL_PATH, "lind_compile")):
            print("File exists and is a regular file!")
        else:
            print("File not found or it's a directory!")

>>>>>>> main:scripts/wasmtestreport.py

def wasm_compile(source_file):
    testcase = str(source_file.with_suffix(""))
    cmd = [LINDTOOL_PATH, "compile_test", testcase]
    if DEBUG_MODE:
        print("WASM compile cmd:", cmd)
    try:
<<<<<<< HEAD:wasmtestreport.py
        with timer() as t:
            proc = subprocess.run(cmd, capture_output=True, text=True)
        if proc.returncode != 0:
            return None, proc.stdout + proc.stderr, t.elapsed
        return Path(testcase + ".wasm"), "", t.elapsed
=======
        result = subprocess.run(compile_cmd, capture_output=True, text=True)
        if result.returncode != 0:
            return (None, result.stdout + "\n" + result.stderr)
        else:
            wasm_file = Path(testcase + ".cwasm")
            return (wasm_file, "")
>>>>>>> main:scripts/wasmtestreport.py
    except Exception as e:
        return None, f"Exception during compilation: {e}", 0.0

# ----------------------------------------------------------------------
# Function: run_compiled_wasm
#
# Purpose:
#   Executes the compiled wasm file using an external bash script 
#   and returns the result code and filtered output.
#
# Variables:
# - Input:
#    wasm_file (Path): path to the .wasm 
#    timeout_sec (int): time limit in seconds for the run
# - Output:
#   A tuple (returncode, output_string). Returncode can be an integer,
#   "timeout" for timeouts, or "unknown_error" for exceptions.
#
# Exceptions:
#   Catches TimeoutExpired and other Exceptions.
#
# Note:
#   Dependancy on the script "lind_run"
#   Since the script outputs the command being run, we ignore 
#   the first line in stdout by the script which is the command itself
# ----------------------------------------------------------------------
<<<<<<< HEAD:wasmtestreport.py
=======
def run_compiled_wasm(wasm_file, timeout_sec=DEFAULT_TIMEOUT):
    run_cmd = [os.path.join(LIND_TOOL_PATH, "lind_run"), wasm_file]
    if DEBUG_MODE:
        print("Running command:", run_cmd)
        if os.path.isfile(os.path.join(LIND_TOOL_PATH, "lind_run")):
            print("File exists and is a regular file!")
        else:
            print("File not found or it's a directory!")

>>>>>>> main:scripts/wasmtestreport.py

def wasm_run(wasm_file, timeout):
    testcase = str(wasm_file.with_suffix(""))
    cmd = [LINDTOOL_PATH, "run", testcase]
    if DEBUG_MODE:
        print("WASM run cmd:", cmd)
    try:
<<<<<<< HEAD:wasmtestreport.py
        with timer() as t:
            proc = subprocess.run(cmd, capture_output=True, text=True, timeout=timeout)
        # drop first line (command echo)
        output_lines = (proc.stdout + proc.stderr).splitlines()[1:]
        return proc.returncode, "\n".join(output_lines), t.elapsed
    except subprocess.TimeoutExpired:
        return "timeout", f"Timed out", timeout
=======
        proc = subprocess.run(run_cmd, capture_output=True, text=True, timeout=timeout_sec)
        full_output = proc.stdout + proc.stderr
        
        #removing the first line in output as it is the command being run by the bash script
        lines = full_output.splitlines()
        filtered_lines = lines[1:]
        filtered_output = "\n".join(filtered_lines)

        return (proc.returncode, full_output)

    except subprocess.TimeoutExpired as e:
        return ("timeout", f"Timed Out (timeout: {timeout_sec}s)")
>>>>>>> main:scripts/wasmtestreport.py
    except Exception as e:
        return "unknown_error", f"Exception: {e}", 0.0

# ----------------------------------------------------------------------
# Function: record_timings
#
# Purpose:
#   Adds individual timing metrics into ``results['test_cases'][test]`` dict.
# ----------------------------------------------------------------------

def record_timings(results_section, test_key, **times):
    results_section["test_cases"].setdefault(test_key, {})
    results_section["test_cases"][test_key].update(times)

# ----------------------------------------------------------------------
# Function: test_single_file_deterministic
#
# Purpose:
#   Compiles and runs a single test, 
#   First compiles and runs using native, then wasm
#   Finally compares the wasm output to the native native output to ensure they match.
#   Logs results (success/failure/timeouts/seg faults)
#
# Variables:
# - Input:
#   source_file : The .c file path.
#   result : The shared results dictionary.
#   timeout_sec : Timeout for the run in seconds.
# - Output:
#   Updates 'result' dictionary.
#
# Exceptions:
#   Recorded into 'result'.
#
# Note:
#   Cleans up generated files (wasm/cwasm/native output).
# ----------------------------------------------------------------------
def test_single_file_deterministic(source_file, result, timeout_sec = DEFAULT_TIMEOUT):
    source_file = source_file.resolve()
    native_out_path = source_file.with_suffix(".o")

    # Native compile & run (or read expected)
    expected_output_file = source_file.parent / EXPECTED_DIRECTORY / f"{source_file.stem}.output"
    if expected_output_file.is_file():
        with open(expected_output_file) as f:
            native_output = f.read()
        native_compile_t = native_run_t = 0.0
    else:
        # Compile
        rc, out, native_compile_t = native_compile(source_file)
        if rc != 0:
            add_test_result(result, str(source_file), "Failure", "Failure_native_compiling", out)
            record_timings(result, str(source_file), native_compile_time=native_compile_t)
            return
        # Run
        rc, out, native_run_t = native_run(native_out_path, timeout_sec)
        if rc in (134, 139):
            add_test_result(result, str(source_file), "Failure", "Native_Segmentation_Fault", out)
            record_timings(result, str(source_file), native_compile_time=native_compile_t, native_run_time=native_run_t)
            return
        if rc == "timeout":
            add_test_result(result, str(source_file), "Failure", "Native_Timeout", out)
            record_timings(result, str(source_file), native_compile_time=native_compile_t, native_run_time=native_run_t)
            return
        if rc != 0:
            add_test_result(result, str(source_file), "Failure", "Failure_native_running", out)
            record_timings(result, str(source_file), native_compile_time=native_compile_t, native_run_time=native_run_t)
            return
        native_output = out

    # WASM compile
    wasm_file, compile_err, wasm_compile_t = wasm_compile(source_file)
    record_timings(result, str(source_file), native_compile_time=native_compile_t, native_run_time=native_run_t, wasm_compile_time=wasm_compile_t)
    if wasm_file is None:
        add_test_result(result, str(source_file), "Failure", "Lind_wasm_compiling", compile_err)
        return

    # WASM run
    rc, wasm_output, wasm_run_t = wasm_run(wasm_file, timeout_sec)
    record_timings(result, str(source_file), wasm_run_time=wasm_run_t)

    if rc == "timeout":
        add_test_result(result, str(source_file), "Failure", "Lind_wasm_Timeout", wasm_output)
        return
    if rc == "unknown_error":
        add_test_result(result, str(source_file), "Failure", "Lind_wasm_runtime", wasm_output)
        return
    if rc in (134, 139):
        add_test_result(result, str(source_file), "Failure", "Lind_wasm_Segmentation_Fault", wasm_output)
        return
    if rc != 0:
        add_test_result(result, str(source_file), "Failure", "Unknown_Failure", wasm_output)
        return

    # Compare outputs
    if wasm_output.strip() == native_output.strip():
        add_test_result(result, str(source_file), "Success", None, wasm_output)
    else:
        mismatch_info = (
            "=== Native Output ===\n" + native_output + "\n\n=== WASM Output ===\n" + wasm_output
        )
        add_test_result(result, str(source_file), "Failure", "Output_mismatch", mismatch_info)

# ----------------------------------------------------------------------
# Function: test_single_file_non_deterministic
#
# Purpose:
#   Compiles and runs a single test the given test case is compiled into wasm and run. 
#   
#   Logs results (success/failure/timeouts/seg faults)
#
# Variables:
# - Input
#   source_file : The test case file path.
#   result : The results dictionary.
#   timeout_sec : Timeout for the run in seconds.
# - Output
#   Updates 'result' dictionary.
#
# Exceptions:
#   Recorded into 'result'.
#
# Note:
#   Cleans up generated files(wasm/cwasm)
#   Segmentation Fault is identified by return code of 134/139
# ----------------------------------------------------------------------
# TODO: Currently for non deterministic cases, we are only compiling and running the test case, success means the compiled test case ran, need to add more specific tests
# 

def test_single_file_non_deterministic(source_file, result, timeout_sec=DEFAULT_TIMEOUT):
    source_file = source_file.resolve()
    native_out_path = source_file.with_suffix(".o")

    # ---------- native / expected ----------
    expected_output_file = source_file.parent / EXPECTED_DIRECTORY / f"{source_file.stem}.output"
    if expected_output_file.is_file():
        with open(expected_output_file) as f:
            native_output = f.read()
        native_compile_t = native_run_t = 0.0
    else:
        print(f"Expected output not found; Compiling native")
        rc, out, native_compile_t = native_compile(source_file)
        if rc != 0:
            add_test_result(result, str(source_file), "Failure", "Failure_native_compiling", out)
            record_timings(result, str(source_file), native_compile_time=native_compile_t)
            return
        rc, out, native_run_t = native_run(native_out_path, timeout_sec)
        if rc in (134, 139):
            add_test_result(result, str(source_file), "Failure", "Native_Segmentation_Fault", out)
            record_timings(result, str(source_file), native_compile_time=native_compile_t, native_run_time=native_run_t)
            return
        if rc == "timeout":
            add_test_result(result, str(source_file), "Failure", "Native_Timeout", out)
            record_timings(result, str(source_file), native_compile_time=native_compile_t, native_run_time=native_run_t)
            return
        if rc != 0:
            add_test_result(result, str(source_file), "Failure", "Failure_native_running", out)
            record_timings(result, str(source_file), native_compile_time=native_compile_t, native_run_time=native_run_t)
            return
        native_output = out

    # ---------- wasm compile ----------
    wasm_file, compile_err, wasm_compile_t = wasm_compile(source_file)
    record_timings(result, str(source_file),native_compile_time=native_compile_t,native_run_time=native_run_t,wasm_compile_time=wasm_compile_t)
    if wasm_file is None:
        add_test_result(result, str(source_file), "Failure", "Lind_wasm_compiling", compile_err)
        return

    # ---------- wasm run ----------
    rc, wasm_output, wasm_run_t = wasm_run(wasm_file, timeout_sec)
    record_timings(result, str(source_file), wasm_run_time=wasm_run_t)
    if rc == "timeout":
        add_test_result(result, str(source_file), "Failure", "Lind_wasm_Timeout", wasm_output)
        return
    if rc == "unknown_error":
        add_test_result(result, str(source_file), "Failure", "Lind_wasm_runtime", wasm_output)
        return
    if rc in (134, 139):
        add_test_result(result, str(source_file), "Failure", "Lind_wasm_Segmentation_Fault", wasm_output)
        return
    if rc != 0:
        add_test_result(result, str(source_file), "Failure", "Unknown_Failure", wasm_output)
        return

    # ---------- compare ----------
    compare_script = source_file.with_suffix(".py")
    if compare_script.is_file():
        try:
            proc = subprocess.run([str(compare_script), wasm_output.strip(), native_output.strip()],
                                  capture_output=True, text=True, timeout=timeout_sec)
            filtered = "\n".join((proc.stdout + proc.stderr).splitlines()[1:])
            compare_outputs = (
                "=== Native Output ===\n" + native_output + "\n\n=== WASM Output ===\n" + wasm_output
            )
            if proc.returncode == 0:
                add_test_result(result, str(source_file), "Success", None, compare_outputs)
            else:
                compare_outputs += "\n" + filtered
                add_test_result(result, str(source_file), "Failure", "Output_mismatch", compare_outputs)
        except Exception as e:
            add_test_result(result, str(source_file), "Failure", "Compare_script_failure", f"Exception: {e}")
    else:
        add_test_result(result, str(source_file), "Success", None, wasm_output)

# ----------------------------------------------------------------------
# Function: pre_test
#
# Purpose:
#   Creates /src/RawPOSIX/tmp/testfiles directory, 
#   Creates readlinkfile.txt file and a soft link to it as readlinkfile(for the purpose of readlinkfile tests)
#   Copies the required test files from TESTFILES_SRC to TESTFILES_DST defined above
#
# Variables:
# - Input:
#   None
# - Output:
#   None
# ----------------------------------------------------------------------
def pre_test():
    os.makedirs(TESTFILES_DST, exist_ok=True)
    shutil.copytree(TESTFILES_SRC, TESTFILES_DST, dirs_exist_ok=True)
    readlink_txt = TESTFILES_DST / "readlinkfile.txt"
    readlink_link = TESTFILES_DST / "readlinkfile"
    open(readlink_txt, "a").close()
    try:
        readlink_link.unlink()
    except FileNotFoundError:
        pass
    os.symlink(readlink_txt, readlink_link)

# ----------------------------------------------------------------------
# Function: generate_html_report
#
# Purpose:
#   Generates the HTML report from the results object
#
# Variables:
# - Input:
#   result: The results dictionary
# - Output:
#   html_content: The contents of the HTML file as a string
#
# ----------------------------------------------------------------------
def generate_html_report(report):
    html = []
    html.append(
        """<!DOCTYPE html><html><head><meta charset='UTF-8'>
        <style>
            table,th,td{border:1px solid black;border-collapse:collapse;}
        </style></head><body><h1>Test Report</h1>"""
    )

    for category, res in report.items():
        html.append(f"<h2>{category}</h2>")
        # Summary table
        html.append("<table><tr><th>Metric</th><th>Count</th></tr>")
        html.append(f"<tr><td>Total</td><td>{res['total_test_cases']}</td></tr>")
        html.append(f"<tr><td>Successes</td><td>{res['success_count']}</td></tr>")
        html.append(f"<tr><td>Failures</td><td>{res['failure_count']}</td></tr>")
        for err in error_types:
            html.append(f"<tr><td>{error_types[err]}</td><td>{res[f'{err}_count']}</td></tr>")
        html.append("</table><br>")

        # Detailed per‑test table
        test_cases = res.get("test_cases", {})
        if not test_cases:
            continue
        html.append(
            "<table><tr><th>Test</th><th>Status</th><th>Error</th>"
            "<th>Nat&nbsp;Comp&nbsp;(s)</th><th>Nat&nbsp;Run&nbsp;(s)</th>"
            "<th>WASM&nbsp;Comp&nbsp;(s)</th><th>WASM&nbsp;Run&nbsp;(s)</th>"
            "<th>Output/Msg</th></tr>"
        )
        for test, info in test_cases.items():
            status = info.get("status", "")
            color = "lightgreen" if status.lower() == "success" else "red"
            html.append(
                f"<tr style='background:{color};'><td>{test}</td>"
                f"<td>{status}</td><td>{info.get('error_type','')}</td>"
                f"<td>{info.get('native_compile_time',0):.2f}</td>"
                f"<td>{info.get('native_run_time',0):.2f}</td>"
                f"<td>{info.get('wasm_compile_time',0):.2f}</td>"
                f"<td>{info.get('wasm_run_time',0):.2f}</td>"
                f"<td><pre>{(info.get('output') or '')[:500]}</pre></td></tr>"
            )
        html.append("</table><br>")
    html.append("</body></html>")
    return "\n".join(html)
# ----------------------------------------------------------------------
# Function: is_file_in_folder
#
# Purpose:
#   Helper function to check if a given file path is inside any of the given folders.
#
# Variables:
# - Input:
#   file_path : The file to check.
#   folder_list : List of folder paths to check against.
# - Output:
#   Returns True if the file resides in (or is the same as) any folder
#   in 'folder_list'; otherwise, returns False.
# ----------------------------------------------------------------------
def is_file_in_folder(file_path: Path, folder_list):
    resolved_file = file_path.resolve()
    for folder in folder_list:
        resolved_folder = (TEST_FILE_BASE / folder).resolve()
        if resolved_file == resolved_folder:
            return True
        if resolved_folder in resolved_file.parents:
            return True
    return False

# ----------------------------------------------------------------------
# Function: should_run_file
#
# Purpose:
#   Determines if a given test file should be executed, based on 
#   included 'run_folders' and 'skip_folders'
#
# Variables:
# - Input:
#   file_path : The file to test.
#   run_folders : Array of folders we do want to run.
#   skip_folders : Array of folders we dont want to run.
# - Output:
#   Returns False if 'skip_folders' are provided and the file is 
#   in one of them;
#   Returns True if no 'run_folders' are specified or if the file is 
#   in one of them; 
#   Otherwise False;
# ----------------------------------------------------------------------

def should_run_file(file_path, run_folders, skip_folders, skip_test_cases):
    if file_path in skip_test_cases:
        return False
    if skip_folders and is_file_in_folder(file_path, skip_folders):
        return False
    return not run_folders or is_file_in_folder(file_path, run_folders)

# Flag parser (use short names as shown in help text) --------------------------------

def check_timeout(value):
    ivalue = int(value)
    if ivalue <= 0:
        raise argparse.ArgumentTypeError("Timeout must be >0")
    return ivalue

# ----------------------------------------------------------------------
# Function: parse_arguments
#
# Purpose:
#   Parse the arguments given by user in the command.
#
# Variables:
# - Input:
#   None
# - Output:
#   Returns a dictionary with the parsed arguments
# ----------------------------------------------------------------------

def parse_arguments():
    p = argparse.ArgumentParser()
    p.add_argument("--skip", nargs="*", default=SKIP_FOLDERS)
    p.add_argument("--run", nargs="*", default=RUN_FOLDERS)
    p.add_argument("--timeout", type=check_timeout, default=DEFAULT_TIMEOUT)
    p.add_argument("--output", default=JSON_OUTPUT)
    p.add_argument("--report", default=HTML_OUTPUT)
    p.add_argument("--generate-html", action="store_true")
    p.add_argument("--pre-test-only", action="store_true")
    p.add_argument("--clean-testfiles", action="store_true")
    p.add_argument("--clean-results", action="store_true")
    return p.parse_args()

# Main ------------------------------------------------------------------------------

def main():
    os.chdir(LIND_WASM_BASE)
    args = parse_arguments()

    out_json = str(Path(args.output).with_suffix(".json"))
    out_html = str(Path(args.report).with_suffix(".html"))

    if args.clean_results:
        for f in (out_json, out_html):
            try:
                os.remove(f)
            except FileNotFoundError:
                pass
        try:
            shutil.rmtree(TESTFILES_DST)
        except FileNotFoundError:
            pass
        return

    results = {
        "deterministic": get_empty_result(),
        "non_deterministic": get_empty_result(),
    }

    try:
        shutil.rmtree(TESTFILES_DST)
    except FileNotFoundError:
        pass

    if args.clean_testfiles:
        return

    pre_test()
    if args.pre_test_only:
        print("Testfiles copied; pre‑test step only.")
        return

    all_tests = list(TEST_FILE_BASE.rglob("*.c"))
    skip_cases = set()
    try:
        with open(SKIP_TESTS_FILE) as f:
            skip_cases = {TEST_FILE_BASE / line.strip() for line in f if line.strip()}
    except FileNotFoundError:
        pass

    run_list = [tc for tc in all_tests if should_run_file(tc, args.run, args.skip, skip_cases)]
    if not run_list:
        print("No tests to run.")
        return

    for idx, test in enumerate(run_list, 1):
        print(f"[{idx}/{len(run_list)}] {test.relative_to(TEST_FILE_BASE)}")
        parent = test.parent.name
        if parent == DETERMINISTIC_PARENT_NAME:
            test_single_file_deterministic(test, results["deterministic"], args.timeout)
        else:
            test_single_file_non_deterministic(test, results["non_deterministic"], args.timeout)
        # clean artifacts
        for suf in (".wasm", ".cwasm", ".o"):
            try:
                test.with_suffix(suf).unlink()
            except FileNotFoundError:
                pass

    shutil.rmtree(TESTFILES_DST, ignore_errors=True)

<<<<<<< HEAD:wasmtestreport.py
    with open(out_json, "w") as fp:
=======
    total_count = len(tests_to_run)
    for i, source_file in enumerate(tests_to_run):
        print(f"[{i+1}/{total_count}] {source_file}")
        parent_name = source_file.parent.name

        # checks the name of immediate parent folder to see if a test is deterministic or non deterministic.
        if parent_name == DETERMINISTIC_PARENT_NAME:
            test_single_file_deterministic(source_file, results["deterministic"], timeout_sec)
        elif parent_name == NON_DETERMINISTIC_PARENT_NAME:
            test_single_file_non_deterministic(source_file, results["non_deterministic"], timeout_sec)

        wasm_file = source_file.with_suffix(".wasm")
        cwasm_file = source_file.with_suffix(".cwasm")
        native_file = source_file.with_suffix(".o")
        if wasm_file and wasm_file.exists():
            wasm_file.unlink()
        if cwasm_file and cwasm_file.exists():
            cwasm_file.unlink()
        if native_file and native_file.exists():
            native_file.unlink()
    
    shutil.rmtree(TESTFILES_DST) # removes the test files from the lind fs root
    
    os.chdir(LIND_WASM_BASE)
    with open(output_file, "w") as fp:
>>>>>>> main:scripts/wasmtestreport.py
        json.dump(results, fp, indent=4)
    print(f"Saved results to {out_json}")

<<<<<<< HEAD:wasmtestreport.py
    html = generate_html_report(results)
    with open(out_html, "w", encoding="utf-8") as fp:
        fp.write(html)
    print(f"Saved HTML report to {out_html}")
=======
    if should_generate_html:
        report_html = generate_html_report(results)
        with open(output_html_file, "w", encoding="utf-8") as out:
            out.write(report_html)
        print(f"'{os.path.abspath(output_html_file)}' generated.")

    print(f"'{os.path.abspath(output_file)}' generated.")
>>>>>>> main:scripts/wasmtestreport.py

if __name__ == "__main__":
    main()
