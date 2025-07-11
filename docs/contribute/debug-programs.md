# Debugging with GDB

To debug the WebAssembly module, you can use GDB with Wasmtime. Ensure that you have compiled the module with the `-g` flag to include debugging information.

**NOTE**: currently this debugging tool does not support inspecting instructions. And operations like `layout split` and `si` might break the terminal. Using `layout src` is recommended.

### Running GDB with Wasmtime

Use the following command to run GDB with Wasmtime:

```sh
gdb --args ../wasmtime/target/debug/wasmtime run -D debug-info -O opt-level=0 malloc-test.wasm
```

- `gdb --args`: Passes the arguments to GDB.
- `../wasmtime/target/debug/wasmtime run`: Specifies the Wasmtime executable.
- `-D debug-info`: Enables debugging information.
- `-O opt-level=0`: Sets the optimization level to 0 for debugging.

### Example Debugging Session

1. **Start GDB**:
   ```sh
   gdb --args ../wasmtime/target/debug/wasmtime run -D debug-info -O opt-level=0 malloc-test.wasm
   ```

2. **Set Breakpoints**:
   In the GDB prompt, set breakpoints as needed, for example:
   ```sh
   (gdb) break main
   ```

3. **Run the Program**:
   Start the execution of the WebAssembly module:
   ```sh
   (gdb) run
   ```

4. **Inspect and Debug**:
   Use GDB commands to inspect variables, step through the code, and debug your program:
   ```sh
   (gdb) next
   (gdb) print p
   (gdb) continue
   ```

By following these steps, you can compile, run, and debug WebAssembly modules using Wasmtime and GDB. This provides a powerful environment for developing and debugging WebAssembly applications.

For more details, refer to the official [Wasmtime documentation](https://wasmtime.dev/) and the [GDB documentation](https://www.gnu.org/software/gdb/documentation/).

