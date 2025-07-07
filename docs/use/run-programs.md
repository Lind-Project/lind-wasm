## Run wasmtime
Note: You should first follow the instructions in 
[Getting started]
(https://lind-project.github.io/lind-wasm/use/getting-started/)
 and be inside the container before this example. 

Run the `.wasm` file, modify the wasmtime path to your own

Here is an example to run the `printf.c` with wasmtime

```
cd $HOME/lind-wasm
./scripts/lindtool.sh cpwasm
./scripts/lindtool.sh cptest tests/unit-tests/file_tests/deterministic/printf
src/wasmtime/target/debug/wasmtime run --wasi threads=y --wasi preview2=n \
tests/unit-tests/file_tests/deterministic/printf.wasm
```

For printf.wasm, you should get `Hello World!`.


Now let try to print `hello world!` by printf

```
./scripts/lindtool.sh cptest tests/unit-tests/file_tests/deterministic/printf
./scripts/lindtool.sh run tests/unit-tests/file_tests/deterministic/printf
```

## Running the WebAssembly Module with Wasmtime

After compiling the WebAssembly module, you can run it using Wasmtime:

```sh
../wasmtime/target/debug/wasmtime run malloc-test.wasm
```