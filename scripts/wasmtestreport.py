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
import json
import os
import subprocess
from pathlib import Path
import argparse
import shutil
import logging

# Configure logger
logger = logging.getLogger("wasmtestreport")
logger.setLevel(logging.DEBUG)  # default to DEBUG, we will be overriding with CLI args

# Console handler
ch = logging.StreamHandler()
formatter = logging.Formatter("[%(levelname)s] %(message)s")
ch.setFormatter(formatter)
logger.addHandler(ch)

DEFAULT_TIMEOUT = 5 # in seconds

JSON_OUTPUT = "results.json"
HTML_OUTPUT = "report.html"
SKIP_FOLDERS = [] # Add folders to be skipped, the test cases inside these will not run
RUN_FOLDERS = [] # Add folders to be run, only test cases in these folders will run

LIND_WASM_BASE = os.environ.get("LIND_WASM_BASE", "/home/lind/lind-wasm")
LIND_FS_ROOT = os.environ.get("LIND_FS_ROOT", "/home/lind/lind-wasm/src/RawPOSIX/tmp")

LIND_TOOL_PATH = Path(f"{LIND_WASM_BASE}/scripts")
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
    "Output_mismatch": "GCC and Wasm Output mismatch"
    }

# ----------------------------------------------------------------------
# Function: is_segmentation_fault
#
# Purpose:
#   Checks if a given return code corresponds to a segmentation fault
#
# Variables:
# - Input: The return code
# - Output: Returns True if the return code is 139 or 134 which corresponds to a Segmentation fault
# ----------------------------------------------------------------------
def is_segmentation_fault(returncode):
    return returncode in (134, 139)
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
        "number_of_success": 0,
        "success": [],
        "number_of_failures": 0,
        "failures": [],
        "test_cases": {}
    }
    
    for err in error_types.keys():
        result[f"number_of_{err}"] = 0
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
    result["test_cases"][file_path] = {
        "status": status,
        "error_type": error_type,
        "output": output
    }

    if status.lower() == "success":
        result["number_of_success"] += 1
        result["success"].append(file_path)
        logger.info("SUCCESS")
    else:
        result["number_of_failures"] += 1
        result["failures"].append(file_path)
        
        error_message = error_types.get(error_type, "Undefined Failure")

        logger.error(f"FAILURE: {error_message}")
        if error_type in error_types:
            result[f"number_of_{error_type}"] += 1
            result[error_type].append(file_path)
        else:
            result["number_of_Unknown_Failure"] += 1
            result["Unknown_Failure"].append(file_path)


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
def compile_c_to_wasm(source_file):
    source_file = Path(source_file).resolve()
    testcase = str(source_file.with_suffix(''))
    compile_cmd = [os.path.join(LIND_TOOL_PATH, "lind_compile"), source_file]
    
    logger.debug(f"Running command: {' '.join(map(str, compile_cmd))}") 
    if os.path.isfile(os.path.join(LIND_TOOL_PATH, "lind_compile")):
        logger.debug("File exists and is a regular file!")
    else:
        logger.debug("File not found or it's a directory!")


    try:
        result = run_subprocess(compile_cmd, label="wasm compile", shell = False)
        if result.returncode != 0:
            return (None, result.stdout + "\n" + result.stderr)
        else:
            wasm_file = Path(testcase + ".cwasm")
            return (wasm_file, "")
    except Exception as e:
        return (None, f"Exception during compilation: {str(e)}")

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
def run_compiled_wasm(wasm_file, timeout_sec=DEFAULT_TIMEOUT):
    run_cmd = [os.path.join(LIND_TOOL_PATH, "lind_run"), wasm_file]
    
    logger.debug(f"Running command: {' '.join(map(str, run_cmd))}") 
    if os.path.isfile(os.path.join(LIND_TOOL_PATH, "lind_run")):
        logger.debug("File exists and is a regular file!")
    else:
        logger.debug("File not found or it's a directory!")


    try:
        proc = run_subprocess(run_cmd,label="wasm run",timeout=timeout_sec, cwd=None, shell = False)
        full_output = proc.stdout + proc.stderr
        
        #removing the first line in output as it is the command being run by the bash script
        lines = full_output.splitlines()
        filtered_lines = lines[1:]
        filtered_output = "\n".join(filtered_lines)

        return (proc.returncode, full_output)

    except subprocess.TimeoutExpired as e:
        return ("timeout", f"Timed Out (timeout: {timeout_sec}s)")
    except Exception as e:
        return ("unknown_error", f"Exception during wasm run: {str(e)}")

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
    source_file = Path(source_file).resolve()

    wasm_file, compile_err = compile_c_to_wasm(source_file)
    if wasm_file is None:
        add_test_result(result, str(source_file), "Failure", "Lind_wasm_compiling", compile_err)
        return

    try:
        retcode, output = run_compiled_wasm(wasm_file, timeout_sec)
        if retcode == "timeout":
            add_test_result(result, str(source_file), "Failure", "Lind_wasm_Timeout", output)
        elif retcode == "unknown_error":
            add_test_result(result, str(source_file), "Failure", "Lind_wasm_runtime", output)
        else:
            if retcode == 0:
                add_test_result(result, str(source_file), "Success", None, output)
            elif is_segmentation_fault(retcode):
                add_test_result(result, str(source_file), "Failure", "Lind_wasm_Segmentation_Fault", output)
            else:
                add_test_result(result, str(source_file), "Failure", "Unknown_Failure", output)
    finally:
        if wasm_file and wasm_file.exists():
            wasm_file.unlink()

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
def test_single_file_deterministic(source_file, result, timeout_sec=DEFAULT_TIMEOUT):
    source_file = Path(source_file).resolve()
    expected_output_file = source_file.parent / EXPECTED_DIRECTORY / f"{source_file.stem}.output"

    native_output = source_file.parent / f"{source_file.stem}.o"
    native_compile_cmd = ["gcc", str(source_file), "-o", str(native_output)]
    original_cwd = os.getcwd()

    if expected_output_file.is_file():
        try:
            with open(expected_output_file, 'r') as f:
                logger.info(f"Expected output found at {expected_output_file}")
                native_run_output = f.read()
        except Exception as e:
            add_test_result(result, str(source_file), "Failure", "Failure_reading_expected_file",
                            f"Exception: {e}")
            return
    else:
        logger.info(f"No expected output found at {expected_output_file}")
        #trying native compile
        try:
            proc_compile = run_subprocess(native_compile_cmd, label="gcc compile", cwd=LIND_FS_ROOT, shell=False)
            if proc_compile.returncode != 0:
                add_test_result(result, str(source_file), "Failure", "Failure_native_compiling",
                                proc_compile.stdout + proc_compile.stderr)
                return
        except Exception as e:
            add_test_result(result, str(source_file), "Failure", "Failure_native_compiling", f"Exception: {e}")
            return

        #trying native run
        try:
            proc_run = run_subprocess([str(native_output)], label="native run", cwd=LIND_FS_ROOT, shell=False)
            if proc_run.returncode != 0:
                add_test_result(result, str(source_file), "Failure", "Failure_native_running",
                                proc_run.stdout + proc_run.stderr)
                return
            native_run_output = proc_run.stdout
        except Exception as e:
            add_test_result(result, str(source_file), "Failure", "Failure_native_running", f"Exception: {e}")
            return
    
    #wasm compile
    wasm_file, compile_err = compile_c_to_wasm(source_file)
    if wasm_file is None:
        add_test_result(result, str(source_file), "Failure", "Lind_wasm_compiling", compile_err)
        return

    #wasm run
    try:
        retcode, wasm_run_output = run_compiled_wasm(wasm_file, timeout_sec)
        if retcode == "timeout":
            add_test_result(result, str(source_file), "Failure", "Lind_wasm_timeout", wasm_run_output)
        elif retcode == "unknown_error":
            add_test_result(result, str(source_file), "Failure", "Lind_wasm_runtime", wasm_run_output)
        else:
            if retcode == 0:
                expected_content = native_run_output.strip()
                wasm_content = wasm_run_output.strip()

                #verifying against expected output from native
                if wasm_content == expected_content:
                    add_test_result(result, str(source_file), "Success", None, wasm_run_output)
                else:
                    mismatch_info = (
                        "=== native Output ===\n"
                        f"{expected_content}\n\n"
                        "=== WASM Output ===\n"
                        f"{wasm_content}\n"
                    )
                    add_test_result(result, str(source_file), "Failure", "Output_mismatch", mismatch_info)
            elif is_segmentation_fault(retcode):
                add_test_result(result, str(source_file), "Failure", "Lind_wasm_Segmentation_Fault", wasm_run_output)
            else:
                add_test_result(result, str(source_file), "Failure", "Unknown_Failure", wasm_run_output)
    except:
        add_test_result(result, str(source_file), "Failure", "Unknown_Failure", wasm_run_output)

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

    readlinkfile_path = TESTFILES_DST / "readlinkfile.txt"
    symlink_path = TESTFILES_DST / "readlinkfile"
    open(readlinkfile_path, 'a').close()
    os.symlink(readlinkfile_path, symlink_path)

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
    html_content = []

    html_header = """<!DOCTYPE html>
    <html>
    <head>
        <meta charset="UTF-8">
    </head>
    <style>
        table, th, td {
        border: 1px solid black;
        border-collapse: collapse;
        }
    </style>
    <body>
    <h1>Test Report</h1>
    """

    html_content.append(html_header)

    for test_type, test_result in report.items():
        html_content.append(f'<div class="child-section">')
        html_content.append(f'<h2>{test_type}</h2>')

        html_content.append('<table class="summary-table">')
        html_content.append('<tr><th>Metric</th><th>Count</th></tr>')
        html_content.append(f'<tr><td>Total Test Cases</td><td>{test_result.get("total_test_cases", 0)}</td></tr>')
        html_content.append(f'<tr><td>Number of Successes</td><td>{test_result.get("number_of_success", 0)}</td></tr>')
        html_content.append(f'<tr><td>Number of Failures</td><td>{test_result.get("number_of_failures", 0)}</td></tr>')
        for error_type in error_types:
            html_content.append(f'<tr><td>Number of {error_types[error_type]}</td><td>{test_result.get(f"number_of_{error_types[error_type]}", 0)}</td></tr>')
        html_content.append('</table>')

    for test_type, test_result in report.items():
        html_content.append(f'<div class="child-section">')
        html_content.append(f'<h2>{test_type}</h2>')
        html_content.append('<div class="test-lists">')

        failures = test_result.get("failure", [])
        if failures:
            html_content.append("<h3>Failures:</h3>")
            html_content.append("<ul>")
            for test in failures:
                html_content.append(f"<li>{test}</li>")
            html_content.append("</ul>")

        for error_type in error_types:
            section = test_result.get(error_types[error_type],[])
            if section:
                html_content.append(f"<h3>{error_type}:</h3>")
                html_content.append("<ul>")
                for test in section:
                    html_content.append(f"<li>{test}</li>")
                html_content.append("</ul>")

        html_content.append("</div>")
        html_content.append("</div>") 

    for test_type, test_result in report.items():
        test_cases = test_result.get("test_cases", {})
        if test_cases:
            html_content.append(f'<h3>{test_type}:</h3>')
            html_content.append('<table>')
            html_content.append('<tr><th>Test Case</th><th>Status</th><th>Error Type</th><th>Output</th></tr>')
            for test, result in test_cases.items():
                if result['status'].lower() == "success":
                    bg_color = "lightgreen"
                elif result['status'].lower() == "timeout":
                    bg_color = "orange"
                else:
                    bg_color = "red"
                html_content.append(
                    f'<tr style="background-color: {bg_color};"><td>{test}</td>'
                    f'<td>{result["status"]}</td><td>{result["error_type"]}</td>'
                    f'<td><pre>{result["output"]}</pre></td></tr>'
                )
            html_content.append('</table>')

    html_content.append("</body>\n</html>")
    html_content.append("\n")
    html_content = "\n".join(html_content)
    return html_content


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
def is_file_in_folder(file_path, folder_list):
    resolved_file = file_path.resolve()
    for folder in folder_list:
        folder = TEST_FILE_BASE / f"{folder}"
        resolved_folder = folder.resolve()
        if resolved_file == resolved_folder:
            return True
        if resolved_file.is_relative_to(resolved_folder):
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
        logger.info(f"Skipping {file_path}")
        return False

    if skip_folders and is_file_in_folder(file_path, skip_folders):
        return False

    if not run_folders or is_file_in_folder(file_path, run_folders):
        return True

    return False

# ----------------------------------------------------------------------
# Function: check_timeout
#
# Purpose:
#   Determines if the given timeout is a positive integer
# Variables:
# - Input:
#   value: The value to be checked
# - Output:
#   Returns the value if it is a positive integer
#   Otherwise raise ArgumentTypeError
# ----------------------------------------------------------------------
def check_timeout(value):
    ivalue = int(value)
    if ivalue <= 0:
        raise argparse.ArgumentTypeError("Timeout should be an integer greater than 0")
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
    parser = argparse.ArgumentParser(description="Specify folders to skip or run.")
    parser.add_argument("--skip", nargs="*", default=SKIP_FOLDERS, help="List of folders to be skipped")
    parser.add_argument("--run", nargs="*", default=RUN_FOLDERS, help="List of folders to be run")
    parser.add_argument("--timeout", type=check_timeout, default=DEFAULT_TIMEOUT, help="Timeout in seconds")
    parser.add_argument("--output", default=JSON_OUTPUT, help="Name of the output file")
    parser.add_argument("--report", default=HTML_OUTPUT, help="Name of the report HTML file")
    parser.add_argument("--generate-html", action="store_true", help="Flag to generate HTML file")
    parser.add_argument("--pre-test-only", action="store_true", help="Flag to run only the copying of required testfiles")
    parser.add_argument("--clean-testfiles", action="store_true", help="Flag to remove the testfiles")
    parser.add_argument("--clean-results", action="store_true", help="Flag to clean up result files")
    parser.add_argument("--testfiles", type=Path, nargs = "+", help="Run one or more specific test files")
    parser.add_argument("--debug", action="store_true", help="Enable detailed stdout/stderr output for subprocesses")

    args = parser.parse_args()
    return args

def compare_test_results(file1, file2):
    with open(file1, 'r') as f1, open(file2, 'r') as f2:
        main_report = json.load(f1)
        curr_report = json.load(f2)
    
    new_failures = []
    status = True
    
    for test_type in main_report:
        prev_failures = set(main_report[test_type].get("failure", []))
        curr_failures = set(curr_report[test_type].get("failure", []))

        new_fails = curr_failures - prev_failures
        if new_fails:
            status = False
            new_failures.extend(new_fails)
    
    return (status, new_failures)

# ----------------------------------------------------------------------
# Function: run_subprocess
#
# Purpose:
#   Wrapper for subprocess.run with debug logging. 
#   Improves visibility during unit test execution, and replaces unsafe os.chdir usage by allowing a cwd argument.
#
# Variables:
# - Input:
#   cmd     : Command to execute (e.g., ["gcc", "main.c"]).
#   label   : Label used to tag debug log outputs.
#   cwd     : Working directory for the command.
#   shell   : Whether to run the command in a shell.
#   timeout : Timeout for command execution.
#
# - Output:
#   The result of subprocess.run()
#
# Raises:
#   ValueError - When cmd type is inconsistent with the shell mode
#   subprocess.TimeoutExpired - If the command exceeds the timeout.
#   Exception - For all other unexpected execution errors.
# ----------------------------------------------------------------------
def run_subprocess(cmd, label="", cwd=None, shell=False, timeout=None):
    """
    Wrapper for subprocess.run with optional debug logging.
    """
    # Guardrails: check cmd type consistency with shell mode
    if shell and not isinstance(cmd, str):
        raise ValueError("When shell=True, 'cmd' must be a string.")
    if not shell and not isinstance(cmd, (list, tuple)):
        raise ValueError("When shell=False, 'cmd' must be a list or tuple of args.")

    try:
        proc = subprocess.run(
            cmd,
            capture_output=True,
            text=True,
            cwd=cwd,
            shell=shell,
            timeout=timeout
        )

        
        logger.debug(f">>> {label.upper()} CMD: {' '.join(map(str, cmd))}")
        if proc.stdout.strip():
            logger.debug(f"[{label} STDOUT]\n{proc.stdout.strip()}")
        if proc.stderr.strip():
            logger.debug(f"[{label} STDERR]\n{proc.stderr.strip()}")
        return proc
    except subprocess.TimeoutExpired as e:
        logger.error(f"[{label}] TIMEOUT after {timeout}s")
        raise
    except Exception as e:
        logger.error(f"[{label}] EXCEPTION: {str(e)}")
        raise

def main():
    os.chdir(LIND_WASM_BASE)
    args = parse_arguments()
    skip_folders = args.skip
    run_folders = args.run
    timeout_sec = args.timeout
    output_file = str(Path(args.output).with_suffix('.json'))
    output_html_file = str(Path(args.report).with_suffix('.html'))
    should_generate_html = True
    pre_test_only = args.pre_test_only
    clean_testfiles = args.clean_testfiles
    clean_results = args.clean_results

    # Set DEBUG_MODE from the passed argument
    # global DEBUG_MODE
    # DEBUG_MODE = args.debug

    if args.debug:
        logger.setLevel(logging.DEBUG)
    else:
        logger.setLevel(logging.INFO)
    
    if clean_results:
        if os.path.isfile(output_file):
            os.remove(output_file)
        if os.path.isfile(output_html_file):
            os.remove(output_html_file)
        logger.debug(Path(LIND_FS_ROOT))
        for file in Path(LIND_FS_ROOT).iterdir():
            file.unlink()
        return

    results = {
        "deterministic": get_empty_result(),
        "non_deterministic": get_empty_result()
    }

    try:
        shutil.rmtree(TESTFILES_DST)
        logger.info(f"Testfiles at {LIND_FS_ROOT} deleted")
    except FileNotFoundError as e:
        logger.error(f"Testfiles not present at {LIND_FS_ROOT}")
    
    if clean_testfiles:
        return

    pre_test()
    if pre_test_only:
        logger.info(f"Testfiles copied to {LIND_FS_ROOT}")
        return

    skip_folders_paths = [Path(sf) for sf in skip_folders]
    run_folders_paths = [Path(rf) for rf in run_folders]
    
    skip_test_cases = set()
    try:
        with open(SKIP_TESTS_FILE, "r") as f:
            skip_test_cases = {TEST_FILE_BASE / line.strip() for line in f if line.strip()}
    except FileNotFoundError:
        logger.error(f"{SKIP_TESTS_FILE} not found")

    # Override test cases in skip_test_cases by passing individual test cases as arguments
    if args.testfiles: 
        tests_to_run = [Path(f).resolve() for f in args.testfiles]
    else:   
        test_cases = list(TEST_FILE_BASE.rglob("*.c")) # Gets all c files in the TEST_FILE_BASE path at all depths
        tests_to_run = []
        for test_case in test_cases:
            if should_run_file(test_case, run_folders_paths, skip_folders_paths, skip_test_cases):
                tests_to_run.append(test_case)

    if not tests_to_run:
        logger.warning("No tests found")
        return

    total_count = len(tests_to_run)
    for i, source_file in enumerate(tests_to_run):
        logger.info(f"[{i+1}/{total_count}] {source_file}")	
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
        json.dump(results, fp, indent=4)

    if should_generate_html:
        report_html = generate_html_report(results)
        with open(output_html_file, "w", encoding="utf-8") as out:
            out.write(report_html)
        logger.info(f"'{os.path.abspath(output_html_file)}' generated.")	

    logger.info(f"'{os.path.abspath(output_file)}' generated.")

if __name__ == "__main__":
    main()

