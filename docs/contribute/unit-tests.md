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
docker build --platform=linux/amd64 -f Docker/Dockerfile.e2e -t dev --target base .
```
4. Run the image 
```bash
# Note: The `-v` option mounts your repo into the container. This means, you can
# live-edit the files in the container using your host editor. And files created
# or edited in the container, e.g. when running `make`, persist on the host.

docker run --platform=linux/amd64 -v $(PWD):/lind -w /lind -it dev /bin/bash
```
5. Build toolchain (glibc and wasmtime)
```
# this may take a while ...
make lind-boot sysroot
```
6. Run the test suite 
```
./scripts/wasmtestreport.py
```
Run `scripts/wasmtestreport.py --help` to list available usage options.



## What test suite does

1. **Test Case Collection:** Scans `unit-tests` folder for `.c` files.

2. **Filtering:** Applies include/exclude filters (`--run`, `--skip`, and
   `skip_test_cases.txt`).
3. **Test Execution:** Compiles and executes each test case twice, with native
   gcc and with lind-wasm, and records the output. *(note: gcc is skipped for
   tests with expected output fixture, and for tests with non-deterministic output)*
4. **Comparing Outputs:**  Marks test as successful, if outputs match
   *(note: non-deterministic tests always succeed, if compilation and execution
   succeeds)*
5. **Reporting:** Test results are written to a JSON- and  an HTML-formatted
   report in the current working directory. The reports include a summary of the
   full test run, and status, error type, and output of each test case.

## Error Types

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

Any failure in compiling or running using gcc or lind-wasm is considered a
failure. Mismatch in native (gcc) and wasm outputs are also considered a
failure.


## Example Combined Usage

```
./scripts/wasmtestreport.py \
  --generate-html \
  --skip config_tests file_tests \
  --timeout 10 \
  --output results_json \
  --report test_report  
```

This will:

- Skip specified folders
- Use a 10-second timeout
- Save output as `results_json.json`
- Generate a report `test_report.html`

