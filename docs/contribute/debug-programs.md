# Debugging

## Debugging with GDB

To debug a WebAssembly module using GDB, ensure that your module is compiled with debugging information (e.g., using the -g flag during compilation). Additionally, the runtime itself must be built in debug mode, with `make lind-debug`, to enable effective debugging of both the runtime and the module. This allows GDB to access symbol information from both your program and the runtime.

> **Note:** Current limitations in GDB support for WebAssembly include lack of instruction-level inspection. Commands like `layout split` and `si` (step instruction) may break the terminal. It’s recommended to use `layout src` for source-level debugging.

> **Note:** GDB debugs the Rust runtime, not the C code running inside the WebAssembly module.

---

### Running GDB with the runtime

Build the debug runtime, then start GDB against it:

```sh
make lind-debug
sudo gdb --args ./build/lind-boot malloc-test.cwasm
```

**Explanation of arguments:**

- `gdb --args`: Passes arguments to the program through GDB.
- `sudo`: The runtime `chroot`s into `lindfs`, which requires root. `scripts/bin/lind_run` normally does this for you.
- `./build/lind-boot`: The runtime binary. `make lind-debug` installs the debug build here, at the same path as the release build.
- `malloc-test.cwasm`: Your program, as a path *inside* `lindfs` (see [Getting Started](../getting-started.md#what-just-happened)).

Run `./build/lind-boot --help` for the full set of runtime options.

---

### Example Debugging Session

1. **Start GDB**  
   Launch GDB with the runtime and your WebAssembly module:
   ```sh
   sudo gdb --args ./build/lind-boot malloc-test.cwasm
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

**Building with the Feature:**

Build the project from the root of the repository with `lind-debug`:

```bash
make lind-debug
```

**Usage:**

1. Decompile the WASM binary

Convert existing `.wasm` file to `.wat` format:

 ```bash
 wasm2wat <filename.wasm> --enable-all -o <filename.wat>
```

2. Add Debug Calls

Open the `.wat` file and locate the area to inspect. Since these functions return their input back to the stack, you must either use the returned value or drop it to maintain stack integrity.

Example: Debugging an Integer

```wat
;; Push a value or local onto the stack
local.get 0
;; Call the debugger (prints value to host stderr)
call $__lind_debug_num
;; Drop the returned value to keep the stack clean
drop
```

Example: Debugging a String

```wat
;; Push the memory offset (pointer) where the string starts
i32.const 1024
;; Call the debugger (prints value to host stderr)
call $__lind_debug_str
;; Drop the returned value to keep the stack clean
drop
```

> ⚠️ **Warning:** Use the offset of the pre-defined string in the binary. Defining a new string at an uncalculated offset might result in segmentation fault.

3. Recompile to WASM

After inserting debug calls, convert the file back to a binary:

```bash
wat2wasm <filename.wat> --enable-threads -o <filename.wasm>
```
