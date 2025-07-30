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

Coming soon