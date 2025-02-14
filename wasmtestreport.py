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
import json
import os
import subprocess
from pathlib import Path
import argparse

DEFAULT_TIMEOUT = 5 # in seconds

JSON_OUTPUT = "results.json"
HTML_OUTPUT = "report.html"
SKIP_FOLDERS = [] # Add folders to be skipped, the test cases inside these will not run
RUN_FOLDERS = [] # Add folders to be run, only test cases in these folders will run

DETERMINISTIC_PARENT_NAME = "deterministic"
NON_DETERMINISTIC_PARENT_NAME = "non-deterministic"
EXPECTED_DIRECTORY = Path("./expected")
SKIP_TESTS_FILE = "skip_test_cases.txt"

LIND_WASM_BASE = os.environ.get("LIND_WASM_BASE", "/home/lind/lind-wasm")
TEST_FILE_BASE = Path(f"{LIND_WASM_BASE}/tests/unit-tests")

error_types = {
    "Failure_native_compiling": "native_compile_failures",
    "Failure_native_running": "native_runtime_failures",
    "Segmentation_Fault": "segfaults",
    "Timeout": "timeouts"
    }

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
    return {
        "total_test_cases": 0,
        "number_of_success": 0,
        "number_of_failures": 0,
        "number_of_segfaults": 0,
        "number_of_timeouts": 0,
        "number_of_native_compile_failures": 0,
        "number_of_native_runtime_failures": 0,
        "native_compile_failures": [],
        "native_runtime_failures": [],
        "success": [],
        "failure": [],
        "segfaults": [],
        "timeouts": [],
        "test_cases": {}
    }


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

    if status == "Success":
        result["number_of_success"] += 1
        print("SUCCESS")
        result["success"].append(file_path)
    else:
        result["number_of_failures"] += 1
        print("FAILURE")
        result["failure"].append(file_path)
        if error_type in error_types:
            result[f"number_of_{error_types[error_type]}"] += 1
            result[error_types[error_type]].append(file_path)


# ----------------------------------------------------------------------
# Function: compile_c_to_wasm
#
# Purpose:
#   Given a path to a .c file, calls the lindtool script to compile it into wasm.
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
#   Dependancy on the script "./lindtool.sh compile_test".
# ----------------------------------------------------------------------
def compile_c_to_wasm(source_file):
    source_file = Path(source_file).resolve()
    testcase = str(source_file.with_suffix(''))
    compile_cmd = ["./lindtool.sh", "compile_test", testcase]

    try:
        result = subprocess.run(compile_cmd, capture_output=True, text=True)
        if result.returncode != 0:
            return (None, result.stdout + "\n" + result.stderr)
        else:
            wasm_file = Path(testcase + ".wasm")
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
#   Dependancy on the script "./lindtool.sh run"
#   Since the script outputs the command being run, we ignore 
#   the first line in stdout by the script which is the command itself
# ----------------------------------------------------------------------
def run_compiled_wasm(wasm_file, timeout_sec=DEFAULT_TIMEOUT):
    testcase = str(wasm_file.with_suffix(''))
    run_cmd = ["./lindtool.sh", "run", testcase]

    try:
        proc = subprocess.run(run_cmd, capture_output=True, text=True, timeout=timeout_sec)
        full_output = proc.stdout + proc.stderr
        
        #removing the first line in output as it is the command being run by the bash script
        lines = full_output.splitlines()
        filtered_lines = lines[1:]
        filtered_output = "\n".join(filtered_lines)

        return (proc.returncode, filtered_output)

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
        add_test_result(result, str(source_file), "Failure", "Compilation_Error", compile_err)
        return

    try:
        retcode, output = run_compiled_wasm(wasm_file, timeout_sec)
        if retcode == "timeout":
            add_test_result(result, str(source_file), "Failure", "Timeout", output)
        elif retcode == "unknown_error":
            add_test_result(result, str(source_file), "Failure", "Runtime_Error", output)
        else:
            if retcode == 0:
                add_test_result(result, str(source_file), "Success", None, output)
            elif retcode == 134 or retcode == 139:
                add_test_result(result, str(source_file), "Failure", "Segmentation_Fault", output)
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
    native_compile_cmd = f"gcc {source_file} -o {native_output}"

    if expected_output_file.is_file():
        try:
            with open(expected_output_file, 'r') as f:
                print(f"Expected output found at {expected_output_file}")
                native_run_output = f.read()
        except Exception as e:
            add_test_result(result, str(source_file), "Failure", "Failure_reading_expected_file",
                            f"Exception: {e}")
            return
    else:
        print(f"No expected output found at {expected_output_file}")
        #trying native compile
        try:
            proc_compile = subprocess.run(native_compile_cmd, shell=True, capture_output=True, text=True)
            if proc_compile.returncode != 0:
                add_test_result(result, str(source_file), "Failure", "Failure_native_compiling",
                                proc_compile.stdout + proc_compile.stderr)
                return
        except Exception as e:
            add_test_result(result, str(source_file), "Failure", "Failure_native_compiling", f"Exception: {e}")
            return

        #trying native run
        try:
            proc_run = subprocess.run(str(native_output), shell=True, capture_output=True, text=True)
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
        add_test_result(result, str(source_file), "Failure", "Compilation_Error", compile_err)
        return

    #wasm run
    try:
        retcode, wasm_run_output = run_compiled_wasm(wasm_file, timeout_sec)
        if retcode == "timeout":
            add_test_result(result, str(source_file), "Failure", "Timeout", wasm_run_output)
        elif retcode == "unknown_error":
            add_test_result(result, str(source_file), "Failure", "Runtime_Error", wasm_run_output)
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
            elif retcode == 139 or retcode == 134:
                add_test_result(result, str(source_file), "Failure", "Segmentation_Fault", wasm_run_output)
            else:
                add_test_result(result, str(source_file), "Failure", "Unknown_Failure", wasm_run_output)
    except:
        add_test_result(result, str(source_file), "Failure", "Unknown_Failure", wasm_run_output)


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
            html_content.append(f'<tr><td>Number of Segfaults</td><td>{test_result.get(f"number_of_{error_types[error_type]}", 0)}</td></tr>')
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
        print(f"skipping {file_path}")
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

    args = parser.parse_args()
    return args

def main():
    args = parse_arguments()
    skip_folders = args.skip
    run_folders = args.run
    timeout_sec = args.timeout
    output_file = str(Path(args.output).with_suffix('.json'))
    output_html_file = str(Path(args.report).with_suffix('.html'))
    should_generate_html = args.generate_html

    results = {
        "deterministic": get_empty_result(),
        "non_deterministic": get_empty_result()
    }

    skip_folders_paths = [Path(sf) for sf in skip_folders]
    run_folders_paths = [Path(rf) for rf in run_folders]
    
    skip_test_cases = set()
    try:
        with open(SKIP_TESTS_FILE, "r") as f:
            skip_test_cases = {TEST_FILE_BASE / line.strip() for line in f if line.strip()}
    except FileNotFoundError:
        print(f"{SKIP_TESTS_FILE} not found")


    test_cases = list(TEST_FILE_BASE.rglob("*.c")) # Gets all c files in the TEST_FILE_BASE path at all depths
    tests_to_run = []
    for test_case in test_cases:
        if should_run_file(test_case, run_folders_paths, skip_folders_paths, skip_test_cases):
            tests_to_run.append(test_case)

    if not tests_to_run:
        print("No tests found")
        return

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
    
    with open(output_file, "w") as fp:
        json.dump(results, fp, indent=4)

    if should_generate_html:
        report_html = generate_html_report(results)
        with open(output_html_file, "w", encoding="utf-8") as out:
            out.write(report_html)
        print(f"'{output_html_file}' generated.")

    print(f"'{output_file}' generated.")

if __name__ == "__main__":
    main()

