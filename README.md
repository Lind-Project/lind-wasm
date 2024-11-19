First you need to download lind-wasm in your docker to home directory

```
sudo git clone https://github.com/Lind-Project/lind-wasm.git
```

I assume you have rust else use

```
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs/ | sh
rustup install nightly
. "$HOME/.cargo/env"
rustup default nightly
```

Run set.sh file

```
cd /home/lind-wasm
./set.sh
```

Now let try to print `hello world!` by printf

```
cd /home/lind-wasm/lind-wasm-tests
git switch main
cd hello-world
export LD_LIBRARY_PATH=/home/lind-wasm/wasmtime/crates/rustposix:$LD_LIBRARY_PATH
/home/clang+llvm-16.0.4-x86_64-linux-gnu-ubuntu-22.04/bin/clang --target=wasm32-unknown-wasi --sysroot /home/lind-wasm/glibc/sysroot hello.c -g -O0 -o hello.wasm
/home/lind-wasm/wasmtime/target/debug/wasmtime hello.wasm
```
