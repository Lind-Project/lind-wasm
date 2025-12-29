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
from typing import Dict
import argparse
import shutil
import logging
import tempfile

# Configure logger
logger = logging.getLogger("wasmtestreport")
logger.setLevel(logging.DEBUG)  # default to DEBUG, we will be overriding with CLI args

# Console handler
ch = logging.StreamHandler()
formatter = logging.Formatter("[%(levelname)s] %(message)s")
ch.setFormatter(formatter)
logger.addHandler(ch)

DEFAULT_TIMEOUT = 10 # in seconds

JSON_OUTPUT = "results.json"
HTML_OUTPUT = "report.html"
SKIP_FOLDERS = [] # Add folders to be skipped, the test cases inside these will not run
RUN_FOLDERS = [] # Add folders to be run, only test cases in these folders will run

SCRIPT_DIR = Path(__file__).resolve().parent
REPO_ROOT = SCRIPT_DIR.parent
LIND_WASM_BASE = Path(os.environ.get("LIND_WASM_BASE", REPO_ROOT)).resolve()
LIND_ROOT = Path(os.environ.get("LIND_ROOT", LIND_WASM_BASE / "src/tmp")).resolve()
CC = os.environ.get("CC", "gcc")  # C compiler, defaults to gcc

LIND_TOOL_PATH = LIND_WASM_BASE / "scripts"
TEST_FILE_BASE = LIND_WASM_BASE / "tests" / "unit-tests"
TESTFILES_SRC = LIND_WASM_BASE / "tests" / "testfiles"
TESTFILES_DST = LIND_ROOT / "testfiles"
DETERMINISTIC_PARENT_NAME = "deterministic"
NON_DETERMINISTIC_PARENT_NAME = "non-deterministic"
FAIL_PARENT_NAME = "fail"
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
    "Output_mismatch": "C Compiler and Wasm Output mismatch",
    "Fail_native_succeeded": "Fail Test: Native Succeeded (Should Fail)",
    "Fail_wasm_succeeded": "Fail Test: Wasm Succeeded (Should Fail)",
    "Fail_both_succeeded": "Fail Test: Both Native and Wasm Succeeded (Should Fail)",
    "Fail_native_compiling": "Fail Test: Native Compilation Failure (Should Succeed)",
    "Fail_wasm_compiling": "Fail Test: Wasm Compilation Failure (Should Succeed)"
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
# Class: TestResultHandler
#
# Purpose:
#   Centralized handler for test result patterns
#
# Variables:
# - Input: 
#   result_dict (dict) : the results dictionary.
#   source_file (str): the test file path

# - Output: Provides methods to add different types of test results; returns True/False
# ----------------------------------------------------------------------
class TestResultHandler:
    """Centralized handler for test result patterns"""
    
    def __init__(self, result_dict, source_file):
        self.result = result_dict
        self.source_file = str(source_file)
    
    def add_success(self, output=""):
        add_test_result(self.result, self.source_file, "Success", None, output)
    
    def add_compile_failure(self, error_msg, is_native=False):
        error_type = "Failure_native_compiling" if is_native else "Lind_wasm_compiling"
        add_test_result(self.result, self.source_file, "Failure", error_type, error_msg)
    
    def add_runtime_failure(self, error_msg, is_native=False):
        error_type = "Failure_native_running" if is_native else "Lind_wasm_runtime"
        add_test_result(self.result, self.source_file, "Failure", error_type, error_msg)
    
    def add_timeout(self, output, is_native=False):
        error_type = "Native_Timeout" if is_native else "Lind_wasm_Timeout"
        add_test_result(self.result, self.source_file, "Failure", error_type, output)
    
    def add_segfault(self, output, is_native=False):
        error_type = "Native_Segmentation_Fault" if is_native else "Lind_wasm_Segmentation_Fault"
        add_test_result(self.result, self.source_file, "Failure", error_type, output)
    
    def handle_return_code(self, returncode, output, is_native=False):
        """Handle return code patterns consistently"""
        if returncode == 0:
            return True  # Success case, let caller handle
        elif returncode == "timeout":
            self.add_timeout(output, is_native)
        elif returncode == "unknown_error":
            self.add_runtime_failure(output, is_native)
        elif is_segmentation_fault(returncode):
            self.add_segfault(output, is_native)
        else:
            add_test_result(self.result, self.source_file, "Failure", "Unknown_Failure", output)
        return False


# ----------------------------------------------------------------------
# Function: compile_and_run_native
#
# Purpose:
#   Compile and run native version of a test
#
# Variables:
# - Input: source_file - path to the .c file, timeout_sec - timeout for execution
# - Output: (success, output, returncode, error_type) tuple
# ----------------------------------------------------------------------
def compile_and_run_native(source_file, timeout_sec=DEFAULT_TIMEOUT):
    """Compile and run native version, return (success, output, returncode, error_type)"""
    source_file = Path(source_file)
    native_output = source_file.parent / f"{source_file.stem}.o"
    
    # Prepare any executable dependencies required by this test (like execv targets)
    executable_backups: Dict[Path, Path] = {}
    created_native_execs = set()
    native_dependencies = analyze_executable_dependencies([source_file])
    for exec_path, dependency_source in native_dependencies.items():
        dest_path = (LIND_ROOT / exec_path).resolve()
        dest_path.parent.mkdir(parents=True, exist_ok=True)

        if dest_path.exists():
            fd, backup_path = tempfile.mkstemp(prefix=f"{dest_path.name}_orig_", dir=str(dest_path.parent))
            os.close(fd)
            shutil.copy2(str(dest_path), backup_path)
            executable_backups[dest_path] = Path(backup_path)

        dep_compile_cmd = [CC, str(dependency_source), "-o", str(dest_path)]
        try:
            dep_proc = run_subprocess(dep_compile_cmd, label="native dep compile", shell=False)
        except Exception as e:
            return False, f"Exception compiling dependency {dependency_source}: {e}", "compile_error", "Failure_native_compiling"

        if dep_proc.returncode != 0:
            error_output = dep_proc.stdout + dep_proc.stderr
            return False, f"Failed to compile dependency {dependency_source}: {error_output}", "compile_error", "Failure_native_compiling"

        created_native_execs.add(dest_path)

    # Ensure paths are absolute to prevent cwd confusion
    if not source_file.is_absolute():
        raise ValueError(f"Source file must be absolute path, got: {source_file}")
    if not native_output.is_absolute():
        raise ValueError(f"Native output path must be absolute, got: {native_output}")

    # Compile
    compile_cmd = [CC, str(source_file), "-o", str(native_output)]
    try:
        proc = run_subprocess(compile_cmd, label=f"{CC} compile", cwd=LIND_ROOT, shell=False)
        if proc.returncode != 0:
            return False, proc.stdout + proc.stderr, "compile_error", "Failure_native_compiling"
    except Exception as e:
        return False, f"Exception: {e}", "compile_error", "Failure_native_compiling"
    
    # Run
    try:
        proc = run_subprocess(["stdbuf", "-oL", str(native_output)], label="native run", cwd=LIND_ROOT, shell=False, timeout=timeout_sec)
        if proc.returncode == 0:
            return True, proc.stdout, 0, None
        else:
            return False, proc.stdout + proc.stderr, proc.returncode, "Failure_native_running"
    except subprocess.TimeoutExpired:
        return False, f"Timed Out (timeout: {timeout_sec}s)", "timeout", "Native_Timeout"
    except Exception as e:
        return False, f"Exception: {e}", "unknown_error", "Failure_native_running"
    finally:
        # Clean up native binary
        native_output.unlink(missing_ok=True)

        # Restore any executable dependencies that were swapped out
        for dest_path in created_native_execs:
            try:
                dest_path.unlink(missing_ok=True)
            except (FileNotFoundError, PermissionError) as cleanup_err:
                logger.debug(f"Failed to remove native dependency {dest_path}: {cleanup_err}")

        for dest_path, backup_path in executable_backups.items():
            try:
                if backup_path.exists():
                    shutil.move(str(backup_path), str(dest_path))
            except (FileNotFoundError, PermissionError) as restore_err:
                logger.warning(f"Failed to restore dependency {dest_path} from backup: {restore_err}")

# ----------------------------------------------------------------------
# Function: get_expected_output
#
# Purpose:
#   Get expected output from file or native execution
#
# Variables:
# - Input: source_file - path to the .c file
# - Output: (success, output, error_msg, error_type) tuple
# ----------------------------------------------------------------------
def get_expected_output(source_file):
    """Get expected output from file or native execution"""
    source_file = Path(source_file)
    expected_output_file = source_file.parent / EXPECTED_DIRECTORY / f"{source_file.stem}.output"
    
    if expected_output_file.is_file():
        try:
            with open(expected_output_file, 'r') as f:
                logger.info(f"Expected output found at {expected_output_file}")
                return True, f.read(), None, None
        except Exception as e:
            return False, None, f"Exception: {e}", "Failure_reading_expected_file"
    
    # Fall back to native execution
    # TODO: Add expected output support later
    # logger.info(f"No expected output found at {expected_output_file}")
    success, output, returncode, error_type = compile_and_run_native(source_file)
    return success, output, f"Native execution: {output}" if not success else None, error_type


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
    source_file = Path(source_file)
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
            # lind_compile now places files in LIND_ROOT
            # Compute expected output path
            source_str = str(source_file)
            if "tests/unit-tests/" in source_str:
                # Extract relative path after tests/unit-tests/
                rel_path = source_str.split("tests/unit-tests/")[1]
                test_dir = Path(rel_path).parent
                filename = source_file.stem
                wasm_file = LIND_ROOT / "tests" / test_dir / f"{filename}.cwasm"
            else:
                wasm_file = LIND_ROOT / "bin" / f"{source_file.stem}.cwasm"

            if not wasm_file.exists():
                return (None, f"Expected output not found: {wasm_file}")

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
def test_single_file_unified(source_file, result, timeout_sec=DEFAULT_TIMEOUT, test_mode="deterministic"):
    """Unified test function for both deterministic, non-deterministic and failing tests"""
    source_file = Path(source_file)
    handler = TestResultHandler(result, source_file)
    
    # For fail tests, we need to run both native and wasm
    if test_mode == "fail":
        # Run native version
        native_success, native_output, native_retcode, native_error = compile_and_run_native(source_file, timeout_sec)
        
        # NOTE: We explicitly early-abort here and report the native compilation failure
        # rather than treating it as a successful "fail-test".
        if native_error == "Failure_native_compiling":
            # Record this specifically as a fail-test native-compilation error so it is
            # counted alongside other `Fail_*` test categories instead of the generic
            # compilation error bucket used elsewhere.
            failure_info = (
                "=== FAILURE: Native compilation failed during fail-test (expected runtime failure) ===\n"
                f"Native output:\n{native_output}"
            )
            add_test_result(result, str(source_file), "Failure", "Fail_native_compiling", failure_info)
            return
        
        # Compile and run WASM
        wasm_file, wasm_compile_error = compile_c_to_wasm(source_file)
        if wasm_file is None:
            # Record this specifically as a fail-test WASM-compilation error so it is
            # counted alongside other `Fail_*` test categories instead of the generic
            # Lind_wasm_compiling bucket used elsewhere.
            failure_info = (
                "=== FAILURE: Wasm compilation failed during fail-test (expected runtime failure) ===\n"
                f"Wasm compile output:\n{wasm_compile_error}"
            )
            add_test_result(result, str(source_file), "Failure", "Fail_wasm_compiling", failure_info)
            return
        
        try:
            wasm_retcode, wasm_output = run_compiled_wasm(wasm_file, timeout_sec)
            
            # Normalize return codes for comparison
            native_failed = native_retcode != 0
            
            # Check if wasm_retcode is an integer or string
            if isinstance(wasm_retcode, str):
                wasm_failed = wasm_retcode in ["timeout", "unknown_error"]  # Explicitly check for failure strings
            else:
                wasm_failed = wasm_retcode != 0
            
            # Both should fail for this test to pass
            if native_failed and wasm_failed:
                # Success: both failed as expected
                output_info = (
                    f"Native exit code: {native_retcode}\n"
                    f"Wasm exit code: {wasm_retcode}\n"
                    "Both failed as expected."
                )
                handler.add_success(output_info)
            elif not native_failed and not wasm_failed:
                # Both succeeded when they should have failed
                failure_info = build_fail_message("both", native_output, wasm_output, native_retcode, wasm_retcode)
                add_test_result(result, str(source_file), "Failure", "Fail_both_succeeded", failure_info)
            elif not native_failed:
                # Only native succeeded
                failure_info = build_fail_message("native_only", native_output, wasm_output, native_retcode, wasm_retcode)
                add_test_result(result, str(source_file), "Failure", "Fail_native_succeeded", failure_info)
            else:
                # Only wasm succeeded
                failure_info = build_fail_message("wasm_only", native_output, wasm_output, native_retcode, wasm_retcode)
                add_test_result(result, str(source_file), "Failure", "Fail_wasm_succeeded", failure_info)
        
        finally:
            # Always clean up WASM file
            if wasm_file and wasm_file.exists():
                wasm_file.unlink()
        
        return  # Exit early for fail tests
    
    # For deterministic tests, get expected output
    expected_output = None
    if test_mode == "deterministic":
        success, expected_output, error_msg, error_type = get_expected_output(source_file)
        if not success:
            add_test_result(result, str(source_file), "Failure", error_type, error_msg)
            return
    
    # Compile and run WASM
    wasm_file, wasm_compile_error = compile_c_to_wasm(source_file)
    if wasm_file is None:
        handler.add_compile_failure(wasm_compile_error)
        return
    
    try:
        retcode, wasm_output = run_compiled_wasm(wasm_file, timeout_sec)
        
        # Handle WASM execution result
        if handler.handle_return_code(retcode, wasm_output, is_native=False):
            # Success case - check output for deterministic tests
            if test_mode == "deterministic" and expected_output is not None:
                if wasm_output.strip() == expected_output.strip():
                    handler.add_success(wasm_output)
                else:
                    mismatch_info = (
                        "=== Expected Output ===\n"
                        f"{expected_output.strip()}\n\n"
                        "=== WASM Output ===\n"
                        f"{wasm_output.strip()}\n"
                    )
                    add_test_result(result, str(source_file), "Failure", "Output_mismatch", mismatch_info)
            else:
                # Non-deterministic test - just check it ran successfully
                handler.add_success(wasm_output)
    
    finally:
        # Always clean up WASM file
        if wasm_file and wasm_file.exists():
            wasm_file.unlink()

# Wrapper functions for deterministic and non-deterministic tests
def test_single_file_deterministic(source_file, result, timeout_sec=DEFAULT_TIMEOUT):
    test_single_file_unified(source_file, result, timeout_sec, "deterministic")

def test_single_file_non_deterministic(source_file, result, timeout_sec=DEFAULT_TIMEOUT):
    test_single_file_unified(source_file, result, timeout_sec, "non_deterministic")

def test_single_file_fail(source_file, result, timeout_sec=DEFAULT_TIMEOUT):
    test_single_file_unified(source_file, result, timeout_sec, "fail")

# ----------------------------------------------------------------------
# Function: analyze_testfile_dependencies
#
# Purpose:
#   Analyzes test files to determine which testfiles they need
#
# Variables:
# - Input:
#   tests_to_run: List of test files to analyze
# - Output:
#   Set of testfile names that tests actually reference
# ----------------------------------------------------------------------
def analyze_testfile_dependencies(tests_to_run):
    import re
    
    # Always include essential files for readlink tests
    all_dependencies = {'readlinkfile.txt'}
    
    # Analyze all tests to find their testfile dependencies
    for test_file in tests_to_run:
        try:
            with open(test_file, 'r') as f:
                content = f.read()
            
            # Look for any "testfiles/..." string literals in the code
            testfile_pattern = r'"testfiles/([^"]+)"'
            matches = re.findall(testfile_pattern, content, re.IGNORECASE)
            all_dependencies.update(matches)
            
            if matches:
                logger.debug(f"Found testfile dependencies in {test_file.name}: {matches}")
                
        except Exception as e:
            logger.debug(f"Could not analyze dependencies for {test_file}: {e}")
    
    return all_dependencies

# ----------------------------------------------------------------------
# Function: analyze_executable_dependencies
#
# Purpose:
#   Analyzes test files to determine which executables they need
#
# Variables:
# - Input:
#   tests_to_run: List of test files to analyze
# - Output:
#   Dictionary mapping executable paths to their source file paths
#   e.g., {'automated_tests/hello-arg': Path('hello-arg.c')}
# ----------------------------------------------------------------------
def analyze_executable_dependencies(tests_to_run):
    import re
    
    executable_deps: Dict[str, Path] = {}
    
    for test_file in tests_to_run:
        try:
            with open(test_file, 'r') as f:
                content = f.read()
            
            # Look for execv/execve/execl calls with string literal paths
            # NOTE: This intentionally only matches simple string literals like execv("path", ...)
            # It does NOT match variable references like execv(argv[0], ...) or macro expansions
            # This is a deliberate limitation to keep the dependency analysis simple and predictable
            # Pattern matches: execv("path/to/executable", ...), execl("/bin/ls", ...), etc.
            exec_pattern = r'exec[vle]+\s*\(\s*"([^"]+)"'
            matches = re.findall(exec_pattern, content, re.IGNORECASE)
            
            for exec_path in matches:
                exec_name = Path(exec_path).name

                candidate_sources = [
                    test_file.parent / f"{exec_name}.c",
                    test_file.resolve().parent / f"{exec_name}.c"
                ]

                selected_source = None
                for candidate in candidate_sources:
                    if candidate.exists():
                        selected_source = candidate
                        break

                if selected_source:
                    executable_deps[exec_path] = selected_source
                    logger.debug(f"Found executable dependency in {test_file.name}: {exec_path} -> {selected_source.name}")
                else:
                    logger.debug(
                        f"Executable {exec_path} referenced but no matching source found near {test_file}"
                    )
                
        except Exception as e:
            logger.debug(f"Could not analyze executable dependencies for {test_file}: {e}")
    
    return executable_deps

# ----------------------------------------------------------------------
# Function: create_required_executables
#
# Purpose:
#   Compiles required executables and places them in LIND_ROOT
#
# Variables:
# - Input:
#   executable_deps: Dictionary mapping executable paths to source files
# - Output:
#   None (creates executables in LIND_ROOT)
# ----------------------------------------------------------------------
def create_required_executables(executable_deps):
    if not executable_deps:
        return
    
    logger.info(f"Creating {len(executable_deps)} required executable(s)")

    for exec_path, source_file in executable_deps.items():
        try:
            # Compile the source file to WASM
            wasm_file, compile_err = compile_c_to_wasm(source_file)
            if wasm_file is None:
                logger.error(f"Failed to compile {source_file}: {compile_err}")
                continue
            # Create destination directory in LIND_ROOT
            dest_path = LIND_ROOT / exec_path
            if not dest_path.exists():
                # If not at expected path, create directory and copy
                dest_path.parent.mkdir(parents=True, exist_ok=True)
                shutil.copy2(str(wasm_file), str(dest_path))

            logger.info(f"Created executable: {dest_path}")
        except Exception as e:
            logger.error(f"Failed to create executable {exec_path}: {e}")

# ----------------------------------------------------------------------
# Function: pre_test
#
# Purpose:
#   Creates /src/tmp/testfiles directory, 
#   Creates readlinkfile.txt file and a soft link to it as readlinkfile(for the purpose of readlinkfile tests)
#   Copies the required test files from TESTFILES_SRC to TESTFILES_DST defined above
#
# Variables:
# - Input:
#   tests_to_run: Optional list of test files to analyze for dependencies
# - Output:
#   None
# ----------------------------------------------------------------------
def pre_test(tests_to_run=None):
    # Ensure LIND_ROOT exists (For CI Environment)
    os.makedirs(LIND_ROOT, exist_ok=True)
    
    # If tests_to_run is provided, use selective copying
    if tests_to_run:
        all_dependencies = analyze_testfile_dependencies(tests_to_run)
        
        # Create destination directory
        os.makedirs(TESTFILES_DST, exist_ok=True)
        
        # Copy only required files
        copied_count = 0
        missing_files = []
        
        for filename in all_dependencies:
            src_file = TESTFILES_SRC / filename
            dst_file = TESTFILES_DST / filename
            
            if src_file.exists():
                try:
                    # Create parent directories if needed
                    dst_file.parent.mkdir(parents=True, exist_ok=True)
                    shutil.copy2(src_file, dst_file)
                    copied_count += 1
                    logger.debug(f"Copied testfile: {filename}")
                except Exception as e:
                    logger.warning(f"Failed to copy {filename}: {e}")
            else:
                missing_files.append(filename)
                logger.debug(f"Referenced file not found in testfiles: {filename}")
        
        logger.info(f"Selective copy: {copied_count} files copied, {len(missing_files)} missing")
        
    else:
        # Fallback to copying all files if no tests provided
        logger.info("No test list provided, copying all testfiles")
        os.makedirs(TESTFILES_DST, exist_ok=True)
        shutil.copytree(TESTFILES_SRC, TESTFILES_DST, dirs_exist_ok=True)
    
    # Always create the readlinkfile symlink
    readlinkfile_path = TESTFILES_DST / "readlinkfile.txt"
    symlink_path = TESTFILES_DST / "readlinkfile"
    open(readlinkfile_path, 'a').close()
    if not symlink_path.exists():
        try:
            os.symlink(readlinkfile_path, symlink_path)
        except OSError:
            # Fallback to copying in case symlink creation fails
            shutil.copy2(readlinkfile_path, symlink_path)
    
    # Create required executables
    if tests_to_run:
        executable_deps = analyze_executable_dependencies(tests_to_run)
        create_required_executables(executable_deps)

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
        body {
            color: black;
            font-family: Arial, sans-serif;
            margin: 20px;
            background-color: white;
        }
        table, th, td {
            border: 1px solid black;
            border-collapse: collapse;
            padding: 8px;
        }
        th {
            background-color: #f2f2f2;
            font-weight: bold;
        }
        .test-section {
            margin: 30px 0;
            border: 2px solid #333;
            border-radius: 8px;
            padding: 20px;
        }
        .test-section h2 {
            margin-top: 0;
            color: #2c3e50;
            border-bottom: 2px solid #3498db;
            padding-bottom: 10px;
        }
        .summary-table {
            width: 100%;
            margin-bottom: 20px;
        }
        .test-results-table {
            width: 100%;
        }
        .success-row {
            background-color: #d4edda;
            color: black;
        }
        .failure-row {
            background-color: #f8d7da;
            color: black;
        }
        .timeout-row {
            background-color: #fff3cd;
            color: black;
        }
        .test-type-header {
            background-color: #e9ecef;
            text-align: center;
            font-weight: bold;
            font-size: 1.1em;
            padding: 12px;
        }
    </style>
    <body>
    <h1>Test Report</h1>
    """

    html_content.append(html_header)

    # Generate sections for Deterministic/Non-Deterministic test type
    for test_type, test_result in report.items():
        html_content.append(f'<div class="test-section">')
        html_content.append(f'<h2>{test_type.replace("_", " ").title()} Tests</h2>')
        
        # Summary table
        html_content.append('<h3>Summary</h3>')
        html_content.append('<table class="summary-table">')
        html_content.append('<tr><th>Metric</th><th>Count</th></tr>')
        html_content.append(f'<tr><td>Total Test Cases</td><td>{test_result.get("total_test_cases", 0)}</td></tr>')
        html_content.append(f'<tr><td>Number of Successes</td><td>{test_result.get("number_of_success", 0)}</td></tr>')
        html_content.append(f'<tr><td>Number of Failures</td><td>{test_result.get("number_of_failures", 0)}</td></tr>')
        for error_type in error_types:
            html_content.append(f'<tr><td>Number of {error_types[error_type]}</td><td>{test_result.get(f"number_of_{error_type}", 0)}</td></tr>')
        html_content.append('</table>')
        
        # Test cases organized by test type (process, file, memory, etc.)
        test_cases = test_result.get("test_cases", {})
        if test_cases:
            html_content.append('<h3>Test Results by Category</h3>')
            
            # Group test cases by their category
            test_categories = {}
            for test_path, result in test_cases.items():
                # Extract category from path
                path_parts = test_path.split('/')
                # Category defaults to unknown
                category = "unknown"
                for part in path_parts:
                    if part.endswith('_tests'):
                        category = part
                        break
                
                if category not in test_categories:
                    test_categories[category] = []
                test_categories[category].append((test_path, result))
            
            # Generate table with category headers
            html_content.append('<table class="test-results-table">')
            html_content.append('<tr><th>Test Case</th><th>Status</th><th>Error Type</th><th>Output</th></tr>')
            
            # Sort categories for consistent output
            for category in sorted(test_categories.keys()):
                # Add category header row
                category_display = category.replace('_', ' ').title()
                html_content.append(f'<tr class="test-type-header"><td colspan="4">{category_display}</td></tr>')
                
                # Sort tests within category
                test_cases_in_category = sorted(test_categories[category], key=lambda x: x[0])
                
                for test_path, result in test_cases_in_category:
                    # Determine row class based on status
                    if result['status'].lower() == "success":
                        row_class = "success-row"
                    elif result['status'].lower() == "timeout":
                        row_class = "timeout-row"
                    else:
                        row_class = "failure-row"
                    
                    # Extract just the test file name for display
                    test_name = test_path.split('/')[-1]
                    
                    # For successful tests, just show "Success" instead of full output
                    # For failures, show the full output for debugging
                    output_display = "Success" if result['status'].lower() == "success" else result["output"]
                    
                    html_content.append(
                        f'<tr class="{row_class}"><td>{test_name}</td>'
                        f'<td>{result["status"]}</td><td>{result["error_type"]}</td>'
                        f'<td><pre>{output_display}</pre></td></tr>'
                    )
            
            html_content.append('</table>')
        html_content.append('</div>') 

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
    parser.add_argument("--artifacts-dir", type=Path, help="Directory to store build artifacts (default: temp dir)")
    parser.add_argument("--keep-artifacts", action="store_true", help="Keep artifacts directory after run for troubleshooting")

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

# ----------------------------------------------------------------------
# Function: setup_test_environment
#
# Purpose:
#   Setup test environment and return configuration, centralizing test setup logic
#
# Variables:
# - Input: args - parsed command line arguments
# - Output: config dictionary with test setup information
# ----------------------------------------------------------------------
def setup_test_environment(args):
    """Setup test environment and return configuration"""
    config = {
        'skip_folders_paths': [Path(sf) for sf in args.skip],
        'run_folders_paths': [Path(rf) for rf in args.run],
        'skip_test_cases': set(),
        'tests_to_run': []
    }
    
    # Load skip test cases
    try:
        with open(SKIP_TESTS_FILE, "r") as f:
            config['skip_test_cases'] = {TEST_FILE_BASE / line.strip() for line in f if line.strip()}
    except FileNotFoundError:
        logger.debug(f"{SKIP_TESTS_FILE} not found")
    
    # Determine tests to run
    if args.testfiles:
        config['tests_to_run'] = [Path(f).resolve() for f in args.testfiles]
    else:
        test_cases = list(TEST_FILE_BASE.rglob("*.c"))
        config['tests_to_run'] = [
            test_case for test_case in test_cases
            if should_run_file(test_case, config['run_folders_paths'], 
                               config['skip_folders_paths'], config['skip_test_cases'])
        ]
    
    return config

# ----------------------------------------------------------------------
# Function: setup_test_file_in_artifacts
#
# Purpose:
#   Setup a test file in the artifacts directory, centralizing file setup logic
#
# Variables:
# - Input: original_source - path to original test file, artifacts_root - artifacts directory
# - Output: dest_source - path to test file in artifacts directory
# ----------------------------------------------------------------------
def setup_test_file_in_artifacts(original_source, artifacts_root):
    """Setup a test file in the artifacts directory"""
    try:
        rel_path = original_source.relative_to(TEST_FILE_BASE)
    except ValueError:
        rel_path = Path(original_source.name)
    
    dest_dir = artifacts_root / rel_path.parent
    dest_dir.mkdir(parents=True, exist_ok=True)
    dest_source = dest_dir / original_source.name
    
    # Create symlink or copy file
    if not dest_source.exists():
        try:
            dest_source.symlink_to(original_source)
        except OSError:
            shutil.copy2(original_source, dest_source)
    
    # Copy expected outputs directory if present
    expected_dir_src = original_source.parent / EXPECTED_DIRECTORY
    if expected_dir_src.is_dir():
        expected_dir_dst = dest_dir / EXPECTED_DIRECTORY
        if not expected_dir_dst.exists():
            shutil.copytree(expected_dir_src, expected_dir_dst)
    
    return dest_source

# ----------------------------------------------------------------------
# Function: run_tests
#
# Purpose:
#   Execute all tests, centralizing test execution logic
#
# Variables:
# - Input: config - test configuration, artifacts_root - artifacts directory, 
#          results - results dictionary, timeout_sec - timeout for tests
# - Output: None (modifies results dictionary)
# ----------------------------------------------------------------------
def run_tests(config, artifacts_root, results, timeout_sec):
    """Execute all tests"""
    total_count = len(config['tests_to_run'])
    
    for i, original_source in enumerate(config['tests_to_run']):
        logger.info(f"[{i+1}/{total_count}] {original_source}")
        
        dest_source = setup_test_file_in_artifacts(original_source, artifacts_root)
        
        # Determine test type and run appropriate test
        parent_name = original_source.parent.name
        if parent_name == DETERMINISTIC_PARENT_NAME:
            test_single_file_deterministic(dest_source, results["deterministic"], timeout_sec)
        elif parent_name == NON_DETERMINISTIC_PARENT_NAME:
            test_single_file_non_deterministic(dest_source, results["non_deterministic"], timeout_sec)
        elif parent_name == FAIL_PARENT_NAME:
            test_single_file_fail(dest_source, results["fail"], timeout_sec)
        else:
            # Log warning for tests not in deterministic/non-deterministic/fail folders
            logger.warning(f"Test file {original_source} is not in a deterministic, non-deterministic, or fail folder - skipping")

def build_fail_message(case: str, native_output: str, wasm_output: str, native_retcode=None, wasm_retcode=None) -> str:
    """
    Build a consistent failure message for fail-tests.

    Args:
        case: One of "both", "native_only", "wasm_only" describing which succeeded.
        native_output: Captured native stdout/stderr text.
        wasm_output: Captured wasm stdout/stderr text.
        native_retcode: Native return code (optional, included where helpful).
        wasm_retcode: Wasm return code (optional, included where helpful).

    Returns:
        A formatted failure string.
    """
    if case == "both":
        return (
            "=== FAILURE: Both Native and Wasm succeeded when they should fail ===\n"
            f"Native output:\n{native_output}\n\n"
            f"Wasm output:\n{wasm_output}"
        )
    elif case == "native_only":
        return (
            "=== FAILURE: Native succeeded when it should fail ===\n"
            f"Native output:\n{native_output}\n\n"
            f"Wasm failed with exit code {wasm_retcode}:\n{wasm_output}"
        )
    elif case == "wasm_only":
        return (
            "=== FAILURE: Wasm succeeded when it should fail ===\n"
            f"Wasm output:\n{wasm_output}\n\n"
            f"Native failed with exit code {native_retcode}:\n{native_output}"
        )
    else:
        return (
            "=== FAILURE: Unexpected fail-test result ===\n"
            f"Native (rc={native_retcode}) output:\n{native_output}\n\n"
            f"Wasm (rc={wasm_retcode}) output:\n{wasm_output}"
        )

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
    artifacts_dir_arg = args.artifacts_dir
    keep_artifacts = args.keep_artifacts

    if args.debug:
        logger.setLevel(logging.DEBUG)
    else:
        logger.setLevel(logging.INFO)
    
    # Prevent contradictory flags
    if clean_results and keep_artifacts:
        logger.error("Error: Cannot use --clean-results with --keep-artifacts")
        logger.error("--clean-results exits before running any tests")
        logger.error("--keep-artifacts only applies after tests have generated artifacts")
        return
    
    if clean_results:
        if os.path.isfile(output_file):
            os.remove(output_file)
        if os.path.isfile(output_html_file):
            os.remove(output_html_file)
        logger.debug(Path(LIND_ROOT))
        for file in Path(LIND_ROOT).iterdir():
            file.unlink()
        return

    results = {
        "deterministic": get_empty_result(),
        "non_deterministic": get_empty_result(),
        "fail": get_empty_result()
    }

    # Prepare artifacts root
    created_temp_dir = False
    if artifacts_dir_arg:
        artifacts_root = artifacts_dir_arg.resolve()
        try:
            artifacts_root.mkdir(parents=True, exist_ok=True)
            # Test writability using tempfile
            with tempfile.NamedTemporaryFile(dir=artifacts_root, delete=True):
                pass  # Successfully created and auto-deleted
        except (OSError, PermissionError) as e:
            logger.error(f"Cannot write to artifacts directory {artifacts_root}: {e}")
            return
    else:
        try:
            artifacts_root = Path(tempfile.mkdtemp(prefix="wasmtest_artifacts_"))
            created_temp_dir = True
        except OSError as e:
            logger.error(f"Cannot create temporary artifacts directory: {e}")
            return
    logger.debug(f"Artifacts root: {artifacts_root}")

    try:
        # All the main execution logic goes here
        try:
            shutil.rmtree(TESTFILES_DST)
            logger.info(f"Testfiles at {LIND_ROOT} deleted")
        except FileNotFoundError as e:
            logger.error(f"Testfiles not present at {LIND_ROOT}")
        
        if clean_testfiles:
            return

        # Setup test environment first to get tests list
        config = setup_test_environment(args)
        
        if not config['tests_to_run']:
            logger.warning("No tests found")
            return

        # Use selective testfile copying based on test dependencies
        pre_test(config['tests_to_run'])
        if pre_test_only:
            logger.info(f"Testfiles copied to {LIND_ROOT}")
            return

        # Run all tests
        run_tests(config, artifacts_root, results, timeout_sec)

        os.chdir(LIND_WASM_BASE)
        with open(output_file, "w") as fp:
            json.dump(results, fp, indent=4)

        if should_generate_html:
            report_html = generate_html_report(results)
            with open(output_html_file, "w", encoding="utf-8") as out:
                out.write(report_html)
            logger.info(f"'{os.path.abspath(output_html_file)}' generated.")	

        logger.info(f"'{os.path.abspath(output_file)}' generated.")
        if keep_artifacts:
            logger.info("Artifacts kept for troubleshooting.")

    finally:
        # ALWAYS clean up, regardless of success/failure/interruption
        try:
            shutil.rmtree(TESTFILES_DST)
        except FileNotFoundError:
            pass
            
        # Remove artifacts directory if it was temp and not requested to keep
        if created_temp_dir and not keep_artifacts:
            try:
                shutil.rmtree(artifacts_root, ignore_errors=True)
                logger.debug(f"Cleaned up temporary artifacts directory: {artifacts_root}")
            except Exception as e:
                logger.warning(f"Failed to clean up temporary artifacts directory {artifacts_root}: {e}")
        else:
            logger.info(f"Artifacts retained at: {artifacts_root}")

if __name__ == "__main__":
    main()
