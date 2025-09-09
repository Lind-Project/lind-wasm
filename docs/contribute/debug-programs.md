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