# Running unit tests
This document is a practical guide to setting up and using the Lind testing
infrastructure. It outlines the steps needed to run the test suite, execute unit
tests, and understand the results produced by the test suite, and how to
contribute new tests to the framework.

Since Lind is currently limited to the AMD64 architecture, Docker is used to
provide a consistent and controlled testing environment across different host
systems.
You can install [Docker from its website](https://docs.docker.com/engine/install/).

## Testing Workflow

1. Clone the repo using 
```
git clone https://github.com/Lind-Project/lind-wasm.git
```
2. Change directory to repo 
```
cd lind-wasm
```
3. Build Docker Image 
```
docker build -t testing_image -f .devcontainer/Dockerfile --build-arg DEV_MODE=true --platform=linux/amd64 .
```
4. Run the image 
```
docker run -it testing_image /bin/bash
```
5. Build toolchain (glibc and wasmtime)
```
bazel build //:make_glibc //:make_wasmtime
```
6. Run the test suite 
```
bazel run //:python_tests
```
(This will run the whole test suite.  Use `scripts/wasmtestreport.py --help` to
list available arguments and flags)
Note: Pass test suite arguments using
`bazel run //:python_tests -- <wasmtestreport arguments>`.
For example: `bazel run //:python_tests -- --timeout 10`



## What test suite does

1. **Test Case Collection:** Scans `unit-tests` folder for `.c` files.

2. **Filtering:** Applies include/exclude filters (`--run`, `--skip`, and
   `skip_test_cases.txt`).
3. **Checking for expected outputs:** Test suite looks for expected output from
   native run (the output of running the test case after compiling using gcc).
   If it's not found, we will compile using gcc and run it to get the native
   output.
4. **Comparing Outputs:** For deterministic test cases, the outputs are directly
   compared and considered a success if they match. For non-deterministic test
   cases, the outputs are parsed and compared using a Python script.
5. **Result Recording:** All test outcomes are stored with status, error type,
   and full output.
6. **Reporting:** JSON and HTML test reports are generated.



## Output Files
- **JSON Report:** Detailed test summary in JSON format, saved at `lind-wasm/results.json`
- **HTML Report:** Human-readable visualization of test outcomes, saved at `lind-wasm/report.html`

The output will show the total number of test cases, along with counts for
successes, failures, and each of the following error types:

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

The report also includes individual test case results, showing status
(success/failure), error type (if any), time taken (for compiling and running),
and output generated.

The outputs are split into deterministic and non-deterministic based on how the
lind-wasm outputs are compared to the native gcc output. 


## Directory Structure

- `tests/unit-tests/`: Folder containing all `.c` test cases.
- `expected/`: Directory under each test folder for expected output files.
- `testfiles/`: Extra files needed by tests, copied into Lind FS.

## How to add test cases
To add test cases, a file with .c extension containing c code can be added
to the appropriate folder in the tests/unit-tests folder.  During the test
suite run, the test case will be picked up and run. If the outputs of the file
can be directly compared, i.e. contents of gcc run == contents of lind-wasm
run, that would be enough

If there are changes in the outputs of the native run and lind-wasm run and the
outputs need to be parsed and compared with custom functions, the comparator
python script can be added at the same folder as the test case and the python
script should return success (exit 0) if all conditions are met, or error if
the test fails.

Any failure in compiling or running using gcc or lind-wasm is considered a
failure. Additionally, if there is a script for comparing outputs, the failure
of that script is also considered a failure. Mismatch in native(gcc) and wasm
outputs are also considered a failure.


## Example Combined Usage

```
scripts/wasmtestreport.py --generate-html --skip config_tests file_tests --timeout 10 --output results_json --report test_report
```

This will:

- Skip specified folders
- Use a 10-second timeout
- Save output as `results_json.json`
- Generate a report `test_report.html`

