# Debugging

## Debugging with GDB

To debug a WebAssembly module using GDB, ensure that your module is compiled with debugging information (e.g., using the -g flag during compilation). Additionally, Wasmtime itself must be compiled in debug mode (i.e., without the --release flag) to enable effective debugging of both the runtime and the module. This allows GDB to access symbol information from both your program and Wasmtime.

> **Note:** Current limitations in GDB support for WebAssembly include lack of instruction-level inspection. Commands like `layout split` and `si` (step instruction) may break the terminal. It’s recommended to use `layout src` for source-level debugging.

---

### Running GDB with Wasmtime

Use the following command to start GDB with Wasmtime:

```sh
gdb --args ../wasmtime/target/debug/wasmtime run -D debug-info -O opt-level=0 malloc-test.wasm
```

**Explanation of arguments:**

- `gdb --args`: Passes arguments to the program through GDB.
- `../wasmtime/target/debug/wasmtime run`: Runs your WebAssembly module using the Wasmtime binary.
- `-D debug-info`: Enables Wasmtime’s debug information support.
- `-O opt-level=0`: Disables optimizations for easier debugging.

---

### Example Debugging Session

1. **Start GDB**  
   Launch GDB with Wasmtime and your WebAssembly module:
   ```sh
   gdb --args ../wasmtime/target/debug/wasmtime run -D debug-info -O opt-level=0 malloc-test.wasm
   ```

2. **Set Breakpoints**  
   In the GDB prompt, set breakpoints as needed:
   ```sh
   (gdb) break main
   ```

3. **Run the Program**  
   Start execution:
   ```sh
   (gdb) run
   ```

4. **Inspect and Debug**  
   Use GDB commands to step through and inspect your code:
   ```sh
   (gdb) next
   (gdb) print p
   (gdb) continue
   ```

---

### Additional Resources

- [Wasmtime Documentation](https://wasmtime.dev/)
- [GDB Manual](https://www.gnu.org/software/gdb/documentation/)

---

## Other Debugging Techniques

### Disabling Signals for Debugging

The `signal-disable` feature added in this PR allows `lind-wasm` to run binaries without inserting Wasmtime epoch signals, which is useful for debugging purposes. When this feature is enabled, the signal handler is not set, and any unexpected signals (e.g., timeouts or faults) will cause the program to crash directly in RawPOSIX, making issues easier to trace.

> ⚠️ **Warning:** This feature is intended for debugging only and should not be used in production environments.

To use this feature, compile `lind-wasm` with the `signal-disable` feature enabled. Here’s how to do it:

**Building with the Feature:**

From the root of the repository, navigate to `src/wasmtime` and build with the `signal-disable` feature:

```bash
cd src/wasmtime

# Build lind-wasm with the signal-disable feature
cargo build --features signal-disable
```

### Debugging at WASM/WAT Level

Two host-defined functions, `lind_debug_num()` and `lind_debug_str()`, are imported into the compiled WASM binary to support debugging at the WASM/WAT level. These functions facilitate debugging at the WASM/WAT level, allowing for the inspection of stack values and memory contents in environments where traditional debuggers (like GDB) cannot easily attach or provide visibility.

1. Decompile the WASM binary

Convert existing `.wasm` file to `.wat` format:

 `wasm2wat <filename.wasm> --enable-all -o <filename.wat>`

2. Add Debug Calls

Open the `.wat` file and locate the area to inspect. Since these functions return their input back to the stack, you must either use the returned value or drop it to maintain stack integrity.

Example: Debugging an Integer

```
;; Push a value or local onto the stack
local.get 0
;; Call the debugger (prints value to host stderr)
call $__lind_debug_num
;; Drop the returned value to keep the stack clean
drop
```

Example: Debugging a String

```
;; Push the memory offset (pointer) where the string starts
i32.const 1024
;; Call the debugger (prints value to host stderr)
call $__lind_debug_str
;; Drop the returned value to keep the stack clean
drop
```

> ⚠️ **Warning:** Use the offset of the pre-defined string in the binary. Defining a new string at an uncalculated offset might result in SIGSEGV.

3. Recompile to WASM

After inserting debug calls, convert the file back to a binary:

`wat2wasm <filename.wat> --enable-threads -o <filename.wasm>`
