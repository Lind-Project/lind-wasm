## Dependencies
If running on docker, the only dependency user has to install will be docker and the other dependencies will be installed while running the docker file
- Docker (https://docs.docker.com/engine/install/)

If running locally,
- Rust (https://www.rust-lang.org/tools/install)
- Bazel (https://bazel.build/install)
- Environment variables:
  If user has created environment variables with the same name, it might change the behaviour of the test suite. 
  - `LIND_WASM_BASE`: Root path of the Lind WASM project(Default: /home/lind/lind-wasm/).
  - `LIND_FS_ROOT`: Path to the filesystem root used during tests(Default: /home/lind/lind-wasm/src/RawPOSIX).
  Can be set using export <env_variable_name> = <value> (in bash)
- Scripts:
  - `scripts/lindtool.sh`(File included in the repo)
---

## Testing Workflow

1. Install Docker
2. Clone the repo using `git clone https://github.com/Lind-Project/lind-wasm.git`
3. Change Directory to repo `cd lind-wasm`
3. Build Docker Image `DOCKER_BUILDKIT=1 docker build -t <image_name> -f .devcontainer/Dockerfile --build-arg DEV_MODE=true --platform=linux/amd64 .`
4. Run the image `docker run -it <image_name> /bin/bash`
5. Run the test suite `scripts/wasmtestreport.py` (This will run the whole test suite, `scripts/wasmtestreport.py --help` will provide the available arguments and flags)

## What test suite does
1. **Test Case Collection:** Scans `unit-tests` folder for `.c` files.
2. **Filtering:** Applies include/exclude filters (`--run`, `--skip`, and `skip_test_cases.txt`).
3. **Execution:** Compare WASM output with native execution.
4. **Result Recording:** All test outcomes stored with status, error type, and full output.
5. **Reporting:** JSON and HTML test report generated

---

## Output Files
- **JSON Report:** Detailed test summary in JSON format, saved at `lind-wasm/results.json`
- **HTML Report:** Human-readable visualization of test outcomes, saved at `lind-wasm/report.html`

The output will show the total number of test cases, count for success, failure and count for each of the following error types

- "Failure_native_compiling": Failed during GCC compiling
- "Failure_native_running": Failed while running GCC compiled binary
- "Native_Segmentation_Fault": Segmentation Fault while running GCC binary
- "Native_Timeout": Timed Out during GCC run
- "Lind_wasm_compiling": Failed during compilation using lind-wasm
- "Lind_wasm_runtime": Failed while running lind-wasm compiled binary
- "Lind_wasm_Segmentation_Fault": Segmentation Fault while running wasm binary
- "Lind_wasm_Timeout": Timed out During Lind Wasm run
- "Output_mismatch": Mismatch in GCC and Wasm outputs
- "Unknown_Failure": Unknown Failure

Report will also have a report on individual test cases with the status(success/failure), error_type(if failure), time taken(for compiling and running) and output generated.

---

## Directory Structure

- `tests/unit-tests/`: Folder containing all `.c` test cases.
- `expected/`: Directory under each test folder for expected output files.
- `testfiles/`: Extra files needed by tests, copied into Lind FS.

---

## Example Combined Usage

```bash
scrips/wasmtestreport.py --generate-html --skip config_tests file_tests --timeout 10 --output results_json --report test_report
```

This will:
- Skip specified folders
- Use a 10-second timeout
- Save output as `results_json.json`
- Generate a report `test_report.html`

---
