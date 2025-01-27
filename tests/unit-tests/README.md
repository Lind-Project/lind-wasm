For Compile, see [https://lind-project.github.io/lind-wasm-docs/wasmCompile/](https://lind-project.github.io/lind-wasm-docs/wasmCompile/)

Now let try to print `hello world!` by printf

```
cd /home/lind-wasm/lind-wasm-tests
git switch main
cd hello-world
export LD_LIBRARY_PATH=/home/lind-wasm/wasmtime/crates/rustposix:$LD_LIBRARY_PATH
/home/clang+llvm-16.0.4-x86_64-linux-gnu-ubuntu-22.04/bin/clang --target=wasm32-unknown-wasi --sysroot /home/lind-wasm/glibc/sysroot hello.c -g -O0 -o hello.wasm
/home/lind-wasm/wasmtime/target/debug/wasmtime hello.wasm
```
