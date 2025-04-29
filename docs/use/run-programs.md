## Run wasmtime
Run the `.wasm` file, modify the wasmtime path to your own

```
/home/lind-wasm/wasmtime/target/debug/wasmtime add.wasm
```

For printf.wasm, you should get `Hello World!`.


Now let try to print `hello world!` by printf

```
./lindtool.sh cptest PATH_TO_TEST
./lindtool.sh run PATH_TO_TEST
```

## Running the WebAssembly Module with Wasmtime

After compiling the WebAssembly module, you can run it using Wasmtime:

```sh
../wasmtime/target/debug/wasmtime run malloc-test.wasm
```