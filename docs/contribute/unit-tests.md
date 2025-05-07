# wasmtestreport.py Documentation

## Command-Line Options
scripts/wasmtestreport.py --help will provide the available arguments and flags.
| **Option** | **Description** | **Example Usage** |
|------------|------------------|--------------------|
| `--skip <folder1> <folder2>` | Skip tests in the specified folders. | `./wasmtestreport.py --skip config_tests file_tests` |
| `--run <folder1> <folder2>` | Run only the tests in the specified folders. | `./wasmtestreport.py --run config_tests file_tests` |
| `--timeout <seconds>` | Timeout value in seconds for each test. Must be a positive integer. | `./wasmtestreport.py --timeout 10` |
| `--output <filename>` | Output JSON file name (default: `results.json`). | `./wasmtestreport.py --output newresult` |
| `--report <filename>` | Output HTML report file name (default: `report.html`). | `./wasmtestreport.py --report myreport.html` |
| `--pre-test-only` | Only prepare the test files in Lind FS, no test execution. | `./wasmtestreport.py --pre-test-only` |
| `--clean-testfiles` | Clean up the copied test files from Lind FS. | `./wasmtestreport.py --clean-testfiles` |
| `--clean-results` | Clean all result files and outputs. | `./wasmtestreport.py --clean-results` |

---

## Testing Workflow

1. **Test Case Collection:** Scans `unit-tests` folder for `.c` files.
2. **Filtering:** Applies include/exclude filters (`--run`, `--skip`, and `skip_test_cases.txt`).
3. **Execution:** Compare WASM output with native execution.
4. **Result Recording:** All test outcomes stored with status, error type, and full output.
5. **Reporting:** JSON and HTML test report generated

---

## Output Files
The output will show the total number of test cases, count for success, failure and count for each of the following error types
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

The report will also have details of the paths to the testcases in each type.
Report will also have a report on individual test cases with the status(success/failure), error_type(if failure), and output generated


- **JSON Report:** Detailed test summary in structured format.
- **HTML Report:** Human-readable visualization of test outcomes.


---

## Directory Structure

- `tests/unit-tests/`: Folder containing all `.c` test cases.
- `expected/`: Directory under each test folder for expected output files.
- `testfiles/`: Extra files needed by tests, copied into Lind FS.

---

## Dependencies

- Environment variables:
  If user has created environment variables with the same name, it might change the behaviour of the test suite. 
  - `LIND_WASM_BASE`: Root path of the Lind WASM project(Default: /home/lind/lind-wasm/).
  - `LIND_FS_ROOT`: Path to the filesystem root used during tests(Default: /home/lind/lind-wasm/src/RawPOSIX).
  Can be set using export <env_variable_name> = <value> (in bash)
- External Scripts:
  - `scripts/lindtool.sh`
  File included in the repo
---

## Example Combined Usage

```bash
./wasmtestreport.py --generate-html --skip config_tests file_tests --timeout 10 --output results_json --report test_report
```

This will:
- Skip specified folders
- Use a 10-second timeout
- Save output as `results_json.json`
- Generate a report `test_report.html`

---
