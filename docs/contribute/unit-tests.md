

|**Commands**|**Short Commands**|**Description**|**Example Usage**|
|-|-|-|-|
| **singlecompile**|`sc`| Compile a single `.c` file to `.wasm` and `.cwasm`. | `./wasmtest.sh sc file.c`|
| **singlerun**|`sr`| Run a single previously compiled `.wasm`/`.cwasm`file. |`./wasmtest.sh sr file.c`|
| **single** |`s`| Compile and run a single `.c` file in sequence (first compile to WASM, then run).| `./wasmtest.sh s file.c`|
| **allcompile**| `ac`| Compile all `.c` test files in `$LIND_WASM_BASE/tests/unit-tests`. | `./wasmtest.sh ac`|
| **allrun**|`ar`| Run all compiled tests in `$LIND_WASM_BASE/tests/unit-tests`.| `./wasmtest.sh ar`|
| all|`a`| Compile and run all test files in `$LIND_WASM_BASE/tests/unit-tests`.| `./wasmtest.sh a`|
| **filescompile**, **filecompile**|`fc`| Compile test files listed in a user-provided text file (each line should contain one `.c` filename).| `./wasmtest.sh fc filelist.txt`|
| **filesrun**, **filerun**|`fr`| Run test files listed in a user-provided text file (assumes the `.wasm`/`.cwasm` files are already compiled). |`./wasmtest.sh fr filelist.txt`|
| **files**, **file** |`f`| Compile and run test files listed in a user-provided text file (each line should contain one `.c` filename).| `./wasmtest.sh f filelist.txt`                             |
| **`--timeout=<seconds>`** (optional)| *(none)*| Set a custom test timeout (in seconds). Default: **5 seconds**.| `./wasmtest.sh single file.c --timeout=10`|


### Notes
- **`LIND_WASM_BASE`**: Defaults to `/home/lind/lind-wasm`. You can override it:  
  ```bash
  export LIND_WASM_BASE="/custom/path/to/lind-wasm"
- **`<file>.c`**: A single test file to compile or run, found in `$LIND_WASM_BASE/tests/unit-tests`.  
- **`<filelist>`**: A text file containing one `.c` filename per line, found in `$LIND_WASM_BASE/tests/unit-tests`.  

